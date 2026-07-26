//! Baseline orchestration.
//!
//! For every configured baseline:
//!   fresh `git clone --local --no-checkout` from the local mirror
//!   → `git checkout --detach <pin_sha>` (verified against the pin)
//!   → optional build (timeout-bounded, logged, binary hashed)
//!   → run over every corpus input `repeats` times in fresh work dirs
//!   → hash every produced artifact, compare across repeats.
//!
//! A typed failure in any stage marks THAT baseline `failed` and the runner
//! continues with the next one (M0 gate: one baseline's failure must not
//! break the others' reports).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::{
    load_config, substitute_tokens, BaselineKind, BaselineSpec, BaselinesConfig, RunCwd,
    CONFIG_SCHEMA,
};
use crate::corpus::{self, CorpusManifest};
use crate::envinfo;
use crate::error::{BaselineError, RunError, TopError};
use crate::exec::run_with_timeout;
use crate::fsutil::{
    absolute, collect_artifacts, force_remove_dir_all, git_ok, git_stdout, rel_display,
};
use crate::hashing::{sha256_file, sha256_hex};
use crate::limits::{validate_png_file, ResourceLimits};
use crate::report::{
    hashes_from_report, BaselineReport, BuildRecord, CorpusEntry, DeterminismMismatch,
    DeterminismRecord, RunRecord, RunReport, ToolInfo, REPORT_SCHEMA,
};

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub config_path: PathBuf,
    pub corpus_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub mirror_root: PathBuf,
    pub out_dir: PathBuf,
    /// Empty = all baselines from the config.
    pub baseline_filter: Vec<String>,
    pub repeats: u32,
    pub skip_build: bool,
}

/// Load config from file and execute. Writes `report.json`, `hashes.json`
/// and `env.json` into `out_dir`.
pub fn run(opts: &RunOptions) -> Result<RunReport, TopError> {
    let cfg = load_config(&opts.config_path)?;
    for f in &opts.baseline_filter {
        if !cfg.baselines.iter().any(|b| &b.name == f) {
            return Err(TopError::Config(format!(
                "unknown baseline {f:?} in --baseline"
            )));
        }
    }
    let config_sha256 = sha256_file(&opts.config_path).map_err(|e| TopError::Io {
        context: "hash config".into(),
        detail: e.to_string(),
    })?;
    run_with_config(
        &cfg,
        opts.config_path.display().to_string(),
        config_sha256,
        opts,
    )
}

