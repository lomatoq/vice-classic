use super::*;

fn cubic_chain(t0: f64, t1: f64) -> RefitChain {
    RefitChain {
        nodes: vec![
            RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
            RefitNode {
                pos: Pt::new(10.0, 0.0),
                tangent_rad: Some(t0),
            },
            RefitNode {
                pos: Pt::new(20.0, 5.0),
                tangent_rad: Some(t1),
            },
        ],
        segments: vec![
            RefitSegment::Cubic {
                head: Handle::Free(Pt::new(3.0, -2.0)),
                tail: Handle::Shared { length_px: 3.0 },
            },
            RefitSegment::Cubic {
                head: Handle::Shared { length_px: 4.0 },
                tail: Handle::Free(Pt::new(18.0, 6.0)),
            },
        ],
    }
}

/// **The property this module exists for, with the positive control that
/// makes it a measurement.**
///
/// The same instrument reads the refit chain and `vice-ir`'s canonical
/// fixture. If it read zero on both it would prove nothing.
#[test]
fn refit_holds_g1_where_the_ir_fixture_does_not() {
    let c = cubic_chain(0.6, -0.3);
    let lowered = c.lower().expect("lowers");
    let readings = g1_readings(&lowered, c.start(), c.end());
    assert_eq!(readings.len(), 1, "one smooth interior node");
    let worst = readings.iter().map(|r| r.spread_rad).fold(0.0f64, f64::max);
    println!(
        "refit chain worst G1 spread {worst:.3e} rad ({:.3e} deg)",
        worst.to_degrees()
    );
    assert!(
        worst < 1e-12,
        "the refit representation lowered to a spread of {worst} rad; the control points are \
             supposed to be built from ONE angle"
    );

    // The positive control: the same instrument on a chain that stores its
    // control points independently of its declaration.
    let broken = CurveChain {
        interior_nodes: vec![ChainNode {
            pos: Pt::new(10.0, 0.0),
            join: JoinKind::SmoothG1 {
                tangent_angle_rad: 0.25,
            },
        }],
        segments: vec![
            Segment::Quad {
                ctrl: Pt::new(5.0, 2.5),
            },
            Segment::Quad {
                ctrl: Pt::new(15.0, 0.0),
            },
        ],
    };
    let control = g1_readings(&broken, Pt::new(0.0, 0.0), Pt::new(20.0, 0.0));
    assert!(
        control[0].spread_rad > 0.1,
        "the instrument reads {} rad on a chain whose declaration and geometry disagree; then \
             the zero above is a property of the instrument, not of the representation",
        control[0].spread_rad
    );
}

/// Changing the ONE angle moves BOTH control points, which is what "stored
/// once" means operationally.
#[test]
fn one_angle_moves_both_incident_control_points() {
    let a = cubic_chain(0.6, -0.3).lower().expect("lowers");
    let b = cubic_chain(0.9, -0.3).lower().expect("lowers");
    let (Segment::Cubic { ctrl2: a_in, .. }, Segment::Cubic { ctrl1: a_out, .. }) =
        (&a.segments[0], &a.segments[1])
    else {
        panic!("cubics");
    };
    let (Segment::Cubic { ctrl2: b_in, .. }, Segment::Cubic { ctrl1: b_out, .. }) =
        (&b.segments[0], &b.segments[1])
    else {
        panic!("cubics");
    };
    assert!(
        (*a_in - *b_in).length() > 1e-6 && (*a_out - *b_out).length() > 1e-6,
        "changing the node angle moved only one side: there are two copies of the direction"
    );
}

/// A zero shared handle has no tangent. The old witness silently replaced
/// that derivative with the span chord, so an independently chosen chord
/// could disagree with the declared angle while `lower()` still accepted
/// the chain.
#[test]
fn a_zero_shared_handle_is_not_a_smooth_representation() {
    let mut c = cubic_chain(0.6, -0.3);
    c.segments[0] = RefitSegment::Cubic {
        head: Handle::Free(Pt::new(3.0, -2.0)),
        tail: Handle::Shared { length_px: 0.0 },
    };
    assert!(matches!(
        c.lower(),
        Err(RefitRefusal::NonPositiveSharedHandle {
            segment: 0,
            length_px: 0.0
        })
    ));
}

