//! Shape, determinism and non-vacuity of a recall run.
//!
//! The corpus-wide numbers of §28 M4.5 are produced by the `gt-corpus
//! topology` command and checked against the committed artifact; what lives
//! here is what the default `cargo test` path can afford: that a run
//! produces arms, that it is deterministic, that the ground truth it scores
//! against is a property of the SCENE, and that each gate row's controls can
//! actually fail.

use super::report;
use super::*;

fn run_once() -> TopologyRun {
    run(TopologyScope::Test).expect("the test-scope topology run must succeed")
}

/// The gate-row thresholds as the COMMITTED gate file has them.
///
/// The tests evaluate the rows against the same source production does, so a
/// test cannot be green against a threshold nobody registered (RT45-A10).
fn gate_cfg() -> crate::topology::gate::TopologyGateConfig {
    let g = crate::gates::GatesFile::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../configs/GATES_V1.toml"),
    )
    .expect("the committed gate file must load");
    crate::topology::gate::TopologyGateConfig::from_gates(&g).expect("the topology gate section")
}

/// A run produces arms and a report, and the three gate rows are computed
/// from data that exists.
#[test]
fn a_run_produces_arms_and_the_three_gate_rows() {
    let r = run_once();
    let rep = report::build(&r);
    println!(
        "scenes {}, arms {}, refused {}, audit groups skipped {}, opaque excluded {}",
        r.scenes,
        r.arms.len(),
        r.refused.len(),
        r.sealed_audit_groups_skipped,
        r.opaque_exterior_arms
    );
    for (name, ok, why) in rep.gate_table(&gate_cfg()) {
        println!("[{}] {name}: {why}", if ok { "MET" } else { "NOT MET" });
    }
    println!("recall {:?}", rep.recall_all);
    println!("events only {:?}", rep.recall_events_only);
    println!("fixed only {:?}", rep.recall_fixed_only);
    println!("fields {:?}", rep.field_contributions);
    assert!(r.scenes > 0);
    assert!(!r.arms.is_empty());
    assert!(
        r.sealed_audit_groups_skipped > 0,
        "the audit must be skipped"
    );
    assert_eq!(
        rep.gate_table(&gate_cfg()).len(),
        3,
        "spec 28 M4.5 has three clauses"
    );
}

/// The sealed audit is filtered in BOTH loops, not only in the one where
/// the rule was first written.
///
/// The recall loop skips audit groups; the ambiguity loop reads
/// `adversarial::ambiguity_pairs()` directly, so it needs its own filter.
/// F-0026 is exactly a project that applied a split filter to four
/// measurements and not to the fifth, and the fifth was the one that
/// mattered. The count is published rather than left as a property of
/// today's split assignment: if the corpus ever moves an ambiguity pair into
/// the audit, the number appears in the artifact instead of the audit being
/// scored in silence.
#[test]
fn the_sealed_audit_is_filtered_in_both_loops() {
    let r = run_once();
    assert!(
        r.sealed_audit_groups_skipped > 0,
        "the recall loop must be skipping audit groups, or this test compares two zeroes"
    );
    assert_eq!(
        r.ambiguity_pairs_in_sealed_audit_skipped, 0,
        "no ambiguity pair is in the audit today; the number exists so that a corpus change \
         becomes visible rather than silent"
    );
    // And the filter is REACHED: every pair the loop saw was asked for its
    // split, which is what makes the zero above a measurement.
    for pair in crate::gt::adversarial::ambiguity_pairs() {
        let split = SPLIT_POLICY_V1.split_of_group(&pair.group);
        println!("{} -> {}", pair.group.id, split.as_str());
        assert_ne!(
            split,
            Split::SealedAudit,
            "{} is in the sealed audit and the harness would have to skip it",
            pair.group.id
        );
    }
    assert_eq!(
        r.ambiguity.len() + r.ambiguity_pairs_in_sealed_audit_skipped as usize,
        crate::gt::adversarial::ambiguity_pairs().len(),
        "every corpus pair is either measured or skipped by a NAMED filter"
    );
}

/// The run is deterministic, which is what makes the committed artifact a
/// comparison rather than a judgement call.
#[test]
fn the_topology_report_is_deterministic() {
    let a = report::build(&run_once());
    let b = report::build(&run_once());
    assert_eq!(a.canonical_json(), b.canonical_json());
}

