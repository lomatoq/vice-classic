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
use vice_bench::gates::{same_commit_violation, GatesFile};
use vice_bench::gt::corpus::{build_manifest, fast_cell_filter, test_cell_filter, CorpusManifest};
use vice_bench::gt::split::{AuditSeal, SPLIT_POLICY_V1};
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
    Verify {
        #[arg(long)]
        manifest: PathBuf,
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
    /// Enforce §27.7: a gate file and production code may not change in the
    /// same commit. Pass the changed paths (e.g. from `git diff --name-only`).
    GatesCheck {
        #[arg(long = "changed")]
        changed: Vec<String>,
    },
}

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
        Cmd::Verify { manifest } => {
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
            let rebuilt_json: serde_json::Value =
                serde_json::from_str(&rebuilt.canonical_json()).expect("round trip");
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
        Cmd::GatesCheck { changed } => match same_commit_violation(&changed) {
            None => {
                println!("no gate/feature co-change in {} path(s)", changed.len());
                0
            }
            Some((gate, feature)) => {
                eprintln!(
                    "spec §27.7 violation: {gate} changed together with {feature}. A gate change \
                     is a separate reviewed commit."
                );
                1
            }
        },
    }
}
