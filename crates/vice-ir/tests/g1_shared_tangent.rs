//! **Measurement, not a check.** What the SHARED tangent parameter of
//! [`JoinKind::SmoothG1`] is bound to, and what it is not.
//!
//! `curve.rs` carries the claim in prose — *"M1 carries the shared tangent
//! parameter in the types but does not yet enforce tangent/geometry
//! consistency"* — and `docs/STATUS_M6.md` §5 records a design decision built
//! on top of it: that segments reading their endpoint tangents from a shared
//! node variable makes exact G1 *"a claim the compiler enforces"*. That
//! decision is marked **not verified** in the document that states it, and an
//! unrun claim does not become a declared property of the tree
//! (FAILURE_LEDGER F-0078: *"derived from X" is an assertion about a LINE*).
//!
//! This file runs it. The result is that the claim is **half true**, in the
//! same shape REVIEW_M5_A D2-N1 found for §12's "closed **and** oriented":
//!
//! - **Held.** The tangent is stored ONCE, at the node. Two adjacent segments
//!   cannot carry two different *declared* tangents, because neither carries
//!   a declared tangent at all. That much is representation.
//! - **Not held.** `Segment::Quad`/`Cubic` store absolute control points, and
//!   a Bezier's actual endpoint tangent is `ctrl - p0`. The declared angle and
//!   the geometry are two independent values, and nothing in this workspace
//!   compares them. Sharing the *declaration* is not sharing the *tangent*.
//!
//! The witness below is **not** an adversarial value I constructed. It is
//! `common::mixed_island`, this crate's own canonical VALID fixture, asserted
//! valid by `validate_rejects::valid_scenes_pass` since M1. Its smooth node
//! carries three mutually different tangents. That matters for M-2: a
//! hand-built attack would show the type admits the value; the project's own
//! example of a good scene shows the gap is where the tree already lives.
//!
//! **Owner: §28 M6**, whose gate says "exact G1 after joint solve". §14.3 says
//! `angle < tolerance` is not G1, so the closure is not a comparison added
//! here — it is a representation in which the disagreement cannot be written
//! down: the segment must not store data that independently determines its
//! endpoint tangent. This test is written to FAIL when that lands, which is
//! the point of keeping the attack in the tree (FAILURE_LEDGER F-0055).

mod common;

use common::*;
use vice_geom::Pt;
use vice_ir::{validate_scene, GraphError, JoinKind, SceneError, Segment};

/// The angle of the tangent a segment actually leaves `a` with, and the one
/// it actually arrives at `b` with, in the frame `curve.rs` declares (x right,
/// y down, `atan2`).
///
/// Computed here from the Bezier definition rather than called from
/// `vice_ir` — deliberately. F-0048 Q4 asks whether the guard shares its
/// origin with the mechanism; an instrument that measured the tangent with
/// the library's own accessor would move together with a defect inside it.
/// Arcs return `None`: this witness does not need them, and a helper that
/// silently returned something for a case it cannot compute is F-0075.
fn actual_tangents(seg: &Segment, a: Pt, b: Pt) -> Option<(f64, f64)> {
    let ang = |dx: f64, dy: f64| dy.atan2(dx);
    match *seg {
        Segment::Line => {
            let t = ang(b.x - a.x, b.y - a.y);
            Some((t, t))
        }
        Segment::Quad { ctrl } => Some((
            ang(ctrl.x - a.x, ctrl.y - a.y),
            ang(b.x - ctrl.x, b.y - ctrl.y),
        )),
        Segment::Cubic { ctrl1, ctrl2 } => Some((
            ang(ctrl1.x - a.x, ctrl1.y - a.y),
            ang(b.x - ctrl2.x, b.y - ctrl2.y),
        )),
        Segment::CircularArc { .. } | Segment::EllipticArc { .. } => None,
    }
}

/// The chain of `mixed_island` that carries the smooth interior node, with
/// the three angles that meet there: what segment 0 arrives with, what
/// segment 1 leaves with, and what the node DECLARES.
///
/// Derived by searching the scene for the node rather than by indexing a
/// literal: if the fixture is rewritten so that no chain has a `SmoothG1`
/// interior node, this panics instead of measuring the wrong chain (F-0051 —
/// a negative result that is an instrument refusal, not a fact about the
/// world).
fn the_three_angles_at_the_smooth_node() -> (f64, f64, f64) {
    let scene = build_scene(128, 96, &[mixed_island(Pt::new(20.0, 20.0), red())]);
    validate_scene(&scene).expect("the fixture this crate calls valid must validate");

    for b in &scene.graph.boundaries {
        let chain = &b.curve;
        for (ni, node) in chain.interior_nodes.iter().enumerate() {
            let JoinKind::SmoothG1 { tangent_angle_rad } = node.join else {
                continue;
            };
            let pts = chain.node_positions(
                scene.graph.vertices[b.start_vertex.0 as usize].pos,
                scene.graph.vertices[b.end_vertex.0 as usize].pos,
            );
            let (incoming, outgoing) = (
                actual_tangents(&chain.segments[ni], pts[ni], pts[ni + 1]),
                actual_tangents(&chain.segments[ni + 1], pts[ni + 1], pts[ni + 2]),
            );
            let (Some((_, arrives)), Some((leaves, _))) = (incoming, outgoing) else {
                continue;
            };
            return (arrives, leaves, tangent_angle_rad);
        }
    }
    panic!(
        "no chain in this crate's own valid fixture has a SmoothG1 interior node between two \
         segments whose tangents this test can compute: the measurement below would be about \
         nothing, so it refuses rather than passing vacuously"
    );
}

