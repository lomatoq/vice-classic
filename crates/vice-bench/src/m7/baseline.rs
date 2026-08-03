//! Paired internal-baseline comparison and identity-blind source-level court.

use std::collections::BTreeMap;

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::release::{catastrophic_with_gates, target_rows, M7ReleaseGates};
use super::{
    InternalBaselineMeasurement, MeasurementReport, MeasurementRow, SceneComplexity,
    M7_SEALED_POPULATION_POLICY,
};
use crate::gates::GatesFile;
use crate::gt::split::{AuditSeal, SealStatus};
use crate::m7::governance::M7ThresholdSource;
use crate::prereg::Preregistration;

pub const M7_BASELINE_COURT_SCHEMA: &str = "vice-classic/m7-baseline-blind-court/v5";

#[derive(Debug, Clone, Copy)]
struct CourtGates {
    release: M7ReleaseGates,
    max_complexity_growth_ratio: f64,
    min_blind_source_trials: u64,
    max_blind_one_sided_p_value: f64,
    min_blind_preference_rate: f64,
}

impl CourtGates {
    fn from_file(gates: &GatesFile) -> Result<Self, String> {
        Ok(Self {
            release: M7ReleaseGates::from_file(gates)?,
            max_complexity_growth_ratio: number(
                gates,
                "m7_selective",
                "gate_max_complexity_growth_ratio",
            )?,
            min_blind_source_trials: integer(
                gates,
                "m7_selective",
                "gate_min_blind_source_trials",
            )?,
            max_blind_one_sided_p_value: number(
                gates,
                "m7_selective",
                "gate_max_blind_one_sided_p_value",
            )?,
            min_blind_preference_rate: number(
                gates,
                "m7_selective",
                "gate_min_blind_preference_rate",
            )?,
        })
    }
}

fn number(gates: &GatesFile, section: &str, key: &str) -> Result<f64, String> {
    let value = gates
        .gate_value(section, key)
        .map_err(|error| error.to_string())?;
    value
        .as_float()
        .or_else(|| value.as_integer().map(|number| number as f64))
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("{section}.{key} is not finite"))
}

