use vice_evidence::{BoundaryChain, BoundarySample};
use vice_geom::Pt;

use super::{discrete_proposal_chain, k_best_boundary_models_bounded, observed_support_polyline};

fn open_line(samples: usize) -> BoundaryChain {
    let observations = (0..samples)
        .map(|index| BoundarySample {
            p: Pt::new(index as f64 * 0.25, 4.0),
            normal: Pt::new(0.0, 1.0),
            halfwidth: 0.35,
            confidence: 1.0,
            weight_ds: 0.25,
            corr_length_px: 0.5,
        })
        .collect();
    BoundaryChain {
        samples: observations,
        closed: false,
        length_px: samples as f64 * 0.25,
        corr_length_px: 0.5,
        vertices: samples as u64,
    }
}

#[test]
fn bounded_proposal_preserves_endpoints_order_and_physical_mass() {
    let chain = open_line(257);
    let expected_mass = chain
        .samples
        .iter()
        .map(|sample| sample.weight_ds)
        .sum::<f64>();
    let (proposal, indices) = discrete_proposal_chain(&chain, 32);
    assert_eq!(proposal.samples.len(), 32);
    assert_eq!(indices.first(), Some(&0));
    assert_eq!(indices.last(), Some(&256));
    assert!(indices.windows(2).all(|window| window[0] < window[1]));
    let actual_mass = proposal
        .samples
        .iter()
        .map(|sample| sample.weight_ds)
        .sum::<f64>();
    assert!((actual_mass - expected_mass).abs() < 1e-12);
}

#[test]
fn bounded_search_certifies_and_recodes_on_every_observation() {
    let chain = open_line(257);
    let run = k_best_boundary_models_bounded(&chain, &crate::FIT_BUDGET_V1, 128.0, 2, 32)
        .expect("a straight line remains representable after bounded proposal");
    assert_eq!(run.observed_samples, 257);
    assert_eq!(run.discrete_search_samples, 32);
    assert_eq!(
        run.continuous_solve_samples,
        super::CONTINUOUS_REFIT_SAMPLE_CAP_V1
    );
    assert!(!run.full_resolution_refit);
    assert!(run.full_resolution_certified);
    assert!(!run.models.is_empty());
    assert!(run.models.iter().all(|model| {
        model.worst_normal_deviation_px <= chain.samples[0].halfwidth
            && model.code.residual_bits.is_finite()
    }));
    for model in &run.models {
        let typed = model
            .stage_h_free_geometry
            .typed_chain()
            .expect("the straight-line free sibling is typed");
        assert_eq!(
            model.stage_h_free_code.residual_bits,
            crate::code::chain_residual_bits(typed, &chain.samples, &crate::GEOMETRY_CODE_TABLE_V1,),
            "the published residual code must use every physical observation"
        );
    }
}

#[test]
fn hierarchy_stops_after_the_first_certified_level_and_reports_the_rest() {
    let chain = open_line(257);
    let run = k_best_boundary_models_bounded(
        &chain,
        &crate::FIT_BUDGET_V1,
        128.0,
        2,
        super::DISCRETE_PROPOSAL_SAMPLE_CAP_V1,
    )
    .expect("the coarsest straight-line proposal certifies");
    assert_eq!(run.discrete_search_levels, vec![32]);
    assert_eq!(run.proposal_levels_skipped_after_certification, 3);
    assert!(run.full_resolution_certified);
}

#[test]
fn binding_certificate_closes_only_a_declared_closed_support() {
    let points = [Pt::new(0.0, 0.0), Pt::new(2.0, 2.0), Pt::new(4.0, 0.0)];
    let samples = points
        .into_iter()
        .map(|p| BoundarySample {
            p,
            normal: Pt::new(0.0, 1.0),
            halfwidth: 0.1,
            confidence: 1.0,
            weight_ds: 0.25,
            corr_length_px: 0.5,
        })
        .collect::<Vec<_>>();
    let open = observed_support_polyline(&samples, false);
    assert_eq!(open, points);
    assert_ne!(open.first(), open.last());
    let closed = observed_support_polyline(&samples, true);
    assert_eq!(&closed[..closed.len() - 1], points);
    assert_eq!(closed.first(), closed.last());
}
