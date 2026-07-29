//! Properties of the candidate stage, on chains whose true shape is known.
//!
//! M-3 is the standing rule these are written against: a test that compares
//! against nothing cannot close the class of a plausible wrong answer. A fit
//! that returns a finite curve inside the canvas passes every totality
//! property and can still be the wrong curve. So each family is given a chain
//! sampled from a shape it should reproduce EXACTLY, and is required to
//! reproduce it — and, in the same test, required to fail on a shape it
//! should not, so that "small deviation" is not simply what this measurement
//! always reports.

use vice_evidence::{BoundaryChain, BoundarySample};
use vice_fit::{
    fit, flattening_error_px, hierarchical_schedule, proposal_cost, span_candidates, FitRefusal,
    SpanFamily, Support, DEVIATION_CHORD_TOLERANCE_PX, FITTED_FAMILIES, FIT_BUDGET_V1,
    MIN_SUPPORT_SAMPLES, PARAMETER_REFINEMENT_PASSES,
};
use vice_geom::Pt;

const HALFWIDTH_PX: f64 = 0.35;

/// Build a chain from a polyline, with trapezoid arclength weights and unit
/// normals from the local tangent — the shape `vice_evidence::observe_
/// boundaries` produces, without depending on an image to produce it.
fn chain_from(points: &[Pt]) -> BoundaryChain {
    let n = points.len();
    assert!(n >= 2, "a chain needs two points");
    let mut samples = Vec::with_capacity(n);
    let mut total = 0.0f64;
    for i in 0..n {
        let prev = if i == 0 { None } else { Some(points[i - 1]) };
        let next = if i + 1 == n {
            None
        } else {
            Some(points[i + 1])
        };
        let back = prev.map_or(0.0, |p| (points[i] - p).length());
        let fwd = next.map_or(0.0, |p| (p - points[i]).length());
        let weight_ds = 0.5 * (back + fwd);
        total += weight_ds;

        let tangent = match (prev, next) {
            (Some(a), Some(b)) => b - a,
            (None, Some(b)) => b - points[i],
            (Some(a), None) => points[i] - a,
            (None, None) => unreachable!("n >= 2"),
        };
        let len = tangent.length().max(f64::MIN_POSITIVE);
        // Left normal in the (x right, y down) frame.
        let normal = Pt::new(-tangent.y / len, tangent.x / len);

        samples.push(BoundarySample {
            p: points[i],
            normal,
            halfwidth: HALFWIDTH_PX,
            confidence: 1.0,
            weight_ds,
            corr_length_px: 1.0,
        });
    }
    BoundaryChain {
        samples,
        closed: false,
        length_px: total,
        corr_length_px: 1.0,
        vertices: n as u64,
    }
}

/// Points on a circular arc of `radius`, spanning `sweep` radians, sampled at
/// approximately `step` px of arclength.
fn arc_points(radius: f64, sweep: f64, step: f64) -> Vec<Pt> {
    let n = ((radius * sweep / step).round() as usize).max(3);
    (0..=n)
        .map(|i| {
            let a = sweep * i as f64 / n as f64;
            Pt::new(50.0 + radius * a.cos(), 50.0 + radius * a.sin())
        })
        .collect()
}

fn line_points(from: Pt, to: Pt, n: usize) -> Vec<Pt> {
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            from + (to - from) * t
        })
        .collect()
}

fn whole_run(chain: &BoundaryChain) -> Support {
    Support::new(0, chain.samples.len() - 1).expect("chain has two samples")
}

// ---------------------------------------------------------------------------
// Each family reproduces its own shape, and does not reproduce another's
// ---------------------------------------------------------------------------

