//! Properties of the shared DCEL over the sizes and shapes it is actually
//! applied to (spec v1.3 §12, §27.5, §28 M5).
//!
//! ## Why this file exists at all
//!
//! F-0054 / F-9 is the standing class of this project: **an instrument's
//! domain of proof must be compared with its domain of use, and the comparison
//! must be by COMPOSITION rather than by boundary.** M4.5's judge was proved
//! on twelve hand-written fixtures no larger than 7x7 and decided questions on
//! renders up to 512 px; the gap was found by the red team, and freezing it
//! cost the milestone a delta.
//!
//! So the DCEL's proof domain is built as three axes, on purpose, and each one
//! is named:
//!
//! | axis | how it is covered | what would be missed without it |
//! |---|---|---|
//! | **exhaustive** | every labelling of 4x3 and 4x4, both convention arms ([`vice_topology::audit_every_labelling`]) | any defect reachable at small size — there is no subclass left to hide in |
//! | **size** | the corpus sizes 32, 64, 128 and the declared-but-unreached 256 and 512 | a defect that only triggers above the fixture ceiling — F-8 in M4.5 |
//! | **structure** | five structural fixtures AT EVERY SIZE: annulus, nested annulus, bridge, two components, DIAGONAL PINCH | the register the corpus actually lives in. REVIEW_M4_5 M45-N42 / condition 51: the class `(1,1)` appeared in 450 random comparisons ONCE, by chance |
//!
//! The fifth fixture is the diagonal pinch and not a triple junction. This
//! table said "triple junction" until delta-1 while `fixtures.rs` had already
//! substituted it and explained why — the silent substitution limitation 29
//! says must not be made, made in the axis table itself (REVIEW_M5_A N8f,
//! REVIEW_M5_B N8).
//!
//! The third axis is condition 51, and its acceptance criterion is that the
//! class `(1,1)` is present BY CONSTRUCTION rather than by chance.
//! [`structural_fixtures`] returns it at every size, and
//! `the_structural_register_is_covered_by_construction` asserts the classes
//! rather than hoping for them.

use std::collections::BTreeSet;

use vice_ir::{ComplementaryConnectivity, PixelConnectivity};
use vice_topology::continuation::EditKind;
use vice_topology::dcel::{
    apply, audit, is_the_assembly_of_its_own_labelling, Edit, IncrementalRebuildError,
    InvariantViolation, Outcome, Roi, TransactionRefusal,
};
use vice_topology::{
    audit_every_labelling, signature, structural_fixtures, with_a_distant_witness, Dcel, Labelling,
    STRUCTURAL_SIZES_PX, TX_CONFIG_V1,
};

/// The sizes the corpus uses, plus the two the size list declares and no
/// fixture reached in M4.5. F-8 is that gap; here it is not a gap.
pub const SIZES_PX: [usize; 5] = [32, 64, 128, 256, 512];

/// The sizes cheap enough for the default test path. The two large ones run
/// under `--ignored` in release, which is where the corpus-wide measurements
/// already live.
///
/// **Not a declaration any more.** STATUS_M5 limitation 53: this list and the
/// `vice-bench` harness's structural cell sizes were two independent copies of
/// one axis. The declaration now lives beside the generator it describes, in
/// `vice_topology::STRUCTURAL_SIZES_PX`, and the harness is checked against it
/// from the `vice-bench` side (the dependency cannot run the other way).
pub const FAST_SIZES_PX: [usize; 3] = STRUCTURAL_SIZES_PX;

fn arms() -> [ComplementaryConnectivity; 2] {
    ComplementaryConnectivity::arms()
}