fn integer(gates: &GatesFile, section: &str, key: &str) -> Result<u64, String> {
    gates
        .gate_value(section, key)
        .map_err(|error| error.to_string())?
        .as_integer()
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| format!("{section}.{key} is not a non-negative integer"))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TailComparison {
    pub selected_p95_px: f64,
    pub baseline_p95_px: f64,
    pub selected_p99_px: f64,
    pub baseline_p99_px: f64,
    pub selected_max_px: f64,
    pub baseline_max_px: f64,
    pub no_tail_regression: bool,
    pub strict_tail_improvement: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ComplexityComparison {
    pub vertices_growth_ratio: f64,
    pub boundaries_growth_ratio: f64,
    pub curve_segments_growth_ratio: f64,
    pub canonical_delivery_bytes_growth_ratio: f64,
    pub max_growth_ratio: f64,
    pub gate: f64,
    pub gate_met: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlindChoice {
    Left,
    Right,
    Tie,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlindTrial {
    pub source_group: String,
    pub presentation_commitment_sha256: String,
    pub left_artifact_bundle_sha256: String,
    pub right_artifact_bundle_sha256: String,
    pub judge_choice_before_reveal: BlindChoice,
    pub reveal_selected_side: String,
    pub selected_preferred: bool,
    pub baseline_preferred: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlindVerdict {
    pub source_trials: u64,
    pub non_tied_trials: u64,
    pub selected_wins: u64,
    pub baseline_wins: u64,
    pub ties: u64,
    pub selected_preference_rate: f64,
    pub exact_one_sided_binomial_p_value: f64,
    pub min_source_trials: u64,
    pub min_preference_rate: f64,
    pub max_one_sided_p_value: f64,
    pub trials: Vec<BlindTrial>,
    pub gate_met: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PresetBaselineVerdict {
    pub preset: vice_core::Preset,
    pub accepted_paired_rows: u64,
    pub baseline_refused_rows: u64,
    pub paired_source_groups: u64,
    pub selected_catastrophic_source_groups: u64,
    pub baseline_catastrophic_source_groups: u64,
    pub catastrophic_not_worse: bool,
    pub tails: TailComparison,
    pub complexity: ComplexityComparison,
    pub blind: BlindVerdict,
    pub gate_met: bool,
    pub refusals: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BaselineCourtVerdict {
    pub schema: &'static str,
    pub audit_generation: u32,
    pub corpus_sha256: String,
    pub population_commitment_sha256: String,
    pub preregistration_sha256: String,
    pub gates_sha256: String,
    pub release_commit_sha: String,
    pub runner_attestation_sha256: String,
    pub gate_provenance_sha256: String,
    pub quality_report_sha256: String,
    pub fast_report_sha256: String,
    pub quality: PresetBaselineVerdict,
    pub fast: PresetBaselineVerdict,
    pub gate_met: bool,
    pub refusals: Vec<String>,
}

pub fn analyze(
    quality: &MeasurementReport,
    fast: &MeasurementReport,
    audit: &AuditSeal,
    threshold_source: &M7ThresholdSource,
) -> Result<BaselineCourtVerdict, String> {
    let gates_file = &threshold_source.gates;
    validate(quality, vice_core::Preset::Quality)?;
    validate(fast, vice_core::Preset::Fast)?;
    if audit.status != SealStatus::Opened
        || audit.gates_hash != gates_file.sha256
        || audit.prereg_hash != Preregistration::v1().hash()
        || quality.procedural_generation != audit.generation
        || fast.procedural_generation != audit.generation
        || quality.population_policy != M7_SEALED_POPULATION_POLICY
        || fast.population_policy != M7_SEALED_POPULATION_POLICY
    {
        return Err(
            "baseline court is not bound to this opened audit, gates, and preregistration".into(),
        );
    }
    let gates = CourtGates::from_file(gates_file)?;
    let quality_report_sha256 = super::report_content_sha256(quality);
    let fast_report_sha256 = super::report_content_sha256(fast);
    let population_commitment_sha256 = quality.population_commitment_sha256.clone();
    let quality = analyze_preset(quality, gates)?;
    let fast = analyze_preset(fast, gates)?;
    let mut refusals = Vec::new();
    if !quality.gate_met {
        refusals.push("Quality did not beat the frozen internal baseline court".into());
    }
    if !fast.gate_met {
        refusals.push("Fast did not beat the frozen internal baseline court".into());
    }
    Ok(BaselineCourtVerdict {
        schema: M7_BASELINE_COURT_SCHEMA,
        audit_generation: audit.generation,
        corpus_sha256: audit.corpus_hash.clone(),
        population_commitment_sha256,
        preregistration_sha256: audit.prereg_hash.clone(),
        gates_sha256: audit.gates_hash.clone(),
        release_commit_sha: threshold_source.event_commit_sha.clone(),
        runner_attestation_sha256: threshold_source.attestation_sha256.clone(),
        gate_provenance_sha256: threshold_source.provenance_sha256.clone(),
        quality_report_sha256,
        fast_report_sha256,
        gate_met: refusals.is_empty(),
        quality,
        fast,
        refusals,
    })
}

fn validate(report: &MeasurementReport, preset: vice_core::Preset) -> Result<(), String> {
    super::validate_sealed_population(report)?;
    super::validate_execution_attestation(report)?;
    if report.preset != preset {
        return Err(format!(
            "baseline court requires a complete {preset:?} sealed-audit report"
        ));
    }
    Ok(())
}

fn analyze_preset(
    report: &MeasurementReport,
    gates: CourtGates,
) -> Result<PresetBaselineVerdict, String> {
    let rows = target_rows(report)
        .into_iter()
        .filter(|row| row.production_accepted && row.production_provenance)
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return Err(format!("{:?}: no accepted target rows", report.preset));
    }
    let missing = rows
        .iter()
        .filter(|row| {
            (row.internal_baseline.is_none() && row.internal_baseline_refusals.is_empty())
                || row.selected_complexity.is_none()
        })
        .count();
    if missing != 0 {
        return Err(format!(
            "{:?}: {missing} accepted rows lack a measured or typed-refused internal baseline",
            report.preset
        ));
    }
    let contradictory = rows
        .iter()
        .filter(|row| row.internal_baseline.is_some() && !row.internal_baseline_refusals.is_empty())
        .count();
    if contradictory != 0 {
        return Err(format!(
            "{:?}: {contradictory} accepted rows report both a measured and refused baseline",
            report.preset
        ));
    }
    let paired_rows = rows
        .iter()
        .copied()
        .filter(|row| row.internal_baseline.is_some())
        .collect::<Vec<_>>();
    if paired_rows.is_empty() {
        return Err(format!(
            "{:?}: no accepted rows have a measurable paired baseline",
            report.preset
        ));
    }
    let baseline_refused_rows = rows.len().saturating_sub(paired_rows.len()) as u64;

    let selected_p95 = paired_rows
        .iter()
        .map(|row| row.boundary.as_ref().expect("accepted court row").p95_px)
        .collect::<Vec<_>>();
    let selected_p99 = paired_rows
        .iter()
        .map(|row| row.boundary.as_ref().expect("accepted court row").p99_px)
        .collect::<Vec<_>>();
    let selected_max = paired_rows
        .iter()
        .map(|row| row.boundary.as_ref().expect("accepted court row").max_px)
        .collect::<Vec<_>>();
    let baseline_p95 = paired_rows
        .iter()
        .map(|row| {
            row.internal_baseline
                .as_ref()
                .expect("checked")
                .boundary
                .p95_px
        })
        .collect::<Vec<_>>();
    let baseline_p99 = paired_rows
        .iter()
        .map(|row| {
            row.internal_baseline
                .as_ref()
                .expect("checked")
                .boundary
                .p99_px
        })
        .collect::<Vec<_>>();
    let baseline_max = paired_rows
        .iter()
        .map(|row| {
            row.internal_baseline
                .as_ref()
                .expect("checked")
                .boundary
                .max_px
        })
        .collect::<Vec<_>>();
    let tails = TailComparison {
        selected_p95_px: quantile(&selected_p95, 0.95),
        baseline_p95_px: quantile(&baseline_p95, 0.95),
        selected_p99_px: quantile(&selected_p99, 0.99),
        baseline_p99_px: quantile(&baseline_p99, 0.99),
        selected_max_px: max(&selected_max),
        baseline_max_px: max(&baseline_max),
        no_tail_regression: false,
        strict_tail_improvement: false,
    };
    let tails = TailComparison {
        no_tail_regression: tails.selected_p95_px <= tails.baseline_p95_px
            && tails.selected_p99_px <= tails.baseline_p99_px
            && tails.selected_max_px <= tails.baseline_max_px,
        strict_tail_improvement: tails.selected_p95_px < tails.baseline_p95_px
            || tails.selected_p99_px < tails.baseline_p99_px
            || tails.selected_max_px < tails.baseline_max_px,
        ..tails
    };

    let selected_complexity = sum_complexity(
        paired_rows
            .iter()
            .map(|row| row.selected_complexity.as_ref().expect("checked")),
    );
    let baseline_complexity = sum_complexity(
        paired_rows
            .iter()
            .map(|row| &row.internal_baseline.as_ref().expect("checked").complexity),
    );
    let ratios = [
        ratio(selected_complexity.vertices, baseline_complexity.vertices),
        ratio(
            selected_complexity.boundaries,
            baseline_complexity.boundaries,
        ),
        ratio(
            selected_complexity.curve_segments,
            baseline_complexity.curve_segments,
        ),
        ratio(
            selected_complexity.canonical_delivery_bytes,
            baseline_complexity.canonical_delivery_bytes,
        ),
    ];
    let complexity = ComplexityComparison {
        vertices_growth_ratio: ratios[0],
        boundaries_growth_ratio: ratios[1],
        curve_segments_growth_ratio: ratios[2],
        canonical_delivery_bytes_growth_ratio: ratios[3],
        max_growth_ratio: ratios.into_iter().fold(0.0, f64::max),
        gate: gates.max_complexity_growth_ratio,
        gate_met: ratios
            .into_iter()
            .all(|value| value <= gates.max_complexity_growth_ratio),
    };

    let grouped = group_rows(&rows);
    let paired_grouped = group_rows(&paired_rows);
    let selected_catastrophic_source_groups = grouped
        .values()
        .filter(|group| {
            group
                .iter()
                .any(|row| !catastrophic_with_gates(row, gates.release).is_empty())
        })
        .count() as u64;
    let baseline_catastrophic_source_groups = grouped
        .values()
        .filter(|group| {
            group
                .iter()
                .any(|row| baseline_row_catastrophic(row, gates.release))
        })
        .count() as u64;
    let catastrophic_not_worse =
        selected_catastrophic_source_groups <= baseline_catastrophic_source_groups;
    let blind = blind_court(&grouped, report, gates);
    let mut refusals = Vec::new();
    if !catastrophic_not_worse {
        refusals
            .push("selected candidate has more catastrophic source groups than baseline".into());
    }
    if !tails.no_tail_regression || !tails.strict_tail_improvement {
        refusals
            .push("selected candidate does not strictly improve a tail without regression".into());
    }
    if !complexity.gate_met {
        refusals.push("selected candidate exceeds the frozen complexity-growth gate".into());
    }
    if !blind.gate_met {
        refusals.push("identity-blind source-level paired court is not significant".into());
    }
    Ok(PresetBaselineVerdict {
        preset: report.preset,
        accepted_paired_rows: paired_rows.len() as u64,
        baseline_refused_rows,
        paired_source_groups: paired_grouped.len() as u64,
        selected_catastrophic_source_groups,
        baseline_catastrophic_source_groups,
        catastrophic_not_worse,
        tails,
        complexity,
        blind,
        gate_met: refusals.is_empty(),
        refusals,
    })
}

fn baseline_catastrophic(baseline: &InternalBaselineMeasurement, gates: M7ReleaseGates) -> bool {
    !baseline.topology.exact
        || !baseline.verifier_clean
        || baseline.max_palette_code_delta > gates.max_palette_code_delta
        || baseline.profile_max_channel_delta > gates.max_profile_channel_delta
        || baseline.profile_mean_channel_delta > gates.max_profile_mean_channel_delta
        || baseline.internal_to_pure_max_channel_delta > gates.max_internal_channel_delta
        || baseline.internal_to_pure_mean_channel_delta > gates.max_internal_mean_channel_delta
        || baseline.internal_to_seam_max_channel_delta > gates.max_internal_channel_delta
        || baseline.internal_to_seam_mean_channel_delta > gates.max_internal_mean_channel_delta
}

fn baseline_row_catastrophic(row: &MeasurementRow, gates: M7ReleaseGates) -> bool {
    row.internal_baseline.as_ref().map_or_else(
        || !row.internal_baseline_refusals.is_empty(),
        |baseline| baseline_catastrophic(baseline, gates),
    )
}

fn group_rows<'a>(rows: &[&'a MeasurementRow]) -> BTreeMap<String, Vec<&'a MeasurementRow>> {
    let mut grouped = BTreeMap::<String, Vec<&MeasurementRow>>::new();
    for row in rows {
        grouped.entry(row.group_id.clone()).or_default().push(*row);
    }
    grouped
}

#[derive(Debug, Clone, PartialEq)]
struct BlindMetrics {
    catastrophic: u64,
    boundary_p95_sum: f64,
    boundary_p99_sum: f64,
    boundary_max: f64,
    palette_sum: u64,
    curve_segments: u64,
    delivery_bytes: u64,
}

fn blind_court(
    grouped: &BTreeMap<String, Vec<&MeasurementRow>>,
    report: &MeasurementReport,
    gates: CourtGates,
) -> BlindVerdict {
    let mut trials = Vec::new();
    let mut selected_wins = 0u64;
    let mut baseline_wins = 0u64;
    let mut ties = 0u64;
    for (source_group, rows) in grouped {
        let selected = selected_metrics(rows, gates.release);
        let baseline = baseline_metrics(rows, gates.release);
        let selected_bundle = bundle_digest(
            rows.iter()
                .filter_map(|row| row.selected_artifact_bundle_sha256.as_deref()),
        );
        let baseline_bundle = baseline_bundle_digest(rows);
        let commitment = hex::encode(Sha256::digest(
            format!(
                "m7-blind-v1|{}|{}|{}|{}",
                report.identity.config_sha256,
                report.delivery_policy_sha256,
                source_group,
                report.preset as u8
            )
            .as_bytes(),
        ));
        let selected_on_left =
            u8::from_str_radix(&commitment[..2], 16).expect("hex digest") % 2 == 0;
        let (left_metrics, right_metrics, left_bundle, right_bundle) = if selected_on_left {
            (&selected, &baseline, &selected_bundle, &baseline_bundle)
        } else {
            (&baseline, &selected, &baseline_bundle, &selected_bundle)
        };
        // The judge receives only the randomized metric packets. Candidate
        // identity is revealed after the choice has been recorded.
        let choice = judge_blind(left_metrics, right_metrics);
        let selected_preferred = (selected_on_left && choice == BlindChoice::Left)
            || (!selected_on_left && choice == BlindChoice::Right);
        let baseline_preferred = (selected_on_left && choice == BlindChoice::Right)
            || (!selected_on_left && choice == BlindChoice::Left);
        selected_wins += u64::from(selected_preferred);
        baseline_wins += u64::from(baseline_preferred);
        ties += u64::from(choice == BlindChoice::Tie);
        trials.push(BlindTrial {
            source_group: source_group.clone(),
            presentation_commitment_sha256: commitment,
            left_artifact_bundle_sha256: left_bundle.clone(),
            right_artifact_bundle_sha256: right_bundle.clone(),
            judge_choice_before_reveal: choice,
            reveal_selected_side: if selected_on_left { "left" } else { "right" }.into(),
            selected_preferred,
            baseline_preferred,
        });
    }
    let non_tied_trials = selected_wins + baseline_wins;
    let selected_preference_rate = if non_tied_trials == 0 {
        0.0
    } else {
        selected_wins as f64 / non_tied_trials as f64
    };
    let p = one_sided_binomial_tail(selected_wins, non_tied_trials);
    let gate_met = trials.len() as u64 >= gates.min_blind_source_trials
        && selected_preference_rate >= gates.min_blind_preference_rate
        && p <= gates.max_blind_one_sided_p_value;
    BlindVerdict {
        source_trials: trials.len() as u64,
        non_tied_trials,
        selected_wins,
        baseline_wins,
        ties,
        selected_preference_rate,
        exact_one_sided_binomial_p_value: p,
        min_source_trials: gates.min_blind_source_trials,
        min_preference_rate: gates.min_blind_preference_rate,
        max_one_sided_p_value: gates.max_blind_one_sided_p_value,
        trials,
        gate_met,
    }
}

fn selected_metrics(rows: &[&MeasurementRow], gates: M7ReleaseGates) -> BlindMetrics {
    BlindMetrics {
        catastrophic: rows
            .iter()
            .filter(|row| !catastrophic_with_gates(row, gates).is_empty())
            .count() as u64,
        boundary_p95_sum: rows
            .iter()
            .map(|row| row.boundary.as_ref().expect("accepted").p95_px)
            .sum(),
        boundary_p99_sum: rows
            .iter()
            .map(|row| row.boundary.as_ref().expect("accepted").p99_px)
            .sum(),
        boundary_max: rows
            .iter()
            .map(|row| row.boundary.as_ref().expect("accepted").max_px)
            .fold(0.0, f64::max),
        palette_sum: rows
            .iter()
            .map(|row| u64::from(row.max_palette_code_delta.expect("accepted")))
            .sum(),
        curve_segments: rows
            .iter()
            .map(|row| {
                row.selected_complexity
                    .as_ref()
                    .expect("checked")
                    .curve_segments
            })
            .sum(),
        delivery_bytes: rows
            .iter()
            .map(|row| {
                row.selected_complexity
                    .as_ref()
                    .expect("checked")
                    .canonical_delivery_bytes
            })
            .sum(),
    }
}

fn baseline_metrics(rows: &[&MeasurementRow], gates: M7ReleaseGates) -> BlindMetrics {
    let penalty_p95 = gates.boundary_p95_px + 1.0;
    let penalty_p99 = gates.boundary_p99_px + 1.0;
    let penalty_max = gates.boundary_max_px + 1.0;
    BlindMetrics {
        catastrophic: rows
            .iter()
            .filter(|row| baseline_row_catastrophic(row, gates))
            .count() as u64,
        boundary_p95_sum: rows
            .iter()
            .map(|row| {
                row.internal_baseline
                    .as_ref()
                    .map_or(penalty_p95, |baseline| baseline.boundary.p95_px)
            })
            .sum(),
        boundary_p99_sum: rows
            .iter()
            .map(|row| {
                row.internal_baseline
                    .as_ref()
                    .map_or(penalty_p99, |baseline| baseline.boundary.p99_px)
            })
            .sum(),
        boundary_max: rows
            .iter()
            .map(|row| {
                row.internal_baseline
                    .as_ref()
                    .map_or(penalty_max, |baseline| baseline.boundary.max_px)
            })
            .fold(0.0, f64::max),
        palette_sum: rows
            .iter()
            .map(|row| {
                row.internal_baseline
                    .as_ref()
                    .map_or(u64::from(gates.max_palette_code_delta) + 1, |baseline| {
                        u64::from(baseline.max_palette_code_delta)
                    })
            })
            .fold(0u64, u64::saturating_add),
        curve_segments: rows
            .iter()
            .map(|row| {
                row.internal_baseline
                    .as_ref()
                    .map_or(u64::MAX / 1024, |baseline| {
                        baseline.complexity.curve_segments
                    })
            })
            .fold(0u64, u64::saturating_add),
        delivery_bytes: rows
            .iter()
            .map(|row| {
                row.internal_baseline
                    .as_ref()
                    .map_or(u64::MAX / 1024, |baseline| {
                        baseline.complexity.canonical_delivery_bytes
                    })
            })
            .fold(0u64, u64::saturating_add),
    }
}

fn judge_blind(left: &BlindMetrics, right: &BlindMetrics) -> BlindChoice {
    use std::cmp::Ordering;
    for order in [
        left.catastrophic.cmp(&right.catastrophic),
        left.boundary_p95_sum.total_cmp(&right.boundary_p95_sum),
        left.boundary_p99_sum.total_cmp(&right.boundary_p99_sum),
        left.boundary_max.total_cmp(&right.boundary_max),
        left.palette_sum.cmp(&right.palette_sum),
        left.curve_segments.cmp(&right.curve_segments),
        left.delivery_bytes.cmp(&right.delivery_bytes),
    ] {
        match order {
            Ordering::Less => return BlindChoice::Left,
            Ordering::Greater => return BlindChoice::Right,
            Ordering::Equal => {}
        }
    }
    BlindChoice::Tie
}

fn bundle_digest<'a>(digests: impl Iterator<Item = &'a str>) -> String {
    let mut digests = digests.collect::<Vec<_>>();
    digests.sort_unstable();
    let mut hash = Sha256::new();
    for digest in digests {
        hash.update((digest.len() as u64).to_be_bytes());
        hash.update(digest.as_bytes());
    }
    hex::encode(hash.finalize())
}

fn baseline_bundle_digest(rows: &[&MeasurementRow]) -> String {
    let mut entries = Vec::new();
    for row in rows {
        if let Some(baseline) = &row.internal_baseline {
            entries.push(baseline.artifact_bundle_sha256.clone());
        } else {
            for refusal in &row.internal_baseline_refusals {
                entries.push(hex::encode(Sha256::digest(
                    format!("m7-baseline-refusal-v1|{}|{refusal}", row.cell_id).as_bytes(),
                )));
            }
        }
    }
    entries.sort_unstable();
    bundle_digest(entries.iter().map(String::as_str))
}

fn sum_complexity<'a>(values: impl Iterator<Item = &'a SceneComplexity>) -> SceneComplexity {
    values.fold(
        SceneComplexity {
            vertices: 0,
            boundaries: 0,
            curve_segments: 0,
            canonical_delivery_bytes: 0,
        },
        |mut total, value| {
            total.vertices = total.vertices.saturating_add(value.vertices);
            total.boundaries = total.boundaries.saturating_add(value.boundaries);
            total.curve_segments = total.curve_segments.saturating_add(value.curve_segments);
            total.canonical_delivery_bytes = total
                .canonical_delivery_bytes
                .saturating_add(value.canonical_delivery_bytes);
            total
        },
    )
}

fn ratio(selected: u64, baseline: u64) -> f64 {
    match (selected, baseline) {
        (0, 0) => 1.0,
        (_, 0) => f64::MAX,
        _ => selected as f64 / baseline as f64,
    }
}

fn quantile(values: &[f64], q: f64) -> f64 {
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    let index = ((values.len() - 1) as f64 * q).ceil() as usize;
    values[index.min(values.len() - 1)]
}

fn max(values: &[f64]) -> f64 {
    values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

fn one_sided_binomial_tail(wins: u64, trials: u64) -> f64 {
    if trials == 0 {
        return 1.0;
    }
    let mut probability = 2.0f64.powi(-(trials as i32));
    let mut tail = if wins == 0 { probability } else { 0.0 };
    for k in 0..trials {
        probability *= (trials - k) as f64 / (k + 1) as f64;
        if k + 1 >= wins {
            tail += probability;
        }
    }
    tail.min(1.0)
}

#[cfg(test)]
mod tests;
