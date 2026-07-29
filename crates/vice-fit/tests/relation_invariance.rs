use vice_evidence::BoundarySample;
use vice_fit::{
    relation_hypotheses, ArcAnchor, BoundaryModel, ChainCode, RefitChain, RefitNode, RefitSegment,
    RelationHypothesis, RelationKind, SelectedBoundaryGeometry, SpanFamily, GEOMETRY_CODE_TABLE_V1,
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
    BoundaryModel {
        geometry: SelectedBoundaryGeometry::TypedChain { chain },
        families: vec![SpanFamily::CircularArc; 2],
        breakpoints: vec![1],
        smooth: vec![false],
        closure_smooth: false,
        code: ChainCode {
            geometry_bits: 200.0,
            topology_bits: 10.0,
            relation_bits: 0.0,
            residual_bits: 0.0,
        },
        proposal_cost_px: 0.0,
        worst_g1_spread_rad: 0.0,
        worst_normal_deviation_px: 0.0,
        worst_model_to_evidence_px: 0.0,
        residual_before: 0.0,
        residual_after: 0.0,
        primitives: Vec::new(),
        primitive_kept: None,
        relations: Vec::new(),
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
    )
    .iter()
    .any(|h| h.kind == RelationKind::SharedBaseline && h.segments == [0, 2]));
}
