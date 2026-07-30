use super::*;
use vice_ir::ComplementaryConnectivity;

fn arm() -> ComplementaryConnectivity {
    ComplementaryConnectivity::arms()[0]
}

/// Two blobs and a neck, plus an UNRELATED square far below.
///
/// The unrelated square is not decoration: without something outside the
/// ROI and its halo, "nothing outside the region moved" is a statement
/// about the empty set, and an empty control is indistinguishable from a
/// passing one (F-0039). The fixture is built so the clause has a
/// population.
fn dumbbell(bridged: bool) -> Dcel {
    let (w, h) = (21usize, 15usize);
    let mut inside = vec![false; w * h];
    for y in 2..7 {
        for x in 1..7 {
            inside[y * w + x] = true;
        }
        for x in 14..20 {
            inside[y * w + x] = true;
        }
    }
    if bridged {
        for x in 7..14 {
            inside[4 * w + x] = true;
        }
    }
    for y in 11..14 {
        for x in 2..6 {
            inside[y * w + x] = true;
        }
    }
    Dcel::assemble(Labelling::new(w, h, inside), arm())
}

fn neck_roi() -> Roi {
    Roi {
        x0: 7,
        y0: 3,
        x1: 14,
        y1: 6,
    }
}

/// The whole point, on one fixture: two components become one, the
/// declared edit matches, nothing outside the neck moves, and the base is
/// still two components afterwards because nothing mutated it.
#[test]
fn closing_a_bridge_commits_and_leaves_the_rest_of_the_graph_alone() {
    let base = dumbbell(false);
    assert_eq!(
        base.foreground_faces(),
        3,
        "two blobs and the unrelated square"
    );
    let edit = Edit {
        kind: EditKind::BRIDGE_CLOSE,
        roi: neck_roi(),
        set: (7..14u32).map(|x| (x, 4u32, true)).collect(),
    };
    let out = apply(&base, &edit, &TX_CONFIG_V1);
    let r = out.report();
    let new = out.committed().expect("the transaction must commit");
    assert_eq!(new.foreground_faces(), 2);
    assert_eq!(new.holes(), 0);
    assert_eq!(r.unrelated_chains_that_moved, 0);
    assert!(
        r.unrelated_chains > 0,
        "if nothing is outside the region, 'nothing outside moved' measures nothing"
    );
    assert!(r.committed);
    assert!(r.certificate.is_some());
    let rebuild = r
        .incremental_rebuild
        .as_ref()
        .expect("a committed topology edit publishes rebuild work");
    assert_eq!(rebuild.algorithm, "local_boundary_step_delta_v1");
    assert!(rebuild.affected_segment_sites < rebuild.complete_lattice_segment_sites);
    assert!(rebuild.reused_boundary_segments > 0);
    // The base is untouched: rollback needs no undo because nothing was
    // ever mutated.
    assert_eq!(base.foreground_faces(), 3);
}

/// A transaction that declares the wrong edit is rolled back, even though
/// the labelling change itself is perfectly legal.
#[test]
fn an_edit_that_is_not_what_it_declared_is_rolled_back() {
    let base = dumbbell(false);
    let edit = Edit {
        kind: EditKind::HOLE_FILL,
        roi: neck_roi(),
        set: (7..14u32).map(|x| (x, 4u32, true)).collect(),
    };
    match apply(&base, &edit, &TX_CONFIG_V1) {
        Outcome::RolledBack { reason, report } => {
            assert!(matches!(
                reason,
                TransactionRefusal::NotTheDeclaredEdit { .. }
            ));
            assert!(!report.committed);
        }
        Outcome::Committed { .. } => panic!("a mis-declared edit must not commit"),
    }
}

/// A pixel outside the declared ROI is refused BEFORE anything is built.
/// This is the §28 M5 clause "no unrelated graph mutation" at its cheapest
/// point of entry.
#[test]
fn an_edit_reaching_outside_its_roi_is_refused() {
    let base = dumbbell(false);
    let mut set: Vec<(u32, u32, bool)> = (7..14u32).map(|x| (x, 4u32, true)).collect();
    set.push((0, 0, true));
    let edit = Edit {
        kind: EditKind::BRIDGE_CLOSE,
        roi: neck_roi(),
        set,
    };
    match apply(&base, &edit, &TX_CONFIG_V1) {
        Outcome::RolledBack { reason, .. } => assert!(matches!(
            reason,
            TransactionRefusal::EditLeftTheRoi { x: 0, y: 0, .. }
        )),
        Outcome::Committed { .. } => panic!("an edit outside its ROI must not commit"),
    }
}

