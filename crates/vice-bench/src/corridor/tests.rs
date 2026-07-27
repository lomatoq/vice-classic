//! Shape and determinism of a corridor run.
//!
//! The MEASUREMENTS that set the frozen M4 coefficients used to live here.
//! They no longer can, and that is condition D1 of the REVIEW_M4 addendum
//! (M4-N11) rather than tidying: the guard on their population was a walk
//! over THIS file's source looking for two literals, and the reviewer walked
//! around it with `use crate::gt::corpus::all_groups as every_group;` — 286
//! renders instead of 104, the sealed audit back inside the frozen kernel
//! table, and every test green.
//!
//! A text scan is a habit; a seal is a type. `all_groups` and
//! `procedural_groups` are `pub(crate)` now, and the measurements moved to
//! `crates/vice-bench/tests/frozen_calibration.rs`, which is a SEPARATE
//! CRATE and therefore cannot name them at all — an alias fails to compile
//! (E0603), which is the same answer M4 gave `ObservationSource` and
//! `KeyedMeasurement`. The scan stays as second echelon and covers every
//! integration test rather than one file.
//!
//! What remains here is what needs the crate's internals and freezes
//! nothing: that a run produces arms and a report, and that it is
//! deterministic — which is what makes the committed artifact a comparison
//! rather than a judgement call.

use super::*;
use std::sync::OnceLock;

fn run_once() -> &'static CorridorRun {
    static R: OnceLock<CorridorRun> = OnceLock::new();
    R.get_or_init(|| run(CorridorScope::Test).expect("the test-scope corridor run must succeed"))
}

/// A run produces arms, and the three gate rows are computed from data that
/// exists.
#[test]
fn a_run_produces_arms_and_a_report() {
    let r = run_once();
    let rep = report::build(r);
    println!(
        "scenes {}, arms {}, refused {}, samples {}, audit groups skipped {}",
        r.scenes,
        r.arms.len(),
        r.refused.len(),
        r.samples.len(),
        r.sealed_audit_groups_skipped
    );
    for (name, ok, why) in rep.gate_table() {
        println!("[{}] {name}: {why}", if ok { "MET" } else { "NOT MET" });
    }
    println!("overall {:?}", rep.overall);
    println!("held-out {:?}", rep.held_out);
    println!("formation {:?}", rep.formation_recovery);
    println!("semi {:?}", rep.semi_transparent);
    println!("steps {:?}", rep.step_invariance);
    assert!(r.scenes > 0);
    assert!(!r.arms.is_empty());
    assert!(
        r.sealed_audit_groups_skipped > 0,
        "the audit must be skipped"
    );
}

/// The run is deterministic, which is what makes the committed artifact a
/// comparison rather than a judgement call.
#[test]
fn the_corridor_report_is_deterministic() {
    let a = report::build(run_once());
    let b = report::build(&run(CorridorScope::Test).unwrap());
    assert_eq!(a.canonical_json(), b.canonical_json());
}
