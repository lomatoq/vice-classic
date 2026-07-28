//! The envelope-membership property, in its own module for the §4.1 size rule.
//!
//! It lives apart from `tests.rs` because it is the one check that must NOT
//! share a chain with what it verifies (RT45-A30), and keeping it separate makes
//! that visible rather than a comment in a nine-hundred-line file.

use super::{run, TopologyScope};

fn run_once() -> super::TopologyRun {
    run(TopologyScope::Test).expect("the test-scope topology run must succeed")
}

/// Every class a pair PUBLISHES came out of the envelope it claims to describe.
///
/// RT45-A24. The threshold site used to filter the published list by
/// `< PLAUSIBLE_CLASS_BOUND`, which is a bound describing the sentinel the red
/// team had already shown me — so padding with `(3, 1)` walked past it, the
/// artifact stayed byte-identical, eleven guards stayed green, and
/// `gate_min_classes_per_retaining_pair` stopped being falsifiable.
///
/// Comparing two readings of ONE list cannot catch padding: both readings see
/// the padded list. The input has to be the ENVELOPE, so this asks the envelope
/// again — the same call `ambiguity.rs` makes, with the same config — and
/// requires the published list to be exactly what comes back. There is no
/// constant to tune and nothing to append.
#[test]
fn every_published_class_came_from_the_envelope() {
    let run = run_once();
    let pairs = crate::gt::adversarial::ambiguity_pairs();
    let mut checked = 0usize;
    for row in run.ambiguity.iter().filter(|p| p.is_topology_pair) {
        let pair = pairs
            .iter()
            .find(|p| p.group.id == row.group_id)
            .expect("every published row has its pair");
        for (scene, published) in [
            (pair.group.scenes.first(), &row.classes_from_a),
            (pair.group.scenes.get(1), &row.classes_from_b),
        ] {
            let Some(scene) = scene else { continue };
            if published.is_empty() {
                continue;
            }
            let reachable = classes_reachable_by_thresholding(scene, &pair.collapse_cell);
            for class in published {
                assert!(
                    reachable.contains(class),
                    "pair {} publishes the class {class:?}, which NO threshold of its own                      observation produces. The envelope selects among labellings a threshold can                      yield, so a class outside that set did not come out of this observation -                      and no plausibility bound and no second call to the same function can tell                      the difference (RT45-A24, RT45-A30)",
                    row.group_id
                );
            }
            checked += 1;
        }
    }
    assert!(
        checked >= 2,
        "only {checked} published class lists were recomputed; this test would be checking          nothing"
    );
}

/// Every signature REACHABLE by thresholding the same observation, counted by
/// the independent chain.
///
/// RT45-A30 / M45-N34, and both cold contexts measured the same thing: the
/// delta-6 "independent path" shared EIGHT of nine links with what it checked -
/// `render_cell`, canonicalization, `analyze_full`, `ANALYSIS_CONFIG_V1`,
/// `observations_for`, `propose`, `TOPOLOGY_CONFIG_V1`, `signature_classes`.
/// What differed was the NAME OF THE FUNCTION the sequence was written in, so an
/// injection into `Envelope::signature_classes` cancelled between the two sides
/// and only the frozen artifact noticed - a number, not a property, and one that
/// had been noticing it before delta-6 too. The comparison I drew with
/// `independent.rs` was wrong: that module rewrites the ALGORITHM (bit-quad
/// Euler against union-find, holes DERIVED), not the call.
///
/// This rewrites the algorithm. It does not build an envelope at all: it walks
/// threshold labellings of the coverage field directly and counts components and
/// holes with `independent::signature_of`, which is the union-find-free chain
/// already written for the GT signature. `propose`, `signature_classes`,
/// `observations_for` and `TOPOLOGY_CONFIG_V1` are gone from the path.
///
/// FOUR links remain shared, and naming the number is the point: `render_cell`,
/// the canonical image, `analyze_full` and `ANALYSIS_CONFIG_V1`. Those produce
/// the OBSERVATION, which both sides must agree on for the comparison to mean
/// anything - an injection there changes the picture itself, and that is what
/// the frozen artifact is for.
///
/// The relation is CONTAINMENT, not equality: an envelope selects among the
/// labellings a threshold can produce, so every class it publishes must be one
/// of them. A class that no threshold of this observation yields did not come
/// out of this observation.
fn classes_reachable_by_thresholding(
    scene: &crate::gt::GtScene,
    cell: &crate::gt::degradation::DegradationCell,
) -> std::collections::BTreeSet<(u32, u32)> {
    use vice_evidence::analysis::{analyze_full, ANALYSIS_CONFIG_V1};
    use vice_image::{CanonicalImage, IccAssumption};
    use vice_ir::ComplementaryConnectivity;

    let fixture = crate::gt::degradation::render_cell(scene, cell, 2).expect("the render");
    let img = CanonicalImage::from_straight_srgb8(
        fixture.width_px,
        fixture.height_px,
        fixture.rgba8,
        true,
        IccAssumption::NoProfileAssumedSrgb,
    )
    .expect("canonical image");
    let out = analyze_full(&img, &ANALYSIS_CONFIG_V1, None);
    let ev = out.chosen.expect("a coverage field");
    let alpha = ev.alpha_field();
    let (w, h) = (ev.width_px() as usize, ev.height_px() as usize);

    let mut reachable = std::collections::BTreeSet::new();
    // Every 8-bit level, both admissible connectivity conventions. The envelope
    // may only ever propose a labelling some threshold of this field produces.
    for step in 0..=256u32 {
        let level = f64::from(step) / 256.0;
        let inside: Vec<bool> = alpha.iter().map(|v| *v >= level).collect();
        for conn in ComplementaryConnectivity::arms() {
            let sig = crate::topology::independent::signature_of(&inside, w, h, conn);
            reachable.insert((sig.components, sig.holes));
        }
    }
    reachable
}
