//! The §28 M5 harness, and the knockouts that make its clauses falsifiable.
//!
//! F-7 and the red team's tenth obligation are the same thing: an attack that
//! lives in a deletable clone protects nothing tomorrow, and a check on the
//! PRESENCE of a mechanism sees a phrase rather than a behaviour. These tests
//! are the M5 mechanisms' attacks, in the tree, each with a positive control:
//! apply the knockout and the clause must go red; remove it and the clause must
//! go green. That is the form the red team asked for in addendum 7 §5 and
//! priced at three of nine deltas.
//!
//! `#[ignore]`d because the run walks the corpus and takes minutes; CI runs
//! them in release beside the other corpus-wide measurements.

use vice_bench::dcel::{self, ProxyKnockout, RoiKnockout, RunKnockouts, PRODUCTION};
use vice_bench::topology::TopologyScope;

fn run(k: RunKnockouts) -> dcel::report::DcelReport {
    run_at(TopologyScope::Test, k)
}

fn run_at(scope: TopologyScope, k: RunKnockouts) -> dcel::report::DcelReport {
    let r = dcel::run(scope, k).expect("dcel run");
    dcel::report::build(&r)
}

/// The production run, and what it has to contain for the four clauses to be
/// about anything. Asserted rather than printed, because a population that
/// quietly narrows is how a green clause stops meaning what it said.
#[test]
#[ignore = "walks the corpus; runs in CI in release"]
fn the_production_run_has_a_population_for_every_clause() {
    let r = run(PRODUCTION);
    println!(
        "arms {} ({} corpus, {} structural), groups {}, classes {:?}",
        r.arms_measured, r.corpus_arms, r.structural_arms, r.groups, r.classes
    );
    println!(
        "transactions {} attempted, {} committed; unrelated chains {}, moved {}",
        r.transactions_attempted,
        r.transactions_committed,
        r.unrelated_chains_total,
        r.unrelated_chains_that_moved
    );
    println!("resolving power {:?}", r.audit_resolving_power);
    assert!(r.arms_measured > 0);
    assert!(r.structural_arms > 0, "clause 1 has no population");
    assert!(
        r.convention_dependent_groups > 0,
        "no group disagrees between the arms, so clause 1 is about singletons"
    );
    assert!(r.transactions_committed > 0, "clause 3 has no population");
    assert!(
        r.unrelated_chains_total > 0,
        "no chain lies outside a transaction's region; clause 3 is about the empty set"
    );
    assert!(r.audit_resolving_power.arrangements_probed > 0);
    assert!(r.distinct_classes >= 3, "{:?}", r.classes);

    // And the production run is clean on every property the clauses assert.
    assert_eq!(r.classes_out, r.classes_in);
    assert_eq!(r.arms_disagreeing_with_the_independent_chain, 0);
    assert_eq!(r.arms_failing_the_euler_identity, 0);
    assert_eq!(r.unrelated_chains_that_moved, 0);
    assert_eq!(r.arms_failing_the_audit, 0);
    assert_eq!(r.arms_that_are_not_their_own_assembly, 0);
    assert_eq!(r.audit_resolving_power.uncaught_by_audit, 0);
    assert_eq!(r.audit_resolving_power.no_ops, 0);
}

/// **Knockout 1, both directions.** A stage that picks a winner reduces the
/// number of classes per group, and clause 1 must see it.
#[test]
#[ignore = "walks the corpus; runs in CI in release"]
fn a_stage_that_picks_a_winner_is_visible_to_clause_one() {
    let clean = run(PRODUCTION);
    assert_eq!(
        clean.classes_out, clean.classes_in,
        "positive control: production carries every topology through"
    );
    assert!(
        clean.groups > 0,
        "the clean run has no groups to carry through"
    );
    let knocked = run(RunKnockouts {
        proxy: ProxyKnockout::Select,
        ..PRODUCTION
    });
    assert!(
        knocked.classes_out < knocked.classes_in,
        "the knockout must LOSE classes: in {} out {}. If it does not, clause 1 is measuring \
         nothing, because a population of singletons cannot be reduced",
        knocked.classes_in,
        knocked.classes_out
    );
}

