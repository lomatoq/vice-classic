use vice_evidence::{BoundaryChain, BoundarySample};
use vice_geom::Pt;

use super::{
    discrete_proposal_chain, k_best_boundary_models_bounded, observed_polyline_rescue_model,
    observed_support_polyline, retain_binding_certified_stage_h, ObservedPolylineRefusal,
    MAX_OBSERVED_POLYLINE_SEGMENTS_V1,
};

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
    let cloned = run.models[0].clone();
    assert!(
        std::sync::Arc::ptr_eq(&run.models[0].relations, &cloned.relations)
            && std::sync::Arc::ptr_eq(&run.models[0].primitives, &cloned.primitives),
        "variant clones must share immutable Stage-H catalogs"
    );
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

#[test]
fn observed_polyline_rescue_is_priced_bounded_and_full_resolution_certified() {
    let points = [
        Pt::new(2.0, 2.0),
        Pt::new(14.0, 2.0),
        Pt::new(14.0, 14.0),
        Pt::new(2.0, 14.0),
    ];
    let chain = BoundaryChain {
        samples: points
            .into_iter()
            .map(|p| BoundarySample {
                p,
                normal: Pt::new(0.0, 1.0),
                halfwidth: 0.35,
                confidence: 1.0,
                weight_ds: 12.0,
                corr_length_px: 1.0,
            })
            .collect(),
        closed: true,
        length_px: 48.0,
        corr_length_px: 1.0,
        vertices: 4,
    };
    let model = observed_polyline_rescue_model(&chain, 128.0).expect("square line rescue");
    assert_eq!(model.families.len(), 4);
    assert!(model.code.total_bits().is_finite() && model.code.total_bits() > 0.0);
    assert!(model.worst_normal_deviation_px <= 0.35 * crate::FEASIBLE_HALFWIDTHS);
    model
        .geometry
        .typed_chain()
        .expect("typed rescue")
        .lower()
        .expect("rescue lowers without coincident nodes");
}

#[test]
fn observed_polyline_rescue_refuses_instead_of_exceeding_its_structural_cap() {
    let samples = (0..MAX_OBSERVED_POLYLINE_SEGMENTS_V1 + 2)
        .map(|index| BoundarySample {
            p: Pt::new(index as f64 * 0.5, f64::from((index % 2) as u8)),
            normal: Pt::new(0.0, 1.0),
            halfwidth: 0.001,
            confidence: 1.0,
            weight_ds: 0.5,
            corr_length_px: 0.5,
        })
        .collect::<Vec<_>>();
    let chain = BoundaryChain {
        length_px: samples.len() as f64 * 0.5,
        vertices: samples.len() as u64,
        samples,
        closed: false,
        corr_length_px: 0.5,
    };
    assert!(matches!(
        observed_polyline_rescue_model(&chain, 128.0),
        Err(ObservedPolylineRefusal::TooComplex {
            segments,
            cap: MAX_OBSERVED_POLYLINE_SEGMENTS_V1
        }) if segments > MAX_OBSERVED_POLYLINE_SEGMENTS_V1
    ));
}

#[test]
fn an_unbound_stage_h_sibling_falls_back_to_the_certified_free_chain() {
    let chain = open_line(257);
    let run = k_best_boundary_models_bounded(&chain, &crate::FIT_BUDGET_V1, 128.0, 2, 32)
        .expect("straight free chain");
    let mut model = run.models[0].clone();
    let free = model.stage_h_free_geometry.clone();
    let free_code = model.stage_h_free_code;
    let typed = model
        .geometry
        .typed_chain()
        .expect("straight selected chain")
        .clone();
    let mut displaced = typed;
    for node in &mut displaced.nodes {
        node.pos.y += 8.0;
    }
    model.geometry = crate::SelectedBoundaryGeometry::TypedChain { chain: displaced };
    model.relations_kept = 1;
    model.relation_kept_indices = vec![0];

    retain_binding_certified_stage_h(&mut model, &chain.samples, false)
        .expect("the valid free sibling must survive an invalid Stage-H sibling");
    assert_eq!(model.geometry, free);
    assert_eq!(model.code, free_code);
    assert_eq!(model.relations_kept, 0);
    assert!(model.relation_kept_indices.is_empty());
}

#[test]
fn a_stage_h_sibling_cannot_spend_the_whole_tube_when_the_free_chain_is_tighter() {
    let chain = open_line(257);
    let run = k_best_boundary_models_bounded(&chain, &crate::FIT_BUDGET_V1, 128.0, 2, 32)
        .expect("straight free chain");
    let mut model = run.models[0].clone();
    let free = model.stage_h_free_geometry.clone();
    let free_code = model.stage_h_free_code;
    let typed = model
        .geometry
        .typed_chain()
        .expect("straight selected chain")
        .clone();
    let mut displaced = typed;
    for node in &mut displaced.nodes {
        node.pos.y += 0.20;
    }
    model.geometry = crate::SelectedBoundaryGeometry::TypedChain { chain: displaced };
    model.relations_kept = 1;
    model.relation_kept_indices = vec![0];

    retain_binding_certified_stage_h(&mut model, &chain.samples, false)
        .expect("the tighter free sibling remains certified");
    assert_eq!(model.geometry, free);
    assert_eq!(model.code, free_code);
    assert_eq!(model.relations_kept, 0);
    assert!(model.relation_kept_indices.is_empty());
}