/// A chain sampled from a line is reproduced by the line family to within the
/// arithmetic — and the same measurement on a chain sampled from an arc is
/// LARGE, so "small" is a fact about the fit rather than about the ruler.
#[test]
fn the_line_family_reproduces_a_line_and_does_not_reproduce_an_arc() {
    let straight = chain_from(&line_points(Pt::new(10.0, 10.0), Pt::new(90.0, 40.0), 40));
    let s = whole_run(&straight);
    let seg = fit(&straight.samples, s, SpanFamily::Line).expect("line always fits");
    let worst = proposal_cost(&straight.samples, s, &seg)
        .expect("line flattens")
        .max_normal_deviation_px;
    assert!(
        worst < 1e-9,
        "the line family deviates by {worst} px from a chain that IS a line"
    );

    let curved = chain_from(&arc_points(40.0, 1.2, 0.5));
    let s2 = whole_run(&curved);
    let seg2 = fit(&curved.samples, s2, SpanFamily::Line).expect("line always fits");
    let worst2 = proposal_cost(&curved.samples, s2, &seg2)
        .expect("line flattens")
        .max_normal_deviation_px;
    assert!(
        worst2 > 1.0,
        "the line family deviates by only {worst2} px from a 1.2 rad arc of radius 40; then the \
         deviation measurement is not measuring shape"
    );
}

/// The arc family recovers the radius it was sampled from, on both sweep
/// directions and across the half-turn where `large_arc` flips.
#[test]
fn the_arc_family_recovers_its_radius_and_its_arc_flags() {
    for radius in [8.0f64, 25.0, 120.0] {
        for sweep in [0.4f64, 1.5, 2.9, 4.2] {
            let chain = chain_from(&arc_points(radius, sweep, 0.4));
            let s = whole_run(&chain);
            let seg = fit(&chain.samples, s, SpanFamily::CircularArc).expect("arc fits an arc");
            let vice_ir::Segment::CircularArc {
                radius_px,
                large_arc,
                ..
            } = seg
            else {
                panic!("the arc family returned {seg:?}");
            };
            let rel = (radius_px - radius).abs() / radius;
            assert!(
                rel < 1e-6,
                "radius {radius}, sweep {sweep}: recovered {radius_px} (relative error {rel})"
            );
            assert_eq!(
                large_arc,
                sweep > std::f64::consts::PI,
                "radius {radius}, sweep {sweep}: large_arc flag disagrees with the swept angle"
            );

            // The deviation of an EXACTLY recovered arc is not zero, and the
            // reason is the ruler rather than the fit: the measurement is
            // taken against the flattened polyline, which is certified to
            // within `DEVIATION_CHORD_TOLERANCE_PX` of the true arc. So the
            // bound asserted here is the flattener's own certificate, not a
            // number chosen to make the test pass — this assertion was
            // written as `< 0.05` and the run reported 0.0599 px, which is
            // the tolerance showing through, not a bad fit.
            let worst = proposal_cost(&chain.samples, s, &seg)
                .expect("arc flattens")
                .max_euclidean_deviation_px;
            let certified = flattening_error_px(&seg, chain.samples[0].p, chain.samples[s.hi()].p);
            assert!(
                worst <= certified,
                "radius {radius}, sweep {sweep}: the recovered arc deviates {worst} px from its \
                 own samples, beyond the flattener's certified {certified} px — that is the fit, \
                 not the ruler"
            );
            assert!(
                certified <= DEVIATION_CHORD_TOLERANCE_PX * 1.01,
                "the flattener's certificate ({certified} px) exceeds the tolerance it was asked \
                 for; the bound above would then be vacuous"
            );
        }
    }
}

