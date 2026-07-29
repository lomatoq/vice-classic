use super::*;

#[test]
fn jet_classes_wrap_and_neighbours_are_compatible() {
    assert_eq!(jet_class(0.0), 0);
    assert_eq!(jet_class(std::f64::consts::TAU), 0);
    assert!(jet_compatible(0, JET_CLASSES - 1), "the wrap is compatible");
    assert!(jet_compatible(5, 6));
    assert!(!jet_compatible(5, 7));
    // A right angle is never compatible, at any rotation.
    for a in 0..JET_CLASSES {
        let b = (a + JET_CLASSES / 4) % JET_CLASSES;
        assert!(!jet_compatible(a, b), "{a} and {b}");
    }
}

/// The exact scalar counts, family by family, in both directions.
#[test]
fn sharing_a_tangent_removes_exactly_the_scalars_it_determines() {
    use SpanFamily::*;
    assert_eq!(free_scalars(Line, false, false), 0);
    assert_eq!(free_scalars(Line, true, true), 0);
    assert_eq!(free_scalars(CircularArc, false, false), 1);
    assert_eq!(free_scalars(CircularArc, true, false), 0);
    assert_eq!(free_scalars(CircularArc, true, true), 0);
    assert_eq!(free_scalars(Quad, false, false), 2);
    assert_eq!(free_scalars(Quad, true, false), 1);
    assert_eq!(free_scalars(Quad, true, true), 0);
    assert_eq!(free_scalars(Cubic, false, false), 4);
    assert_eq!(free_scalars(Cubic, true, false), 3);
    assert_eq!(free_scalars(Cubic, true, true), 2);
}

#[test]
fn unmaterializable_smooth_transitions_cannot_consume_a_k_best_slot() {
    use SpanFamily::*;
    assert!(!smooth_transition_is_representable(Quad, false, Cubic));
    assert!(!smooth_transition_is_representable(
        CircularArc,
        true,
        Cubic
    ));
    assert!(!smooth_transition_is_representable(Line, false, Line));
    assert!(smooth_transition_is_representable(
        CircularArc,
        false,
        Cubic
    ));
    assert!(smooth_transition_is_representable(Cubic, true, Line));
}

fn ranking_samples() -> Vec<vice_evidence::BoundarySample> {
    (0..3)
        .map(|x| vice_evidence::BoundarySample {
            p: vice_geom::Pt::new(x as f64, 0.0),
            normal: vice_geom::Pt::new(0.0, 1.0),
            halfwidth: 0.35,
            confidence: 1.0,
            weight_ds: 1.0,
            corr_length_px: 1.0,
        })
        .collect()
}

fn ranking_edge(candidate: usize, from: usize, to: usize, proposal: f64) -> GrammarEdge {
    GrammarEdge {
        candidate,
        from,
        to,
        family: SpanFamily::Cubic,
        entry_class: 0,
        exit_class: 0,
        entry_rad: 0.0,
        exit_rad: 0.0,
        residual_bits: 0.0,
        proposal_cost_px: proposal,
    }
}

#[test]
fn every_k_truncation_uses_the_declared_proposal_tie_break() {
    let edges = [
        ranking_edge(0, 0, 1, 10.0),
        ranking_edge(1, 0, 1, 1.0),
        ranking_edge(2, 1, 2, 0.0),
    ];
    let samples = ranking_samples();
    let open = k_best_paths(&edges, &samples, &crate::GEOMETRY_CODE_TABLE_V1, 256.0, 1)
        .expect("valid samples");
    assert_eq!(open[0].candidates, vec![1, 2]);

    let closed = k_best_paths_for_objective(
        &edges,
        &samples,
        &crate::GEOMETRY_CODE_TABLE_V1,
        256.0,
        1,
        (PathObjective::PhysicalCode, ClosureMode::Smooth),
        crate::code::first_sample_residual_bits(&samples, &crate::GEOMETRY_CODE_TABLE_V1, 256.0)
            .expect("valid samples"),
    );
    assert_eq!(closed[0].candidates, vec![1, 2]);
}
