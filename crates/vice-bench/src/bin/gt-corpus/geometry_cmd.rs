//! CLI adapter for the M6 geometry-oracle artifact.

use std::path::Path;

use vice_bench::artifact;
use vice_bench::gates::GatesFile;
use vice_bench::geometry;

pub fn run(gates: &Path, out: &Path) -> i32 {
    let gates = match GatesFile::load_for_a_gate_decision(gates) {
        Ok(gates) => gates,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let report = match geometry::measure(&gates) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = serde_json::to_string_pretty(&report).expect("geometry report serializes");
    if let Err(e) = std::fs::write(out, format!("{text}\n")) {
        eprintln!("error: write {}: {e}", out.display());
        return 2;
    }
    println!(
        "M6 geometry: {} boundaries, {} injections, {} oracle-selector changes",
        report.measurements.boundaries_measured,
        report.measurements.oracle_candidate_injections,
        report.measurements.oracle_selector_changes
    );
    for row in &report.gate.rows {
        println!(
            "  [{}] {}: measured {}, required {}",
            if row.met { "MET" } else { "NOT MET" },
            row.clause,
            row.measured,
            row.required
        );
    }
    println!("geometry report: {}", out.display());
    if report.gate.met {
        0
    } else {
        1
    }
}

pub fn check(gates: &Path, report: &Path) -> i32 {
    let recorded = match super::read_manifest(&report.to_path_buf()) {
        Ok(value) => value,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    if recorded["measurements"]["platform"] != artifact::platform_here() {
        eprintln!(
            "error: M6 geometry metrics are a Tier-A artifact recorded on {}, current platform \
             is {}; re-run on the recording platform",
            recorded["measurements"]["platform"],
            artifact::platform_here()
        );
        return 2;
    }
    let gates = match GatesFile::load_for_a_gate_decision(gates) {
        Ok(gates) => gates,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let rebuilt = match geometry::measure(&gates) {
        Ok(value) => serde_json::to_value(value).expect("geometry report serializes"),
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    if recorded == rebuilt {
        println!(
            "M6 geometry report reproduced: {} boundaries",
            rebuilt["measurements"]["boundaries_measured"]
        );
        0
    } else {
        eprintln!("M6 geometry report did NOT reproduce");
        1
    }
}