/// **The measurement.** At a node the type calls G1-smooth, the segment that
/// arrives, the segment that leaves, and the declaration all disagree — and
/// the scene is valid.
#[test]
fn the_declared_tangent_at_a_smooth_node_is_bound_to_no_geometry() {
    let (arrives, leaves, declared) = the_three_angles_at_the_smooth_node();

    // Published, not merely asserted: a row standing on a quantity prints it,
    // so a later reader can see WHICH way it failed rather than only that it
    // did (F-0039, F-0059).
    let deg = |r: f64| r.to_degrees();
    println!(
        "smooth node, three tangents: arrives {:.5} rad ({:.2} deg) | leaves {:.5} rad \
         ({:.2} deg) | DECLARED {:.5} rad ({:.2} deg) | spread {:.2} deg",
        arrives,
        deg(arrives),
        leaves,
        deg(leaves),
        declared,
        deg(declared),
        deg(arrives.max(leaves).max(declared) - arrives.min(leaves).min(declared))
    );

    // Not a rounding question. A tenth of a radian is 5.7 degrees, which is a
    // visible kink — `vice_bench::prereg` names `broken_g1` as a
    // CATASTROPHIC failure kind for exactly this.
    let gap = 0.1_f64;
    assert!(
        (arrives - leaves).abs() > gap,
        "the two segments at the smooth node agree on the tangent ({arrives} vs {leaves}); this \
         test measures the case where they do NOT, and the fixture no longer provides it"
    );
    assert!(
        (arrives - declared).abs() > gap && (leaves - declared).abs() > gap,
        "the declared tangent {declared} agrees with a segment ({arrives} / {leaves}); the \
         measurement is about a declaration bound to NEITHER"
    );

    // The claim under test, stated as the assertion that fails when it lands.
    // G1 held BY REPRESENTATION means this value is not constructible. It is
    // constructible, it is this crate's own example of a valid scene, and
    // three angles that pairwise differ by more than 5.7 degrees sit at a node
    // whose type name is `SmoothG1`.
    //
    // §28 M6 owns it. When the joint solve derives control points FROM the
    // node tangent, this assertion is the thing that must stop holding.
    assert!(
        (arrives - leaves).abs() > gap,
        "G1 is now held by the representation; delete this witness and record where"
    );
}

/// The instrument is alive and it is looking at this exact field: `validate`
/// reads `tangent_angle_rad` and rejects it out of canonical range. Its power
/// over the field is RANGE, and range is all.
///
/// Without this leg the test above measures "nothing rejected it", which is
/// equally consistent with nothing looking (F-0048 Q5: red is half, and empty
/// from outside is indistinguishable from success).
#[test]
fn the_judge_reads_the_tangent_field_and_its_only_power_over_it_is_range() {
    let mut scene = build_scene(128, 96, &[mixed_island(Pt::new(20.0, 20.0), red())]);

    let (bi, ni) = scene
        .graph
        .boundaries
        .iter()
        .enumerate()
        .find_map(|(bi, b)| {
            b.curve
                .interior_nodes
                .iter()
                .position(|n| matches!(n.join, JoinKind::SmoothG1 { .. }))
                .map(|ni| (bi, ni))
        })
        .expect("the fixture carries a smooth interior node");

    // Leg 1 — ANY in-range declaration is accepted, however far from the
    // geometry. The disagreement is not bounded by the judge; it is unbounded
    // up to the canonical interval.
    for angle in [0.0_f64, 1.5, 3.0, -3.0] {
        scene.graph.boundaries[bi].curve.interior_nodes[ni].join = JoinKind::SmoothG1 {
            tangent_angle_rad: angle,
        };
        assert!(
            validate_scene(&scene).is_ok(),
            "declared tangent {angle} rad was rejected; then something DOES compare the \
             declaration against the geometry, and the measurement above is wrong"
        );
    }

    // Leg 2 — the field is read. Out of `(-pi, pi]` it is a typed reject, so
    // "nothing rejected the inconsistent value" is a statement about what the
    // judge checks, not about whether it ran.
    scene.graph.boundaries[bi].curve.interior_nodes[ni].join = JoinKind::SmoothG1 {
        tangent_angle_rad: 4.0,
    };
    match validate_scene(&scene) {
        Err(SceneError::Graph(GraphError::TangentAngleOutOfRange { node, .. })) => {
            assert_eq!(node, ni);
        }
        other => panic!(
            "expected the range check on the very field under measurement, got {other:?}: the \
             positive control for this instrument is gone"
        ),
    }
}
