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

/// How much of the corpus the corridor calibration covers. Part of its
/// config hash, so a cheap run is not comparable with a full one.
#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum CorridorScopeArg {
    Full,
    Test,
}

impl From<CorridorScopeArg> for CorridorScope {
    fn from(v: CorridorScopeArg) -> CorridorScope {
        match v {
            CorridorScopeArg::Full => CorridorScope::Full,
            CorridorScopeArg::Test => CorridorScope::Test,
        }
    }
}

/// How much of the corpus the topology recall run covers. Part of its
/// config hash, so a cheap run is not comparable with a full one.
#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum TopologyScopeArg {
    Full,
    Test,
}

impl From<TopologyScopeArg> for TopologyScope {
    fn from(v: TopologyScopeArg) -> TopologyScope {
        match v {
            TopologyScopeArg::Full => TopologyScope::Full,
            TopologyScopeArg::Test => TopologyScope::Test,
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

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum M7ScopeArg {
    Smoke,
    CalibrationSmoke,
    Calibration,
}

impl From<M7ScopeArg> for MeasurementScope {
    fn from(value: M7ScopeArg) -> Self {
        match value {
            M7ScopeArg::Smoke => MeasurementScope::Smoke,
            M7ScopeArg::CalibrationSmoke => MeasurementScope::CalibrationSmoke,
            M7ScopeArg::Calibration => MeasurementScope::Calibration,
        }
    }
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
    /// Run the M4 corridor calibration and write its report (§13.1).
    Corridor {
        #[arg(long)]
        out: PathBuf,
        #[arg(long, value_enum, default_value_t = CorridorScopeArg::Full)]
        scope: CorridorScopeArg,
    },
    /// Re-run the corridor calibration at the report's own scope and
    /// compare. Tier A, exactly like the oracle report.
    CorridorCheck {
        #[arg(long)]
        report: PathBuf,
        #[arg(long)]
        structural: bool,
    },
    /// Run the M4.5 topology candidate-recall harness and write its report.
    Topology {
        #[arg(long)]
        out: PathBuf,
        #[arg(long, value_enum, default_value_t = TopologyScopeArg::Full)]
        scope: TopologyScopeArg,
    },
    /// Re-run the topology harness at the report's own scope and compare.
    /// Tier A, exactly like the corridor and oracle reports.
    TopologyCheck {
        #[arg(long)]
        report: PathBuf,
        #[arg(long)]
        structural: bool,
    },
    /// Run the M5 shared-DCEL and transaction harness and write its report.
    Dcel {
        #[arg(long)]
        out: PathBuf,
        #[arg(long, value_enum, default_value_t = TopologyScopeArg::Full)]
        scope: TopologyScopeArg,
    },
    /// Re-run the DCEL harness at the report's own scope and compare. Tier A.
    DcelCheck {
        #[arg(long)]
        report: PathBuf,
        #[arg(long)]
        structural: bool,
    },
    /// Run the M6 G00/G10/G01/G11/G20 geometry decomposition and gate it.
    GeometryM6 {
        #[arg(long)]
        gates: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Re-run the M6 geometry decomposition and compare the Tier-A artifact.
    GeometryM6Check {
        #[arg(long)]
        gates: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
    /// Measure raw M7 selected-scene quality on development smoke or the
    /// calibration split. The sealed audit has a separate burn-controlled
    /// release command and cannot be opened here.
    M7Measure {
        #[arg(long)]
        out: PathBuf,
        #[arg(long, value_enum, default_value_t = M7ScopeArg::Smoke)]
        scope: M7ScopeArg,
        /// Optional deterministic size shard for smoke/restartable court runs.
        #[arg(long)]
        size: Option<u32>,
        /// Parallel Quality runs. Defaults conservatively to at most two.
        #[arg(long)]
        workers: Option<usize>,
        /// Zero-based source-group shard. A source group is never split.
        #[arg(long, default_value_t = 0)]
        shard_index: u32,
        #[arg(long, default_value_t = 1)]
        shard_count: u32,
        /// Resume from the digest-bound JSONL checkpoint beside `--out`.
        #[arg(long)]
        resume: bool,
    },
    /// Merge disjoint completed M7 source-group shard reports.
    M7Merge {
        #[arg(long, required = true, num_args = 1..)]
        inputs: Vec<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
    /// Propose the frozen R1 confidence policy from a complete calibration
    /// report without opening the sealed audit.
    M7Calibrate {
        #[arg(long)]
        report: PathBuf,
        #[arg(long)]
        audit_seal: PathBuf,
        #[arg(long)]
        out: PathBuf,
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
        /// Gate paths that ALREADY EXISTED at the base of the push.
        ///
        /// The exemption "a gate that does not exist cannot be weakened" is
        /// about existence, and a commit's own parent is not the right place
        /// to ask: deleting the gate file in one commit and re-adding it
        /// with the code in the next is legal per commit and changes the
        /// frozen value anyway (REVIEW_M3_5 M35-N6). CI passes the gate
        /// paths that exist at `github.event.before`.
        #[arg(long = "existing-gate")]
        existing_gate: Vec<String>,
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
        Cmd::M7Measure {
            out,
            scope,
            size,
            workers,
            shard_index,
            shard_count,
            resume,
        } => {
            let mut request = m7::MeasurementRequest::new(scope.into());
            request.size_filter = size;
            request.workers = workers.unwrap_or_else(m7::default_worker_count);
            request.shard_index = shard_index;
            request.shard_count = shard_count;
            let report = match m7::measure_to_path(request, &out, resume) {
                Ok(report) => report,
                Err(error) => {
                    eprintln!("error: {error}");
                    return 2;
                }
            };
            println!(
                "M7 {} shards {:?}/{}: {} groups, {}/{} renders, {} selected candidates, {} \
                 truncated",
                report.scope,
                report.included_shards,
                report.shard_count,
                report.source_groups,
                report.renders,
                report.expected_renders_included_shards,
                report.candidates_available,
                report.truncated_renders
            );
            println!("M7 raw measurement: {}", out.display());
            0
        }
        Cmd::M7Merge { inputs, out } => {
            let mut reports = Vec::with_capacity(inputs.len());
            for input in &inputs {
                match m7::read_report(input) {
                    Ok(report) => reports.push(report),
                    Err(error) => {
                        eprintln!("error: {error}");
                        return 2;
                    }
                }
            }
            let report = match m7::merge_reports(reports) {
                Ok(report) => report,
                Err(error) => {
                    eprintln!("error: {error}");
                    return 2;
                }
            };
            if let Some(parent) = out.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    eprintln!("error: create {}: {error}", parent.display());
                    return 2;
                }
            }
            if let Err(error) = m7::write_report(&out, &report) {
                eprintln!("error: {error}");
                return 2;
            }
            println!(
                "M7 merged shards {:?}/{}: {}/{} renders; complete={}",
                report.included_shards,
                report.shard_count,
                report.renders,
                report.expected_renders_included_shards,
                report.complete
            );
            0
        }
        Cmd::M7Calibrate {
            report,
            audit_seal,
            out,
        } => {
            let report = match m7::read_report(&report) {
                Ok(report) => report,
                Err(error) => {
                    eprintln!("error: {error}");
                    return 2;
                }
            };
            let audit: AuditSeal = match std::fs::read_to_string(&audit_seal)
                .map_err(|error| error.to_string())
                .and_then(|text| serde_json::from_str(&text).map_err(|error| error.to_string()))
            {
                Ok(audit) => audit,
                Err(error) => {
                    eprintln!("error: read {}: {error}", audit_seal.display());
                    return 2;
                }
            };
            let analysis = match m7::analysis::analyze_calibration(&report, &audit) {
                Ok(analysis) => analysis,
                Err(error) => {
                    eprintln!("error: {error}");
                    return 2;
                }
            };
            if let Some(parent) = out.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    eprintln!("error: create {}: {error}", parent.display());
                    return 2;
                }
            }
            let text = serde_json::to_string_pretty(&analysis).expect("analysis serializes");
            if let Err(error) = std::fs::write(&out, format!("{text}\n")) {
                eprintln!("error: write {}: {error}", out.display());
                return 2;
            }
            println!(
                "M7 calibration: gate_met={}, threshold={:?}, unexplored_upper={}, runtime_p95={}ms",
                analysis.gate_met,
                analysis.selected_threshold,
                analysis.empirical_unexplored_relative_mass_upper_bound,
                analysis.quality_runtime_p95_ms
            );
            for refusal in &analysis.refusals {
                eprintln!("M7 calibration refusal: {refusal}");
            }
            if analysis.gate_met {
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
                if buf.contains(' ') {
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
    }
}
