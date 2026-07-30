use super::*;
use vice_evidence::BoundarySample;

#[test]
fn every_relation_kind_names_a_universe_family() {
    let names: Vec<&str> = RelationKind::ALL
        .iter()
        .map(|k| k.universe_name())
        .collect();
    assert_eq!(
        names,
        vec![
            "equal_radius",
            "concentric",
            "parallel_perpendicular",
            "parallel_perpendicular",
            "shared_baseline",
            "mirror_symmetry",
            "repeated_transforms"
        ]
    );
}

#[test]
fn line_constraints_are_geometrically_distinct() {
    let mut c = RefitChain {
        nodes: vec![
            crate::refit::RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(10.0, 0.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(12.0, 3.0),
                tangent_rad: None,
            },
        ],
        segments: vec![RefitSegment::Line, RefitSegment::Line],
    };
    let before = (c.nodes[2].pos - c.nodes[1].pos).length();
    assert!(bind_lines(&mut c, 0, 1, RelationKind::Parallel));
    let after = c.nodes[2].pos - c.nodes[1].pos;
    assert!((after.length() - before).abs() < 1e-9, "length changed");
    assert!(after.y.abs() < 1e-9, "not parallel with the x axis");
    assert!(after.x > 0.0, "the direction of travel reversed");

    let mut perpendicular = RefitChain {
        nodes: vec![
            crate::refit::RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(10.0, 0.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(12.0, 3.0),
                tangent_rad: None,
            },
        ],
        segments: vec![RefitSegment::Line, RefitSegment::Line],
    };
    assert!(bind_lines(
        &mut perpendicular,
        0,
        1,
        RelationKind::Perpendicular
    ));
    let d = perpendicular.nodes[2].pos - perpendicular.nodes[1].pos;
    assert!(d.dot(Pt::new(10.0, 0.0)).abs() < 1e-9);

    let mut baseline = RefitChain {
        nodes: vec![
            crate::refit::RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(10.0, 2.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(12.0, 5.0),
                tangent_rad: None,
            },
        ],
        segments: vec![RefitSegment::Line, RefitSegment::Line],
    };
    assert!(bind_lines(
        &mut baseline,
        0,
        1,
        RelationKind::SharedBaseline
    ));
    let first = baseline.nodes[1].pos - baseline.nodes[0].pos;
    assert!(
        (baseline.nodes[2].pos - baseline.nodes[0].pos)
            .cross(first)
            .abs()
            < 1e-9
    );
}

#[test]
fn repeated_transform_materializes_the_same_line_vector() {
    let mut chain = RefitChain {
        nodes: vec![
            crate::refit::RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(8.0, 2.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(20.0, 5.0),
                tangent_rad: None,
            },
        ],
        segments: vec![RefitSegment::Line, RefitSegment::Line],
    };
    assert!(bind_repeated_line(&mut chain, 0, 1));
    assert_eq!(
        chain.nodes[2].pos - chain.nodes[1].pos,
        chain.nodes[1].pos - chain.nodes[0].pos
    );
}

