//! `vicec` — the command-line path of vice-classic.
//!
//! M7 adds the §30 `vectorize` executable path while retaining M4's
//! `evidence` diagnostic path. Vectorization is selective: non-success writes
//! a typed canonical report and never publishes SVG bytes.
//!
//! ```text
//! vicec evidence input.png --out out/sample
//!       [--fg R,G,B] [--bg R,G,B] [--exterior transparent|opaque]
//! vicec vectorize input.png --mode flat2 --intent clean --preset quality
//!       --out out/sample
//! ```
//!
//! Exit codes are the §1.4 outcomes, kept apart rather than collapsed into
//! "error":
//!
//! ```text
//! 0  supported     one mixture class explains the pixels
//! 3  ambiguous     several physically different readings do
//! 4  unsupported   none does (or the input is outside Flat2 v1, spec 1.6)
//! 2  failed        decode/IO failure - a fault, not a verdict
//! ```
//!
//! `--fg/--bg/--exterior` are ORACLE overrides (§9.2, §30). They mark the run
//! NON-PRODUCTION, and the mark travels in the artifact rather than in the
//! operator's memory.

#![forbid(unsafe_code)]

mod product;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_evidence::analysis::{analyze, Flat2Outcome, ANALYSIS_CONFIG_V1};
use vice_evidence::palette::oracle_override;
use vice_image::{CanonicalImage, DecodeLimits};
use vice_ir::color::srgb_encoded_to_linear;
use vice_ir::LinearRgb;

#[derive(Parser)]
#[command(
    name = "vicec",
    version,
    about = "vice-classic: selective classical Flat2 vectorization"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum ExteriorArg {
    Transparent,
    Opaque,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum ModeArg {
    Flat2,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum IntentArg {
    Exact,
    Clean,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum PresetArg {
    Fast,
    Quality,
}

#[derive(Subcommand)]
enum Cmd {
    /// Decode an image and report the Flat2 evidence for it.
    Evidence {
        input: PathBuf,
        /// Directory for `evidence.json`. Without it the report goes to
        /// stdout only.
        #[arg(long)]
        out: Option<PathBuf>,
        /// ORACLE override: foreground colour as three sRGB codes `R,G,B`.
        /// Marks the run non-production (§30).
        #[arg(long)]
        fg: Option<String>,
        /// ORACLE override: background colour as three sRGB codes.
        #[arg(long)]
        bg: Option<String>,
        /// ORACLE override: which exterior model to assume.
        #[arg(long, value_enum)]
        exterior: Option<ExteriorArg>,
    },
    /// Selectively reconstruct and seal a Flat2 scene.
    Vectorize {
        input: PathBuf,
        #[arg(long, value_enum)]
        mode: ModeArg,
        #[arg(long, value_enum, default_value = "clean")]
        intent: IntentArg,
        #[arg(long, value_enum, default_value = "quality")]
        preset: PresetArg,
        /// New or empty output directory.
        #[arg(long)]
        out: PathBuf,
        /// Digest-pinned release configuration. When omitted, the canonical
        /// repository config for the selected preset is used.
        #[arg(long)]
        production_config: Option<PathBuf>,
        #[arg(long)]
        trace: bool,
        #[arg(long, default_value_t = 0)]
        dump_candidates: usize,
        #[arg(long)]
        strict: bool,
        /// Diagnostic feature switch; marks the run non-production.
        #[arg(long)]
        milestone_debug: Option<String>,
        /// ORACLE override: foreground colour as three sRGB codes `R,G,B`.
        #[arg(long)]
        fg: Option<String>,
        /// ORACLE override: background colour as three sRGB codes.
        #[arg(long)]
        bg: Option<String>,
        /// ORACLE override: which exterior model to assume.
        #[arg(long, value_enum)]
        exterior: Option<ExteriorArg>,
    },
    /// Run an explicitly pinned external legacy engine. This never reports a
    /// Classic success and is never called as a fallback by `vectorize`.
    LegacyVectorize {
        input: PathBuf,
        #[arg(long)]
        engine: PathBuf,
        #[arg(long)]
        engine_sha256: String,
        /// Direct argv token; `{input}` and `{output}` are substituted. No
        /// shell is involved, so leading-hyphen values are safe.
        #[arg(long = "arg", allow_hyphen_values = true)]
        args: Vec<String>,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=600))]
        timeout_secs: u64,
    },
    /// Print the deterministic M12 technical/legal release status. With
    /// `--check`, refuse if it differs from the committed cross-platform vector.
    ReleaseStatus {
        #[arg(long)]
        check: Option<PathBuf>,
    },
}