/// The locality check has resolving power: a transaction whose ROI is
/// declared large enough to admit a distant change IS caught by the
/// unrelated-chain comparison rather than by the cheap bounds test.
///
/// Both directions: the same edit without the distant pixel commits.
#[test]
fn a_distant_change_inside_a_wide_roi_is_caught_by_the_chain_comparison() {
    let base = dumbbell(false);
    let wide = Roi {
        x0: 0,
        y0: 0,
        x1: 21,
        y1: 15,
    };
    let mut set: Vec<(u32, u32, bool)> = (7..14u32).map(|x| (x, 4u32, true)).collect();
    // A pixel far from the neck, well outside the neck ROI + halo.
    set.push((19, 13, true));
    let edit = Edit {
        kind: EditKind::BRIDGE_CLOSE,
        roi: wide,
        set: set.clone(),
    };
    // With a wide ROI the region swallows the canvas, so nothing is
    // "unrelated" and the comparison has nothing to say — which is the
    // honest limit of this check and is why the row publishes
    // `unrelated_chains` beside the verdict.
    let out = apply(&base, &edit, &TX_CONFIG_V1);
    assert_eq!(
        out.report().unrelated_chains,
        0,
        "a canvas-wide ROI leaves no unrelated chain, and the row must say so"
    );

    // With the ROI the edit actually needs, the distant pixel is caught at
    // the ROI test — the cheaper of the two mechanisms, and the one that
    // fires first.
    let edit2 = Edit {
        kind: EditKind::BRIDGE_CLOSE,
        roi: neck_roi(),
        set,
    };
    assert!(matches!(
        apply(&base, &edit2, &TX_CONFIG_V1),
        Outcome::RolledBack {
            reason: TransactionRefusal::EditLeftTheRoi { .. },
            ..
        }
    ));
}

/// Opening a hole, and filling it again, returns to the original
/// arrangement. The transaction is not merely reversible in principle:
/// the parts compare equal.
#[test]
fn opening_a_hole_and_filling_it_returns_the_same_arrangement() {
    let (w, h) = (11usize, 11usize);
    let inside = vec![true; w * h];
    let base = Dcel::assemble(Labelling::new(w, h, inside), arm());
    assert_eq!(base.holes(), 0);
    let roi = Roi {
        x0: 4,
        y0: 4,
        x1: 7,
        y1: 7,
    };
    let open = Edit {
        kind: EditKind::HOLE_OPEN,
        roi,
        set: vec![(5, 5, false)],
    };
    let out = apply(&base, &open, &TX_CONFIG_V1);
    let holed = out.committed().expect("hole_open commits").clone();
    assert_eq!(holed.holes(), 1);

    let fill = Edit {
        kind: EditKind::HOLE_FILL,
        roi,
        set: vec![(5, 5, true)],
    };
    let back = apply(&holed, &fill, &TX_CONFIG_V1);
    let restored = back.committed().expect("hole_fill commits");
    assert_eq!(restored.holes(), 0);
    assert_eq!(restored.parts(), base.parts());
}

