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
            // CONDITION 44: containment is only a check while the reference set
            // is NARROW. Saturate `reachable` and every published class is
            // inside it trivially - 1600 foreign classes used to pass in
            // silence, which is the M35-N4 shape ("green because the population
            // is empty") with the emptiness on the other side.
            //
            // The bound is a property of the observation, not a tuned number: a
            // labelling of an N-pixel field has at most N components, and the
            // envelope is a SELECTION among threshold labellings, so a reference
            // set approaching that ceiling is not selecting anything.
            let ceiling = (pair.collapse_cell.size_px as u64).pow(2);
            assert!(
                (reachable.len() as u64) * 8 < ceiling,
                "the reference set holds {} classes against a ceiling of {ceiling} for this                  field. Containment in a set that wide is not a check: it is satisfied by                  anything (condition 44)",
                reachable.len()
            );
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
/// SEVEN links remain shared, and the number is counted here rather than
/// asserted, because the previous two counts were both wrong in my favour -
/// eight claimed as "independent", then four when the real figure was more.
/// RT45-A33 found the fifth; recounting properly finds two more, so the list is
/// given in full and a reader can check it against the code without trusting me:
///
///   1. `render_cell`                 produces the pixels
///   2. `CanonicalImage::from_straight_srgb8`
///   3. `analyze_full`                produces the coverage field
///   4. `ANALYSIS_CONFIG_V1`          parameterizes it
///   5. `ComplementaryConnectivity::arms()`   which two arms exist (RT45-A33)
///   6. `conn.foreground()`           accessor used by both counters
///   7. `alpha_field()` / `width_px()` / `height_px()`  the observation's shape
///
/// Links 1 to 4 produce the OBSERVATION, which both sides must agree on for the
/// comparison to mean anything at all - an injection there changes the picture
/// itself, and the frozen artifact is what stands against that. Links 5 to 7 are
/// accessors with no arithmetic in them; sharing them narrows what this test can
/// see, and it is named rather than netted out.
///
/// What is NOT shared is the thing that matters: the counting. `propose`,
/// `signature_classes`, `observations_for` and `TOPOLOGY_CONFIG_V1` are gone,
/// and components and holes are computed by the bit-quad chain rather than by
/// union-find.
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

/// The JUDGE is measured over the sizes it judges, not over seven-by-seven.
///
/// RT45-A32, and it is meta-rule M-2 applied to the instrument itself.
/// `independent::signature_of` now decides two things: the ground truth of
/// clause 1, and which classes are reachable for clause 2. Its own proof was
/// TWELVE hand-written fixtures, the largest 7x7, while it rules on coverage
/// fields of 16 and 128 px and corpus arms from 32 to 512. An edit inside the
/// judge that only fires above 8 px left 504 tests green, the artifact
/// byte-identical, fmt clean, clippy silent - and its own cross-check green,
/// because the cross-check lives entirely inside the region the edit avoids.
///
/// A judge whose area of proof is smaller than its area of application is not
/// proven; it is spot-checked. So this is a DIFFERENTIAL run against the
/// production signature over random labellings at corpus sizes, which is the
/// form the red team used for 1 115 536 comparisons in addendum 2 - and which
/// never entered the tree, because it was its probe and not the project's test.
/// The project's own cross-check was twenty-four comparisons.
///
/// Deterministic: a fixed seed, so a failure is reproducible and a green run is
/// not a different sample each time.
#[test]
fn the_independent_judge_agrees_with_production_at_corpus_sizes() {
    use vice_ir::ComplementaryConnectivity;
    use vice_topology::cubical::Labelling;

    // xorshift64*, written here so the sample does not depend on a dependency
    // and so the seed is visibly fixed.
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    let mut compared = 0u64;
    // The sizes the judge actually rules on: coverage fields at 16 and 128, and
    // corpus arms from 32 to 512. `9` is included as the smallest size ABOVE
    // the 7x7 its hand fixtures reach - the first place an edit could hide.
    for &size in &[9usize, 16, 32, 64, 128] {
        for trial in 0..40 {
            // Two regimes: sparse (many small components) and dense (few large
            // ones with holes). Density is swept so neither end is the only
            // thing sampled.
            let threshold = match trial % 4 {
                0 => u64::MAX / 8,
                1 => u64::MAX / 3,
                2 => u64::MAX / 2,
                _ => u64::MAX / 4 * 3,
            };
            let inside: Vec<bool> = (0..size * size).map(|_| next() < threshold).collect();
            let labelling = Labelling::new(size, size, inside.clone());
            for conn in ComplementaryConnectivity::arms() {
                let mine = crate::topology::independent::signature_of(&inside, size, size, conn);
                let theirs = vice_topology::signature(&labelling, conn);
                assert_eq!(
                    (mine.components, mine.holes),
                    (theirs.components, theirs.holes),
                    "the independent judge and the production signature disagree at {size}x{size}, \
                     trial {trial}, foreground {:?}: independent says ({}, {}) and production says \
                     ({}, {}). One of them is wrong, and the judge rules on BOTH the ground truth \
                     of clause 1 and the reachable classes of clause 2 (RT45-A32)",
                    conn.foreground(),
                    mine.components,
                    mine.holes,
                    theirs.components,
                    theirs.holes
                );
                compared += 1;
            }
        }
    }
    assert!(
        compared >= 400,
        "only {compared} comparisons made; the differential run is not sampling and a judge \
         spot-checked at 7x7 would pass it"
    );
    println!("{compared} differential comparisons, sizes 9 to 128, both connectivity arms");
}
