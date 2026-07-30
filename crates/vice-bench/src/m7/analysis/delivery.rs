use super::{MeasurementRow, TARGET_BUCKET};
use serde::Serialize;

const PROFILE_CHANNEL_CAP: u8 = 8;
const PROFILE_MEAN_CAP: f64 = 0.01;
const INTERNAL_CHANNEL_CAP: u8 = 128;
// The serialized independent renderer may move boundary antialiasing by
// roughly one code on average while PurePartition and SeamSafe still agree
// to a far tighter profile court. Keep a finite two-code safety ceiling; the
// frozen proposal remains the measured value rounded to 0.25 code.
const INTERNAL_MEAN_CAP: f64 = 2.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeliveryCalibration {
    pub population: &'static str,
    pub candidate_rows: u64,
    pub observed_max_profile_channel_delta: u8,
    pub observed_max_profile_mean_channel_delta: f64,
    pub observed_max_internal_channel_delta: u8,
    pub observed_max_internal_mean_channel_delta: f64,
    pub rounding_policy: &'static str,
    pub proposal: vice_verify::DeliverySealConfig,
}

fn ceil_to_step(value: f64, step: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        (value / step).ceil() * step
    }
}

pub(super) fn calibrate_delivery_seal(
    rows: &[&MeasurementRow],
) -> Result<DeliveryCalibration, String> {
    let candidates = rows
        .iter()
        .copied()
        .filter(|row| row.candidate_available)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err("M7 delivery calibration has no candidate rows".into());
    }
    let observed_profile_channel = candidates
        .iter()
        .filter_map(|row| row.profile_max_channel_delta)
        .max()
        .ok_or_else(|| "candidate rows have no cross-profile channel diagnostics".to_string())?;
    let observed_profile_mean = candidates
        .iter()
        .filter_map(|row| row.profile_mean_channel_delta)
        .max_by(f64::total_cmp)
        .ok_or_else(|| "candidate rows have no cross-profile mean diagnostics".to_string())?;
    let observed_internal_channel = candidates
        .iter()
        .flat_map(|row| {
            [
                row.internal_to_pure_max_channel_delta,
                row.internal_to_seam_max_channel_delta,
            ]
        })
        .flatten()
        .max()
        .ok_or_else(|| "candidate rows have no internal channel diagnostics".to_string())?;
    let observed_internal_mean = candidates
        .iter()
        .flat_map(|row| {
            [
                row.internal_to_pure_mean_channel_delta,
                row.internal_to_seam_mean_channel_delta,
            ]
        })
        .flatten()
        .max_by(f64::total_cmp)
        .ok_or_else(|| "candidate rows have no internal mean diagnostics".to_string())?;
    if observed_profile_channel > PROFILE_CHANNEL_CAP
        || !observed_profile_mean.is_finite()
        || observed_profile_mean > PROFILE_MEAN_CAP
        || observed_internal_channel > INTERNAL_CHANNEL_CAP
        || !observed_internal_mean.is_finite()
        || observed_internal_mean > INTERNAL_MEAN_CAP
    {
        return Err(format!(
            "delivery calibration exceeds the safety envelope: profile max/mean \
             {observed_profile_channel}/{observed_profile_mean}, internal max/mean \
             {observed_internal_channel}/{observed_internal_mean}"
        ));
    }
    let proposal = vice_verify::DeliverySealConfig {
        max_profile_channel_delta: observed_profile_channel
            .checked_add(1)
            .unwrap_or(observed_profile_channel)
            .min(PROFILE_CHANNEL_CAP),
        max_profile_mean_channel_delta: ceil_to_step(observed_profile_mean, 0.0025)
            .min(PROFILE_MEAN_CAP),
        max_internal_channel_delta: observed_internal_channel
            .saturating_add(7)
            .checked_div(8)
            .unwrap_or(u8::MAX)
            .saturating_mul(8)
            .min(INTERNAL_CHANNEL_CAP),
        max_internal_mean_channel_delta: ceil_to_step(observed_internal_mean, 0.25)
            .min(INTERNAL_MEAN_CAP),
    };
    Ok(DeliveryCalibration {
        population: TARGET_BUCKET,
        candidate_rows: candidates.len().try_into().unwrap_or(u64::MAX),
        observed_max_profile_channel_delta: observed_profile_channel,
        observed_max_profile_mean_channel_delta: observed_profile_mean,
        observed_max_internal_channel_delta: observed_internal_channel,
        observed_max_internal_mean_channel_delta: observed_internal_mean,
        rounding_policy: "profile channel +1 code capped at 8; profile mean ceil 0.0025 capped \
                          at 0.01; internal channel ceil 8 codes capped at 128; internal mean ceil \
                          0.25 capped at 2.0",
        proposal,
    })
}

pub(super) fn delivery_diagnostics_permit(
    row: &MeasurementRow,
    seal: vice_verify::DeliverySealConfig,
) -> bool {
    row.profile_max_channel_delta
        .is_some_and(|value| value <= seal.max_profile_channel_delta)
        && row
            .profile_mean_channel_delta
            .is_some_and(|value| value <= seal.max_profile_mean_channel_delta)
        && row
            .internal_to_pure_max_channel_delta
            .is_some_and(|value| value <= seal.max_internal_channel_delta)
        && row
            .internal_to_pure_mean_channel_delta
            .is_some_and(|value| value <= seal.max_internal_mean_channel_delta)
        && row
            .internal_to_seam_max_channel_delta
            .is_some_and(|value| value <= seal.max_internal_channel_delta)
        && row
            .internal_to_seam_mean_channel_delta
            .is_some_and(|value| value <= seal.max_internal_mean_channel_delta)
}