/// **Knockout 2, both directions.** An edit reaching outside its declared ROI
/// must be refused on every arm, and the clean edit must commit on some.
#[test]
#[ignore = "walks the corpus; runs in CI in release"]
fn an_edit_reaching_outside_its_roi_is_refused_on_every_arm() {
    let clean = run(PRODUCTION);
    assert!(
        clean.transactions_committed > 0,
        "positive control: the clean edit commits somewhere"
    );
    let knocked = run(RunKnockouts {
        roi: RoiKnockout::Reach,
        ..PRODUCTION
    });
    // M5A-N9: both assertions below are VACUOUSLY true when the knockout
    // produced no transactions at all, and `RoiKnockout::Reach` adds a pixel
    // BEFORE `kind` is derived, so it can move the population. A knockout must
    // assert its own population (F-0039, RT45-A12).
    assert!(
        knocked.transactions_attempted > 0,
        "the knockout attempted nothing, so the two assertions below are about the empty set"
    );
    assert_eq!(
        knocked.transactions_committed, 0,
        "every transaction whose edit leaves its ROI must roll back"
    );
    assert_eq!(
        knocked.transactions_rolled_back, knocked.transactions_attempted,
        "and all of them must be rolled back, not merely most"
    );
}

/// The FULL-scope population, printed. This is where the frozen thresholds of
/// the `[dcel]` gate section come from: they are read off a run rather than
/// chosen, and this test is the run a reviewer repeats to check that.
#[test]
#[ignore = "full corpus scope; runs in CI in release"]
fn the_full_scope_population_is_what_the_thresholds_were_read_from() {
    let r = dcel::run(TopologyScope::Full, PRODUCTION).expect("dcel run");
    let r = dcel::report::build(&r);
    println!(
        "FULL: arms {} ({} corpus, {} structural), groups {}, convention-dependent {}",
        r.arms_measured, r.corpus_arms, r.structural_arms, r.groups, r.convention_dependent_groups
    );
    println!(
        "FULL: transactions {}/{} committed, unrelated chains {}, probes {}",
        r.transactions_committed,
        r.transactions_attempted,
        r.unrelated_chains_total,
        r.audit_resolving_power.arrangements_probed
    );
    println!("FULL: classes {:?}", r.classes);
}

/// **Every §28 M5 clause has a knockout, and each reddens its OWN row.**
///
/// REDTEAM_M5 §3 lists `RunKnockouts` as one of three mechanisms that fail
/// F-0048: it was one field per clause that happened to have a knockout, so
/// the answer to Q2 was "append a field" and clauses 2 and 4 had no world in
/// which they are false at all.
///
/// The judge here is the GATE TABLE, not this file: the number of knockouts is
/// compared against the number of rows `gate_table()` returns, and each
/// knockout is required to turn the row of the same name red while the clean
/// run has it green. A fifth clause added without a knockout fails this test,
/// and a knockout that reddens the wrong row fails it too.
#[test]
#[ignore = "walks the corpus; wired into CI in release"]
fn every_gate_clause_has_a_knockout_that_reddens_it() {
    let cfg =
        dcel::report::DcelGateConfig::for_tests_from_the_committed_file().expect("gate config");
    // FULL scope: the population floors are what a gate row stands on, and at
    // test scope they are not met, so "the clean run is MET" — the positive
    // control this test needs — could not be established at all.
    let clean = run_at(TopologyScope::Full, PRODUCTION).gate_table(&cfg);
    assert_eq!(clean.len(), 4, "the §28 M5 gate has four clauses");
    for (name, ok, _) in &clean {
        assert!(ok, "positive control: {name} must be MET on the clean run");
    }

    let knockouts = RunKnockouts::one_per_clause();
    assert_eq!(
        knockouts.len(),
        clean.len(),
        "every clause needs a knockout; the count comes from the gate table, not from a list          in this file"
    );

    for (clause, k) in knockouts {
        let rows = run_at(TopologyScope::Full, k).gate_table(&cfg);
        let row = rows
            .iter()
            .find(|(n, _, _)| *n == clause)
            .unwrap_or_else(|| panic!("no row named {clause}"));
        assert!(
            !row.1,
            "the knockout for {clause:?} left its own row MET; a clause whose knockout does not              redden it has no world in which it is false (F-0035)"
        );
    }
}
