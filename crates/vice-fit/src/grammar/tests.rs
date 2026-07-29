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