#[test]
fn concentric_means_the_materialized_arc_centres_coincide() {
    let mut concentric = RefitChain {
        nodes: vec![
            crate::refit::RefitNode {
                pos: Pt::new(5.0, 0.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(0.0, 5.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(-5.0, 0.0),
                tangent_rad: None,
            },
        ],
        segments: vec![
            RefitSegment::Arc(ArcAnchor::Radius {
                radius_px: 5.0,
                large_arc: false,
                ccw: true,
            }),
            RefitSegment::Arc(ArcAnchor::Radius {
                radius_px: 5.0,
                large_arc: false,
                ccw: true,
            }),
        ],
    };
    assert!(bind_arcs(&mut concentric, 0, 1, RelationKind::Concentric));
    assert!(
        (arc_centre(&concentric, 0).unwrap() - arc_centre(&concentric, 1).unwrap()).length()
            <= 32.0 * f64::EPSILON
    );

    // The first arc is the semicircle centred at (1,0). The second
    // chord's perpendicular bisector does not contain that centre. Merely
    // changing its radius can never make the two arcs concentric.
    let mut incompatible = RefitChain {
        nodes: vec![
            crate::refit::RefitNode {
                pos: Pt::new(2.0, -4.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(0.0, 4.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(2.0, 6.0),
                tangent_rad: None,
            },
        ],
        segments: vec![
            RefitSegment::Arc(ArcAnchor::Radius {
                radius_px: 17.0f64.sqrt(),
                large_arc: false,
                ccw: true,
            }),
            RefitSegment::Arc(ArcAnchor::Radius {
                radius_px: 3.0,
                large_arc: false,
                ccw: true,
            }),
        ],
    };
    let before = incompatible.clone();
    assert!(!bind_arcs(
        &mut incompatible,
        0,
        1,
        RelationKind::Concentric
    ));
    assert_eq!(incompatible, before, "a refused projection must be inert");
}

#[test]
fn mirror_loop_projects_a_perturbed_rectangle_to_bilateral_geometry() {
    let chain = RefitChain {
        nodes: vec![
            crate::refit::RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(10.0, 1.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(10.0, 8.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(-1.0, 7.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
        ],
        segments: vec![RefitSegment::Line; 4],
    };
    let (mirrored, saved) = bind_mirror_loop(&chain).expect("closed line loop");
    assert!(saved >= 2);
    assert_eq!(mirrored.nodes[0].pos, mirrored.nodes[4].pos);

    let center = mirrored.nodes[..4]
        .iter()
        .fold(Pt::ZERO, |sum, node| sum + node.pos)
        * 0.25;
    let axis = mirrored.nodes[0].pos - center;
    let u = axis * (1.0 / axis.length());
    let v = Pt::new(-u.y, u.x);
    let a = mirrored.nodes[1].pos - center;
    let b = mirrored.nodes[3].pos - center;
    assert!((a.dot(u) - b.dot(u)).abs() < 1e-12);
    assert!((a.dot(v) + b.dot(v)).abs() < 1e-12);
}

fn sample(p: Pt, normal: Pt) -> BoundarySample {
    BoundarySample {
        p,
        normal,
        halfwidth: 0.5,
        confidence: 1.0,
        weight_ds: 0.5,
        corr_length_px: 0.5,
    }
}

#[test]
fn projected_relation_resolve_improves_the_normal_objective() {
    let mut chain = RefitChain {
        nodes: vec![
            crate::refit::RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(10.0, 0.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(11.0, 10.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(0.0, 10.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
        ],
        segments: vec![RefitSegment::Line; 4],
    };
    assert!(bind_lines(&mut chain, 0, 2, RelationKind::Parallel));
    let samples = vec![
        sample(Pt::new(2.0, 0.0), Pt::new(0.0, 1.0)),
        sample(Pt::new(8.0, 0.0), Pt::new(0.0, 1.0)),
        sample(Pt::new(2.0, 9.5), Pt::new(0.0, 1.0)),
        sample(Pt::new(8.0, 9.5), Pt::new(0.0, 1.0)),
    ];
    let before = normal_objective(&chain, &samples);
    let (resolved, trace) = resolve_constrained(RelationKind::Parallel, &[0, 2], chain, &samples);
    let after = normal_objective(&resolved, &samples);
    assert!(after < before);
    assert!(trace.iter().any(|row| row.accepted));
    let a = resolved.nodes[1].pos - resolved.nodes[0].pos;
    let b = resolved.nodes[3].pos - resolved.nodes[2].pos;
    assert!(a.cross(b).abs() < 1e-9, "projection lost parallelism");
    assert_eq!(resolved.nodes[0].pos, resolved.nodes[4].pos);
}

#[test]
fn finite_difference_matches_the_normal_residual_jacobian() {
    let chain = RefitChain {
        nodes: vec![
            crate::refit::RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
            crate::refit::RefitNode {
                pos: Pt::new(10.0, 0.0),
                tangent_rad: None,
            },
        ],
        segments: vec![RefitSegment::Line],
    };
    let samples = [sample(Pt::new(5.0, 1.0), Pt::new(0.0, 1.0))];
    let eps = 1e-5;
    let shifted = |dy: f64| {
        let mut candidate = chain.clone();
        for node in &mut candidate.nodes {
            node.pos.y += dy;
        }
        normal_residuals(&candidate, &samples).unwrap()[0]
    };
    let jacobian = (shifted(eps) - shifted(-eps)) / (2.0 * eps);
    assert!(
        (jacobian.abs() - 2.0).abs() < 1e-6,
        "normal residual derivative was {jacobian}"
    );
}
