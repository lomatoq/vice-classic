//! Smoke corpus manifest and integrity verification.
//!
//! The corpus is fixed: every file is listed with its SHA-256 and dimensions
//! in `SMOKE_MANIFEST.toml`. A baseline run refuses to start on a corpus that
//! does not verify (missing, extra, mismatched or invalid files), because a
//! report against a drifted corpus is not comparable.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::hashing::sha256_file;
use crate::limits::{validate_png_file, ResourceLimits};

pub const CORPUS_SCHEMA: &str = "vice-classic/smoke-corpus/v1";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusManifest {
    pub schema: String,
    #[serde(rename = "file")]
    pub files: Vec<CorpusFile>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusFile {
    pub name: String,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FileStatus {
    Ok,
    Missing,
    HashMismatch {
        expected: String,
        actual: String,
    },
    DimensionMismatch {
        expected_w: u32,
        expected_h: u32,
        actual_w: u32,
        actual_h: u32,
    },
    Invalid {
        error: String,
    },
    NotInManifest,
}

pub fn load_manifest(path: &Path) -> Result<CorpusManifest, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let m: CorpusManifest =
        toml::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
    if m.schema != CORPUS_SCHEMA {
        return Err(format!(
            "unsupported corpus schema {:?}, expected {:?}",
            m.schema, CORPUS_SCHEMA
        ));
    }
    if m.files.is_empty() {
        return Err("corpus manifest lists no files".to_string());
    }
    Ok(m)
}

pub fn verify(
    corpus_dir: &Path,
    manifest: &CorpusManifest,
    limits: &ResourceLimits,
) -> BTreeMap<String, FileStatus> {
    let mut out = BTreeMap::new();
    for f in &manifest.files {
        let p = corpus_dir.join(&f.name);
        if !p.is_file() {
            out.insert(f.name.clone(), FileStatus::Missing);
            continue;
        }
        let info = match validate_png_file(&p, limits) {
            Ok(i) => i,
            Err(e) => {
                out.insert(
                    f.name.clone(),
                    FileStatus::Invalid {
                        error: e.to_string(),
                    },
                );
                continue;
            }
        };
        if (info.width, info.height) != (f.width, f.height) {
            out.insert(
                f.name.clone(),
                FileStatus::DimensionMismatch {
                    expected_w: f.width,
                    expected_h: f.height,
                    actual_w: info.width,
                    actual_h: info.height,
                },
            );
            continue;
        }
        let status = match sha256_file(&p) {
            Ok(h) if h == f.sha256 => FileStatus::Ok,
            Ok(h) => FileStatus::HashMismatch {
                expected: f.sha256.clone(),
                actual: h,
            },
            Err(e) => FileStatus::Invalid {
                error: e.to_string(),
            },
        };
        out.insert(f.name.clone(), status);
    }
    // Unlisted PNGs are corpus drift, not silently ignored.
    if let Ok(rd) = std::fs::read_dir(corpus_dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.to_ascii_lowercase().ends_with(".png")
                && !manifest.files.iter().any(|f| f.name == name)
            {
                out.insert(name, FileStatus::NotInManifest);
            }
        }
    }
    out
}

pub fn all_ok(statuses: &BTreeMap<String, FileStatus>) -> bool {
    statuses.values().all(|s| *s == FileStatus::Ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hashing::sha256_hex;

    fn tiny_png() -> Vec<u8> {
        // Minimal valid-header PNG prefix is enough for verify() paths that
        // only parse the header; but hash comparison uses full bytes, so any
        // fixed byte string with a valid header works for tests.
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&8u32.to_be_bytes());
        v.extend_from_slice(&8u32.to_be_bytes());
        v.extend_from_slice(&[8, 2, 0, 0, 0]);
        v
    }

    #[test]
    fn verify_detects_ok_mismatch_missing_and_extra() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = tiny_png();
        std::fs::write(dir.path().join("a.png"), &bytes).unwrap();
        std::fs::write(dir.path().join("b.png"), &bytes).unwrap();
        std::fs::write(dir.path().join("extra.png"), &bytes).unwrap();

        let manifest = CorpusManifest {
            schema: CORPUS_SCHEMA.to_string(),
            files: vec![
                CorpusFile {
                    name: "a.png".into(),
                    sha256: sha256_hex(&bytes),
                    width: 8,
                    height: 8,
                    description: String::new(),
                },
                CorpusFile {
                    name: "b.png".into(),
                    sha256: "0".repeat(64),
                    width: 8,
                    height: 8,
                    description: String::new(),
                },
                CorpusFile {
                    name: "gone.png".into(),
                    sha256: "0".repeat(64),
                    width: 8,
                    height: 8,
                    description: String::new(),
                },
            ],
        };
        let statuses = verify(dir.path(), &manifest, &ResourceLimits::default());
        assert_eq!(statuses["a.png"], FileStatus::Ok);
        assert!(matches!(statuses["b.png"], FileStatus::HashMismatch { .. }));
        assert_eq!(statuses["gone.png"], FileStatus::Missing);
        assert_eq!(statuses["extra.png"], FileStatus::NotInManifest);
        assert!(!all_ok(&statuses));
    }
}
