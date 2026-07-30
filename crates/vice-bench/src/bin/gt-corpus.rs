//! GT corpus and M3 scorecard CLI.
//!
//! Every behaviour is an explicit flag; there are no environment switches
//! (spec §32 rule 4). The commands exist so the gate sentences of §28 M3
//! are things a reviewer RUNS rather than reads:
//!
//! ```text
//! gt-corpus build   --out docs/gt/CORPUS_MANIFEST.json [--scope full|fast|test]
//! gt-corpus verify  --manifest docs/gt/CORPUS_MANIFEST.json
//! gt-corpus report  --manifest ... --gates ... --seal ... --out ...
//! gt-corpus audit-status --seal ... --manifest ... --gates ...
//! gt-corpus gates-check --changed <file> [--changed <file> ...]
//! ```

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use vice_bench::artifact;
use vice_bench::corridor::{self, CorridorScope};
use vice_bench::gates::{same_commit_violation_with_base, ChangedPath, GatesFile};
use vice_bench::gt::corpus::{build_manifest, fast_cell_filter, test_cell_filter, CorpusManifest};
use vice_bench::gt::split::{AuditSeal, SPLIT_POLICY_V1};
use vice_bench::m7::{self, MeasurementScope};
use vice_bench::oracle::{self, OracleScope};
use vice_bench::prereg::Preregistration;
use vice_bench::scorecard;
use vice_bench::topology::TopologyScope;

#[path = "gt-corpus/topology_cmd.rs"]
mod topology_cmd;

#[path = "gt-corpus/dcel_cmd.rs"]
mod dcel_cmd;

#[path = "gt-corpus/geometry_cmd.rs"]
mod geometry_cmd;

#[path = "gt-corpus/m7_cmd.rs"]
mod m7_cmd;

#[path = "gt-corpus/cli.rs"]
mod cli;
use cli::*;

#[path = "gt-corpus/m7_dispatch.rs"]
mod m7_dispatch;

fn main() {
    std::process::exit(real_main());
}