/// **Condition 51.** The structural register is covered BY CONSTRUCTION.
///
/// The acceptance criterion REVIEW_M4_5 §11 states is that the class `(1, 1)`
/// appears because the fixture set puts it there, not because a sampler
/// happened to land on it. This asserts the class of every fixture at every
/// fast size under both convention arms, so a generator that stopped producing
/// an annulus fails here rather than quietly narrowing the population.
#[test]
fn the_structural_register_is_covered_by_construction() {
    let mut classes: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut checked = 0usize;
    let mut convention_dependent = 0usize;
    for n in FAST_SIZES_PX {
        for f in structural_fixtures(n) {
            if f.class_fg4 != f.class_fg8 {
                convention_dependent += 1;
            }
            for conn in arms() {
                let four = conn.foreground() == PixelConnectivity::Four;
                let want = if four { f.class_fg4 } else { f.class_fg8 };
                let sig = signature(&f.labelling, conn);
                assert_eq!(
                    (sig.components, sig.holes),
                    want,
                    "{} at {n} px under fg-{} is ({}, {}), not the class it is built to be",
                    f.name,
                    if four { "4" } else { "8" },
                    sig.components,
                    sig.holes
                );
                classes.insert(want);
                checked += 1;
            }
        }
    }
    // DERIVED from the register rather than written as `5 * 2`: the register
    // grew to six in delta-3 and a hard-coded count is the next line somebody
    // appends (F-0048 Q1).
    let per_size = structural_fixtures(FAST_SIZES_PX[0]).len();
    assert_eq!(checked, FAST_SIZES_PX.len() * per_size * 2);
    assert!(
        classes.contains(&(1, 1)),
        "condition 51: the annulus class must be present by construction, saw {classes:?}"
    );
    assert!(classes.contains(&(2, 1)), "{classes:?}");
    assert!(classes.contains(&(2, 0)), "{classes:?}");
    // And the fixture set is not one class wearing five names.
    assert!(classes.len() >= 4, "{classes:?}");
    // One fixture must answer differently under the two arms, or the whole
    // register would be blind to the convention it is supposed to exercise.
    // Counted from the register, not asserted at a literal.
    let expect_cd = structural_fixtures(FAST_SIZES_PX[0])
        .iter()
        .filter(|f| f.class_fg4 != f.class_fg8)
        .count();
    assert!(expect_cd > 0, "no fixture is convention-dependent");
    assert_eq!(
        convention_dependent,
        FAST_SIZES_PX.len() * expect_cd,
        "every convention-dependent fixture must be so at every size"
    );
}

/// The audit is green on every structural fixture at every fast size, under
/// both arms — and the population is non-trivial, which is asserted rather
/// than assumed: an audit that ran on nothing would pass this test otherwise
/// (F-0039).
#[test]
fn the_audit_holds_on_the_structural_register_at_corpus_sizes() {
    let mut audited = 0usize;
    let mut junctions_seen = 0u32;
    for n in FAST_SIZES_PX {
        for f in structural_fixtures(n) {
            for conn in arms() {
                let d = Dcel::assemble(f.labelling.clone(), conn);
                let r = audit(&d).unwrap_or_else(|e| panic!("{} at {n}: {e}", f.name));
                assert!(
                    is_the_assembly_of_its_own_labelling(&d),
                    "{} at {n}: not its own assembly",
                    f.name
                );
                assert!(
                    r.boundaries > 0 && r.loops > 0,
                    "{} at {n}: empty arrangement",
                    f.name
                );
                // A junction is a lattice point of degree greater than two, and
                // it is counted as such. The previous version compared
                // `sum(degrees) > 2 * |V|`, which is `|B| > |V|` — a comparison
                // of two counts, not a junction test (REVIEW_M5_A N6). It gave
                // the right answer on this register and was not testing what
                // its name said.
                if vice_topology::dcel::junction_count(&d) > 0 {
                    junctions_seen += 1;
                }
                audited += 1;
            }
        }
    }
    let per_size = structural_fixtures(FAST_SIZES_PX[0]).len();
    assert_eq!(audited, FAST_SIZES_PX.len() * per_size * 2);
    // The junction-bearing fixtures are those built around a critical 2x2: the
    // pinch and, since delta-3, the staircase. COUNTED from the register rather
    // than written as a literal, which is the line the sixth fixture would
    // otherwise have broken (F-0048 Q1).
    let junction_fixtures = structural_fixtures(FAST_SIZES_PX[0])
        .iter()
        .filter(|f| f.name.starts_with("diagonal_"))
        .count();
    assert!(junction_fixtures > 0, "no fixture carries a junction");
    assert_eq!(
        junctions_seen as usize,
        FAST_SIZES_PX.len() * junction_fixtures * 2,
        "a register with no junction would exercise only simple closed curves"
    );
}