/// **The cubic family does NOT reproduce a cubic exactly, and this is the
/// measurement of by how much.**
///
/// The closed-form least squares solves for control points given a parameter
/// per sample; the parameter is unknown and is estimated, first from chord
/// length and then by projecting the samples onto the fitted curve
/// ([`PARAMETER_REFINEMENT_PASSES`] times). Footpoint iteration converges
/// linearly, so a fixed pass count leaves a floor. Measured on a chain
/// sampled from an exact cubic:
///
/// ```text
/// passes    0     8    16    24    40
/// max dev  2.27  0.56  0.22  0.12  0.07  px
/// ```
///
/// **This is a limitation of the candidate stage, not of the family**, its
/// owner is §28 M6 bullet 4 (the joint constrained refit, NOT STARTED), and
/// its price is either that refit or a Newton footpoint solve in place of the
/// projection. It is asserted here at its measured size rather than at zero,
/// because a test asserting a property the code does not have is worth less
/// than a test asserting the property it does.
///
/// What the stage DOES have is the ordering, and that is the differential leg
/// below (M-3): the cubic beats the quadratic and the line on this chain by a
/// factor of about 27, which is what a candidate stage is for.
#[test]
fn the_cubic_family_fits_a_cubic_to_a_measured_floor_and_ranks_it_first() {
    let (p0, c1, c2, p1) = (
        Pt::new(10.0, 60.0),
        Pt::new(25.0, 5.0),
        Pt::new(70.0, 95.0),
        Pt::new(90.0, 30.0),
    );
    let pts: Vec<Pt> = (0..=60)
        .map(|i| {
            let t = i as f64 / 60.0;
            let u = 1.0 - t;
            p0 * (u * u * u) + c1 * (3.0 * t * u * u) + c2 * (3.0 * t * t * u) + p1 * (t * t * t)
        })
        .collect();
    let chain = chain_from(&pts);
    let s = whole_run(&chain);

    let mut by_family = Vec::new();
    for fam in [SpanFamily::Cubic, SpanFamily::Quad, SpanFamily::Line] {
        let seg = fit(&chain.samples, s, fam).expect("fits");
        let c = proposal_cost(&chain.samples, s, &seg).expect("flattens");
        let (cost, worst) = (c.cost_px, c.max_normal_deviation_px);
        println!(
            "{:>16}: max d_n {worst:8.5} px | max d_euclid {:8.5} px | ratio {:6.3} | proposal              cost {cost:11.4}",
            fam.universe_name(),
            c.max_euclidean_deviation_px,
            c.max_normal_to_euclidean_ratio,
        );
        by_family.push((fam, cost, worst));
    }
    let (_, cubic_cost, cubic_dev) = by_family[0];
    let (_, quad_cost, _) = by_family[1];
    let (_, line_cost, _) = by_family[2];

    // The floor, at the size the sweep in the doc comment measured for
    // PARAMETER_REFINEMENT_PASSES = 8, with headroom rather than at the
    // measurement.
    assert!(
        cubic_dev < 1.0,
        "the cubic family deviates {cubic_dev} px from a chain sampled from an exact cubic; the \
         measured floor for {PARAMETER_REFINEMENT_PASSES} refinement passes is about 0.56 px, so \
         something other than the parameterisation is wrong"
    );
    // The differential leg: "small" has to mean small COMPARED TO something,
    // or it is a statement about the ruler (M-3).
    assert!(
        cubic_cost * 10.0 < quad_cost && cubic_cost * 10.0 < line_cost,
        "cubic {cubic_cost}, quad {quad_cost}, line {line_cost}: the family the chain was drawn \
         from does not clearly win, so the proposal cost is not ordering candidates"
    );
}

// ---------------------------------------------------------------------------
// §14.4: the proposal integral is invariant to resampling
// ---------------------------------------------------------------------------

/// **§14.4's stated property**: the cost is an integral in `ds` and therefore
/// does not depend on how densely the chain was sampled.
///
/// Measured on a residual that is NOT zero — a line fitted to an arc — because
/// a quantity that is zero at every density is invariant for a reason that has
/// nothing to do with the integral. The three densities are §14.5's
/// `0.25 / 0.5 / 1.0 px`, which is the invariance row that list opens with.
///
/// This is the candidate stage only. §14.5 asks the same of the FINAL
/// selection, and the final selection does not exist.
#[test]
fn the_proposal_cost_is_invariant_to_the_sample_step() {
    let mut measured = Vec::new();
    for step in [1.0f64, 0.5, 0.25] {
        let chain = chain_from(&arc_points(40.0, 1.2, step));
        let s = whole_run(&chain);
        let seg = fit(&chain.samples, s, SpanFamily::Line).expect("line always fits");
        let c = proposal_cost(&chain.samples, s, &seg).expect("line flattens");
        let (cost, worst) = (c.cost_px, c.max_normal_deviation_px);
        println!(
            "step {step:>5} px: samples {:5} | cost {cost:12.5} | max d_n {worst:8.5} px",
            chain.samples.len()
        );
        measured.push(cost);
    }
    let lo = measured.iter().cloned().fold(f64::INFINITY, f64::min);
    let hi = measured.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    assert!(
        lo > 0.0,
        "every density reported a zero cost, so this measures nothing"
    );
    let spread = (hi - lo) / lo;
    assert!(
        spread < 0.02,
        "the proposal cost moved {:.3} % across sample steps 1.0 / 0.5 / 0.25 px ({measured:?}); \
         §14.4 says it is invariant to resampling through ds",
        spread * 100.0
    );
}

