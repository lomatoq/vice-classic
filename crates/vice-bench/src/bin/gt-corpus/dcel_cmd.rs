//! The `dcel` and `dcel-check` subcommands of `gt-corpus` (§28 M5).
//!
//! In their own file for the reason §4.1 gives and `topology_cmd.rs` already
//! demonstrates: a rule enforced by a test is a rule that moves code.

use std::path::Path;

use vice_bench::artifact;
use vice_bench::dcel::report::DcelGateConfig;
use vice_bench::dcel::{self, PRODUCTION};
use vice_bench::gates::GatesFile;
use vice_bench::topology::TopologyScope;

/// The gate-row thresholds, from the frozen file at a CONSTANT path.
///
/// The path is `GATE_PATHS[0]` and not a `--gates` flag, which is RT45-A18: a
/// flag makes §27.7 protect a file NAME while the decision follows a line of
/// YAML, and one edited workflow line then relaxes every clause with zero lines
/// in `crates/`.
fn gate_config() -> Result<DcelGateConfig, String> {
    let path = Path::new(vice_bench::gates::GATE_PATHS[0]);
    let g = GatesFile::load_for_a_gate_decision(path)
        .map_err(|e| format!("load {}: {e}", path.display()))?;
    DcelGateConfig::from_gates(&g)
}

pub fn run(out: &Path, scope: TopologyScope) -> i32 {
    let cfg = match gate_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let run = match dcel::run(scope, PRODUCTION) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let report = dcel::report::build(&run);
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(out, format!("{}\n", report.canonical_json())) {
        eprintln!("error: write {}: {e}", out.display());
        return 2;
    }
    println!(
        "dcel: {} scenes, {} arms ({} corpus, {} structural), {} refused, {} sealed-audit groups \
         skipped",
        report.scenes,
        report.arms_measured,
        report.corpus_arms,
        report.structural_arms,
        report.arms_refused,
        report.sealed_audit_groups_skipped
    );
    println!(
        "  classes {:?}; groups {}, classes in {} out {}, convention-dependent {}",
        report.classes,
        report.groups,
        report.classes_in,
        report.classes_out,
        report.convention_dependent_groups
    );
    println!(
        "  transactions {} attempted, {} committed, {} rolled back; unrelated chains {}, moved {}",
        report.transactions_attempted,
        report.transactions_committed,
        report.transactions_rolled_back,
        report.unrelated_chains_total,
        report.unrelated_chains_that_moved
    );
    let p = &report.audit_resolving_power;
    println!(
        "  audit resolving power: {} arrangements of {} arms, {} slots, caught by audit {}, \
         UNCAUGHT {}, no-ops {}",
        p.arrangements_probed,
        p.arms_seen,
        p.slots_perturbed,
        p.caught_by_audit,
        p.uncaught_by_audit,
        p.no_ops
    );
    for f in &p.by_family {
        println!(
            "    {:<20} slots {:>7}  caught {:>7}  uncaught {:>3}  no-ops {:>3}",
            f.family, f.slots, f.caught_by_audit, f.uncaught_by_audit, f.no_ops
        );
    }
    let mut all_ok = true;
    println!("M5 gate table (all four clauses of the spec):");
    for (name, ok, why) in report.gate_table(&cfg) {
        println!("  [{}] {name}: {why}", if ok { "MET" } else { "NOT MET" });
        all_ok &= ok;
    }
    println!("dcel report: {}", out.display());
    if all_ok {
        0
    } else {
        1
    }
}

/// Re-run at the report's own scope and compare. Tier A, like every other
/// committed artifact of this project.
pub fn check(report: &Path, structural: bool) -> i32 {
    let recorded = match read_report(report) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    if recorded.get("platform") != Some(&artifact::platform_here()) && !structural {
        return artifact::check(
            "dcel report",
            &recorded,
            &recorded,
            false,
            dcel::report::structural_projection,
        )
        .exit_code();
    }
    let scope = match recorded["scope"].as_str() {
        Some("full") => TopologyScope::Full,
        Some("test") => TopologyScope::Test,
        other => {
            eprintln!("error: report declares unknown scope {other:?}");
            return 2;
        }
    };
    let run = match dcel::run(scope, PRODUCTION) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let rebuilt: serde_json::Value =
        serde_json::from_str(&dcel::report::build(&run).canonical_json()).expect("round trip");
    artifact::check(
        "dcel report",
        &recorded,
        &rebuilt,
        structural,
        dcel::report::structural_projection,
    )
    .exit_code()
}

fn read_report(p: &Path) -> Result<serde_json::Value, String> {
    let text = std::fs::read_to_string(p).map_err(|e| format!("read {}: {e}", p.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", p.display()))
}
