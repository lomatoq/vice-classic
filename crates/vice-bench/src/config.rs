//! Baseline run configuration (`configs/baselines.toml`).
//!
//! Everything is explicit: no hidden env flags, no defaults that change
//! behaviour silently (spec v1.3 §32 rule 4).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::assets::AssetSpec;
use crate::error::TopError;
use crate::limits::ResourceLimits;

pub const CONFIG_SCHEMA: &str = "vice-classic/baselines/v1";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselinesConfig {
    pub schema: String,
    #[serde(default)]
    pub limits: ResourceLimits,
    #[serde(rename = "baseline", default)]
    pub baselines: Vec<BaselineSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineKind {
    /// Rust donor: git checkout of the pin, `cargo build`, run built binary.
    Rust,
    /// Python donor: git checkout of the pin, no build, run via interpreter.
    Python,
    /// Internal copy adapter (selftest only; constructed in code, not config).
    BuiltinCopy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunCwd {
    /// Run with cwd = per-(input,repeat) work directory (default).
    Workdir,
    /// Run with cwd = pinned checkout (for tools relying on relative paths).
    Checkout,
}

fn default_run_cwd() -> RunCwd {
    RunCwd::Workdir
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineSpec {
    pub name: String,
    /// Upstream identity, e.g. "lomatoq/v-ice". Informational.
    pub repo: String,
    /// Pinned commit SHA. The runner refuses to report a checkout that does
    /// not resolve to exactly this commit.
    pub pin_sha: String,
    /// Directory name of the local mirror under --mirror-root.
    pub mirror_hint: String,
    pub kind: BaselineKind,
    /// Build command tokens; empty = no build step. Vars: {checkout}.
    #[serde(default)]
    pub build: Vec<String>,
    /// Built binary path relative to checkout (platform exe suffix is
    /// appended automatically if the plain path does not exist).
    #[serde(default)]
    pub binary: Option<String>,
    /// Run command tokens. Vars: {binary} {input} {out_dir} {stem} {checkout}.
    pub run: Vec<String>,
    /// Declared primary outputs, relative to {out_dir}. Vars: {stem}.
    /// These define the baseline's primary determinism verdict; any other
    /// files written under {out_dir} are recorded and compared as secondary
    /// artifacts.
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default = "default_run_cwd")]
    pub run_cwd: RunCwd,
    /// Out-of-tree files the pinned checkout needs but does not carry, each
    /// pinned by sha256 and length and staged from `--asset-root`
    /// (see [`crate::assets`]; M0 blocker B2).
    #[serde(rename = "asset", default)]
    pub assets: Vec<AssetSpec>,
    /// Free-form documented caveats (e.g. known nondeterministic side files).
    #[serde(default)]
    pub notes: String,
}

pub fn load_config(path: &Path) -> Result<BaselinesConfig, TopError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| TopError::Config(format!("cannot read {}: {e}", path.display())))?;
    let cfg: BaselinesConfig = toml::from_str(&text)
        .map_err(|e| TopError::Config(format!("cannot parse {}: {e}", path.display())))?;
    if cfg.schema != CONFIG_SCHEMA {
        return Err(TopError::Config(format!(
            "unsupported config schema {:?}, expected {:?}",
            cfg.schema, CONFIG_SCHEMA
        )));
    }
    let mut seen = std::collections::BTreeSet::new();
    for b in &cfg.baselines {
        if !seen.insert(b.name.clone()) {
            return Err(TopError::Config(format!(
                "duplicate baseline name {:?}",
                b.name
            )));
        }
        if b.run.is_empty() {
            return Err(TopError::Config(format!(
                "baseline {:?} has empty run command",
                b.name
            )));
        }
        if b.pin_sha.len() != 40 || !b.pin_sha.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(TopError::Config(format!(
                "baseline {:?} pin_sha is not a 40-hex commit sha",
                b.name
            )));
        }
        for a in &b.assets {
            crate::assets::validate_spec(&b.name, a).map_err(TopError::Config)?;
        }
    }
    Ok(cfg)
}

/// Replace `{key}` placeholders in every token. Unknown placeholders are left
/// as-is (and will fail loudly at spawn/output-check time rather than being
/// silently dropped).
pub fn substitute_tokens(tokens: &[String], vars: &BTreeMap<&'static str, String>) -> Vec<String> {
    tokens
        .iter()
        .map(|t| {
            let mut s = t.clone();
            for (k, v) in vars {
                s = s.replace(&format!("{{{k}}}"), v);
            }
            s
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitute_replaces_all_occurrences() {
        let vars = BTreeMap::from([("stem", "logo".to_string()), ("out_dir", "o".to_string())]);
        let toks = vec![
            "{out_dir}/{stem}/{stem}.svg".to_string(),
            "plain".to_string(),
        ];
        assert_eq!(
            substitute_tokens(&toks, &vars),
            vec!["o/logo/logo.svg".to_string(), "plain".to_string()]
        );
    }

    #[test]
    fn config_parses_and_validates() {
        let text = r#"
schema = "vice-classic/baselines/v1"

[limits]
max_input_bytes = 1000
max_png_dimension = 64
run_timeout_secs = 5
build_timeout_secs = 10
max_output_bytes = 1000

[[baseline]]
name = "x"
repo = "a/b"
pin_sha = "0123456789abcdef0123456789abcdef01234567"
mirror_hint = "b"
kind = "rust"
build = ["cargo", "build"]
binary = "target/release/x"
run = ["{binary}", "{input}"]
outputs = ["{stem}.svg"]
"#;
        let cfg: BaselinesConfig = toml::from_str(text).unwrap();
        assert_eq!(cfg.baselines.len(), 1);
        assert_eq!(cfg.baselines[0].run_cwd, RunCwd::Workdir);
        assert_eq!(cfg.limits.max_png_dimension, 64);
    }

    #[test]
    fn config_rejects_bad_sha() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("b.toml");
        std::fs::write(
            &p,
            r#"
schema = "vice-classic/baselines/v1"
[[baseline]]
name = "x"
repo = "a/b"
pin_sha = "notasha"
mirror_hint = "b"
kind = "rust"
run = ["x"]
"#,
        )
        .unwrap();
        assert!(load_config(&p).is_err());
    }
}
