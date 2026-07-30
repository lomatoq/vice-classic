use serde::Serialize;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalibrationBucket {
    pub name: String,
    pub accepted_source_groups: u64,
    pub eligible_source_groups: u64,
    pub minimum_coverage: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConfidenceCalibration {
    pub schema: String,
    pub model_universe_sha256: String,
    pub calibration_split_sha256: String,
    pub sealed_audit_generation: String,
    pub sealed_audit_untouched: bool,
    pub confidence_level: f64,
    pub catastrophic_risk_target: f64,
    pub accepted_source_groups: u64,
    pub catastrophic_source_groups: u64,
    pub posterior_lower_bound_threshold: f64,
    /// Frozen R1 upper bound on omitted search mass relative to the best
    /// retained hypothesis. `None` means truncated search remains Unknown.
    pub empirical_unexplored_relative_mass_upper_bound: Option<f64>,
    pub buckets: Vec<CalibrationBucket>,
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
    ) -> Result<(), &'static str> {
        if self.schema != "vice-classic/confidence-calibration/v1"
            || self.model_universe_sha256 != identity.universe_sha256
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
        if self.buckets.is_empty()
            || self.buckets.iter().any(|bucket| {
                bucket.eligible_source_groups == 0
                    || !bucket.minimum_coverage.is_finite()
                    || !(0.0..=1.0).contains(&bucket.minimum_coverage)
                    || bucket.accepted_source_groups as f64 / (bucket.eligible_source_groups as f64)
                        < bucket.minimum_coverage
            })
        {
            return Err("calibration_coverage_gate");
        }
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
        Ok(())
    }
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
    confidence_sha256: Option<String>,
    sealed_production: bool,
    implementation: &'a str,
}

impl CoreConfig {
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
                    max_elapsed_ms: 10_000,
                },
            },
            trust_region: TrustRegionConfig {
                initial_radius: 1.0 / 255.0,
                minimum_radius: 1.0 / 65_535.0,
                expansion: 1.5,
                contraction: 0.5,
                finite_difference_step: 1.0 / 65_535.0,
                min_bits_improvement: 1e-9,
                max_rounds: 8,
                max_backtracks: 8,
                full_check_every_accepted_blocks: 1,
            },
            k_discrete_paths: 64,
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
        let universe = SupportedModelUniverseV1::v1();
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
            confidence_sha256: self
                .confidence
                .as_ref()
                .map(ConfidenceCalibration::digest_sha256),
            sealed_production: self.sealed_production,
            implementation: "vice-core/m7/v1",
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
