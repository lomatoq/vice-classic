//! M8 partition x paint factorial design (spec v1.3 section 27.6).
//!
//! This module accepts only [`CommensurableArms`], whose members already
//! carry measured compatibility keys.  Consequently a backend/config/budget/
//! fixture/schema mismatch is rejected before any paint effect can exist.

use serde::Serialize;

use super::crime::InverseCrime;
use super::key::{
    sealed, CausalDelta, CommensurableArms, CompatibilityKey, FactorialArm, Incommensurable,
    KeyedMeasurement, MissingArm, Reduce,
};
use vice_image::ObservationTensor;
use vice_ir::color::PremulRgba;
use vice_render::PartitionRender;

pub const PAINT_INTERVENTION_SCHEMA_VERSION: &str = "vice-classic/paint-oracle/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaintSource {
    Auto,
    GroundTruth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PaintPartitionSource {
    Auto,
    GroundTruth,
}

/// PPxy: x is GT partition injection, y is GT paint injection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PaintArm {
    Pp00,
    Pp10,
    Pp01,
    Pp11,
}

impl PaintArm {
    pub const ALL: &'static [PaintArm] = &[
        PaintArm::Pp00,
        PaintArm::Pp10,
        PaintArm::Pp01,
        PaintArm::Pp11,
    ];

    pub fn id(self) -> &'static str {
        match self {
            PaintArm::Pp00 => "PP00",
            PaintArm::Pp10 => "PP10",
            PaintArm::Pp01 => "PP01",
            PaintArm::Pp11 => "PP11",
        }
    }

    pub fn partition(self) -> PaintPartitionSource {
        match self {
            PaintArm::Pp00 | PaintArm::Pp01 => PaintPartitionSource::Auto,
            PaintArm::Pp10 | PaintArm::Pp11 => PaintPartitionSource::GroundTruth,
        }
    }

    pub fn paint(self) -> PaintSource {
        match self {
            PaintArm::Pp00 | PaintArm::Pp10 => PaintSource::Auto,
            PaintArm::Pp01 | PaintArm::Pp11 => PaintSource::GroundTruth,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaintFactorialEffects {
    pub metric: String,
    pub key_fingerprint: String,
    pub present_arms: Vec<String>,
    pub partition_main_effect: CausalDelta,
    pub paint_main_effect: CausalDelta,
    pub interaction: CausalDelta,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PaintOracleError {
    #[error("paint factorial is incomplete; missing arms: {missing:?}")]
    Incomplete { missing: Vec<String> },
    #[error(transparent)]
    MissingArm(#[from] MissingArm),
    #[error("paint-oracle partition, paint, and observation dimensions disagree")]
    DimensionMismatch,
    #[error("paint-oracle measurement is non-finite")]
    NonFinite,
    #[error(transparent)]
    Incommensurable(#[from] Incommensurable),
}

pub const PAINT_ORACLE_METRIC: &str = "mean_squared_premul_error";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaintMeasurementRow {
    pub arm: PaintArm,
    pub partition_source: PaintPartitionSource,
    pub paint_source: PaintSource,
    pub fixture_hash: String,
    pub measured_error: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaintOracleMeasurementReport {
    pub schema: &'static str,
    pub metric: &'static str,
    pub key_fingerprint: String,
    pub rows: Vec<PaintMeasurementRow>,
    pub effects: PaintFactorialEffects,
}

#[derive(Debug)]
struct PaintMeasurement {
    key: CompatibilityKey,
    value: f64,
}

impl sealed::Sealed for PaintMeasurement {}
impl KeyedMeasurement for PaintMeasurement {
    fn measurement_key(&self) -> &CompatibilityKey {
        &self.key
    }
    fn measurement_value(&self, metric: &str) -> Option<f64> {
        (metric == PAINT_ORACLE_METRIC).then_some(self.value)
    }
    fn measurement_crime(&self) -> &InverseCrime {
        &InverseCrime::Clean
    }
}

/// Measure the complete 2x2 partition x paint intervention on one common
/// observation. Every arm uses the same arithmetic and compatibility key;
/// only the two typed intervention inputs change.
pub fn measure_paint_oracle_fixture(
    observation: &ObservationTensor,
    auto_partition: &PartitionRender,
    gt_partition: &PartitionRender,
    auto_paints: &[PremulRgba],
    gt_paints: &[PremulRgba],
    key: CompatibilityKey,
) -> Result<PaintOracleMeasurementReport, PaintOracleError> {
    let n = observation.len();
    let valid_partition = |partition: &PartitionRender| {
        partition.width_px == observation.width_px()
            && partition.height_px == observation.height_px()
            && partition.face_coverage.len() == auto_paints.len()
            && partition.face_coverage.len() == gt_paints.len()
            && partition
                .face_coverage
                .iter()
                .all(|coverage| coverage.len() == n)
    };
    if !valid_partition(auto_partition) || !valid_partition(gt_partition) {
        return Err(PaintOracleError::DimensionMismatch);
    }
    let mut set = CommensurableArms::new();
    let mut rows = Vec::new();
    for &arm in PaintArm::ALL {
        let partition = match arm.partition() {
            PaintPartitionSource::Auto => auto_partition,
            PaintPartitionSource::GroundTruth => gt_partition,
        };
        let paints = match arm.paint() {
            PaintSource::Auto => auto_paints,
            PaintSource::GroundTruth => gt_paints,
        };
        let mut squared = 0.0;
        for pixel in 0..n {
            let mut predicted = [0.0; 4];
            for (coverage, paint) in partition.face_coverage.iter().zip(paints) {
                let amount = coverage[pixel];
                for (out, channel) in predicted
                    .iter_mut()
                    .zip([paint.r, paint.g, paint.b, paint.a])
                {
                    *out += amount * channel;
                }
            }
            for (predicted, observed) in predicted.into_iter().zip(observation.premul(pixel)) {
                squared += (predicted - observed).powi(2);
            }
        }
        let value = squared / (n as f64 * 4.0);
        if !value.is_finite() {
            return Err(PaintOracleError::NonFinite);
        }
        let measurement = PaintMeasurement {
            key: key.clone(),
            value,
        };
        let aggregated =
            FactorialArm::aggregate(arm.id(), PAINT_ORACLE_METRIC, &[&measurement], Reduce::Mean)?;
        set.insert(&aggregated)?;
        rows.push(PaintMeasurementRow {
            arm,
            partition_source: arm.partition(),
            paint_source: arm.paint(),
            fixture_hash: key.fixture_hash.clone(),
            measured_error: value,
        });
    }
    let effects = paint_effects(PAINT_ORACLE_METRIC, &set)?;
    Ok(PaintOracleMeasurementReport {
        schema: PAINT_INTERVENTION_SCHEMA_VERSION,
        metric: PAINT_ORACLE_METRIC,
        key_fingerprint: effects.key_fingerprint.clone(),
        rows,
        effects,
    })
}

/// Build all three 2x2 effects.  There is deliberately no partial output:
/// a two-arm ladder must not be published under a factorial name.
pub fn paint_effects(
    metric: &str,
    arms: &CommensurableArms,
) -> Result<PaintFactorialEffects, PaintOracleError> {
    let missing = PaintArm::ALL
        .iter()
        .filter(|arm| !arms.contains(arm.id()))
        .map(|arm| arm.id().to_string())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(PaintOracleError::Incomplete { missing });
    }
    let partition_main_effect = arms.contrast(
        &format!("partition_main_effect[{metric}]"),
        &[("PP10", 0.5), ("PP00", -0.5), ("PP11", 0.5), ("PP01", -0.5)],
    )?;
    let paint_main_effect = arms.contrast(
        &format!("paint_main_effect[{metric}]"),
        &[("PP01", 0.5), ("PP00", -0.5), ("PP11", 0.5), ("PP10", -0.5)],
    )?;
    let interaction = arms.contrast(
        &format!("partition_x_paint_interaction[{metric}]"),
        &[("PP11", 0.5), ("PP01", -0.5), ("PP10", -0.5), ("PP00", 0.5)],
    )?;
    Ok(PaintFactorialEffects {
        metric: metric.to_string(),
        key_fingerprint: arms.fingerprint(),
        present_arms: PaintArm::ALL
            .iter()
            .map(|arm| arm.id().to_string())
            .collect(),
        partition_main_effect,
        paint_main_effect,
        interaction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oracle::crime::InverseCrime;
    use crate::oracle::key::{
        sealed, CandidateBudget, CompatibilityKey, FactorialArm, Incommensurable, KeyedMeasurement,
        Reduce,
    };
    use vice_image::{CanonicalImage, IccAssumption};
    use vice_ir::BlendSpace;

    struct Measurement(CompatibilityKey, f64);
    impl sealed::Sealed for Measurement {}
    impl KeyedMeasurement for Measurement {
        fn measurement_key(&self) -> &CompatibilityKey {
            &self.0
        }
        fn measurement_value(&self, _: &str) -> Option<f64> {
            Some(self.1)
        }
        fn measurement_crime(&self) -> &InverseCrime {
            &InverseCrime::Clean
        }
    }

    fn key() -> CompatibilityKey {
        CompatibilityKey {
            backend_id: "same-backend".into(),
            config_hash: "same-config".into(),
            candidate_budget: CandidateBudget::Candidates { max: 8 },
            fixture_hash: "same-fixture-population".into(),
            intervention_schema_version: PAINT_INTERVENTION_SCHEMA_VERSION.into(),
        }
    }

    fn set(values: [f64; 4]) -> CommensurableArms {
        let mut set = CommensurableArms::new();
        for (arm, value) in PaintArm::ALL.iter().zip(values) {
            let measurement = Measurement(key(), value);
            let row =
                FactorialArm::aggregate(arm.id(), "error", &[&measurement], Reduce::Mean).unwrap();
            set.insert(&row).unwrap();
        }
        set
    }

    #[test]
    fn complete_four_arm_matrix_yields_two_main_effects_and_interaction() {
        let effects = paint_effects("error", &set([10.0, 6.0, 8.0, 2.0])).unwrap();
        assert_eq!(effects.partition_main_effect.value(), -5.0);
        assert_eq!(effects.paint_main_effect.value(), -3.0);
        assert_eq!(effects.interaction.value(), -1.0);
        assert_eq!(effects.present_arms.len(), 4);
    }

    #[test]
    fn three_arms_are_a_refusal_not_a_fake_zero_arm() {
        let mut arms = CommensurableArms::new();
        for arm in &PaintArm::ALL[..3] {
            let measurement = Measurement(key(), 1.0);
            let row =
                FactorialArm::aggregate(arm.id(), "error", &[&measurement], Reduce::Mean).unwrap();
            arms.insert(&row).unwrap();
        }
        assert!(matches!(
            paint_effects("error", &arms),
            Err(PaintOracleError::Incomplete { .. })
        ));
    }

    #[test]
    fn an_incompatible_backend_is_rejected_before_effect_arithmetic() {
        let mut arms = CommensurableArms::new();
        for (index, arm) in PaintArm::ALL.iter().enumerate() {
            let mut k = key();
            if index == 3 {
                k.backend_id = "other-backend".into();
            }
            let measurement = Measurement(k, 1.0);
            let row =
                FactorialArm::aggregate(arm.id(), "error", &[&measurement], Reduce::Mean).unwrap();
            let inserted = arms.insert(&row);
            if index == 3 {
                assert!(matches!(
                    inserted,
                    Err(Incommensurable::KeyMismatch {
                        component: "backend_id",
                        ..
                    })
                ));
            } else {
                inserted.unwrap();
            }
        }
        assert!(matches!(
            paint_effects("error", &arms),
            Err(PaintOracleError::Incomplete { .. })
        ));
    }

    #[test]
    fn actual_four_arm_measurement_uses_one_observation_and_no_fake_arm() {
        let image = CanonicalImage::from_straight_srgb8(
            2,
            1,
            vec![255, 0, 0, 255, 0, 255, 0, 255],
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap();
        let observation = ObservationTensor::of(&image, BlendSpace::LinearLight);
        let gt = PartitionRender {
            width_px: 2,
            height_px: 1,
            face_coverage: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
            composite: Vec::new(),
        };
        let auto = PartitionRender {
            width_px: 2,
            height_px: 1,
            face_coverage: vec![vec![0.75, 0.25], vec![0.25, 0.75]],
            composite: Vec::new(),
        };
        let gt_paints = [
            PremulRgba {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            PremulRgba {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            },
        ];
        let auto_paints = [
            PremulRgba {
                r: 0.8,
                g: 0.1,
                b: 0.0,
                a: 1.0,
            },
            PremulRgba {
                r: 0.1,
                g: 0.8,
                b: 0.0,
                a: 1.0,
            },
        ];
        let report =
            measure_paint_oracle_fixture(&observation, &auto, &gt, &auto_paints, &gt_paints, key())
                .unwrap();
        assert_eq!(report.rows.len(), 4);
        assert_eq!(report.rows[3].arm, PaintArm::Pp11);
        assert_eq!(report.rows[3].measured_error, 0.0);
        assert!(report.rows[..3].iter().all(|row| row.measured_error > 0.0));
    }
}
