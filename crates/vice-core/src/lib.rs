//! M7 selective Flat2 vectorization.
//!
//! This crate is the first production owner of the complete §7/§30 path:
//! decode → evidence → complementary-connectivity DCEL → typed k-best fit →
//! full-resolution posterior → confidence/abstention → quantized verification
//! → two-profile serialized delivery seal. A non-success outcome contains no
//! SVG bytes.

#![forbid(unsafe_code)]

mod candidate;
mod config;
mod pipeline;
mod scene;
mod types;

pub use config::{
    CalibrationBucket, ConfidenceCalibration, ConfidenceMetrics, CoreConfig, Intent,
    IntentPriorPolicy, PerturbationStability, Preset, ProductionConfigError, VectorizeRequest,
    M7_PRODUCTION_CONFIG_SHA256,
};
pub use pipeline::{vectorize, vectorize_for_calibration, vectorize_with_config};
pub use types::{
    CalibrationRun, CalibrationWitness, CandidateFailureStage, CandidateRefusal, DecisionStatus,
    FailureReason, SuccessArtifacts, TopologyArmRefusal, TopologyArmTrace, TopologyEnvelopeTrace,
    VectorizeOutcome, VectorizeReport, VectorizeSuccess, CORE_REPORT_SCHEMA,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn square_png() -> Vec<u8> {
        let width = 32;
        let height = 32;
        let mut rgba = vec![0u8; width * height * 4];
        for y in 7..25 {
            for x in 7..25 {
                let offset = (y * width + x) * 4;
                rgba[offset..offset + 4].copy_from_slice(&[220, 40, 30, 255]);
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
        }
        bytes
    }

    fn two_component_png() -> Vec<u8> {
        let width = 48usize;
        let height = 32usize;
        let mut rgba = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let mut inside = 0u32;
                for sy in 0..8 {
                    for sx in 0..8 {
                        let px = x as f64 + (f64::from(sx) + 0.5) / 8.0;
                        let py = y as f64 + (f64::from(sy) + 0.5) / 8.0;
                        if [(12.0, 16.0, 7.0), (35.0, 16.0, 7.0)]
                            .iter()
                            .any(|&(cx, cy, radius)| {
                                (px - cx).powi(2) + (py - cy).powi(2) <= radius * radius
                            })
                        {
                            inside += 1;
                        }
                    }
                }
                let alpha = ((inside * 255 + 32) / 64) as u8;
                let offset = (y * width + x) * 4;
                rgba[offset..offset + 4].copy_from_slice(&[220, 40, 30, alpha]);
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
        }
        bytes
    }

    fn annulus_png() -> Vec<u8> {
        let width = 48usize;
        let height = 48usize;
        let mut rgba = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let mut inside = 0u32;
                for sy in 0..8 {
                    for sx in 0..8 {
                        let px = x as f64 + (f64::from(sx) + 0.5) / 8.0 - 24.0;
                        let py = y as f64 + (f64::from(sy) + 0.5) / 8.0 - 24.0;
                        let radius_sq = px * px + py * py;
                        if (6.0f64.powi(2)..=14.0f64.powi(2)).contains(&radius_sq) {
                            inside += 1;
                        }
                    }
                }
                let alpha = ((inside * 255 + 32) / 64) as u8;
                let offset = (y * width + x) * 4;
                rgba[offset..offset + 4].copy_from_slice(&[30, 100, 230, alpha]);
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
        }
        bytes
    }

    fn mirrored_components_png() -> Vec<u8> {
        let (width, height) = (64usize, 40usize);
        let mut rgba = vec![0u8; width * height * 4];
        let left = |point: vice_geom::Pt| {
            let triangle = [
                vice_geom::Pt::new(8.0, 8.0),
                vice_geom::Pt::new(21.0, 13.0),
                vice_geom::Pt::new(11.0, 31.0),
            ];
            let side = |a: vice_geom::Pt, b: vice_geom::Pt| (b - a).cross(point - a);
            let signs = [
                side(triangle[0], triangle[1]),
                side(triangle[1], triangle[2]),
                side(triangle[2], triangle[0]),
            ];
            signs.iter().all(|value| *value >= 0.0) || signs.iter().all(|value| *value <= 0.0)
        };
        for y in 0..height {
            for x in 0..width {
                let mut covered = 0u32;
                for sy in 0..8 {
                    for sx in 0..8 {
                        let px = x as f64 + (f64::from(sx) + 0.5) / 8.0;
                        let py = y as f64 + (f64::from(sy) + 0.5) / 8.0;
                        if left(vice_geom::Pt::new(px, py))
                            || left(vice_geom::Pt::new(width as f64 - px, py))
                        {
                            covered += 1;
                        }
                    }
                }
                let alpha = ((covered * 255 + 32) / 64) as u8;
                let offset = (y * width + x) * 4;
                rgba[offset..offset + 4].copy_from_slice(&[30, 100, 230, alpha]);
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
        }
        bytes
    }

    fn weak_bridge_png() -> Vec<u8> {
        let (width, height) = (48usize, 32usize);
        let mut rgba = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let mut covered = 0u32;
                for sy in 0..8 {
                    for sx in 0..8 {
                        let px = x as f64 + (f64::from(sx) + 0.5) / 8.0;
                        let py = y as f64 + (f64::from(sy) + 0.5) / 8.0;
                        let disc = [(14.0, 16.0), (34.0, 16.0)].iter().any(|&(cx, cy)| {
                            (px - cx).powi(2) + (py - cy).powi(2) <= 7.0f64.powi(2)
                        });
                        let bridge = (20.0..28.0).contains(&px) && (15.75..16.25).contains(&py);
                        if disc || bridge {
                            covered += 1;
                        }
                    }
                }
                let alpha = ((covered * 255 + 32) / 64) as u8;
                let offset = (y * width + x) * 4;
                rgba[offset..offset + 4].copy_from_slice(&[30, 100, 230, alpha]);
            }
        }
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&rgba).unwrap();
        }
        bytes
    }

    #[test]
    fn exact_zero_failure_bound_needs_at_least_459_independent_groups() {
        let identity = CoreConfig::development().identity();
        let calibration = |accepted_source_groups| ConfidenceCalibration {
            schema: "vice-classic/confidence-calibration/v1".into(),
            model_universe_sha256: identity.universe_sha256.clone(),
            pricing_sha256: identity.pricing_sha256.clone(),
            backend_sha256: identity.backend_sha256.clone(),
            config_sha256: identity.config_sha256.clone(),
            calibration_split_sha256: "1".repeat(64),
            sealed_audit_generation: "audit-1".into(),
            sealed_audit_untouched: true,
            confidence_level: 0.99,
            catastrophic_risk_target: 0.01,
            accepted_source_groups,
            catastrophic_source_groups: 0,
            posterior_lower_bound_threshold: 0.95,
            minimum_top2_class_margin_bits: 0.0,
            maximum_posterior_predictive_bits_per_block: 0.1,
            maximum_abs_residual_lag1: 0.9,
            maximum_topology_entropy_bits: 1.0,
            maximum_formation_entropy_bits: 1.0,
            minimum_perturbation_stability: 0.95,
            empirical_unexplored_relative_mass_upper_bound: Some(0.0),
            buckets: vec![CalibrationBucket {
                name: "all".into(),
                accepted_source_groups,
                eligible_source_groups: accepted_source_groups,
                minimum_coverage: 1.0,
            }],
        };
        let delivery = vice_opt::DeliveryPosterior {
            delivery_digest: "d".into(),
            explored_mass: 1.0,
            retained_normalized_mass: 1.0,
            posterior_lower_bound: vice_opt::BoundValue::Certified(1.0),
        };
        let metrics = ConfidenceMetrics {
            top2_class_margin_bits: 10.0,
            posterior_predictive_bits_per_block: 0.01,
            max_abs_residual_lag1: 0.1,
            topology_entropy_upper_bound: vice_opt::BoundValue::Certified(0.0),
            formation_entropy_upper_bound: vice_opt::BoundValue::Certified(0.0),
            perturbation_stability: PerturbationStability::from_legs(true, true, true, true),
        };
        assert!(calibration(458)
            .permits(&identity, &delivery, &metrics)
            .is_err());
        assert!(calibration(459)
            .permits(&identity, &delivery, &metrics)
            .is_ok());
        let mut predictive_mismatch = metrics.clone();
        predictive_mismatch.posterior_predictive_bits_per_block = 0.11;
        assert_eq!(
            calibration(459).permits(&identity, &delivery, &predictive_mismatch),
            Err("posterior_predictive_mismatch")
        );
        let mut spatial_mismatch = metrics.clone();
        spatial_mismatch.max_abs_residual_lag1 = 0.91;
        assert_eq!(
            calibration(459).permits(&identity, &delivery, &spatial_mismatch),
            Err("residual_spatial_mismatch")
        );
        let mut unknown_entropy = metrics.clone();
        unknown_entropy.topology_entropy_upper_bound = vice_opt::BoundValue::Unknown;
        assert_eq!(
            calibration(459).permits(&identity, &delivery, &unknown_entropy),
            Err("topology_entropy_above_calibrated_threshold")
        );
        let mut excessive_entropy = metrics.clone();
        excessive_entropy.formation_entropy_upper_bound = vice_opt::BoundValue::Certified(1.01);
        assert_eq!(
            calibration(459).permits(&identity, &delivery, &excessive_entropy),
            Err("formation_entropy_above_calibrated_threshold")
        );
        let mut unstable = metrics.clone();
        unstable.perturbation_stability = PerturbationStability::from_legs(true, true, true, false);
        assert_eq!(
            calibration(459).permits(&identity, &delivery, &unstable),
            Err("perturbation_stability_below_calibrated_threshold")
        );

        for field in ["universe", "pricing", "backend", "config"] {
            let mut stale = calibration(459);
            match field {
                "universe" => stale.model_universe_sha256 = "2".repeat(64),
                "pricing" => stale.pricing_sha256 = "2".repeat(64),
                "backend" => stale.backend_sha256 = "2".repeat(64),
                "config" => stale.config_sha256 = "2".repeat(64),
                _ => unreachable!(),
            }
            assert!(
                stale.permits(&identity, &delivery, &metrics).is_err(),
                "stale {field} identity was accepted"
            );
        }
    }

    #[test]
    fn invalid_input_is_a_typed_failure_with_no_artifact_variant() {
        let outcome = vectorize(b"not a png", &VectorizeRequest::default());
        assert!(matches!(outcome, VectorizeOutcome::Failed(_)));
        assert!(matches!(
            outcome.report().reason,
            Some(FailureReason::Decode { .. })
        ));
    }

    #[test]
    fn a_flat2_png_enters_the_selective_pipeline_without_false_success() {
        let mut config = CoreConfig::development();
        config.k_discrete_paths = 1;
        let run = vectorize_for_calibration(&square_png(), &VectorizeRequest::default(), &config);
        let witness_expected = run.outcome.report().selected_hypothesis_id.is_some();
        assert_eq!(run.selected.is_some(), witness_expected);
        assert_eq!(
            run.outcome.report().confidence_metrics.is_some(),
            witness_expected
        );
        if witness_expected {
            assert!(!run.outcome.report().selected_boundary_bindings.is_empty());
        }
        let outcome = run.outcome;
        assert!(!matches!(
            outcome,
            VectorizeOutcome::Success(_) | VectorizeOutcome::Failed(_)
        ));
        assert!(outcome.report().evidence.is_some());
        let topology = outcome
            .report()
            .topology
            .as_ref()
            .expect("a supported evidence run publishes its M4.5 envelope");
        assert!(topology.proposal.fields_built >= 4);
        assert!(topology.proposal.events_seen > 0);
        assert!(topology.proposal.event_driven_levels > 0);
        assert!(!topology.materialized_arms.is_empty());
        assert!(outcome.report().candidates.iter().all(|candidate| topology
            .materialized_arms
            .iter()
            .any(|arm| arm.topology_class == candidate.topology_class)));
        let inventory = outcome
            .report()
            .transaction_inventory
            .as_ref()
            .expect("a run that entered search publishes transaction inventory");
        assert!(inventory.complete_kind_enumeration);
        assert_eq!(inventory.rows.len(), vice_opt::TransactionKind::ALL.len());
        let paint = inventory
            .rows
            .iter()
            .find(|row| row.kind == vice_opt::TransactionKind::PaintChange)
            .unwrap();
        assert!(paint.proposed > 0);
        assert!(outcome
            .report()
            .candidates
            .iter()
            .flat_map(|candidate| &candidate.transactions)
            .all(|transaction| transaction.atomic));
    }

    #[test]
    fn disconnected_flat2_components_share_one_scene_and_palette() {
        let mut config = CoreConfig::development_for(Preset::Fast);
        config.k_discrete_paths = 1;
        let run =
            vectorize_for_calibration(&two_component_png(), &VectorizeRequest::default(), &config);
        assert!(
            run.selected.is_some(),
            "a selected multi-component candidate needs a calibration witness"
        );
        let outcome = run.outcome;
        assert!(!matches!(outcome, VectorizeOutcome::Failed(_)));
        assert_eq!(outcome.report().fits.len(), 2);
        assert!(
            !outcome.report().candidates.is_empty(),
            "multi-component candidates were refused: {:?}",
            outcome.report().candidate_refusals
        );
        let paint = outcome
            .report()
            .transaction_inventory
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .find(|row| row.kind == vice_opt::TransactionKind::PaintChange)
            .unwrap();
        assert!(paint.verified_and_exact_scored > 0);
        let scene = &outcome.report().candidates[0].pre_quantization;
        assert_eq!(scene.boundaries, 2);
        assert_eq!(scene.observed_chain_bindings, 2);
        assert!(
            outcome
                .report()
                .candidates
                .iter()
                .any(|candidate| candidate.hypothesis_id.starts_with("scene-repetition-"))
                || outcome
                    .report()
                    .candidate_refusals
                    .iter()
                    .any(|refusal| refusal.hypothesis_id.starts_with("scene-repetition-")),
            "scene repetition was not searched"
        );
        assert!(
            outcome.report().candidates.iter().any(|candidate| {
                candidate.transactions.iter().any(|transaction| {
                    transaction.kind == vice_opt::TransactionKind::RelationPromote
                })
            }),
            "the repeated-scene proposal did not pass through an atomic relation transaction"
        );
    }

    #[test]
    fn transparent_hole_is_preserved_as_a_dcel_face() {
        let config = CoreConfig::development_for(Preset::Fast);
        let outcome = vectorize_with_config(&annulus_png(), &VectorizeRequest::default(), &config);
        assert!(!matches!(outcome, VectorizeOutcome::Failed(_)));
        assert_eq!(outcome.report().fits.len(), 2);
        assert!(
            !outcome.report().candidates.is_empty(),
            "annulus candidates were refused: {:?}",
            outcome.report().candidate_refusals
        );
        let scene = &outcome.report().candidates[0].pre_quantization;
        assert_eq!(scene.boundaries, 2);
        assert_eq!(scene.faces, 3);
        assert_eq!(scene.observed_chain_bindings, 2);
    }

    #[test]
    fn mirrored_components_enter_a_scene_level_relation_transaction() {
        let config = CoreConfig::development_for(Preset::Fast);
        let outcome = vectorize_with_config(
            &mirrored_components_png(),
            &VectorizeRequest::default(),
            &config,
        );
        assert!(!matches!(outcome, VectorizeOutcome::Failed(_)));
        assert_eq!(outcome.report().fits.len(), 2);
        assert!(
            outcome.report().candidates.iter().any(|candidate| candidate
                .hypothesis_id
                .starts_with("scene-mirror-")
                && candidate.transactions.iter().any(|transaction| {
                    transaction.kind == vice_opt::TransactionKind::RelationPromote
                })),
            "scene mirror was not atomically searched: candidates={:?}, refusals={:?}, families={:?}",
            outcome
                .report()
                .candidates
                .iter()
                .map(|candidate| &candidate.hypothesis_id)
                .collect::<Vec<_>>(),
            outcome.report().candidate_refusals,
            outcome
                .report()
                .fits
                .iter()
                .map(|fit| &fit.models[0].families)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn event_topology_arms_receive_complete_refits_and_atomic_graph_transactions() {
        let mut config = CoreConfig::development_for(Preset::Fast);
        config.beam.width = 8;
        config.beam.min_topology_classes = 2;
        config.beam.budget.max_candidates_considered = 32;
        config.beam.budget.max_elapsed_ms = 60_000;
        let outcome =
            vectorize_with_config(&weak_bridge_png(), &VectorizeRequest::default(), &config);
        assert!(!matches!(outcome, VectorizeOutcome::Failed(_)));
        let topology = outcome
            .report()
            .topology
            .as_ref()
            .expect("topology envelope trace");
        let signatures = topology
            .materialized_arms
            .iter()
            .filter(|arm| !arm.fit_models_per_chain.is_empty())
            .map(|arm| (arm.components, arm.holes))
            .collect::<std::collections::BTreeSet<_>>();
        assert!(
            signatures.contains(&(1, 0)) && signatures.contains(&(2, 0)),
            "weak bridge did not retain both complete refits: {signatures:?}"
        );
        assert!(
            outcome
                .report()
                .candidates
                .iter()
                .any(
                    |candidate| candidate.transactions.iter().any(|transaction| matches!(
                        transaction.kind,
                        vice_opt::TransactionKind::TopologyBridge
                            | vice_opt::TransactionKind::TopologySplit
                            | vice_opt::TransactionKind::TopologyMerge
                    ))
                ),
            "no changed-topology candidate survived exact scoring: candidates={:?}, refusals={:?}",
            outcome
                .report()
                .candidates
                .iter()
                .map(|candidate| &candidate.hypothesis_id)
                .collect::<Vec<_>>(),
            outcome.report().candidate_refusals
        );
    }
}
