//! Typed error taxonomy (M0 gate: "typed baseline errors").
//!
//! Two layers:
//! - [`BaselineError`]: prepares/builds one baseline (mirror, checkout, build);
//!   failure marks that baseline `failed` in the report and the runner moves on.
//! - [`RunError`]: one (baseline, input, repeat) execution; failure marks that
//!   run record `failed` and the runner moves on.

use serde::Serialize;

use crate::limits::InputError;

/// Stable machine-readable error record embedded in reports.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ErrorRecord {
    pub kind: String,
    pub message: String,
}

/// Errors while preparing or building a single baseline.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BaselineError {
    #[error("mirror directory not found: {path}")]
    MirrorMissing { path: String },
    #[error("pinned commit {sha} not available from mirror {mirror}: {detail}")]
    PinUnavailable {
        sha: String,
        mirror: String,
        detail: String,
    },
    #[error("git {context} failed: {detail}")]
    GitFailed { context: String, detail: String },
    #[error("checkout mismatch: expected {expected}, resolved {actual}")]
    CheckoutMismatch { expected: String, actual: String },
    #[error("build failed with exit code {code:?}; log: {log}")]
    BuildFailed { code: Option<i32>, log: String },
    #[error("build timed out after {seconds}s")]
    BuildTimeout { seconds: u64 },
    #[error("declared binary missing after build: {path}")]
    BinaryMissing { path: String },
    #[error("{count} pinned asset(s) declared (first: {first}) but no --asset-root was given")]
    AssetRootMissing { count: usize, first: String },
    // Field deliberately not named `source`: thiserror reserves that name for
    // an error cause, and this is a filesystem path.
    #[error("pinned asset {path} not found at {from_path}")]
    AssetMissing { path: String, from_path: String },
    #[error("pinned asset {path}: {what} mismatch, expected {expected}, found {actual}")]
    AssetMismatch {
        path: String,
        what: &'static str,
        expected: String,
        actual: String,
    },
    #[error("io error ({context}): {detail}")]
    Io { context: String, detail: String },
}

impl BaselineError {
    pub fn kind(&self) -> &'static str {
        match self {
            BaselineError::MirrorMissing { .. } => "mirror_missing",
            BaselineError::PinUnavailable { .. } => "pin_unavailable",
            BaselineError::GitFailed { .. } => "git_failed",
            BaselineError::CheckoutMismatch { .. } => "checkout_mismatch",
            BaselineError::BuildFailed { .. } => "build_failed",
            BaselineError::BuildTimeout { .. } => "build_timeout",
            BaselineError::BinaryMissing { .. } => "binary_missing",
            BaselineError::AssetRootMissing { .. } => "asset_root_missing",
            BaselineError::AssetMissing { .. } => "asset_missing",
            BaselineError::AssetMismatch { .. } => "asset_mismatch",
            BaselineError::Io { .. } => "io",
        }
    }

    pub fn to_record(&self) -> ErrorRecord {
        ErrorRecord {
            kind: self.kind().to_string(),
            message: self.to_string(),
        }
    }
}

/// Errors for one (baseline, input, repeat) execution.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RunError {
    #[error("input rejected: {0}")]
    InputRejected(#[from] InputError),
    #[error("run timed out after {seconds}s")]
    Timeout { seconds: u64 },
    #[error("spawn failed: {0}")]
    SpawnFailed(String),
    #[error("nonzero exit code {code:?}")]
    NonZeroExit { code: Option<i32> },
    #[error("declared output missing: {path}")]
    OutputMissing { path: String },
    #[error("output too large: {bytes} bytes > limit {max_bytes}")]
    OutputTooLarge { bytes: u64, max_bytes: u64 },
    #[error("io error: {0}")]
    Io(String),
}

impl RunError {
    pub fn kind(&self) -> &'static str {
        match self {
            RunError::InputRejected(_) => "input_rejected",
            RunError::Timeout { .. } => "timeout",
            RunError::SpawnFailed(_) => "spawn_failed",
            RunError::NonZeroExit { .. } => "nonzero_exit",
            RunError::OutputMissing { .. } => "output_missing",
            RunError::OutputTooLarge { .. } => "output_too_large",
            RunError::Io(_) => "io",
        }
    }

    pub fn to_record(&self) -> ErrorRecord {
        ErrorRecord {
            kind: self.kind().to_string(),
            message: self.to_string(),
        }
    }
}

/// Fatal errors of the runner itself (config/corpus preconditions).
/// These abort the whole run with a nonzero exit code, because a report
/// produced against an unverified corpus or unparsable config is worthless.
#[derive(Debug, thiserror::Error)]
pub enum TopError {
    #[error("config error: {0}")]
    Config(String),
    #[error("corpus integrity failure:\n{0}")]
    CorpusIntegrity(String),
    #[error("io error ({context}): {detail}")]
    Io { context: String, detail: String },
}
