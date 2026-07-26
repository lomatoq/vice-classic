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

use clap::{Parser, Subcommand, ValueEnum};
use vice_bench::gates::{same_commit_violation, ChangedPath, GatesFile};
use vice_bench::gt::corpus::{build_manifest, fast_cell_filter, test_cell_filter, CorpusManifest};
use vice_bench::gt::split::{AuditSeal, SPLIT_POLICY_V1};
use vice_bench::oracle::{self, OracleScope};
use vice_bench::prereg::Preregistration;
use vice_bench::scorecard;

#[derive(Parser)]
#[command(
    name = "gt-corpus",
    version,
    about = "vice-classic M3: build, verify and report on the GT corpus"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

/// How much of the corpus the oracle harness covers. Part of its config
/// hash, hence of every compatibility key it issues (§27.6).
#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum OracleScopeArg {
    Full,
    Test,
}

impl From<OracleScopeArg> for OracleScope {
    fn from(v: OracleScopeArg) -> OracleScope {
        match v {
            OracleScopeArg::Full => OracleScope::Full,
            OracleScopeArg::Test => OracleScope::Test,
        }
    }
}

/// Which degradation cells to include. Recorded in the manifest, so a
/// cheap scope can never be mistaken for the full reproduction.
#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum Scope {
    /// Every cell of the frozen matrix. Minutes of work.
    Full,
    /// Sizes up to 32, without the supersampled box spine.
    Fast,
    /// One size. Seconds; what the unit tests use.
    Test,
}

