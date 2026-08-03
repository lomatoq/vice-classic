use super::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ObservablePolicy {
    pub(super) maximum_predictive_bits_per_block: f64,
    pub(super) maximum_support_displacement_px: f64,
    pub(super) maximum_evidence_palette_shift_codes: u8,
    pub(super) minimum_palette_support_px: u64,
    pub(super) maximum_palette_interval_radius_codes: u8,
}

pub(super) fn fixed_diagnostics_permit(
    row: &MeasurementRow,
    empirical_upper: f64,
    delivery_seal: vice_verify::DeliverySealConfig,
) -> bool {
    let margin = if row.delivery_classes == Some(1) {
        1024.0
    } else {
        row.top2_class_margin_bits.unwrap_or(f64::NEG_INFINITY)
    };
    margin >= PROPOSED_MIN_TOP2_CLASS_MARGIN_BITS
        && row
            .max_abs_lag1
            .is_some_and(|lag| lag <= PROPOSED_MAX_ABS_RESIDUAL_LAG1)
        && calibrated_entropy_upper_bound(row, empirical_upper, true)
            .is_some_and(|bits| bits <= PROPOSED_MAX_TOPOLOGY_ENTROPY_BITS)
        && calibrated_entropy_upper_bound(row, empirical_upper, false)
            .is_some_and(|bits| bits <= PROPOSED_MAX_FORMATION_ENTROPY_BITS)
        && row
            .perturbation_stability
            .is_some_and(|stability| stability >= PROPOSED_MIN_PERTURBATION_STABILITY)
        && row
            .evidence_palette_shift_codes
            .is_some_and(|_| row.palette_support_px.is_some_and(|support| support > 0))
        && row.palette_interval_radius_codes.is_some()
        && row
            .paint_calibration_class
            .as_deref()
            .is_some_and(|class| !class.is_empty() && !class.contains("unknown"))
        && delivery_diagnostics_permit(row, delivery_seal)
}

pub(super) fn diagnostics_permit(
    row: &MeasurementRow,
    empirical_upper: f64,
    delivery_seal: vice_verify::DeliverySealConfig,
    policy: ObservablePolicy,
) -> bool {
    fixed_diagnostics_permit(row, empirical_upper, delivery_seal)
        && row.serialized_pixel_bits_per_block.is_some_and(|bits| {
            bits.is_finite() && bits <= policy.maximum_predictive_bits_per_block
        })
        && row
            .support_isotopy_displacement_px
            .is_some_and(|displacement| {
                displacement.is_finite() && displacement <= policy.maximum_support_displacement_px
            })
        && row
            .evidence_palette_shift_codes
            .is_some_and(|shift| shift <= policy.maximum_evidence_palette_shift_codes)
        && row
            .palette_support_px
            .is_some_and(|support| support >= policy.minimum_palette_support_px)
        && row
            .palette_interval_radius_codes
            .is_some_and(|radius| radius <= policy.maximum_palette_interval_radius_codes)
}

pub(super) fn paint_calibration_classes<'a>(
    rows: impl Iterator<Item = &'a MeasurementRow>,
) -> Vec<vice_core::PaintCalibrationClass> {
    let mut groups = BTreeMap::<String, BTreeSet<String>>::new();
    for row in rows {
        if let Some(class) = row.paint_calibration_class.as_ref() {
            groups
                .entry(class.clone())
                .or_default()
                .insert(row.group_id.clone());
        }
    }
    groups
        .into_iter()
        .filter_map(|(name, groups)| {
            let accepted_source_groups = groups.len().try_into().unwrap_or(u64::MAX);
            (accepted_source_groups >= 2).then_some(vice_core::PaintCalibrationClass {
                name,
                accepted_source_groups,
            })
        })
        .collect()
}