/// Execute with an in-memory config (also used by `selftest`).
pub fn run_with_config(
    cfg: &BaselinesConfig,
    config_desc: String,
    config_sha256: String,
    opts: &RunOptions,
) -> Result<RunReport, TopError> {
    // Normalize every directory to an absolute path once. Git subprocesses
    // run with varying cwds; relative inputs would otherwise resolve against
    // the wrong base (observed as os error 267 on Windows).
    let opts = &RunOptions {
        config_path: opts.config_path.clone(),
        corpus_dir: absolute(&opts.corpus_dir),
        manifest_path: absolute(&opts.manifest_path),
        mirror_root: absolute(&opts.mirror_root),
        out_dir: absolute(&opts.out_dir),
        baseline_filter: opts.baseline_filter.clone(),
        repeats: opts.repeats,
        skip_build: opts.skip_build,
    };
    let manifest = corpus::load_manifest(&opts.manifest_path).map_err(TopError::CorpusIntegrity)?;
    let statuses = corpus::verify(&opts.corpus_dir, &manifest, &cfg.limits);
    if !corpus::all_ok(&statuses) {
        let mut lines = String::new();
        for (name, st) in &statuses {
            if *st != corpus::FileStatus::Ok {
                lines.push_str(&format!("  {name}: {st:?}\n"));
            }
        }
        return Err(TopError::CorpusIntegrity(lines));
    }
    std::fs::create_dir_all(&opts.out_dir).map_err(|e| TopError::Io {
        context: format!("create out dir {}", opts.out_dir.display()),
        detail: e.to_string(),
    })?;

    let env = envinfo::collect();
    let env_json = envinfo::canonical_json(&env);
    let environment_sha256 = sha256_hex(env_json.as_bytes());
    let exe_sha256 = std::env::current_exe()
        .ok()
        .and_then(|p| sha256_file(&p).ok());

    let corpus_entries: Vec<CorpusEntry> = manifest
        .files
        .iter()
        .map(|f| CorpusEntry {
            name: f.name.clone(),
            sha256: f.sha256.clone(),
            width: f.width,
            height: f.height,
        })
        .collect();

    let mut baselines = Vec::new();
    for spec in &cfg.baselines {
        if !opts.baseline_filter.is_empty() && !opts.baseline_filter.contains(&spec.name) {
            continue;
        }
        let ctx = BaselineCtx {
            spec,
            limits: &cfg.limits,
            manifest: &manifest,
            opts,
        };
        baselines.push(ctx.execute());
    }

    let report = RunReport {
        schema: REPORT_SCHEMA.to_string(),
        tool: ToolInfo {
            name: "vice-bench baseline-runner".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            exe_sha256,
        },
        config: config_desc,
        config_sha256,
        environment: env,
        environment_sha256,
        repeats: opts.repeats,
        corpus: corpus_entries,
        baselines,
    };

    write_json(&opts.out_dir.join("report.json"), &report)?;
    write_json(
        &opts.out_dir.join("hashes.json"),
        &hashes_from_report(&report),
    )?;
    std::fs::write(opts.out_dir.join("env.json"), format!("{env_json}\n")).map_err(|e| {
        TopError::Io {
            context: "write env.json".into(),
            detail: e.to_string(),
        }
    })?;
    Ok(report)
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), TopError> {
    let text = serde_json::to_string_pretty(value).map_err(|e| TopError::Io {
        context: format!("serialize {}", path.display()),
        detail: e.to_string(),
    })?;
    std::fs::write(path, format!("{text}\n")).map_err(|e| TopError::Io {
        context: format!("write {}", path.display()),
        detail: e.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Per-baseline execution
// ---------------------------------------------------------------------------

struct BaselineCtx<'a> {
    spec: &'a BaselineSpec,
    limits: &'a ResourceLimits,
    manifest: &'a CorpusManifest,
    opts: &'a RunOptions,
}

impl BaselineCtx<'_> {
    fn execute(&self) -> BaselineReport {
        let mut rep = BaselineReport {
            name: self.spec.name.clone(),
            repo: self.spec.repo.clone(),
            pin_sha: self.spec.pin_sha.clone(),
            mirror: String::new(),
            status: "failed".to_string(),
            error: None,
            resolved_sha: None,
            build: None,
            notes: self.spec.notes.clone(),
            runs: Vec::new(),
            determinism: None,
        };
        match self.prepare_and_run(&mut rep) {
            Ok(()) => rep.status = "completed".to_string(),
            Err(e) => {
                rep.error = Some(e.to_record());
                rep.status = "failed".to_string();
            }
        }
        rep
    }

    fn prepare_and_run(&self, rep: &mut BaselineReport) -> Result<(), BaselineError> {
        let (checkout, binary): (Option<PathBuf>, Option<PathBuf>) = match self.spec.kind {
            BaselineKind::BuiltinCopy => {
                rep.mirror = "internal".to_string();
                // No git provenance for the internal adapter; resolved_sha
                // stays None rather than echoing a fake pin.
                let exe = std::env::current_exe().map_err(|e| BaselineError::Io {
                    context: "current_exe".into(),
                    detail: e.to_string(),
                })?;
                (None, Some(exe))
            }
            BaselineKind::Rust | BaselineKind::Python => {
                let mirror = self.opts.mirror_root.join(&self.spec.mirror_hint);
                rep.mirror = mirror.display().to_string();
                if !mirror.is_dir() {
                    return Err(BaselineError::MirrorMissing {
                        path: mirror.display().to_string(),
                    });
                }
                let checkout_dir = self.opts.out_dir.join("checkouts").join(&self.spec.name);
                self.fresh_checkout(&mirror, &checkout_dir, rep)?;
                let binary = if !self.spec.build.is_empty() && !self.opts.skip_build {
                    self.run_build(&checkout_dir, rep)?
                } else {
                    self.resolve_binary(&checkout_dir)?
                };
                (Some(checkout_dir), binary)
            }
        };
        self.execute_runs(checkout.as_deref(), binary.as_deref(), rep);
        self.compute_determinism(rep);
        Ok(())
    }

    fn fresh_checkout(
        &self,
        mirror: &Path,
        dst: &Path,
        rep: &mut BaselineReport,
    ) -> Result<(), BaselineError> {
        let pin = &self.spec.pin_sha;
        // Precise error when the pin is absent from the mirror.
        let probe = crate::exec::sanitized_command("git")
            .args(["cat-file", "-t", pin])
            .current_dir(mirror)
            .output()
            .map_err(|e| BaselineError::GitFailed {
                context: "cat-file".into(),
                detail: e.to_string(),
            })?;
        let obj_type = String::from_utf8_lossy(&probe.stdout).trim().to_string();
        if !probe.status.success() || obj_type != "commit" {
            return Err(BaselineError::PinUnavailable {
                sha: pin.clone(),
                mirror: mirror.display().to_string(),
                detail: if probe.status.success() {
                    format!("object type {obj_type:?}")
                } else {
                    String::from_utf8_lossy(&probe.stderr).trim().to_string()
                },
            });
        }

        if dst.exists() {
            force_remove_dir_all(dst).map_err(|e| BaselineError::Io {
                context: format!("remove old checkout {}", dst.display()),
                detail: e.to_string(),
            })?;
        }
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).map_err(|e| BaselineError::Io {
                context: "create checkouts dir".into(),
                detail: e.to_string(),
            })?;
        }
        // core.longpaths=true: donor pins contain paths deep enough that a
        // long checkout base breaks the Windows MAX_PATH limit (FAILURE_LEDGER
        // F-0004). Harmless no-op on non-Windows git.
        git_ok(
            &[
                "-c",
                "core.longpaths=true",
                "clone",
                "--local",
                "--no-checkout",
                &mirror.display().to_string(),
                &dst.display().to_string(),
            ],
            self.opts.out_dir.as_path(),
        )?;
        git_ok(
            &["-c", "core.longpaths=true", "checkout", "--detach", pin],
            dst,
        )?;
        let resolved = git_stdout(&["rev-parse", "HEAD"], dst)?;
        rep.resolved_sha = Some(resolved.clone());
        if resolved != *pin {
            return Err(BaselineError::CheckoutMismatch {
                expected: pin.clone(),
                actual: resolved,
            });
        }
        Ok(())
    }

    fn run_build(
        &self,
        checkout: &Path,
        rep: &mut BaselineReport,
    ) -> Result<Option<PathBuf>, BaselineError> {
        let vars = BTreeMap::from([("checkout", checkout.display().to_string())]);
        let cmd = substitute_tokens(&self.spec.build, &vars);
        let log = self
            .opts
            .out_dir
            .join("logs")
            .join(&self.spec.name)
            .join("build.log");
        let outcome = run_with_timeout(
            &cmd,
            checkout,
            Duration::from_secs(self.limits.build_timeout_secs),
            &log,
        )
        .map_err(|e| BaselineError::Io {
            context: "spawn build".into(),
            detail: e,
        })?;

        let record = BuildRecord {
            command: cmd,
            exit_code: outcome.exit_code,
            timed_out: outcome.timed_out,
            duration_ms: outcome.duration_ms,
            binary_path: None,
            binary_sha256: None,
            log: rel_display(&log, &self.opts.out_dir),
        };

        if outcome.timed_out {
            rep.build = Some(record);
            return Err(BaselineError::BuildTimeout {
                seconds: self.limits.build_timeout_secs,
            });
        }
        if outcome.exit_code != Some(0) {
            let log_str = record.log.clone();
            rep.build = Some(record);
            return Err(BaselineError::BuildFailed {
                code: outcome.exit_code,
                log: log_str,
            });
        }
        // Record the successful build before binary resolution so a
        // BinaryMissing error still leaves the build evidence in the report.
        rep.build = Some(record);
        let binary = self.resolve_binary(checkout)?;
        if let Some(b) = &binary {
            if let Some(rec) = rep.build.as_mut() {
                rec.binary_path = Some(b.display().to_string());
                rec.binary_sha256 = sha256_file(b).ok();
            }
        }
        Ok(binary)
    }

    fn resolve_binary(&self, checkout: &Path) -> Result<Option<PathBuf>, BaselineError> {
        let Some(rel) = &self.spec.binary else {
            return Ok(None);
        };
        let plain = checkout.join(rel);
        if plain.is_file() {
            return Ok(Some(plain));
        }
        let suffix = std::env::consts::EXE_SUFFIX;
        if !suffix.is_empty() {
            let with_exe = checkout.join(format!("{rel}{suffix}"));
            if with_exe.is_file() {
                return Ok(Some(with_exe));
            }
        }
        Err(BaselineError::BinaryMissing {
            path: plain.display().to_string(),
        })
    }

    fn execute_runs(
        &self,
        checkout: Option<&Path>,
        binary: Option<&Path>,
        rep: &mut BaselineReport,
    ) {
        for f in &self.manifest.files {
            let input_path = absolute(&self.opts.corpus_dir.join(&f.name));
            let stem = Path::new(&f.name)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| f.name.clone());
            let input_check = validate_png_file(&input_path, self.limits);

            for repeat in 0..self.opts.repeats {
                let workdir = self
                    .opts
                    .out_dir
                    .join("work")
                    .join(&self.spec.name)
                    .join(format!("rep{repeat}"))
                    .join(&stem);
                let out_sub = workdir.join("out");
                let log = self
                    .opts
                    .out_dir
                    .join("logs")
                    .join(&self.spec.name)
                    .join(format!("rep{repeat}"))
                    .join(format!("{stem}.log"));

                let mut record = RunRecord {
                    input: f.name.clone(),
                    repeat,
                    command: Vec::new(),
                    status: "failed".to_string(),
                    error: None,
                    exit_code: None,
                    timed_out: false,
                    duration_ms: 0,
                    artifacts: Vec::new(),
                    log: rel_display(&log, &self.opts.out_dir),
                };

                if let Err(e) = &input_check {
                    record.error = Some(RunError::InputRejected(e.clone()).to_record());
                    rep.runs.push(record);
                    continue;
                }
                if workdir.exists() {
                    if let Err(e) = force_remove_dir_all(&workdir) {
                        record.error = Some(RunError::Io(e.to_string()).to_record());
                        rep.runs.push(record);
                        continue;
                    }
                }
                if let Err(e) = std::fs::create_dir_all(&out_sub) {
                    record.error = Some(RunError::Io(e.to_string()).to_record());
                    rep.runs.push(record);
                    continue;
                }

                let out_sub_abs = absolute(&out_sub);
                let vars = BTreeMap::from([
                    (
                        "binary",
                        binary.map(|b| b.display().to_string()).unwrap_or_default(),
                    ),
                    ("input", input_path.display().to_string()),
                    ("out_dir", out_sub_abs.display().to_string()),
                    ("stem", stem.clone()),
                    (
                        "checkout",
                        checkout
                            .map(|c| c.display().to_string())
                            .unwrap_or_default(),
                    ),
                ]);
                let cmd = substitute_tokens(&self.spec.run, &vars);
                record.command = cmd.clone();
                let cwd = match self.spec.run_cwd {
                    RunCwd::Workdir => workdir.clone(),
                    RunCwd::Checkout => checkout.map(Path::to_path_buf).unwrap_or(workdir.clone()),
                };

                let declared: BTreeSet<String> = {
                    let stem_vars = BTreeMap::from([("stem", stem.clone())]);
                    substitute_tokens(&self.spec.outputs, &stem_vars)
                        .into_iter()
                        .map(|s| s.replace('\\', "/"))
                        .collect()
                };

                match run_with_timeout(
                    &cmd,
                    &cwd,
                    Duration::from_secs(self.limits.run_timeout_secs),
                    &log,
                ) {
                    Err(e) => record.error = Some(RunError::SpawnFailed(e).to_record()),
                    Ok(outcome) => {
                        record.exit_code = outcome.exit_code;
                        record.timed_out = outcome.timed_out;
                        record.duration_ms = outcome.duration_ms;
                        record.artifacts = collect_artifacts(&out_sub, &declared);

                        if outcome.timed_out {
                            record.error = Some(
                                RunError::Timeout {
                                    seconds: self.limits.run_timeout_secs,
                                }
                                .to_record(),
                            );
                        } else if outcome.exit_code != Some(0) {
                            record.error = Some(
                                RunError::NonZeroExit {
                                    code: outcome.exit_code,
                                }
                                .to_record(),
                            );
                        } else if let Some(missing) = declared
                            .iter()
                            .find(|d| !record.artifacts.iter().any(|a| &&a.path == d))
                        {
                            record.error = Some(
                                RunError::OutputMissing {
                                    path: missing.clone(),
                                }
                                .to_record(),
                            );
                        } else if let Some(a) = record
                            .artifacts
                            .iter()
                            .find(|a| a.declared && a.bytes > self.limits.max_output_bytes)
                        {
                            record.error = Some(
                                RunError::OutputTooLarge {
                                    bytes: a.bytes,
                                    max_bytes: self.limits.max_output_bytes,
                                }
                                .to_record(),
                            );
                        } else {
                            record.status = "ok".to_string();
                        }
                    }
                }
                rep.runs.push(record);
            }
        }
    }

    fn compute_determinism(&self, rep: &mut BaselineReport) {
        let repeats = self.opts.repeats;
        if repeats < 2 {
            rep.determinism = Some(DeterminismRecord {
                repeats,
                primary_deterministic: None,
                all_artifacts_deterministic: None,
                mismatches: Vec::new(),
            });
            return;
        }
        // (input, artifact path) -> repeat -> (sha, declared)
        let mut by_key: BTreeMap<(String, String), BTreeMap<u32, (String, bool)>> = BTreeMap::new();
        for r in &rep.runs {
            for a in &r.artifacts {
                by_key
                    .entry((r.input.clone(), a.path.clone()))
                    .or_default()
                    .insert(r.repeat, (a.sha256.clone(), a.declared));
            }
        }
        let mut mismatches = Vec::new();
        let mut any_declared = false;
        let mut any_artifact = false;
        let mut primary_ok = true;
        let mut all_ok_flag = true;
        for ((input, path), map) in &by_key {
            any_artifact = true;
            let declared = map.values().any(|(_, d)| *d);
            if declared {
                any_declared = true;
            }
            let shas: Vec<String> = (0..repeats)
                .map(|r| {
                    map.get(&r)
                        .map(|(s, _)| s.clone())
                        .unwrap_or_else(|| "MISSING".to_string())
                })
                .collect();
            let equal = shas.windows(2).all(|w| w[0] == w[1]);
            if !equal {
                all_ok_flag = false;
                if declared {
                    primary_ok = false;
                }
                mismatches.push(DeterminismMismatch {
                    input: input.clone(),
                    artifact: path.clone(),
                    declared,
                    sha256_by_repeat: shas,
                });
            }
        }
        rep.determinism = Some(DeterminismRecord {
            repeats,
            primary_deterministic: if any_declared { Some(primary_ok) } else { None },
            all_artifacts_deterministic: if any_artifact {
                Some(all_ok_flag)
            } else {
                None
            },
            mismatches,
        });
    }
}

