use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use vice_bench::m8::{
    calibrate, measure_court_shard, merge_courts, release, M8CalibrationArtifact, M8CourtReport,
    M8CourtScope, M8_VARIANTS_PER_FAMILY,
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

fn main() {
    let result = match Cli::parse().command {
        Command::Measure {
            scope,
            variants,
            shard_index,
            shard_count,
            out,
        } => measure_court_shard(scope.into(), variants, shard_index, shard_count).and_then(
            |report| {
                write(&out, &report)?;
                println!(
                    "M8 {:?}: {}/{} accepted; {}",
                    report.scope,
                    report.accepted_groups,
                    report.source_groups,
                    out.display()
                );
                Ok(())
            },
        ),
        Command::Merge { inputs, out } => inputs
            .iter()
            .map(read::<M8CourtReport>)
            .collect::<Result<Vec<_>, _>>()
            .and_then(merge_courts)
            .and_then(|report| {
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
            out,
        } => read::<M8CourtReport>(&report).and_then(|report| {
            let calibration = read::<M8CalibrationArtifact>(&calibration)?;
            let artifact = release(&report, &calibration);
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
    };
    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
