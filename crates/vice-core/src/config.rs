use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vice_opt::{
    model_universe_hash, BeamConfig, BlockLikelihoodConfig, ModelIdentity, SearchBudget,
    SupportedModelUniverseV1, TrustRegionConfig,
};
use vice_verify::{DeliverySealConfig, QuantizationPolicy, VerificationConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    Exact,
    Clean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    Fast,
    Quality,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VectorizeRequest {
    pub intent: Intent,
    pub preset: Preset,
    pub trace: bool,
    pub dump_candidates: usize,
    pub strict: bool,
    pub production: bool,
    pub research_override: bool,
    pub milestone_debug: Option<String>,
    /// Diagnostic/oracle palette input. Its presence is carried into the
    /// report and makes production success unrepresentable.
    pub oracle_override: Option<vice_evidence::Flat2Hypothesis>,
}

impl Default for VectorizeRequest {
    fn default() -> Self {
        Self {
            intent: Intent::Clean,
            preset: Preset::Quality,
            trace: false,
            dump_candidates: 0,
            strict: false,
            production: true,
            research_override: false,
            milestone_debug: None,
            oracle_override: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalibrationBucket {
    pub name: String,
    pub accepted_source_groups: u64,
    pub eligible_source_groups: u64,
    pub minimum_coverage: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfidenceCalibration {
    pub schema: String,
    pub model_universe_sha256: String,
    pub pricing_sha256: String,
    pub backend_sha256: String,
    pub config_sha256: String,
    pub calibration_split_sha256: String,
    pub sealed_audit_generation: String,
    pub sealed_audit_untouched: bool,
    pub confidence_level: f64,
    pub catastrophic_risk_target: f64,
    pub accepted_source_groups: u64,
    pub catastrophic_source_groups: u64,
    pub posterior_lower_bound_threshold: f64,
    pub minimum_top2_class_margin_bits: f64,
    pub maximum_posterior_predictive_bits_per_block: f64,
    pub maximum_abs_residual_lag1: f64,
    pub maximum_topology_entropy_bits: f64,
    pub maximum_formation_entropy_bits: f64,
    /// Frozen R1 upper bound on omitted search mass relative to the best
    /// retained hypothesis. `None` means truncated search remains Unknown.
    pub empirical_unexplored_relative_mass_upper_bound: Option<f64>,
    pub buckets: Vec<CalibrationBucket>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConfidenceMetrics {
    pub top2_class_margin_bits: f64,
    pub posterior_predictive_bits_per_block: f64,
    pub max_abs_residual_lag1: f64,
    pub topology_entropy_upper_bound: vice_opt::BoundValue<f64>,
    pub formation_entropy_upper_bound: vice_opt::BoundValue<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct IntentPriorPolicy {
    pub structural_code_scale: f64,
    pub constrained_promotion_extra_bits: f64,
}

impl ConfidenceCalibration {
    pub fn digest_sha256(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("calibration serializes");
        hex::encode(Sha256::digest(bytes))
    }

    pub fn zero_failure_risk_upper_bound(&self) -> Option<f64> {
        if self.catastrophic_source_groups != 0
            || self.accepted_source_groups == 0
            || !(0.0..1.0).contains(&self.confidence_level)
        {
            return None;
        }
        Some(1.0 - (1.0 - self.confidence_level).powf(1.0 / self.accepted_source_groups as f64))
    }

    pub fn permits(
        &self,
        identity: &ModelIdentity,
        delivery: &vice_opt::DeliveryPosterior,
        metrics: &ConfidenceMetrics,
    ) -> Result<(), &'static str> {
        self.validate_for_identity(identity)?;
        let posterior_lower_bound = match &delivery.posterior_lower_bound {
            vice_opt::BoundValue::Certified(value)
            | vice_opt::BoundValue::EmpiricallyCalibrated(value) => *value,
            vice_opt::BoundValue::Unknown => return Err("posterior_search_mass_unknown"),
        };
        if !posterior_lower_bound.is_finite()
            || posterior_lower_bound < self.posterior_lower_bound_threshold
        {
            return Err("posterior_below_calibrated_threshold");
        }
        if !metrics.top2_class_margin_bits.is_finite()
            || metrics.top2_class_margin_bits < self.minimum_top2_class_margin_bits
        {
            return Err("top2_margin_below_calibrated_threshold");
        }
        if !metrics.posterior_predictive_bits_per_block.is_finite()
            || metrics.posterior_predictive_bits_per_block
                > self.maximum_posterior_predictive_bits_per_block
        {
            return Err("posterior_predictive_mismatch");
        }
        if !metrics.max_abs_residual_lag1.is_finite()
            || metrics.max_abs_residual_lag1 > self.maximum_abs_residual_lag1
        {
            return Err("residual_spatial_mismatch");
        }
        let entropy_value = |bound: &vice_opt::BoundValue<f64>| match bound {
            vice_opt::BoundValue::Certified(value)
            | vice_opt::BoundValue::EmpiricallyCalibrated(value) => Some(*value),
            vice_opt::BoundValue::Unknown => None,
        };
        if entropy_value(&metrics.topology_entropy_upper_bound)
            .is_none_or(|value| !value.is_finite() || value > self.maximum_topology_entropy_bits)
        {
            return Err("topology_entropy_above_calibrated_threshold");
        }
        if entropy_value(&metrics.formation_entropy_upper_bound)
            .is_none_or(|value| !value.is_finite() || value > self.maximum_formation_entropy_bits)
        {
            return Err("formation_entropy_above_calibrated_threshold");
        }
        Ok(())
    }

    pub fn validate_for_identity(&self, identity: &ModelIdentity) -> Result<(), &'static str> {
        if self.schema != "vice-classic/confidence-calibration/v1"
            || self.model_universe_sha256 != identity.universe_sha256
            || self.pricing_sha256 != identity.pricing_sha256
            || self.backend_sha256 != identity.backend_sha256
            || self.config_sha256 != identity.config_sha256
            || !is_sha256(&self.calibration_split_sha256)
            || self.sealed_audit_generation.is_empty()
            || !self.sealed_audit_untouched
        {
            return Err("calibration_identity_or_audit");
        }
        let risk = self
            .zero_failure_risk_upper_bound()
            .ok_or("calibration_requires_zero_failure_exact_bound")?;
        if self.confidence_level != 0.99
            || self.catastrophic_risk_target != 0.01
            || self.accepted_source_groups < 459
            || risk >= self.catastrophic_risk_target
        {
            return Err("calibration_statistical_gate");
        }
        if !self.posterior_lower_bound_threshold.is_finite()
            || !(0.0..=1.0).contains(&self.posterior_lower_bound_threshold)
            || !self.minimum_top2_class_margin_bits.is_finite()
            || self.minimum_top2_class_margin_bits < 0.0
            || !self.maximum_posterior_predictive_bits_per_block.is_finite()
            || self.maximum_posterior_predictive_bits_per_block < 0.0
            || !self.maximum_abs_residual_lag1.is_finite()
            || !(0.0..=1.0).contains(&self.maximum_abs_residual_lag1)
            || !self.maximum_topology_entropy_bits.is_finite()
            || self.maximum_topology_entropy_bits < 0.0
            || !self.maximum_formation_entropy_bits.is_finite()
            || self.maximum_formation_entropy_bits < 0.0
            || self
                .empirical_unexplored_relative_mass_upper_bound
                .is_none_or(|bound| !bound.is_finite() || bound < 0.0)
        {
            return Err("calibration_search_mass_gate");
        }
        let mut bucket_names = std::collections::BTreeSet::new();
        if self.buckets.is_empty()
            || self.buckets.iter().any(|bucket| {
                bucket.name.is_empty()
                    || !bucket_names.insert(bucket.name.as_str())
                    || bucket.eligible_source_groups == 0
                    || bucket.accepted_source_groups > bucket.eligible_source_groups
                    || !bucket.minimum_coverage.is_finite()
                    || !(0.0..=1.0).contains(&bucket.minimum_coverage)
                    || bucket.accepted_source_groups as f64 / (bucket.eligible_source_groups as f64)
                        < bucket.minimum_coverage
            })
        {
            return Err("calibration_coverage_gate");
        }
        Ok(())
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub likelihood: BlockLikelihoodConfig,
    pub verification: VerificationConfig,
    pub quantization: QuantizationPolicy,
    pub seal: DeliverySealConfig,
    pub beam: BeamConfig,
    pub trust_region: TrustRegionConfig,
    pub k_discrete_paths: usize,
    pub export_decimal_places: u32,
    pub apron_width_px: f64,
    pub exact_prior: IntentPriorPolicy,
    pub clean_prior: IntentPriorPolicy,
    pub confidence: Option<ConfidenceCalibration>,
    // Only a verified repository-owned production loader may set this. A
    // caller cannot turn a development config into a production config by
    // attaching a lookalike calibration struct.
    sealed_production: bool,
}

#[derive(Serialize)]
struct ConfigIdentity<'a> {
    likelihood: BlockLikelihoodConfig,
    max_g1_spread_rad: f64,
    curve_separation_margin_px: f64,
    quantization: QuantizationPolicy,
    seal: DeliverySealConfig,
    beam: BeamConfig,
    trust_region: TrustRegionConfig,
    k_discrete_paths: usize,
    export_decimal_places: u32,
    apron_width_px: f64,
    exact_prior: IntentPriorPolicy,
    clean_prior: IntentPriorPolicy,
    implementation: &'a str,
}

/// Release binding updated only after the canonical M7 production
/// configuration is measured. The all-zero value deliberately makes every
/// pre-freeze file fail closed.
pub const M7_PRODUCTION_CONFIG_SHA256: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionConfigFile {
    schema: String,
    preset: Preset,
    delivery_seal: DeliverySealFile,
    calibration: ConfidenceCalibration,
    identity: IdentityFile,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeliverySealFile {
    max_profile_channel_delta: u8,
    max_profile_mean_channel_delta: f64,
    max_internal_channel_delta: u8,
    max_internal_mean_channel_delta: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct IdentityFile {
    universe_sha256: String,
    pricing_sha256: String,
    backend_sha256: String,
    config_sha256: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ProductionConfigError {
    #[error("read production config: {0}")]
    Read(#[from] std::io::Error),
    #[error("production config bytes do not match the release trust anchor")]
    UntrustedDigest,
    #[error("invalid production config JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported production config schema or preset")]
    SchemaOrPreset,
    #[error("production config contains a non-finite delivery threshold")]
    InvalidDeliverySeal,
    #[error("production config identity is malformed")]
    InvalidIdentity,
    #[error("production config identity does not match this executable")]
    StaleIdentity,
    #[error("production confidence calibration is not release-eligible: {0}")]
    Calibration(&'static str),
}

impl CoreConfig {
    /// Load the one release-authorized M7 production configuration.
    ///
    /// Parsing a lookalike file is insufficient: the exact canonical bytes
    /// must match the digest pinned into the executable, and all four model
    /// identities are recomputed before the private production bit is set.
    pub fn load_production_for(preset: Preset, path: &Path) -> Result<Self, ProductionConfigError> {
        let bytes = std::fs::read(path)?;
        Self::production_from_bytes(preset, &bytes, M7_PRODUCTION_CONFIG_SHA256)
    }

    fn production_from_bytes(
        preset: Preset,
        bytes: &[u8],
        expected_digest: &str,
    ) -> Result<Self, ProductionConfigError> {
        let digest = hex::encode(Sha256::digest(bytes));
        if !is_sha256(expected_digest) || digest != expected_digest {
            return Err(ProductionConfigError::UntrustedDigest);
        }
        let file: ProductionConfigFile = serde_json::from_slice(bytes)?;
        if file.schema != "vice-classic/m7-production-config/v1" || file.preset != preset {
            return Err(ProductionConfigError::SchemaOrPreset);
        }
        let seal = DeliverySealConfig {
            max_profile_channel_delta: file.delivery_seal.max_profile_channel_delta,
            max_profile_mean_channel_delta: file.delivery_seal.max_profile_mean_channel_delta,
            max_internal_channel_delta: file.delivery_seal.max_internal_channel_delta,
            max_internal_mean_channel_delta: file.delivery_seal.max_internal_mean_channel_delta,
        };
        if !seal.max_profile_mean_channel_delta.is_finite()
            || seal.max_profile_mean_channel_delta < 0.0
            || !seal.max_internal_mean_channel_delta.is_finite()
            || seal.max_internal_mean_channel_delta < 0.0
        {
            return Err(ProductionConfigError::InvalidDeliverySeal);
        }
        let expected_identity = ModelIdentity::new(
            file.identity.universe_sha256,
            file.identity.pricing_sha256,
            file.identity.backend_sha256,
            file.identity.config_sha256,
        )
        .map_err(|_| ProductionConfigError::InvalidIdentity)?;
        let mut config = Self::development_for(preset);
        config.seal = seal;
        config.confidence = Some(file.calibration);
        let actual_identity = config.identity();
        if actual_identity != expected_identity {
            return Err(ProductionConfigError::StaleIdentity);
        }
        config
            .confidence
            .as_ref()
            .expect("just installed")
            .validate_for_identity(&actual_identity)
            .map_err(ProductionConfigError::Calibration)?;
        config.sealed_production = true;
        Ok(config)
    }

    pub fn development_for(preset: Preset) -> Self {
        let mut config = Self::development();
        if preset == Preset::Fast {
            config.k_discrete_paths = 1;
            config.beam.width = 4;
            config.beam.min_topology_classes = 1;
            config.beam.min_formation_classes = 1;
            config.beam.budget.max_candidates_considered = 16;
            config.beam.budget.max_elapsed_ms = 1_000;
            config.trust_region.max_rounds = 2;
            config.trust_region.max_backtracks = 4;
        }
        config
    }

    pub fn development() -> Self {
        Self {
            likelihood: BlockLikelihoodConfig::new(2, 2.0, [25.57 / 255.0; 4], 4.0)
                .expect("static likelihood"),
            verification: VerificationConfig {
                render_options: vice_render::RenderOptions::default(),
                max_g1_spread_rad: vice_fit::GATE_MAX_G1_SPREAD_RAD,
                curve_separation_margin_px: 1e-9,
            },
            quantization: QuantizationPolicy { decimal_places: 12 },
            // Measurement ceiling only. `sealed_production == false` makes a
            // success impossible; the release loader replaces these values
            // with frozen court gates after corpus calibration.
            seal: DeliverySealConfig {
                max_profile_channel_delta: 255,
                max_profile_mean_channel_delta: 255.0,
                max_internal_channel_delta: 255,
                max_internal_mean_channel_delta: 255.0,
            },
            beam: BeamConfig {
                width: 16,
                within_best_bits: 24.0,
                min_topology_classes: 2,
                min_formation_classes: 2,
                budget: SearchBudget {
                    max_candidates_considered: 256,
                    max_memory_bytes: 1 << 30,
                    // Leave serialization/reporting headroom under the
                    // end-to-end 10 s Quality SLO.
                    max_elapsed_ms: 8_500,
                },
            },
            trust_region: TrustRegionConfig {
                initial_radius: 1.0 / 255.0,
                minimum_radius: 1.0 / 65_535.0,
                expansion: 1.5,
                contraction: 0.5,
                finite_difference_step: 1.0 / 65_535.0,
                min_bits_improvement: 1e-9,
                max_rounds: 2,
                max_backtracks: 4,
                full_check_every_accepted_blocks: 1,
            },
            k_discrete_paths: vice_fit::K_DISCRETE_PATHS,
            export_decimal_places: 12,
            apron_width_px: 0.01,
            exact_prior: IntentPriorPolicy {
                structural_code_scale: 0.75,
                constrained_promotion_extra_bits: 6.0,
            },
            clean_prior: IntentPriorPolicy {
                structural_code_scale: 1.0,
                constrained_promotion_extra_bits: 0.0,
            },
            confidence: None,
            sealed_production: false,
        }
    }

    pub fn is_sealed_production(&self) -> bool {
        self.sealed_production
    }

    pub fn intent_prior(&self, intent: Intent) -> IntentPriorPolicy {
        match intent {
            Intent::Exact => self.exact_prior,
            Intent::Clean => self.clean_prior,
        }
    }

    pub fn identity(&self) -> ModelIdentity {
        let universe = SupportedModelUniverseV1::m7();
        universe.check_finite().expect("frozen universe is finite");
        let universe_sha256 = model_universe_hash(&universe);
        let pricing_sha256 = hex::encode(Sha256::digest(vice_fit::pricing_surface_v1()));
        let backend_sha256 = hex::encode(Sha256::digest(
            format!(
                "{}|{}|{}|{}",
                vice_render::RENDER_DIGEST_SCHEMA,
                vice_svg::SVG_PARSER_ID,
                vice_svg::SVG_RENDERER_ID,
                env!("CARGO_PKG_VERSION")
            )
            .as_bytes(),
        ));
        let identity = ConfigIdentity {
            likelihood: self.likelihood,
            max_g1_spread_rad: self.verification.max_g1_spread_rad,
            curve_separation_margin_px: self.verification.curve_separation_margin_px,
            quantization: self.quantization,
            seal: self.seal,
            beam: self.beam,
            trust_region: self.trust_region,
            k_discrete_paths: self.k_discrete_paths,
            export_decimal_places: self.export_decimal_places,
            apron_width_px: self.apron_width_px,
            exact_prior: self.exact_prior,
            clean_prior: self.clean_prior,
            implementation: "vice-core/m7/v3",
        };
        let config_sha256 = hex::encode(Sha256::digest(
            serde_json::to_vec(&identity).expect("config serializes"),
        ));
        ModelIdentity::new(
            universe_sha256,
            pricing_sha256,
            backend_sha256,
            config_sha256,
        )
        .expect("sha256 identities")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn production_bytes(stale_calibration_field: Option<&str>) -> Vec<u8> {
        let preset = Preset::Fast;
        let seal = DeliverySealConfig {
            max_profile_channel_delta: 1,
            max_profile_mean_channel_delta: 0.01,
            max_internal_channel_delta: 1,
            max_internal_mean_channel_delta: 0.01,
        };
        let mut config = CoreConfig::development_for(preset);
        config.seal = seal;
        let identity = config.identity();
        let mut calibration = json!({
            "schema": "vice-classic/confidence-calibration/v1",
            "model_universe_sha256": identity.universe_sha256,
            "pricing_sha256": identity.pricing_sha256,
            "backend_sha256": identity.backend_sha256,
            "config_sha256": identity.config_sha256,
            "calibration_split_sha256": "1".repeat(64),
            "sealed_audit_generation": "audit-1",
            "sealed_audit_untouched": true,
            "confidence_level": 0.99,
            "catastrophic_risk_target": 0.01,
            "accepted_source_groups": 459,
            "catastrophic_source_groups": 0,
            "posterior_lower_bound_threshold": 0.95,
            "minimum_top2_class_margin_bits": 0.0,
            "maximum_posterior_predictive_bits_per_block": 0.1,
            "maximum_abs_residual_lag1": 0.9,
            "maximum_topology_entropy_bits": 1.0,
            "maximum_formation_entropy_bits": 1.0,
            "empirical_unexplored_relative_mass_upper_bound": 0.25,
            "buckets": [{
                "name": "all",
                "accepted_source_groups": 459,
                "eligible_source_groups": 459,
                "minimum_coverage": 1.0
            }]
        });
        if let Some(field) = stale_calibration_field {
            calibration[field] = json!("2".repeat(64));
        }
        serde_json::to_vec(&json!({
            "schema": "vice-classic/m7-production-config/v1",
            "preset": "fast",
            "delivery_seal": {
                "max_profile_channel_delta": seal.max_profile_channel_delta,
                "max_profile_mean_channel_delta": seal.max_profile_mean_channel_delta,
                "max_internal_channel_delta": seal.max_internal_channel_delta,
                "max_internal_mean_channel_delta": seal.max_internal_mean_channel_delta
            },
            "calibration": calibration,
            "identity": identity
        }))
        .unwrap()
    }

    #[test]
    fn only_exact_trust_anchored_bytes_can_set_production() {
        let bytes = production_bytes(None);
        let digest = hex::encode(Sha256::digest(&bytes));
        let config = CoreConfig::production_from_bytes(Preset::Fast, &bytes, &digest)
            .expect("valid pinned config");
        assert!(config.is_sealed_production());

        let mut tampered = bytes;
        tampered.push(b' ');
        assert!(matches!(
            CoreConfig::production_from_bytes(Preset::Fast, &tampered, &digest),
            Err(ProductionConfigError::UntrustedDigest)
        ));
    }

    #[test]
    fn a_freshly_pinned_file_still_cannot_carry_stale_calibration() {
        for field in [
            "model_universe_sha256",
            "pricing_sha256",
            "backend_sha256",
            "config_sha256",
        ] {
            let bytes = production_bytes(Some(field));
            let digest = hex::encode(Sha256::digest(&bytes));
            assert!(
                matches!(
                    CoreConfig::production_from_bytes(Preset::Fast, &bytes, &digest),
                    Err(ProductionConfigError::Calibration(
                        "calibration_identity_or_audit"
                    ))
                ),
                "stale calibration field {field} was accepted"
            );
        }
    }

    #[test]
    fn confidence_does_not_create_a_circular_model_config_identity() {
        let mut config = CoreConfig::development_for(Preset::Fast);
        let before = config.identity();
        let file: serde_json::Value = serde_json::from_slice(&production_bytes(None)).unwrap();
        let mut calibration: ConfidenceCalibration =
            serde_json::from_value(file["calibration"].clone()).unwrap();
        calibration.model_universe_sha256 = before.universe_sha256.clone();
        calibration.pricing_sha256 = before.pricing_sha256.clone();
        calibration.backend_sha256 = before.backend_sha256.clone();
        calibration.config_sha256 = before.config_sha256.clone();
        config.confidence = Some(calibration);
        config.sealed_production = true;
        assert_eq!(config.identity(), before);
    }
}