// ---------------------------------------------------------------------------
// Selftest: proves the runner's own pipeline (exec, hashing, repeat
// comparison, report writing) is deterministic using an internal copy
// adapter — no mirrors, no network. Used by CI clean-checkout smoke.
// ---------------------------------------------------------------------------

pub fn selftest_spec() -> BaselineSpec {
    BaselineSpec {
        name: "selftest-copy".to_string(),
        repo: "internal".to_string(),
        pin_sha: "0".repeat(40),
        mirror_hint: String::new(),
        kind: BaselineKind::BuiltinCopy,
        build: Vec::new(),
        binary: None,
        run: vec![
            "{binary}".to_string(),
            "copy-adapter".to_string(),
            "{input}".to_string(),
            "{out_dir}/{stem}.copy.png".to_string(),
        ],
        outputs: vec!["{stem}.copy.png".to_string()],
        run_cwd: RunCwd::Workdir,
        notes: "internal determinism selftest; copies input bytes to output".to_string(),
    }
}

pub fn selftest(out_dir: &Path, corpus_dir: &Path, manifest_path: &Path) -> Result<bool, TopError> {
    let cfg = BaselinesConfig {
        schema: CONFIG_SCHEMA.to_string(),
        limits: ResourceLimits::default(),
        baselines: vec![selftest_spec()],
    };
    let opts = RunOptions {
        config_path: PathBuf::new(),
        corpus_dir: corpus_dir.to_path_buf(),
        manifest_path: manifest_path.to_path_buf(),
        mirror_root: PathBuf::new(),
        out_dir: out_dir.to_path_buf(),
        baseline_filter: Vec::new(),
        repeats: 2,
        skip_build: false,
    };
    let report = run_with_config(
        &cfg,
        "<selftest builtin config>".to_string(),
        sha256_hex(b"vice-classic selftest config v1"),
        &opts,
    )?;
    let b = &report.baselines[0];
    let ok = b.status == "completed"
        && b.runs.iter().all(|r| r.status == "ok")
        && b.determinism.as_ref().and_then(|d| d.primary_deterministic) == Some(true);
    Ok(ok)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// Path / artifact / git helpers live in `fsutil` (extracted unchanged so
// this module stays under the §4.1 size rule).
