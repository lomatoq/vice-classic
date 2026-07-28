//! The `topology` and `topology-check` subcommands of `gt-corpus`.
//!
//! In their own file for the reason §4.1 gives: the binary crossed the
//! 800-line rule when they arrived, and a rule that is enforced by a test
//! (`hygiene.rs::no_production_module_is_over_the_size_rule`) is a rule that
//! moves code rather than a rule that is quoted.

use std::path::Path;

use vice_bench::artifact;
use vice_bench::gates::GatesFile;
use vice_bench::topology::gate::TopologyGateConfig;
use vice_bench::topology::{self, TopologyScope};

/// The gate-row thresholds, read from the frozen file rather than from a
/// constant next to the row (RT45-A10).
///
/// The PATH is a constant, not an argument, and that is RT45-A18. §27.7 is
/// enforced over `GATE_PATHS`, so while the source of the thresholds was a
/// `--gates` flag the rule protected a file NAME while the decision followed a
/// line of YAML: a copy of the gate file with all five thresholds at 1, plus one
/// edited line in `ci.yml`, relaxed every §28 M4.5 clause with zero lines in
/// `crates/`, the gate file untouched and `gates-check` exit 0.
fn gate_config() -> Result<TopologyGateConfig, String> {
    let path = Path::new(vice_bench::gates::GATE_PATHS[0]);
    let g = GatesFile::load(path).map_err(|e| format!("load {}: {e}", path.display()))?;
    TopologyGateConfig::from_gates(&g)
}

/// Run the M4.5 recall harness and write its report (§11.3, §28 M4.5).
pub fn run(out: &Path, scope: TopologyScope) -> i32 {
    let cfg = match gate_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let run = match topology::run(scope) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let report = topology::report::build(&run);
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(out, format!("{}\n", report.canonical_json())) {
        eprintln!("error: write {}: {e}", out.display());
        return 2;
    }
    println!(
        "topology: {} scenes, {} arms, {} refused, {} sealed-audit groups skipped, {} \
                 opaque-exterior arms excluded, config_hash {}",
        report.scenes,
        report.arms_measured,
        report.arms_refused,
        report.sealed_audit_groups_skipped,
        report.opaque_exterior_arms_excluded,
        report.config_hash
    );
    println!(
        "  recall all {}/{}  events-only {}/{}  fixed-only {}/{}",
        report.recall_all.hits,
        report.recall_all.arms,
        report.recall_events_only.hits,
        report.recall_events_only.arms,
        report.recall_fixed_only.hits,
        report.recall_fixed_only.arms
    );
    for f in &report.field_contributions {
        println!(
            "  field {:<22} matched {:>5}  sole source {:>5}",
            f.field, f.matched, f.sole_source
        );
    }
    for w in &report.warnings {
        eprintln!("warning: {w}");
    }
    let mut all_ok = true;
    println!("M4.5 gate table (all three clauses of the spec):");
    for (name, ok, why) in report.gate_table(&cfg) {
        println!("  [{}] {name}: {why}", if ok { "MET" } else { "NOT MET" });
        all_ok &= ok;
    }
    println!("topology report: {}", out.display());
    if all_ok {
        0
    } else {
        1
    }
}

/// Re-run at the report's own scope and compare. Tier A, exactly like the
/// corridor and oracle reports.
pub fn check(report: &Path, structural: bool) -> i32 {
    let recorded = match read_report(report) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    // The platform refusal is CHEAP and comes first (REVIEW_M3 M3-D2).
    if recorded.get("platform") != Some(&artifact::platform_here()) && !structural {
        return artifact::check(
            "topology report",
            &recorded,
            &recorded,
            false,
            topology::report::structural_projection,
        )
        .exit_code();
    }
    let scope = match recorded["config"]["scope"].as_str() {
        Some("full") => TopologyScope::Full,
        Some("test") => TopologyScope::Test,
        other => {
            eprintln!("error: report declares unknown scope {other:?}");
            return 2;
        }
    };
    let run = match topology::run(scope) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let rebuilt: serde_json::Value =
        serde_json::from_str(&topology::report::build(&run).canonical_json()).expect("round trip");
    artifact::check(
        "topology report",
        &recorded,
        &rebuilt,
        structural,
        topology::report::structural_projection,
    )
    .exit_code()
}

fn read_report(p: &Path) -> Result<serde_json::Value, String> {
    let text = std::fs::read_to_string(p).map_err(|e| format!("read {}: {e}", p.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", p.display()))
}
