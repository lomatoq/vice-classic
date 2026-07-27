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
    for (name, ok, why) in rep.gate_table() {
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
    assert_eq!(rep.gate_table().len(), 3, "spec 28 M4.5 has three clauses");
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

/// The gate rows are not vacuous: each one FAILS on a report that has been
/// broken in the specific way the row is about.
#[test]
fn each_gate_row_can_fail() {
    let mut rep = report::build(&run_once());
    // Give the report a population big enough that the size controls are
    // not the thing being tested.
    while rep.recall_all.arms < 40 {
        let mut a = rep.arms[0].clone();
        a.scene_id = format!("{}#{}", a.scene_id, rep.arms.len());
        a.group_id = format!("g{}", rep.arms.len() % 9);
        a.identifiability = "identifiable";
        a.outcome = "supported/box".into();
        a.gt_four = GtSignature {
            components: 1,
            holes: 1,
        };
        a.gt_eight = a.gt_four;
        a.gt_in_envelope = true;
        a.gt_in_envelope_events_only = true;
        a.gt_in_envelope_fixed_only = rep.arms.len().is_multiple_of(3);
        rep.arms.push(a);
        let pop: Vec<&TopologyArm> = rep.arms.iter().collect();
        rep.identifiable_supported_arms = pop.len() as u64;
        rep.recall_all = report::Recall {
            arms: pop.len() as u64,
            hits: pop.len() as u64,
            fraction: 1.0,
        };
    }
    // Rebuild the derived numbers the way `build` would.
    let rebuilt = |rep: &report::TopologyReport| -> report::TopologyReport {
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
    };
    let base = rebuilt(&rep);
    let rows = base.gate_table();
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
    assert!(!miss.gate_table()[0].1, "a lost answer must fail row 1");

    // Row 2 fails when a topology pair collapses.
    let mut collapsed = base.clone();
    for p in &mut collapsed.ambiguity {
        if p.is_topology_pair {
            p.both_retained_from_a = Some(false);
        }
    }
    assert!(
        !collapsed.gate_table()[1].1,
        "a collapsed alternative must fail row 2"
    );

    // Row 3 fails in BOTH directions: if removing the fixed probes loses an
    // answer, and if the fixed probes alone already recover everything.
    let mut lost = base.clone();
    lost.arms[in_pop].gt_in_envelope_events_only = false;
    let lost = rebuilt(&lost);
    assert!(
        !lost.gate_table()[2].1,
        "an answer that only a fixed probe finds must fail row 3"
    );
    let mut threshold_is_enough = base.clone();
    for p in &mut threshold_is_enough.ambiguity {
        p.both_retained_fixed_only_from_a = Some(true);
        p.both_retained_fixed_only_from_b = Some(true);
    }
    let threshold_is_enough = rebuilt(&threshold_is_enough);
    assert!(
        !threshold_is_enough.gate_table()[2].1,
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
