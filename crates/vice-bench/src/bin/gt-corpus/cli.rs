use super::*;

#[derive(Parser)]
#[command(
    name = "gt-corpus",
    version,
    about = "vice-classic M3: build, verify and report on the GT corpus"
)]
pub(super) struct Cli {
    #[command(subcommand)]
    pub(super) cmd: Cmd,
}

/// How much of the corpus the oracle harness covers. Part of its config
/// hash, hence of every compatibility key it issues (§27.6).
#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub(super) enum OracleScopeArg {
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
pub(super) enum CorridorScopeArg {
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
pub(super) enum TopologyScopeArg {
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
pub(super) enum Scope {
    /// Every cell of the frozen matrix. Minutes of work.
    Full,
    /// Sizes up to 32, without the supersampled box spine.
    Fast,
    /// One size. Seconds; what the unit tests use.
    Test,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub(super) enum M7ScopeArg {
    Smoke,
    CalibrationSmoke,
    Calibration,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub(super) enum M7PresetArg {
    Fast,
    Quality,
}

impl From<M7PresetArg> for vice_core::Preset {
    fn from(value: M7PresetArg) -> Self {
        match value {
            M7PresetArg::Fast => vice_core::Preset::Fast,
            M7PresetArg::Quality => vice_core::Preset::Quality,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub(super) enum M7RoleArg {
    FastParallel,
    FastPrimary,
    FastRepeat,
    QualityParallel,
    QualityPrimary,
    QualityRepeat,
}

impl From<M7RoleArg> for m7::M7RunRole {
    fn from(value: M7RoleArg) -> Self {
        match value {
            M7RoleArg::FastParallel => Self::FastParallel,
            M7RoleArg::FastPrimary => Self::FastPrimary,
            M7RoleArg::FastRepeat => Self::FastRepeat,
            M7RoleArg::QualityParallel => Self::QualityParallel,
            M7RoleArg::QualityPrimary => Self::QualityPrimary,
            M7RoleArg::QualityRepeat => Self::QualityRepeat,
        }
    }
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

#[derive(Args)]
pub(super) struct M7GovernanceArgs {
    /// Canonical runner attestation produced for the exact release commit.
    #[arg(long)]
    pub(super) runner_attestation: PathBuf,
    /// Structured, commit-bound provenance for every frozen M7 gate.
    #[arg(long)]
    pub(super) gate_provenance: PathBuf,
}

#[derive(Subcommand)]
pub(super) enum Cmd {
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
        /// Override the scope default (Smoke=Fast, calibration=Quality).
        #[arg(long, value_enum)]
        preset: Option<M7PresetArg>,
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
        /// Write the compact digest-pinnable production config proposal when
        /// calibration is green.
        #[arg(long)]
        production_config_out: Option<PathBuf>,
    },
    /// Bind the exact clean release commit, Git, gt-corpus, vicec, gates,
    /// and structured gate provenance to an external event/reviewer anchor.
    M7RunnerAttest {
        #[arg(long)]
        anchor_source: String,
        #[arg(long)]
        event_commit: String,
        #[arg(long)]
        repository_root: PathBuf,
        #[arg(long)]
        git_executable: PathBuf,
        #[arg(long)]
        vicec_executable: PathBuf,
        #[arg(long)]
        gates: PathBuf,
        #[arg(long)]
        gate_provenance: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Deliberately open the untouched sealed-audit generation and bind the
    /// act to the current corpus, preregistration, and frozen gates.
    M7AuditOpen {
        #[command(flatten)]
        governance: M7GovernanceArgs,
        #[arg(long)]
        audit_seal: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        gates: PathBuf,
        /// Must name the immutable release-candidate commit.
        #[arg(long)]
        note: String,
    },
    /// Measure the already-opened M7 sealed audit with the digest-pinned
    /// Quality production configuration.
    M7AuditMeasure {
        #[command(flatten)]
        governance: M7GovernanceArgs,
        #[arg(long)]
        audit_seal: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        gates: PathBuf,
        #[arg(long)]
        production_config: PathBuf,
        #[arg(long, value_enum)]
        preset: M7PresetArg,
        /// Typed logical execution role. All shards of one logical run use
        /// the same role and run ID.
        #[arg(long, value_enum)]
        role: M7RoleArg,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 1)]
        workers: usize,
        #[arg(long, default_value_t = 0)]
        shard_index: u32,
        #[arg(long, default_value_t = 1)]
        shard_count: u32,
        #[arg(long)]
        resume: bool,
    },
    /// Apply the frozen M7 release gates to complete Fast and Quality
    /// sealed-audit reports and write the canonical verdict.
    M7AuditAnalyze {
        #[command(flatten)]
        governance: M7GovernanceArgs,
        #[arg(long)]
        audit_seal: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        gates: PathBuf,
        #[arg(long)]
        quality_report: PathBuf,
        #[arg(long)]
        fast_report: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Prove decisions and selected artifact bytes agree across isolated
    /// repeats and supported worker counts for both presets.
    M7Determinism {
        #[command(flatten)]
        governance: M7GovernanceArgs,
        #[arg(long)]
        audit_seal: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        gates: PathBuf,
        #[arg(long)]
        fast_parallel: PathBuf,
        #[arg(long)]
        fast_primary: PathBuf,
        #[arg(long)]
        fast_repeat: PathBuf,
        #[arg(long)]
        quality_parallel: PathBuf,
        #[arg(long)]
        quality_primary: PathBuf,
        #[arg(long)]
        quality_repeat: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Compare production selections to the paired frozen free-chain
    /// baseline and run the randomized identity-blind source-level court.
    M7BaselineCourt {
        #[command(flatten)]
        governance: M7GovernanceArgs,
        #[arg(long)]
        audit_seal: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        gates: PathBuf,
        #[arg(long)]
        quality_report: PathBuf,
        #[arg(long)]
        fast_report: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Rerun the complete PF00/PF10/PF01/PF11 and
    /// G00/G10/G01/G11/G20/G30 courts plus controlled G20/G30 recovery.
    M7Oracle {
        #[command(flatten)]
        governance: M7GovernanceArgs,
        #[arg(long)]
        audit_seal: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        gates: PathBuf,
        #[arg(long)]
        quality_report: PathBuf,
        #[arg(long)]
        fast_report: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Measure the development-only G00..G30 and recovery populations from
    /// which the final M7 gate-only freeze is read.
    M7OracleGeometryCalibrate {
        #[arg(long)]
        out: PathBuf,
    },
    /// Bind the exact green release, baseline/blind, oracle, and determinism
    /// artifacts plus all model/export/renderer identities into the canonical
    /// replay entry point.
    M7CanonicalArtifact {
        #[arg(long)]
        release: PathBuf,
        #[arg(long)]
        baseline: PathBuf,
        #[arg(long)]
        oracle: PathBuf,
        #[arg(long)]
        determinism: PathBuf,
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
