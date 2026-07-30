use vice_evidence::BoundarySample;
use vice_fit::{
    apply_accepted, relation_hypotheses, ArcAnchor, BoundaryModel, ChainCode, Handle, RefitChain,
    RefitNode, RefitSegment, RelationHypothesis, RelationKind, SelectedBoundaryGeometry,
    SpanFamily, GEOMETRY_CODE_TABLE_V1,
};
use vice_geom::Pt;

fn probe_chain(delta: f64, translation: Pt) -> RefitChain {
    RefitChain {
        nodes: [
            Pt::new(5.0, 0.0),
            Pt::new(0.0, 5.0),
            Pt::new(-5.0 + delta, 0.0),
        ]
        .into_iter()
        .map(|pos| RefitNode {
            pos: pos + translation,
            tangent_rad: None,
        })
        .collect(),
        segments: vec![
            RefitSegment::Arc(ArcAnchor::Radius {
                radius_px: 5.0,
                large_arc: false,
                ccw: true,
            });
            2
        ],
    }
}

fn probe_model(chain: RefitChain) -> BoundaryModel {
    let geometry = SelectedBoundaryGeometry::TypedChain { chain };
    let code = ChainCode {
        geometry_bits: 200.0,
        topology_bits: 10.0,
        relation_bits: 0.0,
        residual_bits: 0.0,
    };
    BoundaryModel {
        stage_h_free_geometry: geometry.clone(),
        stage_h_free_code: code,
        geometry,
        families: vec![SpanFamily::CircularArc; 2],
        breakpoints: vec![1],
        smooth: vec![false],
        closure_smooth: false,
        code,
        proposal_cost_px: 0.0,
        worst_g1_spread_rad: 0.0,
        worst_normal_deviation_px: 0.0,
        worst_model_to_evidence_px: 0.0,
        residual_before: 0.0,
        residual_after: 0.0,
        primitives: Default::default(),
        primitive_kept: None,
        relations: Default::default(),
        relations_kept: 0,
        relation_kept_indices: Vec::new(),
    }
}

fn line_model(points: &[Pt]) -> BoundaryModel {
    let chain = RefitChain {
        nodes: points
            .iter()
            .copied()
            .map(|pos| RefitNode {
                pos,
                tangent_rad: None,
            })
            .collect(),
        segments: vec![RefitSegment::Line; points.len() - 1],
    };
    let mut model = probe_model(chain);
    model.families = vec![SpanFamily::Line; points.len() - 1];
    model.breakpoints = (1..points.len() - 1).collect();
    model.code.geometry_bits = 200.0;
    model.code.topology_bits = 200.0;
    model
}

fn observations(chain: &RefitChain) -> Vec<BoundarySample> {
    chain
        .nodes
        .iter()
        .map(|node| BoundarySample {
            p: node.pos,
            normal: Pt::new(0.0, 1.0),
            halfwidth: 0.01,
            confidence: 1.0,
            weight_ds: 1.0,
            corr_length_px: 1.0,
        })
        .collect()
}

fn decision(hypotheses: &[RelationHypothesis]) -> Vec<(RelationKind, bool, f64)> {
    hypotheses
        .iter()
        .map(|hypothesis| (hypothesis.kind, hypothesis.accepted, hypothesis.net_bits))
        .collect()
}

#[test]
fn translated_concentric_candidate_keeps_the_stage_h_decision() {
    let origin = probe_chain(3e-11, Pt::ZERO);
    let translated = probe_chain(3e-11, Pt::new(10_000.0, -5_000.0));
    let run = |chain: &RefitChain| {
        relation_hypotheses(
            &probe_model(chain.clone()),
            &observations(chain),
            &GEOMETRY_CODE_TABLE_V1,
            16_384.0,
            false,
        )
    };
    assert_eq!(
        decision(&run(&translated)),
        decision(&run(&origin)),
        "translation changed the formed or accepted Stage-H relation set"
    );
}

#[test]
fn shared_baseline_is_not_formed_for_the_adjacent_pair_it_cannot_identify() {
    let adjacent = line_model(&[Pt::new(0.0, 0.0), Pt::new(4.0, 0.0), Pt::new(10.0, 0.0)]);
    let chain = adjacent.geometry.typed_chain().expect("typed chain");
    let hypotheses = relation_hypotheses(
        &adjacent,
        &observations(chain),
        &GEOMETRY_CODE_TABLE_V1,
        16_384.0,
        false,
    );
    assert!(hypotheses
        .iter()
        .any(|h| h.kind == RelationKind::Parallel && h.segments == [0, 1]));
    assert!(!hypotheses
        .iter()
        .any(|h| h.kind == RelationKind::SharedBaseline && h.segments == [0, 1]));

    let separated = line_model(&[
        Pt::new(0.0, 0.0),
        Pt::new(4.0, 0.0),
        Pt::new(7.0, 2.0),
        Pt::new(11.0, 2.0),
    ]);
    let chain = separated.geometry.typed_chain().expect("typed chain");
    assert!(relation_hypotheses(
        &separated,
        &observations(chain),
        &GEOMETRY_CODE_TABLE_V1,
        16_384.0,
        false,
    )
    .iter()
    .any(|h| h.kind == RelationKind::SharedBaseline && h.segments == [0, 2]));
}