/// The transaction machinery on the structural register: closing the gap of
/// `two_components` is a real edit, it commits, and the chains outside the
/// region are byte-identical.
///
/// The knockout is in the same test and in both directions: the same edit with
/// one pixel added outside the ROI must be REFUSED, and the clean edit must be
/// ACCEPTED. A locality check that refused everything would satisfy half of
/// this and measure nothing.
#[test]
fn a_local_transaction_on_a_corpus_sized_fixture_leaves_the_rest_alone() {
    for n in FAST_SIZES_PX {
        let unit = (n as f64 / 16.0).max(1.0) as u32;
        let c = (n / 2) as u32;
        let fixtures = structural_fixtures(n);
        let two = &fixtures[3];
        assert_eq!(two.name, "two_components");
        let witnessed = with_a_distant_witness(&two.labelling, n);
        let base = Dcel::assemble(witnessed, arms()[0]);
        assert_eq!(
            base.foreground_faces(),
            3,
            "two blocks and the corner witness"
        );

        let roi = Roi {
            x0: c - 3 * unit,
            y0: c - unit,
            x1: c + 3 * unit,
            y1: c + unit,
        };
        let set: Vec<(u32, u32, bool)> = (roi.x0..roi.x1)
            .flat_map(|x| (roi.y0..roi.y1).map(move |y| (x, y, true)))
            .collect();
        let edit = Edit {
            kind: EditKind::BRIDGE_CLOSE,
            roi,
            set: set.clone(),
        };
        let out = apply(&base, &edit, &TX_CONFIG_V1);
        let rep = out.report();
        let new = out
            .committed()
            .unwrap_or_else(|| panic!("{n} px: the bridge must close, got {rep:?}"));
        assert_eq!(
            new.foreground_faces(),
            2,
            "the two blocks merge; the witness stays"
        );
        assert_eq!(rep.unrelated_chains_that_moved, 0);
        assert!(
            rep.unrelated_chains > 0,
            "{n} px: nothing lies outside the region, so the clause has no population"
        );

        // The other direction.
        let mut reaching = set;
        reaching.push((0, 0, true));
        let bad = Edit {
            kind: EditKind::BRIDGE_CLOSE,
            roi,
            set: reaching,
        };
        assert!(
            matches!(
                apply(&base, &bad, &TX_CONFIG_V1),
                Outcome::RolledBack { .. }
            ),
            "{n} px: an edit reaching outside its ROI must not commit"
        );
    }
}

/// The two convention arms disagree about a critical 2x2, and the place they
/// disagree is the FACE COUNT.
///
/// §5.3's whole content is that the answer at a saddle depends on the
/// convention. If both arms produced the same arrangement the convention would
/// be decoration, so this is asserted on a fixture built to contain nothing
/// but the ambiguity.
#[test]
fn the_two_conventions_disagree_about_a_critical_2x2() {
    // Two pixels touching at a corner, and nothing else.
    let l = Labelling::new(
        4,
        4,
        (0..16)
            .map(|i| {
                let (x, y) = (i % 4, i / 4);
                (x == 1 && y == 1) || (x == 2 && y == 2)
            })
            .collect(),
    );
    let four = Dcel::assemble(
        l.clone(),
        ComplementaryConnectivity::new(PixelConnectivity::Four),
    );
    let eight = Dcel::assemble(l, ComplementaryConnectivity::new(PixelConnectivity::Eight));
    assert!(audit(&four).is_ok());
    assert!(audit(&eight).is_ok());
    assert_eq!(four.foreground_faces(), 2, "4-connected ink is pinched");
    assert_eq!(
        eight.foreground_faces(),
        1,
        "8-connected ink passes through"
    );
    assert_ne!(four.faces().len(), eight.faces().len());
}

/// The exhaustive axis at 4x4: 65 536 labellings, 131 072 arrangements, every
/// one assembled and audited — including the two EMPTY ones, which C243 stopped
/// skipping. The comment read 131 070 one line above an assertion of
/// `2 * (1 << 16)` until delta-2 (REVIEW_M5_B N13b).
///
/// `#[ignore]` because it is minutes in debug, and it runs in release in CI
/// beside the other corpus-wide measurements. The 4x3 sweep is in the crate's
/// unit tests and runs by default, so the axis is never entirely absent from
/// the fast path.
#[test]
#[ignore = "exhaustive 2^16 sweep; runs in CI in release"]
fn the_audit_is_green_over_every_labelling_of_a_four_by_four() {
    let r = audit_every_labelling(4, 4).expect("exhaustive audit at 4x4");
    assert_eq!(r.arrangements_audited, 2 * (1u64 << 16));
    assert!(r.distinct_classes >= 8, "{:?}", r.classes);
    assert!(r.classes.contains(&(1, 1)));
    assert!(r.labellings_with_a_critical_cell > 1000);
    println!(
        "4x4 exhaustive: {} arrangements, {} classes {:?}, {} labellings with a critical 2x2",
        r.arrangements_audited, r.distinct_classes, r.classes, r.labellings_with_a_critical_cell
    );
}

