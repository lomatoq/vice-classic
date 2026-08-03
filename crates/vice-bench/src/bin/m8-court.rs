use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use clap::{Parser, Subcommand, ValueEnum};
use sha2::{Digest, Sha256};
use vice_bench::m8::{
    calibrate, measure_court_shard, merge_courts, production_policy, release,
    M8CalibrationArtifact, M8CourtReport, M8CourtScope, M8ReleaseArtifact, M8ReleaseAuthority,
    M8_FORMAL_SHARDS, M8_VARIANTS_PER_FAMILY,
};

#[derive(Parser)]
#[command(name = "m8-court")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Measure {
        #[arg(long, value_enum)]
        scope: Scope,
        #[arg(long, default_value_t = M8_VARIANTS_PER_FAMILY)]
        variants: usize,
        #[arg(long, default_value_t = 0)]
        shard_index: u32,
        #[arg(long, default_value_t = 1)]
        shard_count: u32,
        #[arg(long)]
        execution_id: String,
        #[arg(long)]
        out: PathBuf,
    },
    Merge {
        #[arg(long, required = true)]
        inputs: Vec<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
    Calibrate {
        #[arg(long)]
        report: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Release {
        #[arg(long)]
        report: PathBuf,
        #[arg(long)]
        calibration: PathBuf,
        #[arg(long)]
        authority: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Promote {
        #[arg(long)]
        calibration: PathBuf,
        #[arg(long)]
        release: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Scope {
    Smoke,
    Calibration,
    SealedAudit,
}

impl From<Scope> for M8CourtScope {
    fn from(value: Scope) -> Self {
        match value {
            Scope::Smoke => Self::Smoke,
            Scope::Calibration => Self::Calibration,
            Scope::SealedAudit => Self::SealedAudit,
        }
    }
}

fn write(path: &PathBuf, value: &impl serde::Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, format!("{text}\n")).map_err(|e| e.to_string())
}

fn read<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Result<T, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
}

fn read_toml<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Result<T, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    toml::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn file_sha256(path: &PathBuf) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn current_exe_sha256() -> Result<String, String> {
    let path = std::env::current_exe().map_err(|error| error.to_string())?;
    let bytes =
        std::fs::read(&path).map_err(|error| format!("read runner {}: {error}", path.display()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn clean_candidate_sha() -> Result<String, String> {
    let output = ProcessCommand::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| format!("run git rev-parse: {error}"))?;
    if !output.status.success() {
        return Err("M8 measurement requires a Git checkout".into());
    }
    let head = String::from_utf8(output.stdout)
        .map_err(|error| error.to_string())?
        .trim()
        .to_owned();
    let status = ProcessCommand::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .output()
        .map_err(|error| format!("run git status: {error}"))?;
    if !status.status.success() || !status.stdout.is_empty() {
        return Err("M8 formal measurement requires a clean candidate checkout".into());
    }
    let built = env!("VICE_BUILD_GIT_SHA");
    if head.len() != 40 || !head.bytes().all(|byte| byte.is_ascii_hexdigit()) || built != head {
        return Err(format!(
            "M8 runner was built from {built}, not clean candidate {head}; rebuild it"
        ));
    }
    Ok(head)
}

fn validate_local_report(report: &M8CourtReport) -> Result<(), String> {
    let candidate = clean_candidate_sha()?;
    let runner = current_exe_sha256()?;
    if report.candidate_sha != candidate || report.runner_sha256 != runner {
        return Err("M8 report is not bound to this clean candidate and runner".into());
    }
    Ok(())
}

fn validate_config_only_gate_delta(
    calibration_candidate: &str,
    sealed_candidate: &str,
) -> Result<(), String> {
    let ancestor = ProcessCommand::new("git")
        .args([
            "merge-base",
            "--is-ancestor",
            calibration_candidate,
            sealed_candidate,
        ])
        .status()
        .map_err(|error| format!("run git merge-base: {error}"))?;
    if !ancestor.success() || calibration_candidate == sealed_candidate {
        return Err("M8 sealed candidate must descend from the calibration feature commit".into());
    }
    let output = ProcessCommand::new("git")
        .args([
            "diff",
            "--name-only",
            calibration_candidate,
            sealed_candidate,
        ])
        .output()
        .map_err(|error| format!("run git diff: {error}"))?;
    if !output.status.success() {
        return Err("cannot verify the M8 calibration-to-release delta".into());
    }
    let paths = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    let allowed = [
        "configs/M8_PRODUCTION_CALIBRATION_V2.json",
        "configs/M8_GATE_PROVENANCE_V2.toml",
    ];
    if paths.lines().any(|path| !allowed.contains(&path))
        || !allowed
            .iter()
            .all(|expected| paths.lines().any(|path| path == *expected))
    {
        return Err("M8 calibration-to-release delta is not the exact two-file gate freeze".into());
    }
    Ok(())
}

fn main() {
    let result = match Cli::parse().command {
        Command::Measure {
            scope,
            variants,
            shard_index,
            shard_count,
            execution_id,
            out,
        } => {
            let scope = M8CourtScope::from(scope);
            let candidate = clean_candidate_sha();
            let runner = current_exe_sha256();
            if scope != M8CourtScope::Smoke
                && (variants != M8_VARIANTS_PER_FAMILY || shard_count != M8_FORMAL_SHARDS)
            {
                Err("formal M8 measurement requires exactly 650 variants and four shards".into())
            } else {
                candidate.and_then(|candidate| {
                    let runner = runner?;
                    let mut report =
                        measure_court_shard(scope, variants, shard_index, shard_count)?;
                    report.candidate_sha = candidate;
                    report.execution_ids = vec![execution_id];
                    report.runner_sha256 = runner;
                    write(&out, &report)?;
                    println!(
                        "M8 {:?}: {}/{} accepted; {}",
                        report.scope,
                        report.accepted_groups,
                        report.source_groups,
                        out.display()
                    );
                    Ok(())
                })
            }
        }
        Command::Merge { inputs, out } => inputs
            .iter()
            .map(read::<M8CourtReport>)
            .collect::<Result<Vec<_>, _>>()
            .and_then(merge_courts)
            .and_then(|report| {
                validate_local_report(&report)?;
                write(&out, &report)?;
                println!(
                    "M8 merged shards {:?}: {}/{} accepted; {}",
                    report.included_shards,
                    report.accepted_groups,
                    report.source_groups,
                    out.display()
                );
                Ok(())
            }),
        Command::Calibrate { report, out } => read::<M8CourtReport>(&report).and_then(|report| {
            validate_local_report(&report)?;
            let artifact = calibrate(&report);
            write(&out, &artifact)?;
            println!(
                "M8 calibration green={}; {}",
                artifact.gate_met,
                out.display()
            );
            for refusal in &artifact.refusals {
                eprintln!("refusal: {refusal}");
            }
            artifact
                .gate_met
                .then_some(())
                .ok_or_else(|| "M8 calibration is not green".into())
        }),
        Command::Release {
            report,
            calibration,
            authority,
            out,
        } => read::<M8CourtReport>(&report).and_then(|report| {
            validate_local_report(&report)?;
            let calibration_file_sha256 = file_sha256(&calibration)?;
            let calibration = read::<M8CalibrationArtifact>(&calibration)?;
            validate_config_only_gate_delta(&calibration.candidate_sha, &report.candidate_sha)?;
            let authority_file_sha256 = file_sha256(&authority)?;
            let authority = read_toml::<M8ReleaseAuthority>(&authority)?;
            let artifact = release(
                &report,
                &calibration,
                &authority,
                &calibration_file_sha256,
                &authority_file_sha256,
            );
            write(&out, &artifact)?;
            println!("M8 release green={}; {}", artifact.gate_met, out.display());
            for refusal in &artifact.refusals {
                eprintln!("refusal: {refusal}");
            }
            artifact
                .gate_met
                .then_some(())
                .ok_or_else(|| "M8 release is not green".into())
        }),
        Command::Promote {
            calibration,
            release,
            out,
        } => {
            let calibration = read::<M8CalibrationArtifact>(&calibration);
            let release = read::<M8ReleaseArtifact>(&release);
            calibration.and_then(|calibration| {
                let release = release?;
                let policy = production_policy(&calibration, &release)?;
                write(&out, &policy)?;
                println!("M8 production policy: {}", out.display());
                Ok(())
            })
        }
    };
    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