const LEGACY_REPORT_SCHEMA: &str = "vice-classic/legacy-wrapper-report/v1";
const MAX_LEGACY_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Serialize)]
struct LegacyWrapperReport {
    schema: &'static str,
    status: &'static str,
    classic_success: bool,
    source_path: String,
    source_sha256: String,
    engine_path: String,
    engine_sha256_expected: String,
    engine_sha256_before: String,
    engine_sha256_after: String,
    argv: Vec<String>,
    timeout_secs: u64,
    duration_ms: u64,
    timed_out: bool,
    exit_code: Option<i32>,
    output_path: String,
    output_bytes: Option<u64>,
    output_sha256: Option<String>,
    stdout_sha256: String,
    stderr_sha256: String,
    refusal: Option<String>,
}

fn parse_rgb(s: &str) -> Result<LinearRgb, String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 3 {
        return Err(format!("expected R,G,B in 0..255, got {s:?}"));
    }
    let mut v = [0.0f64; 3];
    for (i, p) in parts.iter().enumerate() {
        let n: u16 = p
            .trim()
            .parse()
            .map_err(|_| format!("{p:?} is not a number"))?;
        if n > 255 {
            return Err(format!("{n} is not an 8-bit code"));
        }
        v[i] = srgb_encoded_to_linear(f64::from(n) / 255.0);
    }
    Ok(LinearRgb::new(v[0], v[1], v[2]))
}

fn parse_oracle(
    fg: Option<String>,
    bg: Option<String>,
    exterior: Option<ExteriorArg>,
) -> Result<Option<vice_evidence::Flat2Hypothesis>, String> {
    match (fg, bg, exterior) {
        (None, None, None) => Ok(None),
        (Some(foreground), background, exterior) => {
            let foreground = parse_rgb(&foreground).map_err(|error| format!("--fg: {error}"))?;
            let background = match (background, exterior) {
                (Some(background), _) => {
                    Some(parse_rgb(&background).map_err(|error| format!("--bg: {error}"))?)
                }
                (None, Some(ExteriorArg::Transparent)) | (None, None) => None,
                (None, Some(ExteriorArg::Opaque)) => {
                    return Err("--exterior opaque needs the background colour: pass --bg".into())
                }
            };
            Ok(Some(oracle_override(foreground, background)))
        }
        _ => Err("--bg/--exterior are overrides of a foreground: pass --fg".into()),
    }
}

fn prepare_output_dir(path: &std::path::Path) -> Result<(), String> {
    if path.exists() {
        let mut entries =
            std::fs::read_dir(path).map_err(|error| format!("read {}: {error}", path.display()))?;
        if entries.next().is_some() {
            return Err(format!(
                "output directory {} is not empty; refusing to mix a new verdict with stale artifacts",
                path.display()
            ));
        }
    } else {
        std::fs::create_dir_all(path)
            .map_err(|error| format!("create {}: {error}", path.display()))?;
    }
    Ok(())
}

fn write_artifact(directory: &std::path::Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    let path = directory.join(name);
    std::fs::write(&path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}

fn sha256_file(path: &Path, limit: u64) -> Result<(String, u64), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("stat {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > limit {
        return Err(format!(
            "{} is not a regular file within the {limit}-byte limit",
            path.display()
        ));
    }
    let mut file =
        std::fs::File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65_536];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok((hex::encode(digest.finalize()), metadata.len()))
}