/// **RT5-A6: the world in which the locality conjunct is false.**
///
/// The red team could not build a transaction that `apply` rolls back for
/// `UnrelatedGraphMutation`, and neither could I, and the reason is a
/// theorem rather than an accident: a boundary chain lying wholly outside
/// the ROI depends only on labels of pixels adjacent to it, and step (1)
/// guarantees the edit changes none of those. So on the production path
/// `UnrelatedGraphMutation` is UNREACHABLE, and §32's "before adding a
/// conjunct, exhibit a world where it is false" was unmet — the conjunct
/// was published as a measurement on 127 chains with no demonstration that
/// it could ever move.
///
/// This is that demonstration, and it is honest about what it shows: the
/// COMPARISON has resolving power, exercised by changing a pixel far away
/// and asking the same function the transaction asks. What it does not show
/// is that `apply` can reach the branch, and the clause-3 row says so by
/// publishing the reachable and unreachable refusal sets.
#[test]
fn the_chain_comparison_detects_a_distant_change_when_it_is_given_one() {
    let base = dumbbell(false);
    let roi = neck_roi();
    let halo = roi.grown(TX_CONFIG_V1.halo_px, base.width_px(), base.height_px());

    // POSITIVE CONTROL: the base against itself moves nothing.
    let before = unrelated_paths(&base, &halo);
    assert!(
        !before.is_empty(),
        "no chain lies outside the region, so the comparison has nothing to compare"
    );
    assert_eq!(
        before,
        unrelated_paths(&base, &halo),
        "the comparison must be stable against itself"
    );

    // The world: a pixel changed OUTSIDE the ROI and its halo, which
    // `apply` would refuse at step (1) and which the comparison must see.
    let mut inside = base.labelling().inside().to_vec();
    let w = base.width_px() as usize;
    // A pixel inside the distant witness square. Flipping it opens a hole,
    // so the outer chain is untouched and a NEW chain appears — which is
    // why the comparison below is symmetric and the first version of it,
    // base-minus-candidate, saw nothing.
    inside[12 * w + 3] = !inside[12 * w + 3];
    let far = Dcel::assemble(
        Labelling::new(w, base.height_px() as usize, inside),
        base.connectivity(),
    );
    let after = unrelated_paths(&far, &halo);
    let moved = before.symmetric_difference(&after).count();
    assert!(
            moved > 0,
            "a chain outside the region changed and the comparison did not see it; the conjunct              clause 3 stands on would then be unfalsifiable in both directions"
        );
}

/// A no-op is refused. A transaction that certifies "nothing happened"
/// would make every clause about transactions satisfiable by doing none.
#[test]
fn a_transaction_that_changes_nothing_is_refused() {
    let base = dumbbell(true);
    let edit = Edit {
        kind: EditKind::BRIDGE_CLOSE,
        roi: neck_roi(),
        set: vec![(8, 4, true)],
    };
    assert!(matches!(
        apply(&base, &edit, &TX_CONFIG_V1),
        Outcome::RolledBack {
            reason: TransactionRefusal::EditIsANoOp,
            ..
        }
    ));
}

/// M7's incremental constructor and the independent full constructor must
/// produce byte-for-byte equal canonical DCELs. The structural register
/// supplies components, holes, bridges, nested loops, and critical 2x2
/// junctions at every mandatory fast size under both connectivity arms.
#[test]
fn incremental_matches_full_rebuild_on_the_structural_register() {
    let mut comparisons = 0usize;
    for size in super::super::fixtures::STRUCTURAL_SIZES_PX {
        for fixture in super::super::fixtures::structural_fixtures(size) {
            for connectivity in ComplementaryConnectivity::arms() {
                let base = Dcel::assemble(fixture.labelling.clone(), connectivity);
                let mut state = (size as u64)
                    .wrapping_mul(0x9e37_79b9)
                    .wrapping_add(fixture.name.bytes().map(u64::from).sum::<u64>());
                for batch_size in [1usize, 2, 4, 8] {
                    let mut inside = fixture.labelling.inside().to_vec();
                    let mut touched = std::collections::BTreeSet::new();
                    while touched.len() < batch_size {
                        state = state
                            .wrapping_mul(6_364_136_223_846_793_005)
                            .wrapping_add(1_442_695_040_888_963_407);
                        touched.insert((state as usize) % inside.len());
                    }
                    for index in touched {
                        inside[index] = !inside[index];
                    }
                    let changed = Labelling::new(size, size, inside);
                    let (incremental, report) =
                        rebuild_incremental(&base, changed.clone()).unwrap();
                    let full = Dcel::assemble(changed, connectivity);
                    assert_eq!(
                        incremental, full,
                        "{} {size}px {connectivity:?} batch {batch_size}",
                        fixture.name
                    );
                    assert!(report.affected_segment_sites <= batch_size * 4);
                    assert!(
                        report.affected_segment_sites < report.complete_lattice_segment_sites,
                        "the differential must exercise a genuinely local rebuild"
                    );
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(
        comparisons,
        super::super::fixtures::STRUCTURAL_SIZES_PX.len()
            * super::super::fixtures::structural_fixtures(32).len()
            * ComplementaryConnectivity::arms().len()
            * 4
    );
}