fn read_manifest(p: &PathBuf) -> Result<serde_json::Value, String> {
    let text = std::fs::read_to_string(p).map_err(|e| format!("read {}: {e}", p.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", p.display()))
}

fn build_for(scope: Scope) -> Result<CorpusManifest, String> {
    match scope {
        Scope::Full => build_manifest(&SPLIT_POLICY_V1, |_| true),
        Scope::Fast => build_manifest(&SPLIT_POLICY_V1, fast_cell_filter),
        Scope::Test => build_manifest(&SPLIT_POLICY_V1, test_cell_filter),
    }
}

/// Rebuild at the scope the recorded manifest itself declares, so a
/// verification cannot accidentally compare different cell sets.
fn rebuild_matching(recorded: &serde_json::Value) -> Result<CorpusManifest, String> {
    let cells: Vec<String> = recorded["cells"]
        .as_array()
        .ok_or("manifest has no cell list")?
        .iter()
        .map(|v| v.as_str().unwrap_or_default().to_string())
        .collect();
    build_manifest(&SPLIT_POLICY_V1, |c| cells.contains(&c.id()))
}

fn real_main() -> i32 {
    let command = match m7_dispatch::run(Cli::parse().cmd) {
        Ok(code) => return code,
        Err(command) => *command,
    };
    match command {
        Cmd::Build { out, scope } => match build_for(scope) {
            Ok(m) => {
                if let Some(parent) = out.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let text = serde_json::to_string_pretty(&m).expect("manifest serializes");
                if let Err(e) = std::fs::write(&out, format!("{text}\n")) {
                    eprintln!("error: write {}: {e}", out.display());
                    return 2;
                }
                println!(
                    "corpus: {} groups, {} scenes, {} renders over {} cells",
                    m.source_groups(),
                    m.groups.iter().map(|g| g.scenes.len()).sum::<usize>(),
                    m.renders.len(),
                    m.cells.len()
                );
                for s in &m.split_summary {
                    println!(
                        "  {}: {} families, {} groups, {} scenes",
                        s.split, s.families, s.groups, s.scenes
                    );
                }
                println!("corpus_hash: {}", m.hash());
                println!("manifest: {}", out.display());
                0
            }
            Err(e) => {
                eprintln!("error: {e}");
                2
            }
        },
        Cmd::Verify {
            manifest,
            structural,
        } => {
            let recorded = match read_manifest(&manifest) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            // The platform refusal is a string comparison and it comes
            // FIRST. It used to sit after the rebuild, which meant 292
            // seconds of work before a typed refusal that needed none of it
            // (REVIEW_M3 M3-D2). A refusal an operator waits five minutes
            // for teaches them to stop running the check.
            let here = serde_json::json!({
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
            });
            let recorded_platform = recorded.get("platform").cloned().unwrap_or_default();
            let same_platform = recorded_platform == here;
            if !same_platform && !structural {
                eprintln!(
                    "error: this manifest records digests for platform {recorded_platform}, and \
                     this is {here}. Render digests are a TIER A artifact (spec 5.5): corpus \
                     geometry comes from libm sin/cos/powf/exp and is not guaranteed \
                     bit-identical across platforms (ADR-0008). Re-run on the recording \
                     platform, or pass --structural to compare the platform-independent \
                     projection."
                );
                return 2;
            }
            let rebuilt = match rebuild_matching(&recorded) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            let rebuilt_json: serde_json::Value =
                serde_json::from_str(&rebuilt.canonical_json()).expect("round trip");
            if structural && !same_platform {
                let a = vice_bench::gt::corpus::structural_projection(&recorded);
                let b = vice_bench::gt::corpus::structural_projection(&rebuilt_json);
                if a == b {
                    println!(
                        "corpus reproduced STRUCTURALLY across platforms ({} renders): \
                         composition, splits, cells, identifiability and inverse-crime flags all \
                         match. Render digests NOT compared - they are Tier A, recorded on \
                         {recorded_platform} and rebuilt on {here}.",
                        rebuilt.renders.len()
                    );
                    return 0;
                }
                eprintln!(
                    "corpus did NOT reproduce structurally - the difference is composition, not \
                     float noise"
                );
                return 1;
            }
            if rebuilt_json == recorded {
                println!("corpus reproduced: {} renders", rebuilt.renders.len());
                println!("corpus_hash: {}", rebuilt.hash());
                return 0;
            }
            // Report the first differing renders rather than a bare "differs".
            let empty = Vec::new();
            let rec_renders = recorded["renders"].as_array().unwrap_or(&empty);
            let mut shown = 0;
            for (i, got) in rebuilt.renders.iter().enumerate() {
                let want = rec_renders.get(i);
                let want_sha = want
                    .and_then(|v| v["sha256"].as_str())
                    .unwrap_or("<absent>");
                if want_sha != got.sha256 {
                    println!(
                        "differs: {} {} recorded {} actual {}",
                        got.scene_id, got.cell_id, want_sha, got.sha256
                    );
                    shown += 1;
                    if shown >= 10 {
                        println!("... (further differences suppressed)");
                        break;
                    }
                }
            }
            if shown == 0 {
                println!("render digests match; metadata differs (scope, splits or truth)");
            }
            eprintln!("corpus did NOT reproduce");
            1
        }
        Cmd::Report {
            manifest,
            gates,
            seal,
            out,
        } => {
            let recorded = match read_manifest(&manifest) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            // Cheap inputs first, expensive rebuild after (REVIEW_M3
            // M3-D2): an unreadable gate file or seal must be reported in
            // milliseconds, not after the corpus has been regenerated.
            let g = match GatesFile::load(&gates) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            let s: AuditSeal = match std::fs::read_to_string(&seal)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
            {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: read seal {}: {e}", seal.display());
                    return 2;
                }
            };
            let m = match rebuild_matching(&recorded) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            let card = scorecard::build(&m, &g, &s);
            if let Some(parent) = out.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&out, format!("{}\n", card.canonical_json())) {
                eprintln!("error: write {}: {e}", out.display());
                return 2;
            }
            let mut all_ok = true;
            println!("M3 gate table:");
            for (name, ok, why) in card.gate_table(&m) {
                println!("  [{}] {name}: {why}", if ok { "MET" } else { "NOT MET" });
                all_ok &= ok;
            }
            println!("scorecard: {}", out.display());
            if all_ok {
                0
            } else {
                1
            }
        }
        Cmd::AuditStatus {
            seal,
            manifest,
            gates,
        } => {
            let s: AuditSeal = match std::fs::read_to_string(&seal)
                .map_err(|e| e.to_string())
                .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
            {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: read seal {}: {e}", seal.display());
                    return 2;
                }
            };
            // The corpus hash MUST be produced the same way every other
            // component produces it: `CorpusManifest::hash()` on a rebuild,
            // exactly as `verify` and `report` do (REVIEW_M3 M3-N1). The
            // first version hashed the PARSED json instead, whose keys
            // serde_json sorts, so the burn check compared a value no
            // component ever produced and a faithful `open` reported BURNED
            // on an untouched corpus. There is now one function, not two.
            let recorded = match read_manifest(&manifest) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            let gates_hash = match GatesFile::load(&gates) {
                Ok(g) => g.sha256,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            let corpus_hash = match rebuild_matching(&recorded) {
                Ok(m) => m.hash(),
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            let prereg_hash = Preregistration::v1().hash();
            println!("audit generation {} status {:?}", s.generation, s.status);
            match s.check(&corpus_hash, &prereg_hash, &gates_hash) {
                Ok(()) => {
                    println!("audit is open and untouched since it was opened");
                    0
                }
                Err(v) => {
                    // A SEALED audit is the expected M3 state, not a failure.
                    if matches!(v, vice_bench::gt::split::BurnViolation::StillSealed) {
                        println!("sealed and never opened: nothing may be scored against it");
                        0
                    } else {
                        eprintln!("BURN POLICY VIOLATION: {v}");
                        1
                    }
                }
            }
        }
        Cmd::Oracle { out, scope } => {
            let run = match oracle::run(scope.into()) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            let report = oracle::report::build(&run);
            if let Some(parent) = out.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&out, format!("{}\n", report.canonical_json())) {
                eprintln!("error: write {}: {e}", out.display());
                return 2;
            }
            println!(
                "oracle: {} scenes, {} arms measured, {} refused, config_hash {}",
                report.scenes, report.arms_measured, report.arms_refused, report.config_hash
            );
            for agg in &report.ceiling {
                println!(
                    "  G30 {} vs cell {}: max {:.1} code, edge-mean {:.3} code over {} arms{}",
                    agg.backend_id,
                    agg.cell_id,
                    agg.max_abs_code,
                    agg.edge_mean_abs_code_mean,
                    agg.arms,
                    if agg.inverse_crime.is_contaminated() {
                        "   [INVERSE CRIME]"
                    } else {
                        ""
                    }
                );
            }
            // Warnings go to stderr as well as into the artifact: a warning
            // only a file carries is a warning an operator never reads.
            for w in &report.warnings {
                eprintln!("warning: {w}");
            }
            let mut all_ok = true;
            println!("M3.5 + M4 gate table:");
            for (name, ok, why) in report.gate_table() {
                println!("  [{}] {name}: {why}", if ok { "MET" } else { "NOT MET" });
                all_ok &= ok;
            }
            println!("oracle report: {}", out.display());
            if all_ok {
                0
            } else {
                1
            }
        }
        Cmd::Corridor { out, scope } => {
            let run = match corridor::run(scope.into()) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            let report = corridor::report::build(&run);
            if let Some(parent) = out.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&out, format!("{}\n", report.canonical_json())) {
                eprintln!("error: write {}: {e}", out.display());
                return 2;
            }
            println!(
                "corridor: {} scenes, {} arms, {} refused, {} sealed-audit groups skipped, \
                 config_hash {}",
                report.scenes,
                report.arms_measured,
                report.arms_refused,
                report.sealed_audit_groups_skipped,
                report.config_hash
            );
            for (name, got, want, ok) in &report.targets {
                println!(
                    "  [{}] {name}: measured {got:.4} against the provisional {want:.4}",
                    if *ok { "MET" } else { "MISSED" }
                );
            }
            println!(
                "  held-out coverage@50/90/95/99 = {}",
                report
                    .held_out
                    .coverage
                    .iter()
                    .map(|(l, c)| format!("{l:.2}:{c:.3}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            for w in &report.warnings {
                eprintln!("warning: {w}");
            }
            let mut all_ok = true;
            println!("M4 gate table (three of four clauses; the factorial is the oracle's):");
            for (name, ok, why) in report.gate_table() {
                println!("  [{}] {name}: {why}", if ok { "MET" } else { "NOT MET" });
                all_ok &= ok;
            }
            println!("corridor report: {}", out.display());
            if all_ok {
                0
            } else {
                1
            }
        }
        Cmd::CorridorCheck { report, structural } => {
            let recorded = match read_manifest(&report) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            // The platform refusal is CHEAP and comes first: rebuilding for
            // minutes before refusing turns a typed refusal into a wait
            // (REVIEW_M3 M3-D2).
            if recorded.get("platform") != Some(&artifact::platform_here()) && !structural {
                return artifact::check(
                    "corridor report",
                    &recorded,
                    &recorded,
                    false,
                    corridor::report::structural_projection,
                )
                .exit_code();
            }
            let scope = match recorded["config"]["scope"].as_str() {
                Some("full") => CorridorScope::Full,
                Some("test") => CorridorScope::Test,
                other => {
                    eprintln!("error: report declares unknown scope {other:?}");
                    return 2;
                }
            };
            let run = match corridor::run(scope) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            let rebuilt: serde_json::Value =
                serde_json::from_str(&corridor::report::build(&run).canonical_json())
                    .expect("round trip");
            artifact::check(
                "corridor report",
                &recorded,
                &rebuilt,
                structural,
                corridor::report::structural_projection,
            )
            .exit_code()
        }
        Cmd::Topology { out, scope } => topology_cmd::run(&out, scope.into()),
        Cmd::TopologyCheck { report, structural } => topology_cmd::check(&report, structural),
        Cmd::Dcel { out, scope } => dcel_cmd::run(&out, scope.into()),
        Cmd::DcelCheck { report, structural } => dcel_cmd::check(&report, structural),
        Cmd::GeometryM6 { gates, out } => geometry_cmd::run(&gates, &out),
        Cmd::GeometryM6Check { gates, report } => geometry_cmd::check(&gates, &report),
        Cmd::OracleCheck { report, structural } => {
            let recorded = match read_manifest(&report) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            // The platform refusal is CHEAP and therefore comes first: the
            // corpus `verify` used to rebuild for minutes before refusing
            // (REVIEW_M3 M3-D2), which turns a typed refusal into a wait.
            if recorded.get("platform") != Some(&artifact::platform_here()) && !structural {
                return artifact::check(
                    "oracle report",
                    &recorded,
                    &recorded,
                    false,
                    oracle::report::structural_projection,
                )
                .exit_code();
            }
            let scope = match recorded["config"]["scope"].as_str() {
                Some("full") => OracleScope::Full,
                Some("test") => OracleScope::Test,
                other => {
                    eprintln!("error: report declares unknown scope {other:?}");
                    return 2;
                }
            };
            let run = match oracle::run(scope) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            let rebuilt: serde_json::Value =
                serde_json::from_str(&oracle::report::build(&run).canonical_json())
                    .expect("round trip");
            artifact::check(
                "oracle report",
                &recorded,
                &rebuilt,
                structural,
                oracle::report::structural_projection,
            )
            .exit_code()
        }
        Cmd::GatesCheck {
            changed,
            stdin,
            existing_gate,
        } => {
            let mut lines: Vec<String> = changed;
            if stdin {
                let mut buf = String::new();
                if let Err(e) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf) {
                    eprintln!("error: read stdin: {e}");
                    return 2;
                }
                // Split on lines only AFTER the NUL check: the whole point
                // of the refusal is that `--name-status -z` has no lines.
                if buf.as_bytes().contains(&0) {
                    eprintln!(
                        "error: {}",
                        vice_bench::gates::UnrecognizedForm::NulSeparated
                    );
                    return 2;
                }
                lines.extend(buf.lines().map(|l| l.to_string()));
            }
            let mut parsed: Vec<ChangedPath> = Vec::new();
            for l in &lines {
                match ChangedPath::parse(l) {
                    Ok(v) => parsed.extend(v),
                    Err(e) => {
                        eprintln!("error: {e}");
                        return 2;
                    }
                }
            }
            match same_commit_violation_with_base(&parsed, &existing_gate) {
                None => {
                    println!("no gate/feature co-change in {} path(s)", parsed.len());
                    0
                }
                Some((gate, feature)) => {
                    eprintln!(
                        "spec §27.7 violation: existing gate {gate} changed together with \
                         {feature}. A gate change is a separate reviewed commit."
                    );
                    1
                }
            }
        }
        _ => unreachable!("M7 commands are dispatched before legacy commands"),
    }
}