fn run_legacy_vectorizer(
    input: PathBuf,
    engine: PathBuf,
    expected_digest: String,
    args: Vec<String>,
    out: PathBuf,
    timeout_secs: u64,
) -> Result<bool, String> {
    let input = input
        .canonicalize()
        .map_err(|error| format!("resolve input: {error}"))?;
    let engine = engine
        .canonicalize()
        .map_err(|error| format!("resolve engine: {error}"))?;
    let expected_digest = expected_digest.to_ascii_lowercase();
    if expected_digest.len() != 64 || !expected_digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("--engine-sha256 must be exactly 64 hexadecimal characters".into());
    }
    let (source_sha256, _) = sha256_file(&input, 64 * 1024 * 1024)?;
    let (engine_before, _) = sha256_file(&engine, 1024 * 1024 * 1024)?;
    if engine_before != expected_digest {
        return Err(
            "legacy engine digest does not match --engine-sha256; refusing execution".into(),
        );
    }
    prepare_output_dir(&out)?;
    let output = out.join("legacy-result.svg");
    let argv = args
        .iter()
        .map(|arg| match arg.as_str() {
            "{input}" => input.to_string_lossy().into_owned(),
            "{output}" => output.to_string_lossy().into_owned(),
            _ => arg.clone(),
        })
        .collect::<Vec<_>>();
    if !argv.iter().any(|arg| arg == &input.to_string_lossy())
        || !argv.iter().any(|arg| arg == &output.to_string_lossy())
    {
        return Err("legacy argv must contain both {input} and {output} placeholders".into());
    }
    let stdout_path = out.join("legacy.stdout.log");
    let stderr_path = out.join("legacy.stderr.log");
    let stdout = std::fs::File::create(&stdout_path)
        .map_err(|error| format!("create {}: {error}", stdout_path.display()))?;
    let stderr = std::fs::File::create(&stderr_path)
        .map_err(|error| format!("create {}: {error}", stderr_path.display()))?;
    let started = Instant::now();
    let mut child = Command::new(&engine)
        .args(&argv)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| format!("spawn {}: {error}", engine.display()))?;
    let deadline = started + Duration::from_secs(timeout_secs);
    let (status, timed_out, output_limit_exceeded) = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("wait legacy engine: {error}"))?
        {
            break (status, false, false);
        }
        let live_output_bytes = [&stdout_path, &stderr_path, &output]
            .into_iter()
            .filter_map(|path| std::fs::symlink_metadata(path).ok())
            .map(|metadata| metadata.len())
            .sum::<u64>();
        if live_output_bytes > MAX_LEGACY_OUTPUT_BYTES {
            child
                .kill()
                .map_err(|error| format!("kill oversized legacy engine: {error}"))?;
            break (
                child
                    .wait()
                    .map_err(|error| format!("reap legacy engine: {error}"))?,
                false,
                true,
            );
        }
        if Instant::now() >= deadline {
            child
                .kill()
                .map_err(|error| format!("kill timed-out legacy engine: {error}"))?;
            break (
                child
                    .wait()
                    .map_err(|error| format!("reap legacy engine: {error}"))?,
                true,
                false,
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let (engine_after, _) = sha256_file(&engine, 1024 * 1024 * 1024)?;
    let (stdout_sha256, _) = sha256_file(&stdout_path, MAX_LEGACY_OUTPUT_BYTES)?;
    let (stderr_sha256, _) = sha256_file(&stderr_path, MAX_LEGACY_OUTPUT_BYTES)?;
    let output_result = output
        .exists()
        .then(|| sha256_file(&output, MAX_LEGACY_OUTPUT_BYTES))
        .transpose();
    let (output_sha256, output_bytes, output_error) = match output_result {
        Ok(Some((digest, bytes))) => (Some(digest), Some(bytes), None),
        Ok(None) => (
            None,
            None,
            Some("legacy engine produced no output".to_string()),
        ),
        Err(error) => (None, None, Some(error)),
    };
    let refusal = if output_limit_exceeded {
        Some("legacy engine exceeded the combined 64 MiB output limit".into())
    } else if timed_out {
        Some("legacy engine exceeded its wall-clock timeout".into())
    } else if engine_after != engine_before {
        Some("legacy engine changed on disk during execution".into())
    } else if !status.success() {
        Some("legacy engine exited unsuccessfully".into())
    } else {
        output_error
    };
    let success = refusal.is_none();
    let report = LegacyWrapperReport {
        schema: LEGACY_REPORT_SCHEMA,
        status: if success {
            "legacy_success"
        } else {
            "legacy_failed"
        },
        classic_success: false,
        source_path: input.display().to_string(),
        source_sha256,
        engine_path: engine.display().to_string(),
        engine_sha256_expected: expected_digest,
        engine_sha256_before: engine_before,
        engine_sha256_after: engine_after,
        argv,
        timeout_secs,
        duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        timed_out,
        exit_code: status.code(),
        output_path: output.display().to_string(),
        output_bytes,
        output_sha256,
        stdout_sha256,
        stderr_sha256,
        refusal,
    };
    write_artifact(
        &out,
        "legacy.report.json",
        serde_json::to_vec_pretty(&report)
            .map_err(|error| format!("serialize legacy report: {error}"))?
            .as_slice(),
    )?;
    Ok(success)
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Evidence {
            input,
            out,
            fg,
            bg,
            exterior,
        } => {
            let bytes = match std::fs::read(&input) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("error: read {}: {e}", input.display());
                    return 2;
                }
            };
            let img = match CanonicalImage::decode(&bytes, &DecodeLimits::default()) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("error: decode {}: {e}", input.display());
                    return 2;
                }
            };

            // The override is all-or-nothing: a foreground without an
            // exterior decision would be half a hypothesis, and guessing the
            // other half is what §9.2 forbids.
            let over = match parse_oracle(fg, bg, exterior) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("error: {error}");
                    return 2;
                }
            };

            let report = analyze(&img, &ANALYSIS_CONFIG_V1, over);
            let json = report.canonical_json();
            if let Some(dir) = &out {
                if let Err(e) = std::fs::create_dir_all(dir) {
                    eprintln!("error: create {}: {e}", dir.display());
                    return 2;
                }
                let path = dir.join("evidence.json");
                if let Err(e) = std::fs::write(&path, format!("{json}\n")) {
                    eprintln!("error: write {}: {e}", path.display());
                    return 2;
                }
                println!("evidence report: {}", path.display());
            }

            println!(
                "{}x{} px, sha256 {}, colour assumption {}{}",
                report.image.width_px,
                report.image.height_px,
                &report.image.source_sha256[..16],
                report.image.icc_assumption,
                if report.production {
                    ""
                } else {
                    "   [NON-PRODUCTION: oracle override]"
                }
            );
            println!(
                "{} palette hypotheses, {} (palette, formation) pairs, {} refused",
                report.hypotheses.len(),
                report.evidences.len(),
                report.refused.len()
            );
            if !report.production {
                eprintln!(
                    "warning: --fg/--bg/--exterior are diagnostic overrides; this run is NOT a \
                     production result (spec 30)"
                );
            }
            match &report.outcome {
                Flat2Outcome::Supported {
                    evidence_id,
                    mixture_class,
                    tied_formations,
                } => {
                    println!("outcome: SUPPORTED  {evidence_id}  (class {mixture_class})");
                    if !tied_formations.is_empty() {
                        println!(
                            "  tied formations, not distinguishable from this image: {}",
                            tied_formations.join(", ")
                        );
                    }
                    if let Some(b) = &report.boundary {
                        println!(
                            "  boundary: {} chains, {:.1} px, {} samples at ds {} px; halfwidth \
                             median {:.3} px, p95 {:.3} px at coverage {:.2}",
                            b.chains.len(),
                            b.total_length_px,
                            b.samples,
                            b.sample_step_px,
                            b.median_halfwidth_px,
                            b.p95_halfwidth_px,
                            b.coverage_level
                        );
                        if let vice_evidence::boundary::ChainStatus::Ambiguous { saddle_cells } =
                            b.status
                        {
                            println!(
                                "  {saddle_cells} ambiguous 2x2 cell(s): the alternative \
                                 resolutions are a topology hypothesis M4.5 owns"
                            );
                        }
                    }
                    0
                }
                Flat2Outcome::Ambiguous {
                    mixture_classes,
                    note,
                } => {
                    println!("outcome: AMBIGUOUS  {}", mixture_classes.join(", "));
                    println!("  {note}");
                    3
                }
                Flat2Outcome::Unsupported(reason) => {
                    println!(
                        "outcome: UNSUPPORTED  {}",
                        serde_json::to_string(reason).unwrap_or_default()
                    );
                    4
                }
            }
        }
        Cmd::Vectorize {
            input,
            mode: ModeArg::Flat2,
            intent,
            preset,
            out,
            production_config,
            trace,
            dump_candidates,
            strict,
            milestone_debug,
            fg,
            bg,
            exterior,
        } => {
            let bytes = match std::fs::read(&input) {
                Ok(bytes) => bytes,
                Err(error) => {
                    eprintln!("error: read {}: {error}", input.display());
                    return 2;
                }
            };
            let oracle_override = match parse_oracle(fg, bg, exterior) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("error: {error}");
                    return 2;
                }
            };
            if let Err(error) = prepare_output_dir(&out) {
                eprintln!("error: {error}");
                return 2;
            }
            let request = vice_core::VectorizeRequest {
                intent: match intent {
                    IntentArg::Exact => vice_core::Intent::Exact,
                    IntentArg::Clean => vice_core::Intent::Clean,
                },
                preset: match preset {
                    PresetArg::Fast => vice_core::Preset::Fast,
                    PresetArg::Quality => vice_core::Preset::Quality,
                },
                trace,
                dump_candidates,
                strict,
                production: true,
                research_override: false,
                milestone_debug,
                oracle_override,
            };
            let outcome = match production_config {
                Some(path) => vice_core::vectorize_with_production_config(&bytes, &request, &path),
                None => vice_core::vectorize_embedded_production(&bytes, &request),
            };
            let write_result = match &outcome {
                vice_core::VectorizeOutcome::Success(success) => {
                    let artifacts = &success.artifacts;
                    write_artifact(&out, "result.svg", &artifacts.result_svg)
                        .and_then(|_| {
                            write_artifact(
                                &out,
                                "result.pure-partition.svg",
                                &artifacts.pure_partition_svg,
                            )
                        })
                        .and_then(|_| {
                            write_artifact(&out, "result.scene.json", &artifacts.scene_json)
                        })
                        .and_then(|_| {
                            write_artifact(
                                &out,
                                "result.export-plan.json",
                                &artifacts.export_plan_json,
                            )
                        })
                        .and_then(|_| {
                            write_artifact(&out, "result.report.json", &artifacts.report_json)
                        })
                        .and_then(|_| {
                            write_artifact(&out, "result.render.png", &artifacts.render_png)
                        })
                        .and_then(|_| {
                            write_artifact(&out, "result.seal.json", &artifacts.seal_json)
                        })
                        .and_then(|_| {
                            if let Some(trace_json) = &artifacts.trace_json {
                                let trace_dir = out.join("trace");
                                std::fs::create_dir(&trace_dir).map_err(|error| {
                                    format!("create {}: {error}", trace_dir.display())
                                })?;
                                write_artifact(&trace_dir, "trace.json", trace_json)
                            } else {
                                Ok(())
                            }
                        })
                }
                _ => write_artifact(
                    &out,
                    "result.report.json",
                    outcome.report().canonical_json().as_bytes(),
                ),
            };
            if let Err(error) = write_result {
                eprintln!("error: {error}");
                return 2;
            }
            let report = outcome.report();
            println!(
                "outcome: {:?}; report: {}",
                report.status,
                out.join("result.report.json").display()
            );
            if let Some(reason) = &report.reason {
                println!(
                    "reason: {}",
                    serde_json::to_string(reason).unwrap_or_default()
                );
            }
            match outcome {
                vice_core::VectorizeOutcome::Success(_) => 0,
                vice_core::VectorizeOutcome::Ambiguous(_) => 3,
                vice_core::VectorizeOutcome::Unsupported(_) => 4,
                vice_core::VectorizeOutcome::Failed(_) => 2,
            }
        }
        Cmd::LegacyVectorize {
            input,
            engine,
            engine_sha256,
            args,
            out,
            timeout_secs,
        } => match run_legacy_vectorizer(input, engine, engine_sha256, args, out, timeout_secs) {
            Ok(true) => {
                println!("outcome: LEGACY_SUCCESS (never Classic success)");
                0
            }
            Ok(false) => {
                eprintln!("outcome: LEGACY_FAILED; see legacy.report.json");
                2
            }
            Err(error) => {
                eprintln!("error: {error}");
                2
            }
        },
        Cmd::ReleaseStatus { check } => {
            let json = match product::canonical_json() {
                Ok(json) => json,
                Err(error) => {
                    eprintln!("error: serialize release status: {error}");
                    return 2;
                }
            };
            if let Some(path) = check {
                let expected = match std::fs::read(&path) {
                    Ok(value) => value,
                    Err(error) => {
                        eprintln!("error: read {}: {error}", path.display());
                        return 2;
                    }
                };
                let actual_value: serde_json::Value = serde_json::from_slice(&json).unwrap();
                let expected_value: serde_json::Value = match serde_json::from_slice(&expected) {
                    Ok(value) => value,
                    Err(error) => {
                        eprintln!("error: parse {}: {error}", path.display());
                        return 2;
                    }
                };
                if actual_value != expected_value {
                    eprintln!(
                        "error: M12 structural contract differs from {}",
                        path.display()
                    );
                    return 2;
                }
            }
            println!("{}", String::from_utf8_lossy(&json));
            0
        }
    }
}