#[test]
fn a_closed_smooth_seam_aliases_one_tangent_and_is_measured() {
    let mut chain = RefitChain {
        nodes: vec![
            RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: Some(0.0),
            },
            RefitNode {
                pos: Pt::new(10.0, 10.0),
                tangent_rad: Some(std::f64::consts::PI),
            },
            RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: Some(0.0),
            },
        ],
        segments: vec![
            RefitSegment::Cubic {
                head: Handle::Shared { length_px: 3.0 },
                tail: Handle::Shared { length_px: 3.0 },
            },
            RefitSegment::Cubic {
                head: Handle::Shared { length_px: 3.0 },
                tail: Handle::Shared { length_px: 3.0 },
            },
        ],
    };
    let complete = chain
        .lower_boundary_geometry()
        .expect("closed shared seam lowers");
    assert_eq!(
        complete.closure_join,
        Some(JoinKind::SmoothG1 {
            tangent_angle_rad: 0.0
        })
    );
    let lowered = complete.curve;
    let spread =
        closure_g1_spread_rad(&lowered, chain.start(), chain.end(), 0.0).expect("closure witness");
    assert!(spread < crate::GATE_MAX_G1_SPREAD_RAD);

    chain.nodes[2].tangent_rad = Some(0.2);
    assert!(matches!(
        chain.lower(),
        Err(RefitRefusal::G1Violation { node: 0, .. })
    ));
}

/// A corner node is the deliberate absence of sharing, and the instrument
/// does not report on it at all — there is nothing to be consistent with.
#[test]
fn a_corner_node_declares_no_tangent_and_is_not_measured() {
    let mut c = cubic_chain(0.6, -0.3);
    c.nodes[1].tangent_rad = None;
    c.segments[0] = RefitSegment::Cubic {
        head: Handle::Free(Pt::new(3.0, -2.0)),
        tail: Handle::Free(Pt::new(7.0, -4.0)),
    };
    c.segments[1] = RefitSegment::Cubic {
        head: Handle::Free(Pt::new(13.0, 4.0)),
        tail: Handle::Free(Pt::new(18.0, 6.0)),
    };
    let lowered = c.lower().expect("lowers");
    assert_eq!(lowered.interior_nodes[0].join, JoinKind::Corner);
    assert!(g1_readings(&lowered, c.start(), c.end()).is_empty());
}

/// An arc pinned by a shared tangent is G1 at that node by construction
/// too: its radius is not stored, it is derived from the same angle.
#[test]
fn an_arc_pinned_by_a_shared_tangent_is_g1_at_that_node() {
    let c = RefitChain {
        nodes: vec![
            RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
            RefitNode {
                pos: Pt::new(10.0, 0.0),
                tangent_rad: Some(0.4),
            },
            RefitNode {
                pos: Pt::new(18.0, 9.0),
                tangent_rad: None,
            },
        ],
        segments: vec![
            RefitSegment::Cubic {
                head: Handle::Free(Pt::new(3.0, -1.0)),
                tail: Handle::Shared { length_px: 3.0 },
            },
            RefitSegment::Arc(ArcAnchor::FromHeadTangent),
        ],
    };
    let lowered = c.lower().expect("lowers");
    let r = g1_readings(&lowered, c.start(), c.end());
    assert_eq!(r.len(), 1);
    assert!(
        r[0].spread_rad < 1e-9,
        "cubic-to-arc smooth join reads {} rad",
        r[0].spread_rad
    );
}

/// An arc whose prescribed tangent is along its own chord is a straight
/// line, and is refused rather than given an enormous radius.
#[test]
fn an_arc_tangent_to_its_own_chord_is_refused() {
    let c = RefitChain {
        nodes: vec![
            RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: Some(0.0),
            },
            RefitNode {
                pos: Pt::new(10.0, 0.0),
                tangent_rad: None,
            },
        ],
        segments: vec![RefitSegment::Arc(ArcAnchor::FromHeadTangent)],
    };
    assert_eq!(c.lower(), Err(RefitRefusal::ArcIsALine { segment: 0 }));
}

#[test]
fn the_canonical_angle_range_is_the_irs() {
    for a in [-7.0f64, -3.2, -std::f64::consts::PI, 0.0, 3.2, 7.0, 100.0] {
        let x = canonical_angle(a);
        assert!(
            x > -std::f64::consts::PI && x <= std::f64::consts::PI,
            "{a} folded to {x}"
        );
        assert!((canonical_angle(x - a) % std::f64::consts::TAU).abs() < 1e-9);
    }
}