#[test]
fn closed_wrap_relations_cannot_open_the_canonical_seam() {
    let mut model = line_model(&[
        Pt::new(0.0, 0.0),
        Pt::new(4.0, 0.0),
        Pt::new(4.0, 3.0),
        Pt::new(0.0, 0.0),
    ]);
    let mut chain = model.geometry.typed_chain().expect("typed chain").clone();
    chain.segments[1] = RefitSegment::Quad {
        ctrl: Handle::Free(Pt::new(5.0, 1.5)),
    };
    model.geometry = SelectedBoundaryGeometry::TypedChain { chain };
    model.families[1] = SpanFamily::Quad;

    let chain = model.geometry.typed_chain().expect("typed chain");
    let hypotheses = relation_hypotheses(
        &model,
        &observations(chain),
        &GEOMETRY_CODE_TABLE_V1,
        16_384.0,
        true,
    );
    assert!(
        !hypotheses
            .iter()
            .any(|h| h.kind == RelationKind::SharedBaseline && h.segments == [0, 2]),
        "the first/last pair is adjacent on a closed chain"
    );
    assert!(
        hypotheses.iter().all(|hypothesis| {
            hypothesis.constrained_chain.start() == hypothesis.constrained_chain.end()
        }),
        "a formed closed-chain relation opened the repeated seam node"
    );

    // `apply_accepted` is exported too: even a caller-constructed accepted
    // hypothesis cannot smuggle an open sibling through the second boundary.
    let mut opened = chain.clone();
    opened.nodes.last_mut().expect("last node").pos = Pt::new(1.0, 1.0);
    let injected = RelationHypothesis {
        kind: RelationKind::RepeatedTransform,
        segments: vec![0, 2],
        constrained_chain: opened,
        cost_bits: 0.0,
        saving_bits: 1.0,
        geometry_saving_bits: 1.0,
        topology_saving_bits: 0.0,
        residual_penalty_bits: 0.0,
        net_bits: 1.0,
        worst_normal_deviation_px: 0.0,
        worst_model_to_evidence_px: 0.0,
        allowed_px: 1.0,
        solve_trace: Vec::new(),
        continuous_solve_samples: 0,
        accepted: true,
    };
    assert_eq!(apply_accepted(&mut model, &[injected], true), 0);
    let selected = model.geometry.typed_chain().expect("typed chain");
    assert_eq!(selected.start(), selected.end());
}

#[test]
fn public_relation_application_rejects_unphysical_or_overflowing_codes() {
    let base = line_model(&[Pt::new(0.0, 0.0), Pt::new(4.0, 0.0), Pt::new(8.0, 0.0)]);
    let chain = base.geometry.typed_chain().expect("typed chain").clone();
    let valid_shape = RelationHypothesis {
        kind: RelationKind::Parallel,
        segments: vec![0, 1],
        constrained_chain: chain,
        cost_bits: 1.0,
        saving_bits: 2.0,
        geometry_saving_bits: 2.0,
        topology_saving_bits: 0.0,
        residual_penalty_bits: 0.0,
        net_bits: 1.0,
        worst_normal_deviation_px: 0.0,
        worst_model_to_evidence_px: 0.0,
        allowed_px: 1.0,
        solve_trace: Vec::new(),
        continuous_solve_samples: 0,
        accepted: true,
    };

    let mut cases = Vec::new();
    let mut negative = valid_shape.clone();
    negative.cost_bits = -1.0;
    cases.push((base.clone(), negative));
    let mut infinite = valid_shape.clone();
    infinite.cost_bits = f64::INFINITY;
    cases.push((base.clone(), infinite));
    let mut underflowing_residual = valid_shape.clone();
    underflowing_residual.residual_penalty_bits = -1.0;
    underflowing_residual.net_bits = 2.0;
    cases.push((base.clone(), underflowing_residual));
    let mut oversized = valid_shape.clone();
    oversized.saving_bits = 201.0;
    oversized.geometry_saving_bits = 201.0;
    oversized.net_bits = 200.0;
    cases.push((base.clone(), oversized));
    let mut overflowing_model = base.clone();
    overflowing_model.code.geometry_bits = 0.75 * f64::MAX;
    overflowing_model.code.relation_bits = 0.75 * f64::MAX;
    let mut overflowing_relation = valid_shape;
    overflowing_relation.cost_bits = 0.5 * f64::MAX;
    overflowing_relation.saving_bits = 0.75 * f64::MAX;
    overflowing_relation.geometry_saving_bits = 0.75 * f64::MAX;
    overflowing_relation.net_bits = 0.25 * f64::MAX;
    cases.push((overflowing_model, overflowing_relation));

    for (mut model, hypothesis) in cases {
        let before_code = model.code;
        let before_geometry = model.geometry.clone();
        assert_eq!(apply_accepted(&mut model, &[hypothesis], false), 0);
        assert_eq!(model.code, before_code);
        assert_eq!(model.geometry, before_geometry);
    }
}