/// The size axis at its declared ceiling. F-8 was a judge proved to 128 px
/// deciding questions at 512; the DCEL's proof domain reaches the top of
/// `SIZES_PX` instead, and the cost of that is the reason this one is
/// `#[ignore]`d rather than the reason it does not exist.
#[test]
#[ignore = "256 and 512 px; runs in CI in release"]
fn the_audit_holds_on_the_structural_register_at_every_declared_size() {
    for n in SIZES_PX {
        for f in structural_fixtures(n) {
            for conn in arms() {
                let four = conn.foreground() == PixelConnectivity::Four;
                let want = if four { f.class_fg4 } else { f.class_fg8 };
                let sig = signature(&f.labelling, conn);
                assert_eq!((sig.components, sig.holes), want, "{} at {n}", f.name);
                let d = Dcel::assemble(f.labelling.clone(), conn);
                let r = audit(&d).unwrap_or_else(|e| panic!("{} at {n}: {e}", f.name));
                println!(
                    "{} {n}px fg-{}: V={} B={} L={} F={} ({}, {})",
                    f.name,
                    if four { 4 } else { 8 },
                    r.vertices,
                    r.boundaries,
                    r.loops,
                    r.faces,
                    r.foreground_faces,
                    r.holes
                );
            }
        }
    }
}

/// **RT5-A13's second half.** The structural register must contain loops long
/// enough to HAVE an order, at every size and under both arms.
///
/// The §12 ORIENTED check is only exercised by loops of three or more
/// half-edges. Neither M5 population had them — the corpus averages 1.082 per
/// loop, and this register had zero — so a check written without a fixture
/// would have been green for the reason the absent check was.
#[test]
fn the_register_carries_loops_long_enough_to_have_an_order() {
    let mut sizes_with_long_loops = 0usize;
    for n in FAST_SIZES_PX {
        let mut longest_here = 0usize;
        for f in structural_fixtures(n) {
            for conn in arms() {
                let d = Dcel::assemble(f.labelling.clone(), conn);
                let (longest, _total, at_least_three) =
                    vice_topology::dcel::loop_length_profile(&d);
                longest_here = longest_here.max(longest);
                if f.name == "diagonal_staircase" {
                    assert!(
                        at_least_three > 0,
                        "{} at {n} px under fg-{}: no loop of three or more half-edges, so the                          §12 ORIENTED check is not exercised here at all",
                        f.name,
                        if conn.foreground() == PixelConnectivity::Four {
                            4
                        } else {
                            8
                        }
                    );
                }
                assert!(vice_topology::dcel::loops_agree_with_the_labelling(&d).is_ok());
            }
        }
        assert!(longest_here >= 3, "{n} px: longest loop is {longest_here}");
        sizes_with_long_loops += 1;
    }
    assert_eq!(sizes_with_long_loops, FAST_SIZES_PX.len());
}

/// The connectivity arm the moved transaction tests use.
fn arm() -> ComplementaryConnectivity {
    ComplementaryConnectivity::arms()[0]
}

/// The neck ROI of the dumbbell fixture, used only to build one of each
/// refusal variant.
fn neck_roi() -> Roi {
    Roi {
        x0: 7,
        y0: 3,
        x1: 14,
        y1: 6,
    }
}

// ---------------------------------------------------------------------------
// M6: compound transactions (spec v1.3 §28 M5 "local COMPOUND topology
// transactions"; STATUS_M5 limitations 37 and 44, "no second deferral").
//
// These live here rather than in `src/dcel/transaction.rs` because §4.1 caps a
// module at 800 lines and that file reached 814. They use only the public API,
// so nothing is lost by the move.
// ---------------------------------------------------------------------------

