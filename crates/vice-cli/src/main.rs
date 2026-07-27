//! `vicec` — the command-line path of vice-classic (M4 scope).
//!
//! §4.1 requires every milestone to end in a working CLI path rather than in
//! types, and §30 fixes the shape of the eventual command. M4 owns exactly
//! one stage of §7 — canonical decode, palette/exterior hypotheses, the
//! minimal global formation family, premultiplied mixture evidence, boundary
//! observations and their corridor — so `vicec evidence` is what exists, and
//! `vicec vectorize` is NOT declared: there is no topology, no fitter and no
//! selective delivery behind it, and a subcommand that would have to say
//! "not implemented" is the placeholder §32 rule 7 forbids.
//!
//! ```text
//! vicec evidence input.png --out out/sample
//!       [--fg R,G,B] [--bg R,G,B] [--exterior transparent|opaque]
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

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use vice_evidence::analysis::{analyze, Flat2Outcome, ANALYSIS_CONFIG_V1};
use vice_evidence::palette::oracle_override;
use vice_image::{CanonicalImage, DecodeLimits};
use vice_ir::color::srgb_encoded_to_linear;
use vice_ir::LinearRgb;

#[derive(Parser)]
#[command(
    name = "vicec",
    version,
    about = "vice-classic: Flat2 evidence for a raster image (M4 scope)"
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
            let img = match CanonicalImage::decode_png(&bytes, &DecodeLimits::default()) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("error: decode {}: {e}", input.display());
                    return 2;
                }
            };

            // The override is all-or-nothing: a foreground without an
            // exterior decision would be half a hypothesis, and guessing the
            // other half is what §9.2 forbids.
            let over = match (fg, bg, exterior) {
                (None, None, None) => None,
                (Some(f), b, ext) => {
                    let f = match parse_rgb(&f) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("error: --fg: {e}");
                            return 2;
                        }
                    };
                    let background = match (b, ext) {
                        (Some(b), _) => match parse_rgb(&b) {
                            Ok(v) => Some(v),
                            Err(e) => {
                                eprintln!("error: --bg: {e}");
                                return 2;
                            }
                        },
                        (None, Some(ExteriorArg::Transparent)) | (None, None) => None,
                        (None, Some(ExteriorArg::Opaque)) => {
                            eprintln!(
                                "error: --exterior opaque needs the background colour: pass --bg"
                            );
                            return 2;
                        }
                    };
                    Some(oracle_override(f, background))
                }
                _ => {
                    eprintln!("error: --bg/--exterior are overrides of a foreground: pass --fg");
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
    }
}