/// The same integral under translation. A cost that moved with the origin
/// would rank candidates by where the scene sits on the canvas.
#[test]
fn the_proposal_cost_is_invariant_to_translation() {
    let base = arc_points(40.0, 1.2, 0.5);
    let shifted: Vec<Pt> = base.iter().map(|p| *p + Pt::new(137.0, -61.0)).collect();

    let cost_of = |pts: &[Pt]| {
        let chain = chain_from(pts);
        let s = whole_run(&chain);
        let seg = fit(&chain.samples, s, SpanFamily::Line).expect("line always fits");
        proposal_cost(&chain.samples, s, &seg)
            .expect("line flattens")
            .cost_px
    };
    let (a, b) = (cost_of(&base), cost_of(&shifted));
    assert!(
        a > 0.0,
        "the un-shifted cost is zero, so this measures nothing"
    );
    assert!(
        (a - b).abs() / a < 1e-9,
        "translation moved the proposal cost from {a} to {b}"
    );
}

// ---------------------------------------------------------------------------
// The chain-level entry point, its refusals, and what it publishes
// ---------------------------------------------------------------------------

#[test]
fn a_run_over_a_real_shape_publishes_its_own_population() {
    let chain = chain_from(&arc_points(40.0, 2.0, 0.5));
    let out = span_candidates(&chain, &FIT_BUDGET_V1).expect("a well-formed chain");

    assert_eq!(
        out.supports,
        vice_fit::anchored_schedule(out.chain_samples, &out.anchors).len()
    );
    assert!(
        out.supports >= hierarchical_schedule(out.chain_samples).len(),
        "the anchored schedule offered fewer supports than the plain one"
    );
    assert!(
        out.candidates.len() >= out.supports,
        "{} candidates over {} supports: at least the line family fits every support",
        out.candidates.len(),
        out.supports
    );
    // Every family that has a fitter produced something on a smooth arc.
    assert_eq!(
        out.families_present.len(),
        FITTED_FAMILIES.len(),
        "families present {:?}, no-fits {:?}",
        out.families_present,
        out.no_fits
    );
    // Every candidate's support is an interval of THIS chain.
    for c in &out.candidates {
        assert!(c.support.hi() < out.chain_samples);
        assert!(c.proposal_cost_px().is_finite() && c.proposal_cost_px() >= 0.0);
        assert!(c.max_deviation_px().is_finite());
    }
    println!(
        "samples {} | supports {} | candidates {} | families {:?} | headroom {} | worst d_n / \
         d_euclid {:.3} at {:.5} px | cost refusals {:?}",
        out.chain_samples,
        out.supports,
        out.candidates.len(),
        out.families_present,
        out.budget_headroom,
        out.max_normal_to_euclidean_ratio,
        out.ratio_at_deviation_px,
        out.no_costs,
    );
}

#[test]
fn a_chain_too_short_to_judge_is_refused_with_its_size() {
    let chain = chain_from(&line_points(Pt::new(0.0, 0.0), Pt::new(1.0, 0.0), 1));
    match span_candidates(&chain, &FIT_BUDGET_V1) {
        Err(FitRefusal::ChainTooShort { samples, minimum }) => {
            assert_eq!(samples, 2);
            assert_eq!(minimum, MIN_SUPPORT_SAMPLES);
        }
        other => panic!("expected ChainTooShort, got {other:?}"),
    }
}

