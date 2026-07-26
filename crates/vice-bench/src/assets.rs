//! Explicit asset pin for baselines whose pinned checkout is not
//! self-contained (M0 blocker B2 / FAILURE_LEDGER F-0003, due before M3).
//!
//! The `Vice-` pin does not carry `models/corner_rf.joblib`: upstream
//! gitignores `models/`, and the file is 104 MiB, so a clean checkout of the
//! pin cannot execute the flagship path for two of the five smoke inputs.
//! M0 recorded that honestly as `output_missing` and REVIEW_M0 required a
//! reviewed resolution before M3, when baselines enter the scorecard
//! (spec §27.3).
//!
//! What is NOT allowed (F-0003's standing rule): quietly copying untracked
//! files from a working mirror into the checkout. That would make the run
//! depend on unrecorded state — a provenance hole disguised as a green
//! baseline.
//!
//! What this module does instead: the config DECLARES every out-of-tree
//! asset with its sha256 and byte length. The runner stages declared assets
//! from an explicit `--asset-root`, verifying length and hash BEFORE the
//! copy, and records what it staged in the report and in the normative
//! hashes file. The asset is therefore pinned exactly as strongly as the
//! commit: a different file is a typed refusal, not a different result.
//!
//! Failure is typed and isolated per baseline, like every other stage: a
//! missing asset root or a hash mismatch fails THAT baseline and leaves the
//! others' reports intact.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::BaselineError;
use crate::hashing::sha256_file;

/// One declared out-of-tree asset of a baseline.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssetSpec {
    /// Destination path relative to the pinned checkout root, '/'-separated.
    pub path: String,
    /// Path relative to `<asset_root>/<mirror_hint>`; defaults to `path`.
    #[serde(default)]
    pub source: Option<String>,
    pub sha256: String,
    pub bytes: u64,
    /// Why this file cannot come from the pin itself.
    #[serde(default)]
    pub notes: String,
}

/// What was actually staged, for the report and the hashes file.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AssetRecord {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Reject an asset declaration that cannot be checked or that could escape
/// the checkout. Called at config load so a malformed pin is a hard config
/// error, not a surprise mid-run.
pub fn validate_spec(baseline: &str, a: &AssetSpec) -> Result<(), String> {
    if a.sha256.len() != 64 || !a.sha256.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "baseline {baseline:?} asset {:?}: sha256 is not a 64-hex digest",
            a.path
        ));
    }
    if a.bytes == 0 {
        return Err(format!(
            "baseline {baseline:?} asset {:?}: bytes must be > 0",
            a.path
        ));
    }
    for (field, value) in [("path", Some(&a.path)), ("source", a.source.as_ref())] {
        let Some(value) = value else { continue };
        relative_segments(value)
            .map_err(|why| format!("baseline {baseline:?} asset {field} {value:?}: {why}"))?;
    }
    Ok(())
}

/// Split a declared asset path into path segments, refusing anything that
/// could address a file outside the tree it is joined to.
///
/// The check is on the STRING, with both separators treated as separators,
/// and deliberately not on `std::path::Path`: `Path` semantics are
/// platform-dependent in both directions, so a `Path`-based check passes
/// different declarations on different hosts. Measured, not assumed —
/// `"/abs/x"` is `is_absolute() == false` on Windows, and `"a\\..\\b"` is a
/// single component with no `ParentDir` on Linux. Either would have been a
/// hole on exactly one platform.
fn relative_segments(value: &str) -> Result<Vec<&str>, String> {
    if value.is_empty() {
        return Err("must not be empty".to_string());
    }
    let mut segments = Vec::new();
    for seg in value.split(['/', '\\']) {
        match seg {
            "" => return Err("must not contain an empty path segment or a root".to_string()),
            "." | ".." => return Err("must not contain '.' or '..' segments".to_string()),
            s if s.contains(':') => {
                return Err("must not contain ':' (drive or stream qualifier)".to_string())
            }
            s => segments.push(s),
        }
    }
    Ok(segments)
}

/// Join a validated relative declaration onto a base, one segment at a time
/// so the platform's own parsing never sees the raw string.
fn join_relative(base: &Path, value: &str) -> Result<PathBuf, String> {
    let mut out = base.to_path_buf();
    for seg in relative_segments(value)? {
        out.push(seg);
    }
    Ok(out)
}