/// **`ALL_NAMES` lists every refusal the type can express, both ways.**
///
/// `ALL_NAMES` is a literal, and a report's "never observed" list is its
/// complement — so a variant missing from it would shrink that list
/// silently and a reader would see fewer unexercised refusals than exist.
/// One of each variant is constructed here, so the check is against the
/// TYPE and not against a second copy of the same list.
#[test]
fn every_refusal_variant_is_in_all_names() {
    let roi = neck_roi();
    let one_of_each = [
        TransactionRefusal::EditLeftTheRoi { x: 0, y: 0, roi },
        TransactionRefusal::EditLeftTheCanvas {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
        },
        TransactionRefusal::EditIsANoOp,
        TransactionRefusal::NotTheDeclaredEdit {
            declared: "a".into(),
            performed: "b".into(),
            c0: 0,
            h0: 0,
            c1: 0,
            h1: 0,
        },
        TransactionRefusal::UnrelatedGraphMutation {
            count: 1,
            first: String::new(),
        },
        TransactionRefusal::IncrementalRebuildFailed(IncrementalRebuildError::NoChangedPixels),
        TransactionRefusal::CandidateFailedAudit(InvariantViolation::SuccessorNotAPermutation {
            detail: String::new(),
        }),
    ];
    assert_eq!(
        one_of_each.len(),
        TransactionRefusal::ALL_NAMES.len(),
        "a variant was added without a line in ALL_NAMES, or the reverse"
    );
    let mut seen = std::collections::BTreeSet::new();
    for r in &one_of_each {
        assert!(
            TransactionRefusal::ALL_NAMES.contains(&r.name()),
            "{} is not in ALL_NAMES",
            r.name()
        );
        assert!(seen.insert(r.name()), "duplicate name {}", r.name());
    }
    for n in TransactionRefusal::ALL_NAMES {
        assert!(seen.contains(n), "{n} is in ALL_NAMES and has no variant");
    }
}

/// **A COMPOUND transaction commits, and is certified as compound.**
///
/// This is §28 M5's undelivered bullet, executed. The edit closes the neck
/// AND fills a hole in the same step, so the signature moves by
/// `(-1, -1)` — two unit steps at once, which is precisely the shape the
/// old four-variant `EditKind` could not express and which the harness
/// therefore dropped for 310 of 480 arms.
#[test]
fn a_compound_edit_commits_and_its_certificate_says_compound() {
    // A dumbbell whose LEFT blob carries a hole. Bridging the neck and
    // filling the hole together is one transaction with a two-step delta.
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
    inside[4 * w + 3] = false; // the hole
    let base = Dcel::assemble(Labelling::new(w, h, inside), arm());
    assert_eq!(base.foreground_faces(), 2, "two blobs");
    assert_eq!(base.holes(), 1, "one hole in the left blob");

    let roi = Roi {
        x0: 3,
        y0: 3,
        x1: 14,
        y1: 6,
    };
    let mut set: Vec<(u32, u32, bool)> = (7..14u32).map(|x| (x, 4u32, true)).collect();
    set.push((3, 4, true)); // fill the hole in the same transaction
    let edit = Edit {
        kind: EditKind::new(-1, -1),
        roi,
        set,
    };
    assert!(!edit.kind.is_unit_step(), "the fixture must be compound");
    assert_eq!(edit.kind.steps(), 2);

    let out = apply(&base, &edit, &TX_CONFIG_V1);
    let new = out
        .committed()
        .expect("a correctly declared compound edit must commit");
    assert_eq!(new.foreground_faces(), 1);
    assert_eq!(new.holes(), 0);
    let r = out.report();
    assert_eq!(r.declared, "compound(c-1,h-1)");
    assert_eq!(r.declared_steps, 2);
    let cert = r.certificate.as_ref().expect("a certificate");
    assert_eq!(cert.edit_steps, 2);
    assert_eq!(cert.edit, "compound(c-1,h-1)");

    // BOTH DIRECTIONS: declaring only half of what the edit does is
    // refused, and the refusal names what was actually performed. Without
    // this leg, "compound edits commit" would be satisfied by a check that
    // waved every declaration through.
    let half = Edit {
        kind: EditKind::BRIDGE_CLOSE,
        roi,
        set: edit.set.clone(),
    };
    match apply(&base, &half, &TX_CONFIG_V1) {
        Outcome::RolledBack { reason, .. } => {
            assert_eq!(reason.name(), "NotTheDeclaredEdit");
            assert!(
                reason.to_string().contains("compound(c-1,h-1)"),
                "the refusal must name the edit performed: {reason}"
            );
        }
        Outcome::Committed { .. } => {
            panic!("declaring one step for a two-step edit must not commit")
        }
    }
}