/// The budget REFUSES and names the four numbers. It does not truncate: the
/// schedule is ordered by position and scale, so dropping a tail would drop
/// whole scales, which is the loss §14.2's second sentence forbids.
#[test]
fn a_budget_that_would_bind_refuses_and_names_the_numbers() {
    let chain = chain_from(&arc_points(40.0, 2.0, 0.5));
    let tight = vice_fit::FitBudget::new(8).expect("positive");
    match span_candidates(&chain, &tight) {
        Err(FitRefusal::BudgetExceeded {
            samples,
            supports,
            would_generate,
            cap,
        }) => {
            assert_eq!(cap, 8);
            assert_eq!(would_generate, supports * FITTED_FAMILIES.len());
            assert_eq!(samples, chain.samples.len());
            assert!(would_generate > cap);
        }
        other => panic!("expected BudgetExceeded, got {other:?}"),
    }
}

#[test]
fn a_corridor_halfwidth_of_zero_is_refused_rather_than_floored() {
    let mut chain = chain_from(&arc_points(40.0, 2.0, 0.5));
    chain.samples[7].halfwidth = 0.0;
    match span_candidates(&chain, &FIT_BUDGET_V1) {
        Err(FitRefusal::NonPositiveHalfwidth {
            sample,
            halfwidth_px,
        }) => {
            assert_eq!(sample, 7);
            assert_eq!(halfwidth_px, 0.0);
        }
        other => panic!("expected NonPositiveHalfwidth, got {other:?}"),
    }
}

#[test]
fn a_non_finite_sample_is_refused_before_anything_is_fitted() {
    let mut chain = chain_from(&arc_points(40.0, 2.0, 0.5));
    chain.samples[3].p.x = f64::NAN;
    match span_candidates(&chain, &FIT_BUDGET_V1) {
        Err(FitRefusal::NonFiniteSample { sample }) => assert_eq!(sample, 3),
        other => panic!("expected NonFiniteSample, got {other:?}"),
    }
}

/// A straight chain makes the arc family degenerate, and the refusal is
/// COUNTED rather than swallowed. Without this the difference between "the
/// arc never applies here" and "the arc fitter is broken" is invisible.
#[test]
fn a_degenerate_arc_is_counted_not_swallowed() {
    let chain = chain_from(&line_points(Pt::new(10.0, 10.0), Pt::new(90.0, 10.0), 60));
    let out = span_candidates(&chain, &FIT_BUDGET_V1).expect("a well-formed chain");
    let arcs: usize = out
        .no_fits
        .iter()
        .filter(|(f, why, _)| *f == "circular_arc" && *why == vice_fit::NoFit::ArcIsDegenerate)
        .map(|(_, _, n)| *n)
        .sum();
    assert!(
        arcs > 0,
        "a perfectly straight chain produced no degenerate-arc refusals; no_fits {:?}, families \
         {:?}",
        out.no_fits,
        out.families_present
    );
    assert!(
        !out.families_present.contains(&"circular_arc"),
        "the arc family produced a candidate on a straight chain: it is inventing a radius the \
         data does not choose"
    );
}

/// **The witness M6A-N3 found missing.** F-0088's guard could be reverted to
/// finiteness-only with the entire workspace green: no test constructed a
/// non-unit normal. Two legs, because a red without a green beside it cannot
/// tell a working refusal from a refusal that fires on everything.
#[test]
fn a_zero_normal_is_refused_by_the_contract_check_and_a_unit_one_is_not() {
    let mut chain = chain_from(&arc_points(40.0, 2.0, 0.5));
    chain.samples[0].normal = Pt::new(0.0, 0.0);
    match span_candidates(&chain, &FIT_BUDGET_V1) {
        Err(vice_fit::FitRefusal::NonUnitNormal { sample, length }) => {
            assert_eq!(sample, 0, "the refusal must name the malformed sample");
            assert_eq!(length, 0.0);
        }
        other => panic!(
            "a zero normal must be refused where it is malformed, got {other:?}; unrefused it \
             empties the DAG downstream with no explanation (F-0088)"
        ),
    }
    // Leg two: the same chain with the normal restored passes, so the guard
    // is a contract check and not a refusal that fires on everything.
    let healthy = chain_from(&arc_points(40.0, 2.0, 0.5));
    assert!(span_candidates(&healthy, &FIT_BUDGET_V1).is_ok());
    // And a normal within the declared tolerance is NOT refused: the check is
    // about the contract, not about floating-point piety.
    let mut nearly = chain_from(&arc_points(40.0, 2.0, 0.5));
    let n = nearly.samples[3].normal;
    nearly.samples[3].normal = n * (1.0 + 1e-12);
    assert!(span_candidates(&nearly, &FIT_BUDGET_V1).is_ok());
}