/// Stage every declared asset into the checkout.
///
/// Order matters: length and hash are verified on the SOURCE before any
/// bytes are written, so a mismatched asset never lands in the checkout at
/// all. The length check runs first because it is the cheap discriminator
/// and gives a far more readable error for the common "wrong file" case
/// than a hash difference does.
pub fn stage(
    specs: &[AssetSpec],
    asset_dir: Option<&Path>,
    checkout: &Path,
) -> Result<Vec<AssetRecord>, BaselineError> {
    if specs.is_empty() {
        return Ok(Vec::new());
    }
    let Some(asset_dir) = asset_dir else {
        return Err(BaselineError::AssetRootMissing {
            count: specs.len(),
            first: specs[0].path.clone(),
        });
    };
    let mut out = Vec::with_capacity(specs.len());
    for a in specs {
        let declared_source = a.source.as_ref().unwrap_or(&a.path);
        let src = join_relative(asset_dir, declared_source).map_err(|why| {
            BaselineError::AssetMismatch {
                path: a.path.clone(),
                what: "declaration",
                expected: "a relative in-tree path".to_string(),
                actual: format!("{declared_source:?} ({why})"),
            }
        })?;
        if !src.is_file() {
            return Err(BaselineError::AssetMissing {
                path: a.path.clone(),
                from_path: src.display().to_string(),
            });
        }
        let bytes = std::fs::metadata(&src)
            .map_err(|e| BaselineError::Io {
                context: format!("stat asset {}", src.display()),
                detail: e.to_string(),
            })?
            .len();
        if bytes != a.bytes {
            return Err(BaselineError::AssetMismatch {
                path: a.path.clone(),
                what: "bytes",
                expected: a.bytes.to_string(),
                actual: bytes.to_string(),
            });
        }
        let digest = sha256_file(&src).map_err(|e| BaselineError::Io {
            context: format!("hash asset {}", src.display()),
            detail: e.to_string(),
        })?;
        if digest != a.sha256 {
            return Err(BaselineError::AssetMismatch {
                path: a.path.clone(),
                what: "sha256",
                expected: a.sha256.clone(),
                actual: digest,
            });
        }
        let dst = join_relative(checkout, &a.path).map_err(|why| BaselineError::AssetMismatch {
            path: a.path.clone(),
            what: "declaration",
            expected: "a relative in-tree path".to_string(),
            actual: format!("{:?} ({why})", a.path),
        })?;
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BaselineError::Io {
                context: format!("create asset dir {}", parent.display()),
                detail: e.to_string(),
            })?;
        }
        std::fs::copy(&src, &dst).map_err(|e| BaselineError::Io {
            context: format!("stage asset {} -> {}", src.display(), dst.display()),
            detail: e.to_string(),
        })?;
        out.push(AssetRecord {
            path: a.path.clone(),
            sha256: digest,
            bytes,
        });
    }
    out.sort_by(|x, y| x.path.cmp(&y.path));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hashing::sha256_hex;

    fn spec_for(body: &[u8], path: &str) -> AssetSpec {
        AssetSpec {
            path: path.to_string(),
            source: None,
            sha256: sha256_hex(body),
            bytes: body.len() as u64,
            notes: String::new(),
        }
    }

    #[test]
    fn stage_copies_a_matching_asset_and_records_it() {
        let src_dir = tempfile::tempdir().unwrap();
        let checkout = tempfile::tempdir().unwrap();
        let body = b"pinned model bytes";
        std::fs::create_dir_all(src_dir.path().join("models")).unwrap();
        std::fs::write(src_dir.path().join("models/m.bin"), body).unwrap();

        let specs = vec![spec_for(body, "models/m.bin")];
        let recs = stage(&specs, Some(src_dir.path()), checkout.path()).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].bytes, body.len() as u64);
        assert_eq!(
            std::fs::read(checkout.path().join("models/m.bin")).unwrap(),
            body
        );
    }

    #[test]
    fn a_wrong_asset_is_a_typed_refusal_and_is_never_written() {
        let src_dir = tempfile::tempdir().unwrap();
        let checkout = tempfile::tempdir().unwrap();
        // Same length, different content: only the hash can tell them apart,
        // which is the case the length check must NOT be able to absorb.
        std::fs::write(src_dir.path().join("m.bin"), b"AAAAAAAA").unwrap();
        let specs = vec![spec_for(b"BBBBBBBB", "m.bin")];

        let err = stage(&specs, Some(src_dir.path()), checkout.path()).unwrap_err();
        assert_eq!(err.kind(), "asset_mismatch");
        assert!(format!("{err}").contains("sha256"));
        assert!(
            !checkout.path().join("m.bin").exists(),
            "a mismatched asset must not reach the checkout"
        );
    }

    #[test]
    fn a_truncated_asset_is_named_by_length_not_by_hash() {
        let src_dir = tempfile::tempdir().unwrap();
        let checkout = tempfile::tempdir().unwrap();
        std::fs::write(src_dir.path().join("m.bin"), b"AAAA").unwrap();
        let specs = vec![spec_for(b"AAAAAAAA", "m.bin")];
        let err = stage(&specs, Some(src_dir.path()), checkout.path()).unwrap_err();
        assert!(format!("{err}").contains("bytes"), "{err}");
    }

    #[test]
    fn declared_assets_without_an_asset_root_are_a_typed_refusal() {
        let checkout = tempfile::tempdir().unwrap();
        let specs = vec![spec_for(b"x", "models/m.bin")];
        let err = stage(&specs, None, checkout.path()).unwrap_err();
        assert_eq!(err.kind(), "asset_root_missing");
    }

    #[test]
    fn a_missing_source_names_the_path_it_looked_for() {
        let src_dir = tempfile::tempdir().unwrap();
        let checkout = tempfile::tempdir().unwrap();
        let specs = vec![spec_for(b"x", "models/m.bin")];
        let err = stage(&specs, Some(src_dir.path()), checkout.path()).unwrap_err();
        assert_eq!(err.kind(), "asset_missing");
        assert!(format!("{err}").contains("models"));
    }

    #[test]
    fn no_declared_assets_means_no_asset_root_is_required() {
        let checkout = tempfile::tempdir().unwrap();
        assert!(stage(&[], None, checkout.path()).unwrap().is_empty());
    }

    #[test]
    fn escaping_declarations_are_rejected_at_validation_time() {
        let ok = spec_for(b"x", "models/m.bin");
        assert!(validate_spec("b", &ok).is_ok());

        // Both separators and both platforms' notions of "escaping" are in
        // the class, not just the ones this host would have caught.
        const ESCAPING: [&str; 9] = [
            "../outside.bin",
            "..\\outside.bin",
            "a/../../outside.bin",
            "a\\..\\..\\outside.bin",
            "/abs/x.bin",
            "\\abs\\x.bin",
            "C:/x.bin",
            "models//m.bin",
            "",
        ];
        for bad_path in ESCAPING {
            let mut s = ok.clone();
            s.path = bad_path.to_string();
            assert!(
                validate_spec("b", &s).is_err(),
                "path {bad_path:?} must be rejected"
            );
        }
        // The `source` side is the same class and is checked with the same
        // rule, not merely at the destination (M-1: class, not enumeration).
        for bad_source in ESCAPING {
            let mut s = ok.clone();
            s.source = Some(bad_source.to_string());
            assert!(
                validate_spec("b", &s).is_err(),
                "source {bad_source:?} must be rejected"
            );
        }
        // And the refusal holds at STAGING time too, not only at config
        // load: `stage` is public and a caller could build specs directly.
        let dirs = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let mut escaping = ok.clone();
        escaping.path = "../outside.bin".to_string();
        let err = stage(&[escaping], Some(dirs.0.path()), dirs.1.path()).unwrap_err();
        assert_eq!(err.kind(), "asset_mismatch");
        let mut short_hash = ok.clone();
        short_hash.sha256 = "abc".into();
        assert!(validate_spec("b", &short_hash).is_err());
        let mut zero = ok.clone();
        zero.bytes = 0;
        assert!(validate_spec("b", &zero).is_err());
    }
}