/// The ground truth is a property of the SCENE and the view, and nothing
/// the envelope does can move it.
///
/// The control that makes this mean something: a scene WITH a hole and a
/// scene without one must give different truths at a resolution where the
/// hole is resolved, or the truth function is not measuring topology.
#[test]
fn the_ground_truth_is_a_property_of_the_scene() {
    let pairs = crate::gt::adversarial::ambiguity_pairs();
    let hole = pairs
        .iter()
        .find(|p| p.group.shape_family.contains("hole"))
        .expect("the corpus has a hole/no-hole pair");
    let t = view_for(&hole.separate_cell);
    let [four, _] = ComplementaryConnectivity::arms();
    let a = gt_signature(&hole.group.scenes[0], &t, four).unwrap();
    let b = gt_signature(&hole.group.scenes[1], &t, four).unwrap();
    println!("holed {a:?} solid {b:?}");
    assert_ne!(
        a, b,
        "at the separating cell the two scenes must differ, or the ambiguity pair is not one"
    );
    assert!(
        a.holes == 1 || b.holes == 1,
        "one of the two is the holed scene: {a:?} {b:?}"
    );
    // And it does not depend on anything the envelope computed: taking it
    // twice gives the same answer.
    assert_eq!(a, gt_signature(&hole.group.scenes[0], &t, four).unwrap());
}

/// Ink is the union of OPAQUE faces, not the complement of the exterior.
///
/// F-0025 verbatim: a hole face is neither the exterior nor ink, and the
/// first version of a truth field built as "not the exterior" reported a
/// full unit of error on every scene with a hole. Here the consequence would
/// be worse than an error number — the GT topology of a ring would come out
/// as a disk.
#[test]
fn a_hole_is_not_ink() {
    let pairs = crate::gt::adversarial::ambiguity_pairs();
    let hole = pairs
        .iter()
        .find(|p| p.group.shape_family.contains("hole"))
        .expect("the corpus has a hole/no-hole pair");
    let t = view_for(&hole.separate_cell);
    let holed = &hole.group.scenes[0];
    let ink = exact_ink_coverage(holed, &t).unwrap();
    let total: f64 = ink.iter().sum();
    let n = ink.len() as f64;
    assert!(
        total < n * 0.99,
        "if the hole were counted as ink the coverage would fill the shape: {total} of {n}"
    );
    let [four, _] = ComplementaryConnectivity::arms();
    let sig = gt_signature(holed, &t, four).unwrap();
    assert_eq!(sig.holes, 1, "the ring must read as a ring: {sig:?}");
}