/// **RT6-A2's corruption, replayed as a refusal.** `corr_length_px = 0` used to
/// pass the guard and empty the DAG silently — 1604 candidates, 0 paths,
/// 0 models, refused [] — through the field NEXT TO the one F-0088's fix
/// checked. The guard now covers every field of the contract this crate reads.
#[test]
fn a_non_positive_correlation_length_is_refused_where_it_is_malformed() {
    for bad in [0.0f64, -1.0, f64::NAN] {
        let mut chain = chain_from(&arc_points(40.0, 2.0, 0.5));
        chain.samples[5].corr_length_px = bad;
        match span_candidates(&chain, &FIT_BUDGET_V1) {
            Err(vice_fit::FitRefusal::NonPositiveCorrLength {
                sample,
                corr_length_px,
            }) => {
                assert_eq!(sample, 5);
                assert!(corr_length_px == bad || (bad.is_nan() && corr_length_px.is_nan()));
            }
            other => panic!("corr_length_px = {bad} must be refused, got {other:?}"),
        }
    }
    // Leg two, through the WHOLE pipeline: the red team's measurement was not
    // "span_candidates admits it" but "the run ends with no path and no
    // refusal". The typed refusal must now be what the pipeline reports.
    let mut chain = chain_from(&arc_points(40.0, 2.0, 0.5));
    chain.samples[0].corr_length_px = 0.0;
    let r = vice_fit::k_best_boundary_models(
        &chain,
        &FIT_BUDGET_V1,
        &vice_fit::GEOMETRY_CODE_TABLE_V1,
        256.0,
        vice_fit::K_DISCRETE_PATHS,
    );
    assert!(
        matches!(
            r,
            Err(vice_fit::FitRefusal::NonPositiveCorrLength { sample: 0, .. })
        ),
        "the pipeline must surface the typed refusal, got {r:?}"
    );
}

/// **M6B-N3's probe, kept as the regression test.** An empty CLOSED chain
/// panicked in `rotate` (index out of bounds) while every other degenerate
/// input was refused by name — in the milestone that recorded F-0088's rule.
#[test]
fn an_empty_closed_chain_is_refused_not_a_panic() {
    use vice_evidence::BoundaryChain;
    for n_samples in [0usize, 1, 2, 3] {
        // Built by hand rather than through `chain_from`, whose own contract
        // requires two points — the subject here is precisely the chains no
        // honest builder makes, arriving through the public constructor.
        let samples: Vec<vice_evidence::BoundarySample> = arc_points(40.0, 2.0, 0.5)
            .into_iter()
            .take(n_samples)
            .map(|p| vice_evidence::BoundarySample {
                p,
                normal: Pt::new(0.0, -1.0),
                halfwidth: HALFWIDTH_PX,
                confidence: 1.0,
                weight_ds: 0.5,
                corr_length_px: 1.0,
            })
            .collect();
        let chain = BoundaryChain {
            vertices: samples.len() as u64,
            samples,
            closed: true,
            length_px: 0.0,
            corr_length_px: 1.0,
        };
        match vice_fit::k_best_boundary_models(
            &chain,
            &FIT_BUDGET_V1,
            &vice_fit::GEOMETRY_CODE_TABLE_V1,
            256.0,
            vice_fit::K_DISCRETE_PATHS,
        ) {
            Err(vice_fit::FitRefusal::ChainTooShort { samples, minimum }) => {
                assert_eq!(samples, n_samples);
                assert_eq!(minimum, MIN_SUPPORT_SAMPLES);
            }
            other => {
                panic!("a {n_samples}-sample closed chain must be ChainTooShort, got {other:?}")
            }
        }
    }
}
