//! vice-classic M0 deterministic baseline runner.
//!
//! All behaviour is controlled by explicit CLI flags and the TOML config —
//! no hidden environment variables (spec v1.3 §32 rule 4).

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use vice_bench::corpus;
use vice_bench::envinfo;
use vice_bench::limits::ResourceLimits;
use vice_bench::runner::{self, RunOptions};

#[derive(Parser)]
#[command(
    name = "baseline-runner",
    version,
    about = "vice-classic M0: run pinned donor systems over the smoke corpus, recording hashes, runtimes and typed outcomes"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Execute pinned baselines and write report.json / hashes.json / env.json.
    Run {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        corpus: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        /// Directory containing local mirrors of the pinned repositories.
        #[arg(long)]
        mirror_root: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// Restrict to specific baselines (repeatable). Default: all.
        #[arg(long = "baseline")]
        baselines: Vec<String>,
        /// Number of identical repeats used for the determinism check.
        #[arg(long, default_value_t = 2)]
        repeats: u32,
        /// Skip build steps (binaries must already exist in the checkout).
        #[arg(long)]
        skip_build: bool,
    },
    /// Verify the smoke corpus against its manifest.
    VerifyCorpus {
        #[arg(long)]
        corpus: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
        /// Baselines config; when given, resource limits are taken from its
        /// [limits] section (same limits as a full `run`). Without it the
        /// built-in defaults apply (REVIEW_M0 N4).
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// Print the environment manifest (canonical JSON).
    Env {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Run the internal copy-adapter pipeline twice and verify the runner's
    /// own machinery is deterministic. Needs no mirrors or network.
    Selftest {
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        corpus: PathBuf,
        #[arg(long)]
        manifest: PathBuf,
    },
    /// Internal adapter used by `selftest`: copy input file to output path.
    CopyAdapter {
        input: PathBuf,
        output: PathBuf,
        /// Artificial delay before copying (for timeout tests).
        #[arg(long, default_value_t = 0)]
        delay_ms: u64,
    },
    /// Internal test adapter: write `NAME=value` (or `NAME=<unset>`) of one
    /// environment variable to a file. Used by the child-env-policy tests
    /// to observe what a child process actually sees.
    EnvProbe { name: String, out: PathBuf },
}

fn main() {
    std::process::exit(real_main());
}

fn real_main() -> i32 {
    match Cli::parse().cmd {
        Cmd::Run {
            config,
            corpus,
            manifest,
            mirror_root,
            out,
            baselines,
            repeats,
            skip_build,
        } => {
            if repeats == 0 {
                eprintln!("error: --repeats must be >= 1");
                return 2;
            }
            let opts = RunOptions {
                config_path: config,
                corpus_dir: corpus,
                manifest_path: manifest,
                mirror_root,
                out_dir: out.clone(),
                baseline_filter: baselines,
                repeats,
                skip_build,
            };
            match runner::run(&opts) {
                Ok(report) => {
                    for b in &report.baselines {
                        let err = b
                            .error
                            .as_ref()
                            .map(|e| format!(" ({})", e.kind))
                            .unwrap_or_default();
                        println!("baseline {}: {}{}", b.name, b.status, err);
                        let ok = b.runs.iter().filter(|r| r.status == "ok").count();
                        println!("  runs ok: {}/{}", ok, b.runs.len());
                        if let Some(d) = &b.determinism {
                            println!(
                                "  primary_deterministic: {:?}  all_artifacts_deterministic: {:?}  mismatches: {}",
                                d.primary_deterministic,
                                d.all_artifacts_deterministic,
                                d.mismatches.len()
                            );
                        }
                    }
                    println!("report: {}", out.join("report.json").display());
                    println!("hashes: {}", out.join("hashes.json").display());
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    2
                }
            }
        }
        Cmd::VerifyCorpus {
            corpus,
            manifest,
            config,
        } => {
            let limits = match &config {
                None => ResourceLimits::default(),
                Some(path) => match vice_bench::config::load_config(path) {
                    Ok(cfg) => cfg.limits,
                    Err(e) => {
                        eprintln!("error: {e}");
                        return 2;
                    }
                },
            };
            let m = match corpus::load_manifest(&manifest) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: {e}");
                    return 2;
                }
            };
            let statuses = corpus::verify(&corpus, &m, &limits);
            let mut ok = true;
            for (name, st) in &statuses {
                println!("{name}: {st:?}");
                if *st != corpus::FileStatus::Ok {
                    ok = false;
                }
            }
            if ok {
                0
            } else {
                1
            }
        }
        Cmd::Env { out } => {
            let json = envinfo::canonical_json(&envinfo::collect());
            match out {
                Some(p) => {
                    if let Err(e) = std::fs::write(&p, format!("{json}\n")) {
                        eprintln!("error: write {}: {e}", p.display());
                        return 2;
                    }
                    0
                }
                None => {
                    println!("{json}");
                    0
                }
            }
        }
        Cmd::Selftest {
            out,
            corpus,
            manifest,
        } => match runner::selftest(&out, &corpus, &manifest) {
            Ok(true) => {
                println!("selftest OK: runner pipeline deterministic across 2 repeats");
                0
            }
            Ok(false) => {
                eprintln!("selftest FAILED: see {}", out.join("report.json").display());
                1
            }
            Err(e) => {
                eprintln!("error: {e}");
                2
            }
        },
        Cmd::CopyAdapter {
            input,
            output,
            delay_ms,
        } => {
            if delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
            if let Some(parent) = output.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    eprintln!("error: create {}: {e}", parent.display());
                    return 1;
                }
            }
            match std::fs::copy(&input, &output) {
                Ok(_) => 0,
                Err(e) => {
                    eprintln!(
                        "error: copy {} -> {}: {e}",
                        input.display(),
                        output.display()
                    );
                    1
                }
            }
        }
        Cmd::EnvProbe { name, out } => {
            let value = std::env::var(&name).unwrap_or_else(|_| "<unset>".to_string());
            match std::fs::write(&out, format!("{name}={value}")) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("error: write {}: {e}", out.display());
                    1
                }
            }
        }
    }
}