#[derive(Subcommand)]
enum Cmd {
    /// Regenerate the corpus and write its manifest.
    Build {
        #[arg(long)]
        out: PathBuf,
        #[arg(long, value_enum, default_value_t = Scope::Full)]
        scope: Scope,
    },
    /// Rebuild at the manifest's own scope and compare every render digest.
    ///
    /// Render digests are a TIER A artifact (§5.5): corpus geometry comes
    /// from `sin`/`cos`, colour from `powf` and the gaussian PSF from
    /// `exp`, and Rust does not guarantee libm bit-identity across
    /// platforms (ADR-0008 §8). A manifest recorded on another platform is
    /// refused unless `--structural` is given.
    Verify {
        #[arg(long)]
        manifest: PathBuf,
        /// Compare only the platform-INDEPENDENT projection: composition,
        /// splits, cell list, identifiability labels and inverse-crime
        /// flags - everything except float-valued digests and truth. Says
        /// so in its output; never silently weakens a same-platform check.
        #[arg(long)]
        structural: bool,
    },
    /// Write the M3 scorecard.
    Report {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        gates: PathBuf,
        #[arg(long)]
        seal: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Check the sealed-audit burn policy against the current hashes.
    AuditStatus {
        #[arg(long)]
        seal: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        gates: PathBuf,
    },
    /// Run the M3.5 factorial oracle harness and write its report.
    Oracle {
        #[arg(long)]
        out: PathBuf,
        #[arg(long, value_enum, default_value_t = OracleScopeArg::Full)]
        scope: OracleScopeArg,
    },
    /// Re-run the harness at the report's own scope and compare.
    ///
    /// Oracle metrics are libm-derived floats, so like the corpus manifest
    /// they are a TIER A artifact (§5.5): a report recorded on another
    /// platform is refused unless `--structural` is given.
    OracleCheck {
        #[arg(long)]
        report: PathBuf,
        #[arg(long)]
        structural: bool,
    },
    /// Enforce §27.7: an EXISTING gate file and production code may not
    /// change together. Pass `git diff --name-status` lines (status letter
    /// and path) via `--changed`, or a whole diff on stdin with `--stdin`.
    ///
    /// Creating a gate file alongside its loader is a named exemption: the
    /// rule forbids weakening a gate, and a gate that does not exist cannot
    /// be weakened (REVIEW_M3 M3-N2).
    GatesCheck {
        #[arg(long = "changed")]
        changed: Vec<String>,
        /// Read `git diff --name-status` lines from stdin as well. This is
        /// what CI uses, so the check can cover the whole pushed range
        /// rather than one commit.
        #[arg(long)]
        stdin: bool,
    },
}

fn main() {
    std::process::exit(real_main());
}

/// The part of a manifest that does NOT depend on libm: identity,
/// composition, splits, cell list, identifiability labels and inverse-crime
/// flags. Everything float-valued - render digests, scene digests, measured
/// truth - is dropped, because those are Tier A.
fn structural_projection(m: &serde_json::Value) -> serde_json::Value {
    let renders: Vec<serde_json::Value> = m["renders"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|r| {
            serde_json::json!({
                "group_id": r["group_id"],
                "scene_id": r["scene_id"],
                "cell_id": r["cell_id"],
                "split": r["split"],
                "identifiability": r["identifiability"],
                "inverse_crime": r["inverse_crime"],
                "width_px": r["width_px"],
                "height_px": r["height_px"],
            })
        })
        .collect();
    let groups: Vec<serde_json::Value> = m["groups"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|g| {
            let scenes: Vec<serde_json::Value> = g["scenes"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|sc| {
                    serde_json::json!({
                        "id": sc["id"],
                        "authored_truth_construction": sc["authored_truth_construction"],
                        "visible_faces": sc["partition_truth"]["visible_faces"],
                        "holes": sc["partition_truth"]["holes"],
                        "components": sc["partition_truth"]["components"],
                        "exterior_model": sc["partition_truth"]["exterior_model"],
                    })
                })
                .collect();
            serde_json::json!({
                "id": g["id"],
                "origin": g["origin"],
                "shape_family": g["shape_family"],
                "provenance": g["provenance"],
                "split": g["split"],
                "intentionally_ambiguous": g["intentionally_ambiguous"],
                "equivalence_class": g["equivalence_class"],
                "scenes": scenes,
            })
        })
        .collect();
    serde_json::json!({
        "schema": m["schema"],
        "procedural_variants_per_family": m["procedural_variants_per_family"],
        "split_policy_version": m["split_policy_version"],
        "cells": m["cells"],
        "groups": groups,
        "renders": renders,
        "split_summary": m["split_summary"],
        "identifiability_counts": m["identifiability_counts"],
        "renders_by_profile": m["renders_by_profile"],
    })
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
    match Cli::parse().cmd {
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
            let rebuilt = match rebuild_matching(&recorded) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
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
            let rebuilt_json: serde_json::Value =
                serde_json::from_str(&rebuilt.canonical_json()).expect("round trip");
            if structural && !same_platform {
                let a = structural_projection(&recorded);
                let b = structural_projection(&rebuilt_json);
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
            let m = match rebuild_matching(&recorded) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
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
            let corpus_hash = match rebuild_matching(&recorded) {
                Ok(m) => m.hash(),
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
            println!("M3.5 gate table:");
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
            let here = serde_json::json!({
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
            });
            let recorded_platform = recorded.get("platform").cloned().unwrap_or_default();
            let same_platform = recorded_platform == here;
            if !same_platform && !structural {
                eprintln!(
                    "error: this oracle report records metrics for platform \
                     {recorded_platform}, and this is {here}. The metrics are libm-derived \
                     floats and therefore TIER A (spec 5.5, ADR-0008). Re-run on the recording \
                     platform, or pass --structural to compare the platform-independent \
                     projection."
                );
                return 2;
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
            let (a, b, mode) = if structural && !same_platform {
                (
                    oracle::report::structural_projection(&recorded),
                    oracle::report::structural_projection(&rebuilt),
                    "STRUCTURALLY across platforms (metrics NOT compared - they are Tier A)",
                )
            } else {
                (recorded, rebuilt, "with every metric compared")
            };
            if a == b {
                println!("oracle report reproduced {mode}");
                0
            } else {
                eprintln!("oracle report did NOT reproduce {mode}");
                1
            }
        }
        Cmd::GatesCheck { changed, stdin } => {
            let mut lines: Vec<String> = changed;
            if stdin {
                let mut buf = String::new();
                if let Err(e) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf) {
                    eprintln!("error: read stdin: {e}");
                    return 2;
                }
                lines.extend(buf.lines().map(|l| l.to_string()));
            }
            let parsed: Vec<ChangedPath> =
                lines.iter().filter_map(|l| ChangedPath::parse(l)).collect();
            match same_commit_violation(&parsed) {
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
    }
}
