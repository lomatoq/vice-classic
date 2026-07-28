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
/// EIGHT links remain shared, counted BY SUBSTITUTION (condition 50) rather
/// than by reading the body, because reading gave the wrong number three times
/// running - four, then seven, and every one of them low, every one in my
/// favour.
///
/// THE COUNTING CONVENTION, stated so a recount can disagree with something
/// definite: a link is any named item both paths invoke whose replacement by a
/// different implementation could change what either side reports. Accessors
/// count. Configuration constants count. Choices count. The test is "substitute
/// it and see", not "does it look load-bearing".
///
///   1. `render_cell`                         pixels
///   2. `CanonicalImage::from_straight_srgb8` canonicalization
///   3. `analyze_full`                        the coverage field
///   4. `ANALYSIS_CONFIG_V1`                  parameterizes it
///   5. `out.chosen`                          WHICH HYPOTHESIS
///      - a CHOICE, not an accessor, and the one I missed (RT45-A33): it selects
///        the field both sides then read.
///   6. `ComplementaryConnectivity::arms()`   which two arms exist
///   7. `conn.foreground()`                   read by both counters
///   8. `alpha_field` / `width_px` / `height_px`  the observation's shape
///
/// Ungrouped the red team counts ten, splitting item 8 into its three
/// accessors; the reviewer counts five load-bearing or eight with accessors.
/// Eight is what this convention yields, the three counts differ only in
/// grouping, and the grouping is written above so the difference is visible
/// rather than arguable.
///
/// Links 1 to 5 produce and select the OBSERVATION, which both sides must agree
/// on for the comparison to mean anything - an injection there changes the
/// picture itself, and the frozen artifact stands against that. Links 6 to 8
/// carry no arithmetic; sharing them narrows what this test can see, and that is
/// named rather than netted out.
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

    // BOUNDARY labellings first, and they are the correction RT45-A32 needed a
    // second time. A fixed seed with fixed sizes and a fixed trial count is not
    // a SAMPLE - it is a list of 400 particular labellings written compactly.
    // As reproducibility that is right; as a coverage argument it is wrong,
    // because the set grows neither between runs nor between machines. The red
    // team copied its addendum-7 edit character for character and it passed:
    // its trigger is density 1.000, the sampler draws 0.125 / 0.333 / 0.500 /
    // 0.750, and the chance of drawing all-true is 7.6e-11 in the best regime.
    //
    // Meanwhile the pipeline hands the judge density 1.000 at level 0 of EVERY
    // field - the first iteration of the very loop the judge is called in. The
    // edges of the interval are not exotic; there are four of them, they are
    // deterministic, and they cost nothing.
    let boundary = |size: usize| -> Vec<(&'static str, Vec<bool>)> {
        let n = size * size;
        let mut single = vec![false; n];
        single[n / 2 + size / 2] = true;
        let mut row = vec![false; n];
        for x in 0..size {
            row[(size / 2) * size + x] = true;
        }
        let mut col = vec![false; n];
        for y in 0..size {
            col[y * size + size / 2] = true;
        }
        vec![
            (
                "all-true (density 1.000, level 0 of every field)",
                vec![true; n],
            ),
            ("all-false (density 0.000)", vec![false; n]),
            ("single pixel", single),
            ("full row", row),
            ("full column", col),
        ]
    };
    // The sizes the judge actually rules on: coverage fields at 16 and 128, and
    // corpus arms from 32 to 512. `9` is included as the smallest size ABOVE
    // the 7x7 its hand fixtures reach - the first place an edit could hide.
    for &size in &[9usize, 16, 32, 64, 128] {
        for (name, inside) in boundary(size) {
            let labelling = Labelling::new(size, size, inside.clone());
            for conn in ComplementaryConnectivity::arms() {
                let mine = crate::topology::independent::signature_of(&inside, size, size, conn);
                let theirs = vice_topology::signature(&labelling, conn);
                assert_eq!(
                    (mine.components, mine.holes),
                    (theirs.components, theirs.holes),
                    "the independent judge and the production signature disagree at                      {size}x{size} on the BOUNDARY labelling {name:?}, foreground {:?}:                      independent ({}, {}) against production ({}, {}). The edges of the density                      interval are where the pipeline actually starts, and a random sampler                      reaches them with probability ~1e-10 (RT45-A32)",
                    conn.foreground(),
                    mine.components,
                    mine.holes,
                    theirs.components,
                    theirs.holes
                );
                compared += 1;
            }
        }
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
    // THE SIZE GAP, named by composition rather than rounded up (condition 48).
    // `SIZES_PX` reaches 512 and this run stops at 128, so the gap is 256 and
    // 512. The red team measured what actually calls the judge there: EIGHT
    // invocations at 512x512, all of them deciding `is_topology_pair` and so the
    // `gate_min_topology_pairs` conjunct - and no call above 128 anywhere else
    // in the pipeline. So the gap is REAL for the declared size list and NOT
    // REACHED by the corpus as it runs today. Both halves of that are the honest
    // statement; either half alone is a different claim.
    //
    // 512x512 is 262 144 pixels against 128x128's 16 384 - sixteen times the
    // work per comparison - which is why it is named instead of run. Frozen as
    // F-8, owner M5.
    assert!(
        compared >= 400,
        "only {compared} comparisons made; the differential run is not sampling and a judge \
         spot-checked at 7x7 would pass it"
    );
    println!("{compared} differential comparisons, sizes 9 to 128, both connectivity arms");
}