fn policy_gate_bad(row: &MeasurementRow, delivery_seal: vice_verify::DeliverySealConfig) -> bool {
    !catastrophic_kinds(row, delivery_seal, PROPOSED_MAX_PALETTE_CODE_DELTA).is_empty()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn select_observable_policy(
    rows: &[&MeasurementRow],
    empirical_upper: f64,
    delivery_seal: vice_verify::DeliverySealConfig,
    minimum_source_coverage: f64,
    minimum_render_coverage: f64,
    confidence: f64,
    risk_target: f64,
) -> Result<Option<ObservablePolicy>, String> {
    let mut predictive_thresholds = rows
        .iter()
        .filter(|row| {
            row.candidate_available
                && fixed_diagnostics_permit(row, empirical_upper, delivery_seal)
                && row
                    .support_isotopy_displacement_px
                    .is_some_and(|value| value.is_finite() && value >= 0.0)
        })
        .filter_map(|row| row.serialized_pixel_bits_per_block)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .collect::<Vec<_>>();
    predictive_thresholds.sort_by(f64::total_cmp);
    predictive_thresholds.dedup_by(|left, right| left.total_cmp(right).is_eq());

    let mut best: Option<(f64, f64, ObservablePolicy)> = None;
    for maximum_predictive in predictive_thresholds {
        let predictive_eligible = rows
            .iter()
            .copied()
            .filter(|row| {
                row.candidate_available
                    && fixed_diagnostics_permit(row, empirical_upper, delivery_seal)
                    && row
                        .serialized_pixel_bits_per_block
                        .is_some_and(|value| value.is_finite() && value <= maximum_predictive)
                    && row
                        .support_isotopy_displacement_px
                        .is_some_and(|value| value.is_finite() && value >= 0.0)
            })
            .collect::<Vec<_>>();
        let good_supports = predictive_eligible
            .iter()
            .filter(|row| !policy_gate_bad(row, delivery_seal))
            .filter_map(|row| row.support_isotopy_displacement_px)
            .collect::<Vec<_>>();
        let Some(full_maximum_support) = good_supports.iter().copied().max_by(f64::total_cmp)
        else {
            continue;
        };
        let first_bad_support = predictive_eligible
            .iter()
            .filter(|row| policy_gate_bad(row, delivery_seal))
            .filter_map(|row| row.support_isotopy_displacement_px)
            .min_by(f64::total_cmp);
        let mut support_candidates = vec![full_maximum_support];
        if let Some(safe) = good_supports
            .iter()
            .copied()
            .filter(|support| first_bad_support.is_some_and(|bad| *support < bad))
            .max_by(f64::total_cmp)
        {
            support_candidates.push(safe);
        }
        support_candidates.sort_by(f64::total_cmp);
        support_candidates.dedup_by(|left, right| left.total_cmp(right).is_eq());

        for maximum_support in support_candidates {
            let support_eligible = predictive_eligible
                .iter()
                .copied()
                .filter(|row| {
                    row.support_isotopy_displacement_px
                        .is_some_and(|value| value <= maximum_support)
                })
                .collect::<Vec<_>>();
            let good_shifts = support_eligible
                .iter()
                .filter(|row| !policy_gate_bad(row, delivery_seal))
                .filter_map(|row| row.evidence_palette_shift_codes)
                .collect::<Vec<_>>();
            let Some(full_maximum_shift) = good_shifts.iter().copied().max() else {
                continue;
            };
            let first_bad_shift = support_eligible
                .iter()
                .filter(|row| policy_gate_bad(row, delivery_seal))
                .filter_map(|row| row.evidence_palette_shift_codes)
                .min();
            let mut shift_candidates = BTreeSet::from([full_maximum_shift]);
            if let Some(safe) = good_shifts
                .iter()
                .copied()
                .filter(|shift| first_bad_shift.is_some_and(|bad| *shift < bad))
                .max()
            {
                shift_candidates.insert(safe);
            }

            for maximum_shift in shift_candidates {
                let shift_eligible = support_eligible
                    .iter()
                    .copied()
                    .filter(|row| {
                        row.evidence_palette_shift_codes
                            .is_some_and(|shift| shift <= maximum_shift)
                    })
                    .collect::<Vec<_>>();
                let good_palette_supports = shift_eligible
                    .iter()
                    .filter(|row| !policy_gate_bad(row, delivery_seal))
                    .filter_map(|row| row.palette_support_px)
                    .filter(|support| *support > 0)
                    .collect::<Vec<_>>();
                let Some(full_minimum_palette_support) =
                    good_palette_supports.iter().copied().min()
                else {
                    continue;
                };
                let largest_bad_palette_support = shift_eligible
                    .iter()
                    .filter(|row| policy_gate_bad(row, delivery_seal))
                    .filter_map(|row| row.palette_support_px)
                    .max();
                let mut palette_support_candidates = BTreeSet::from([full_minimum_palette_support]);
                if let Some(safe) = good_palette_supports
                    .iter()
                    .copied()
                    .filter(|support| largest_bad_palette_support.is_some_and(|bad| *support > bad))
                    .min()
                {
                    palette_support_candidates.insert(safe);
                }

                for minimum_palette_support in palette_support_candidates {
                    let palette_support_eligible = shift_eligible
                        .iter()
                        .copied()
                        .filter(|row| {
                            row.palette_support_px
                                .is_some_and(|support| support >= minimum_palette_support)
                        })
                        .collect::<Vec<_>>();
                    let good_interval_radii = palette_support_eligible
                        .iter()
                        .filter(|row| !policy_gate_bad(row, delivery_seal))
                        .filter_map(|row| row.palette_interval_radius_codes)
                        .collect::<Vec<_>>();
                    let Some(full_maximum_interval_radius) =
                        good_interval_radii.iter().copied().max()
                    else {
                        continue;
                    };
                    let first_bad_interval_radius = palette_support_eligible
                        .iter()
                        .filter(|row| policy_gate_bad(row, delivery_seal))
                        .filter_map(|row| row.palette_interval_radius_codes)
                        .min();
                    let mut interval_candidates = BTreeSet::from([full_maximum_interval_radius]);
                    if let Some(safe) = good_interval_radii
                        .iter()
                        .copied()
                        .filter(|radius| first_bad_interval_radius.is_some_and(|bad| *radius < bad))
                        .max()
                    {
                        interval_candidates.insert(safe);
                    }

                    for maximum_interval_radius in interval_candidates {
                        let policy = ObservablePolicy {
                            maximum_predictive_bits_per_block: maximum_predictive,
                            maximum_support_displacement_px: maximum_support,
                            maximum_evidence_palette_shift_codes: maximum_shift,
                            minimum_palette_support_px: minimum_palette_support,
                            maximum_palette_interval_radius_codes: maximum_interval_radius,
                        };
                        let paint_classes =
                            paint_calibration_classes(rows.iter().copied().filter(|row| {
                                row.candidate_available
                                    && diagnostics_permit(
                                        row,
                                        empirical_upper,
                                        delivery_seal,
                                        policy,
                                    )
                            }));
                        let supported_paint_classes = paint_classes
                            .iter()
                            .map(|class| class.name.as_str())
                            .collect::<BTreeSet<_>>();
                        let outcomes = rows
                            .iter()
                            .map(|row| {
                                let accepted = row.candidate_available
                                    && diagnostics_permit(
                                        row,
                                        empirical_upper,
                                        delivery_seal,
                                        policy,
                                    )
                                    && row.paint_calibration_class.as_deref().is_some_and(
                                        |class| supported_paint_classes.contains(class),
                                    );
                                Ok(RenderOutcome {
                                    group_id: row.group_id.clone(),
                                    cell_id: row.cell_id.clone(),
                                    profile: RasterProfile::from_id(&row.rasterizer).ok_or_else(
                                        || {
                                            format!(
                                                "unknown rasterizer profile {:?}",
                                                row.rasterizer
                                            )
                                        },
                                    )?,
                                    accepted,
                                    catastrophic: accepted && policy_gate_bad(row, delivery_seal),
                                    mandatory: true,
                                })
                            })
                            .collect::<Result<Vec<_>, String>>()?;
                        let reliability = risk_coverage(
                            TARGET_BUCKET,
                            &outcomes,
                            confidence,
                            risk_target,
                            Some((ResidualModel::Block, true)),
                        );
                        if !reliability.contract_met
                            || reliability.groups_catastrophic != 0
                            || reliability.coverage_per_source < minimum_source_coverage
                            || reliability.coverage_per_render < minimum_render_coverage
                        {
                            continue;
                        }
                        let candidate = (
                            reliability.coverage_per_render,
                            reliability.coverage_per_source,
                            policy,
                        );
                        let replace = best.is_none_or(|current| {
                            candidate.0 > current.0
                                || (candidate.0 == current.0 && candidate.1 > current.1)
                                || (candidate.0 == current.0
                                    && candidate.1 == current.1
                                    && (candidate.2.maximum_predictive_bits_per_block
                                        < current.2.maximum_predictive_bits_per_block
                                        || (candidate.2.maximum_predictive_bits_per_block
                                            == current.2.maximum_predictive_bits_per_block
                                            && candidate.2.maximum_support_displacement_px
                                                < current.2.maximum_support_displacement_px)))
                        });
                        if replace {
                            best = Some(candidate);
                        }
                    }
                }
            }
        }
    }
    Ok(best.map(|(_, _, policy)| policy))
}