/// Rebuild the derived numbers the way `report::build` would.
fn rebuilt(rep: &report::TopologyReport) -> report::TopologyReport {
    let mut r = rep.clone();
    let pop: Vec<&TopologyArm> = r
        .arms
        .iter()
        .filter(|a| a.identifiability == "identifiable" && a.outcome.starts_with("supported"))
        .collect();
    let hits = pop.iter().filter(|a| a.gt_in_envelope).count() as u64;
    let ev = pop.iter().filter(|a| a.gt_in_envelope_events_only).count() as u64;
    let fx = pop.iter().filter(|a| a.gt_in_envelope_fixed_only).count() as u64;
    let n = pop.len() as u64;
    r.identifiable_supported_arms = n;
    r.recall_shape_families = pop
        .iter()
        .map(|a| a.shape_family.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len() as u64;
    let un = pop
        .iter()
        .filter(|a| a.gt_in_envelope_unrelated_field)
        .count() as u64;
    r.recall_unrelated_field = report::Recall {
        arms: n,
        hits: un,
        fraction: un as f64 / n as f64,
    };
    // The knockout's POSITIVE control, rebuilt the way `build` computes it:
    // over arms that are a plain disk under BOTH conventions, which is
    // where a centred disk is supposed to win.
    let plain = |g: GtSignature| g.components == 1 && g.holes == 0;
    let triv: Vec<&&TopologyArm> = pop
        .iter()
        .filter(|a| plain(a.gt_four) && plain(a.gt_eight))
        .collect();
    let tun = triv
        .iter()
        .filter(|a| a.gt_in_envelope_unrelated_field)
        .count() as u64;
    r.recall_unrelated_field_trivial = report::Recall {
        arms: triv.len() as u64,
        hits: tun,
        fraction: if triv.is_empty() {
            0.0
        } else {
            tun as f64 / triv.len() as f64
        },
    };
    r.arms_missing_a_connectivity_arm = pop
        .iter()
        .filter(|a| a.candidates_by_arm.0 == 0 || a.candidates_by_arm.1 == 0)
        .count() as u64;
    r.recall_source_groups = pop
        .iter()
        .map(|a| a.group_id.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len() as u64;
    r.recall_all = report::Recall {
        arms: n,
        hits,
        fraction: hits as f64 / n as f64,
    };
    r.recall_events_only = report::Recall {
        arms: n,
        hits: ev,
        fraction: ev as f64 / n as f64,
    };
    r.recall_fixed_only = report::Recall {
        arms: n,
        hits: fx,
        fraction: fx as f64 / n as f64,
    };
    let nt: Vec<&&TopologyArm> = pop
        .iter()
        .filter(|a| !(a.gt_four.components == 1 && a.gt_four.holes == 0))
        .collect();
    r.non_trivial_gt_arms = nt.len() as u64;
    let nth = nt.iter().filter(|a| a.gt_in_envelope).count() as u64;
    r.recall_non_trivial = report::Recall {
        arms: nt.len() as u64,
        hits: nth,
        fraction: if nt.is_empty() {
            0.0
        } else {
            nth as f64 / nt.len() as f64
        },
    };
    r
}

/// A doctored-but-CONSISTENT recall report with a population big enough that
/// the size controls are not what is being tested.
///
/// Shared by `each_gate_row_can_fail` and by the effective-boundary scan, so
/// the two tests cannot disagree about what a healthy report looks like.
fn synthetic_base() -> report::TopologyReport {
    let mut rep = report::build(&run_once());
    // Give the report a population big enough that the size controls are
    // not the thing being tested.
    while rep.recall_all.arms < 40 {
        let mut a = rep.arms[0].clone();
        a.scene_id = format!("{}#{}", a.scene_id, rep.arms.len());
        a.group_id = format!("g{}", rep.arms.len() % 9);
        a.shape_family = format!("family/{}", rep.arms.len() % 7);
        a.candidates_by_arm = (2, 2);
        a.identifiability = "identifiable";
        a.outcome = "supported/box".into();
        // Every fourth arm has TRIVIAL ground truth. Without them the
        // knockout's positive control would be measured over an empty
        // population, and a control that is empty is the defect RT45-A12
        // found rather than a test of it.
        a.gt_four = if rep.arms.len().is_multiple_of(4) {
            GtSignature {
                components: 1,
                holes: 0,
            }
        } else {
            GtSignature {
                components: 1,
                holes: 1,
            }
        };
        a.gt_eight = a.gt_four;
        a.gt_in_envelope = true;
        a.gt_in_envelope_events_only = true;
        a.gt_in_envelope_fixed_only = rep.arms.len().is_multiple_of(3);
        a.gt_in_envelope_unrelated_field = rep.arms.len().is_multiple_of(2);
        rep.arms.push(a);
        let pop: Vec<&TopologyArm> = rep.arms.iter().collect();
        rep.identifiable_supported_arms = pop.len() as u64;
        rep.recall_all = report::Recall {
            arms: pop.len() as u64,
            hits: pop.len() as u64,
            fraction: 1.0,
        };
    }
    rebuilt(&rep)
}

/// The gate rows are not vacuous: each one FAILS on a report that has been
/// broken in the specific way the row is about.
#[test]
fn each_gate_row_can_fail() {
    let base = synthetic_base();
    let rows = base.gate_table(&gate_cfg());
    assert!(
        rows[0].1,
        "the doctored-but-consistent report must pass row 1: {}",
        rows[0].2
    );
    assert!(rows[2].1, "and row 3: {}", rows[2].2);

    // The index of an arm that is actually IN the recall population: a
    // knock-out applied outside it would prove nothing, which is exactly the
    // subclass mistake meta-rule M-2 is about.
    let in_pop = base
        .arms
        .iter()
        .position(|a| a.identifiability == "identifiable" && a.outcome.starts_with("supported"))
        .expect("the doctored report has a recall population");

    // Row 1 fails when one arm loses its answer.
    let mut miss = base.clone();
    miss.arms[in_pop].gt_in_envelope = false;
    let miss = rebuilt(&miss);
    assert!(
        !miss.gate_table(&gate_cfg())[0].1,
        "a lost answer must fail row 1"
    );

    // Row 1 fails when the knockout stops losing: an envelope built from a
    // field unrelated to the scene scoring as well as the real one means the
    // clause is measuring nothing (condition 4).
    let mut knockout_ties = base.clone();
    for a in &mut knockout_ties.arms {
        a.gt_in_envelope_unrelated_field = a.gt_in_envelope;
    }
    let knockout_ties = rebuilt(&knockout_ties);
    assert!(
        !knockout_ties.gate_table(&gate_cfg())[0].1,
        "if an unrelated field scored the same as the real one, row 1 would not be a measurement          and must say so"
    );

    // Row 1 fails when the knockout stops being a knockout — THE OTHER
    // DIRECTION, and the one nobody checked (RT45-A12).
    //
    // `recall_unrelated_field.hits < recall_all.hits` is satisfied by a
    // knockout that has been switched off. Shrinking the disk radius from 0.3
    // to 0.0001 empties the unrelated field: it matches nothing, the conjunct
    // above reads 0 < 40, and the clause stayed MET with its only control
    // measuring nothing. A control knocked out only where it FAILS is half a
    // control, and half a control is what an empty one looks like from the
    // outside.
    //
    // The emulation is exact — an empty field matches no arm — and it is
    // deliberately applied to EVERY arm, so it also cannot be dismissed as a
    // sub-population artefact (meta-rule M-2).
    let mut knockout_empty = base.clone();
    for a in &mut knockout_empty.arms {
        a.gt_in_envelope_unrelated_field = false;
    }
    let knockout_empty = rebuilt(&knockout_empty);
    assert!(
        knockout_empty.recall_unrelated_field.hits < knockout_empty.recall_all.hits,
        "the OLD conjunct is still satisfied by the emptied knockout - which is the point: it \
         cannot be the thing that catches this"
    );
    assert!(
        !knockout_empty.gate_table(&gate_cfg())[0].1,
        "an emptied knockout left row 1 green: the clause would be citing a control that measures \
         nothing (RT45-A12)"
    );

    // Row 1 fails when an arm loses a connectivity arm: the clause relaxes its
    // success condition by pointing at both, so both have to be there.
    let mut one_arm = base.clone();
    one_arm.arms[in_pop].candidates_by_arm = (3, 0);
    let one_arm = rebuilt(&one_arm);
    assert!(
        !one_arm.gate_table(&gate_cfg())[0].1,
        "an envelope carrying only one complementary arm must fail row 1 (RT45-A1)"
    );

    // Row 2 fails when a topology pair collapses.
    let mut collapsed = base.clone();
    for p in &mut collapsed.ambiguity {
        if p.is_topology_pair {
            p.both_retained_from_a = Some(false);
        }
    }
    assert!(
        !collapsed.gate_table(&gate_cfg())[1].1,
        "a collapsed alternative must fail row 2"
    );

    // Row 3 fails in BOTH directions: if removing the fixed probes loses an
    // answer, and if the fixed probes alone already recover everything.
    let mut lost = base.clone();
    lost.arms[in_pop].gt_in_envelope_events_only = false;
    let lost = rebuilt(&lost);
    assert!(
        !lost.gate_table(&gate_cfg())[2].1,
        "an answer that only a fixed probe finds must fail row 3"
    );
    let mut threshold_is_enough = base.clone();
    for p in &mut threshold_is_enough.ambiguity {
        p.both_retained_fixed_only_from_a = Some(true);
        p.both_retained_fixed_only_from_b = Some(true);
    }
    let threshold_is_enough = rebuilt(&threshold_is_enough);
    assert!(
        !threshold_is_enough.gate_table(&gate_cfg())[2].1,
        "if the fixed probe ALONE also retained both readings of an ambiguous pair, then nothing \
         in the measurement distinguishes this architecture from one threshold, and the row must \
         say so rather than pass on the strength of its first half"
    );
}

/// The structural projection carries no float, and carries the integers that
/// would move if the topology moved.
#[test]
fn the_projection_drops_the_floats_and_keeps_the_topology() {
    let rep = report::build(&run_once());
    let v: serde_json::Value = serde_json::from_str(&rep.canonical_json()).unwrap();
    let p = report::structural_projection(&v);
    let text = serde_json::to_string(&p).unwrap();
    assert!(
        !text.contains(&rep.fixture_set_hash),
        "the fixture-set hash is a function of scene digests, hence of libm (F-0022)"
    );
    assert!(text.contains(&rep.config_hash), "the config hash survives");
    assert!(
        p["arms"][0]["gt_four"].is_object(),
        "the GT signature is an integer pair and must survive: a drift big enough to move it is \
         exactly what the structural mode has to see"
    );
    assert!(
        p["recall_all"]["fraction"].is_null(),
        "the fraction is a float"
    );
}

/// Every gate row flips at EXACTLY the value the frozen gate file registers.
///
/// This is the acceptance criterion for RT45-A10, and it is deliberately not
/// the one that was used before. The previous check read the source of
/// `gate_table` and required each comparison to name a registered constant —
/// so it registered the SPELLING. `MIN_RECALL_ARMS / 20` keeps the registered
/// name in the line and compares against 1; so does `- 16`, so does
/// `.saturating_sub(19)`. The reviewer's acceptance criterion ("the comparison
/// must return a bare literal") was the very form the guard models, so the
/// instrument was tested with the instrument's own model of the defect.
///
/// What is closed here is the EFFECTIVE comparison value. For each threshold
/// the scan walks the measurement upward until the row changes, and asserts the
/// smallest passing value equals the number in `configs/GATES_V1.toml`. It
/// never looks at the source. Arithmetic on the threshold, arithmetic on the
/// measurement, a second hidden constant, an off-by-one in `met_by` — all of
/// them move the boundary, and all of them fail here.
///
/// The `Threshold` newtype is still worth having: it turns the most likely of
/// those into a compile error instead of a test failure. It is the cheap half.
/// This is the half that actually decides.
#[test]
fn each_gate_row_flips_at_exactly_the_registered_threshold() {
    let cfg = gate_cfg();
    let base = synthetic_base();
    assert!(
        base.gate_table(&cfg)[0].1 && base.gate_table(&cfg)[2].1,
        "the scan starts from a report that PASSES, or every boundary it finds would be an \
         artefact of some other conjunct"
    );

    /// The MEASURED value at which row `row` first becomes true.
    ///
    /// `set(k)` dials one quantity of the report; `measure` reads back what the
    /// row actually compares. The two are deliberately different: dialling
    /// `arms.len()` is not dialling `recall_all.arms`, because the base report
    /// carries real arms that are outside the recall population, and a scan
    /// that confused the two reported a boundary of 22 for a threshold of 20 —
    /// looking exactly like a defect in the gate rather than in the scan.
    ///
    /// The row must be false below the flip and true from there on, or the
    /// comparison is not a threshold and "the registered value" has no meaning.
    fn boundary(
        base: &report::TopologyReport,
        row: usize,
        cfg: &crate::topology::gate::TopologyGateConfig,
        limit: u64,
        set: impl Fn(&mut report::TopologyReport, u64),
        measure: impl Fn(&report::TopologyReport) -> u64,
    ) -> u64 {
        let mut first_true = None;
        for k in 0..=limit {
            let mut r = base.clone();
            set(&mut r, k);
            let r = rebuilt(&r);
            let ok = r.gate_table(cfg)[row].1;
            match (ok, first_true) {
                (true, None) => first_true = Some(measure(&r)),
                (false, Some(f)) => panic!(
                    "row {row} is true at a measurement of {f} and false again at k={k}: the \
                     comparison is not a threshold at all, so 'the registered value' has no \
                     meaning"
                ),
                _ => {}
            }
        }
        first_true.unwrap_or_else(|| {
            panic!("row {row} never became true up to {limit}; the scan measured nothing")
        })
    }
    let topology_pairs = |r: &report::TopologyReport| {
        r.ambiguity.iter().filter(|p| p.is_topology_pair).count() as u64
    };

    // 1. min_recall_arms — shrink the population itself.
    let want = cfg.min_recall_arms.registered_value();
    let got = boundary(
        &base,
        0,
        &cfg,
        base.arms.len() as u64,
        |r, k| r.arms.truncate(k as usize),
        |r| r.recall_all.arms,
    );
    assert_eq!(
        got, want,
        "clause 1 turns green at {got} arms while the gate file registers {want}"
    );

    // 2. min_recall_shape_families — collapse the families without touching
    //    the population size, so the two thresholds cannot stand in for each
    //    other.
    let want = cfg.min_recall_shape_families.registered_value();
    let got = boundary(
        &base,
        0,
        &cfg,
        12,
        |r, k| {
            for (i, a) in r.arms.iter_mut().enumerate() {
                a.shape_family = format!("family/{}", if k == 0 { 0 } else { i as u64 % k });
            }
        },
        |r| r.recall_shape_families,
    );
    assert_eq!(
        got, want,
        "clause 1 turns green at {got} shape families while the gate file registers {want}"
    );

    // 3. min_non_trivial_gt_arms — make all but `k` arms a plain disk. The
    //    ones turned trivial keep matching the knockout, so the positive
    //    control stays alive and this scan measures its own threshold.
    let want = cfg.min_non_trivial_gt_arms.registered_value();
    let got = boundary(
        &base,
        0,
        &cfg,
        20,
        |r, k| {
            for (i, a) in r.arms.iter_mut().enumerate() {
                if (i as u64) >= k {
                    a.gt_four = GtSignature {
                        components: 1,
                        holes: 0,
                    };
                    a.gt_eight = a.gt_four;
                    a.gt_in_envelope_unrelated_field = true;
                }
            }
        },
        |r| r.non_trivial_gt_arms,
    );
    assert_eq!(
        got, want,
        "clause 1 turns green at {got} non-trivial arms while the gate file registers {want}"
    );

    // 4. min_topology_pairs — duplicate the retaining pair `k` times.
    let retaining = base
        .ambiguity
        .iter()
        .find(|p| {
            p.is_topology_pair
                && p.both_retained_from_a == Some(true)
                && p.both_retained_from_b == Some(true)
        })
        .cloned()
        .expect("the corpus has one pair that retains both readings");
    let want = cfg.min_topology_pairs.registered_value();
    let got = boundary(
        &base,
        1,
        &cfg,
        8,
        |r, k| {
            r.ambiguity.retain(|p| !p.is_topology_pair);
            for i in 0..k {
                let mut p = retaining.clone();
                p.group_id = format!("{}#{i}", p.group_id);
                r.ambiguity.push(p);
            }
        },
        topology_pairs,
    );
    assert_eq!(
        got, want,
        "clause 2 turns green at {got} topology pairs while the gate file registers {want}"
    );

    // 5. min_classes_per_retaining_pair — vary how many readings each
    //    retaining pair carries, holding the pair count at the threshold.
    let pairs = cfg.min_topology_pairs.registered_value();
    let want = cfg.min_classes_per_retaining_pair.registered_value();
    let got = boundary(
        &base,
        1,
        &cfg,
        8,
        |r, k| {
            r.ambiguity.retain(|p| !p.is_topology_pair);
            for i in 0..pairs {
                let mut p = retaining.clone();
                p.group_id = format!("{}#{i}", p.group_id);
                let classes: Vec<(u32, u32)> = (0..k as u32).map(|c| (1, c)).collect();
                p.classes_from_a = classes.clone();
                p.classes_from_b = classes;
                r.ambiguity.push(p);
            }
        },
        |r| {
            r.ambiguity
                .iter()
                .filter(|p| p.is_topology_pair)
                .map(|p| p.classes_from_a.len().min(p.classes_from_b.len()) as u64)
                .min()
                .unwrap_or(0)
        },
    );
    assert_eq!(
        got, want,
        "clause 2 turns green at {got} retained readings per pair while the gate file registers \
         {want}"
    );
}
