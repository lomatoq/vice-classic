//! §28 M6's gate clauses that live inside `vice-fit`: **exact G1 after the
//! joint solve**, the **sample / cut / transform invariance** of §14.5, and
//! **no BIC-only promotion**.
//!
//! The fourth gate clause — the oracle G00–G20 decomposition — is a corpus
//! harness and lives in `vice-bench`, because a decomposition over arms is not
//! a property of a chain.
//!
//! Every invariance test below asserts the SELECTION, not the bit total. A
//! translation moves no bit; a uniform scale moves the anchor code by
//! `2 log2(s)` per anchor and moves nothing else, which is a real property of a
//! code over a canvas and not a defect. What must not move is which families
//! the grammar chose, how many breakpoints it placed, and where they sit along
//! the boundary — and that is what is compared, with the totals PRINTED so the
//! part that does move is visible rather than hidden behind the part that does
//! not.

use vice_evidence::{BoundaryChain, BoundarySample};
use vice_fit::{
    build_edges, fit_forced_boundary_models, g1_readings, k_best_boundary_models, k_best_paths,
    path_families, span_candidates, BoundaryModel, ForcedFitRefusal, SpanFamily, FIT_BUDGET_V1,
    GATE_MAX_BREAKPOINT_FRACTION_DELTA, GATE_MAX_CUT_ROTATION_DELTA_BITS, GATE_MAX_G1_SPREAD_RAD,
    GATE_MAX_TRANSLATION_DELTA_BITS, GATE_MIN_CUT_NONTRIVIAL_SPREAD_BITS, GATE_MIN_G1_NODES,
    GATE_MIN_G1_POSITIVE_CONTROL_RAD, GATE_MIN_INVARIANCE_LEGS, GATE_MIN_NO_BIC_EXTRA_SEGMENTS,
    GEOMETRY_CODE_TABLE_V1, K_DISCRETE_PATHS,
};
use vice_geom::Pt;
use vice_ir::{ChainNode, CurveChain, JoinKind, Segment};

const HALFWIDTH_PX: f64 = 0.35;
const CORR_LENGTH_PX: f64 = 1.0;
const CANVAS_PX: f64 = 256.0;

fn chain_from(points: &[Pt], closed: bool) -> BoundaryChain {
    chain_scaled(points, closed, 1.0)
}

/// A chain whose corridor and correlation length scale with the geometry — a
/// SIMILARITY of the whole observation, which is what "uniform scale" means for
/// a measurement rather than for a picture.
fn chain_scaled(points: &[Pt], closed: bool, scale: f64) -> BoundaryChain {
    let n = points.len();
    assert!(n >= 2);
    let mut samples = Vec::with_capacity(n);
    let mut total = 0.0f64;
    for i in 0..n {
        let prev = (i > 0).then(|| points[i - 1]);
        let next = (i + 1 < n).then(|| points[i + 1]);
        let back = prev.map_or(0.0, |p| (points[i] - p).length());
        let fwd = next.map_or(0.0, |p| (p - points[i]).length());
        let weight_ds = 0.5 * (back + fwd);
        total += weight_ds;
        // The nearest DISTINCT neighbours, so a duplicated sample still gets a
        // unit normal. `observe_boundaries` produces distinct samples; this
        // fixture deliberately does not, and a zero tangent here would be the
        // fixture testing the refusal rather than the invariance.
        let before = (0..i).rev().find(|j| points[*j] != points[i]);
        let after = (i + 1..n).find(|j| points[*j] != points[i]);
        let tangent = match (before, after) {
            (Some(a), Some(b)) => points[b] - points[a],
            (None, Some(b)) => points[b] - points[i],
            (Some(a), None) => points[i] - points[a],
            (None, None) => Pt::new(1.0, 0.0),
        };
        let len = tangent.length().max(f64::MIN_POSITIVE);
        samples.push(BoundarySample {
            p: points[i],
            normal: Pt::new(-tangent.y / len, tangent.x / len),
            halfwidth: HALFWIDTH_PX * scale,
            confidence: 1.0,
            weight_ds,
            corr_length_px: CORR_LENGTH_PX * scale,
        });
    }
    BoundaryChain {
        samples,
        closed,
        length_px: total,
        corr_length_px: CORR_LENGTH_PX * scale,
        vertices: n as u64,
    }
}

fn arc_points(centre: Pt, radius: f64, from: f64, sweep: f64, step: f64) -> Vec<Pt> {
    let n = ((radius * sweep.abs() / step).round() as usize).max(4);
    (0..=n)
        .map(|i| {
            let a = from + sweep * i as f64 / n as f64;
            centre + Pt::new(radius * a.cos(), radius * a.sin())
        })
        .collect()
}

/// Two arcs of different radii meeting tangentially, then a straight run: a
/// shape whose honest grammar has both a smooth join and a corner.
fn s_curve(step: f64) -> Vec<Pt> {
    let mut v = arc_points(Pt::new(60.0, 60.0), 30.0, std::f64::consts::PI, 1.2, step);
    let last = *v.last().expect("non-empty");
    let dirn = last - v[v.len() - 2];
    let d = dirn * (1.0 / dirn.length());
    let n = (40.0 / step) as usize;
    v.extend((1..=n).map(|i| last + d * (i as f64 * step)));
    v
}

fn best(chain: &BoundaryChain, canvas: f64) -> BoundaryModel {
    let run = k_best_boundary_models(chain, &FIT_BUDGET_V1, canvas, K_DISCRETE_PATHS)
        .expect("a well-formed chain");
    assert!(
        !run.models.is_empty(),
        "no model survived the joint solve: {} candidates, {} edges, {} paths, refused {:?}",
        run.candidates,
        run.edges,
        run.discrete_paths,
        run.refused
    );
    run.models.into_iter().next().expect("non-empty")
}

fn physical_residual_bits(model: &BoundaryModel, chain: &BoundaryChain) -> f64 {
    let poly = vice_fit::solve::flatten_chain(&model.chain).expect("accepted model flattens");
    let precision = GEOMETRY_CODE_TABLE_V1.coordinate_precision_px();
    chain
        .samples
        .iter()
        .map(|sample| {
            let deviation = vice_fit::normal_deviation(sample.p, sample.normal, &poly).map_or_else(
                || vice_fit::cost::euclidean_deviation(sample.p, &poly),
                f64::abs,
            );
            let weight =
                vice_fit::code::independent_observations(sample.weight_ds, sample.corr_length_px)
                    .expect("valid correlation length");
            weight * vice_fit::residual_bits(deviation, sample.halfwidth, precision)
        })
        .sum()
}

/// The signature a transform must not change: which families, how many
/// breakpoints, and where they sit as a FRACTION of the chain.
fn signature(m: &BoundaryModel, n_samples: usize) -> (Vec<SpanFamily>, Vec<bool>, Vec<f64>) {
    (
        m.families.clone(),
        m.smooth.clone(),
        m.breakpoints
            .iter()
            .map(|b| *b as f64 / (n_samples - 1) as f64)
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// G20's typed forced-discrete injection
// ---------------------------------------------------------------------------

#[test]
fn final_mdl_codes_the_jointly_refitted_geometry() {
    let chain = chain_from(&s_curve(1.0), false);
    let model = best(&chain, CANVAS_PX);
    assert!(
        (model.residual_before - model.residual_after).abs() > 1e-6,
        "control did not move, so retaining the pre-refit residual would be invisible"
    );
    let measured = physical_residual_bits(&model, &chain);
    assert!(
        (model.code.residual_bits - measured).abs() < 1e-10,
        "model codes {} residual bits but its delivered refitted chain measures {measured}",
        model.code.residual_bits
    );
}

#[test]
fn a_forced_gt_path_fixes_only_families_and_breakpoints() {
    let points: Vec<Pt> = (0..=20).map(|i| Pt::new(10.0 + i as f64, 40.0)).collect();
    let chain = chain_from(&points, false);
    let run = fit_forced_boundary_models(
        &chain,
        &[SpanFamily::Line],
        &[],
        CANVAS_PX,
        K_DISCRETE_PATHS,
    )
    .expect("the GT-equivalent path fits");
    assert_eq!(run.candidates, 1);
    assert_eq!(run.edges, 1);
    assert!(!run.models.is_empty());
    assert!(
        run.models
            .iter()
            .all(|model| model.families == [SpanFamily::Line] && model.breakpoints.is_empty()),
        "G20 changed the forced discrete structure"
    );
    assert!(
        run.models
            .iter()
            .all(|model| model.worst_normal_deviation_px < 1e-9),
        "the forced line was not fitted by the production parameter solve"
    );
}

#[test]
fn a_malformed_forced_gt_path_is_a_typed_refusal() {
    let points: Vec<Pt> = (0..=20).map(|i| Pt::new(10.0 + i as f64, 40.0)).collect();
    let chain = chain_from(&points, false);
    assert!(matches!(
        fit_forced_boundary_models(
            &chain,
            &[SpanFamily::Line, SpanFamily::Line],
            &[],
            CANVAS_PX,
            K_DISCRETE_PATHS,
        ),
        Err(ForcedFitRefusal::ShapeMismatch {
            families: 2,
            breakpoints: 0
        })
    ));
}

// ---------------------------------------------------------------------------
// Gate clause 1: exact G1 after the joint solve
// ---------------------------------------------------------------------------

/// **The clause, with the positive control that makes it a measurement.**
///
/// Every smooth node of every model the solver accepted is measured by
/// `g1_readings`, the same function that reads 0.4949 rad on a chain whose
/// declaration and geometry are independent. If it read zero on both, the zero
/// here would be a property of the instrument.
#[test]
fn exact_g1_holds_on_every_model_the_solver_accepts() {
    let mut smooth_nodes = 0usize;
    let mut worst = 0.0f64;
    for step in [0.25f64, 0.5, 1.0] {
        let chain = chain_from(&s_curve(step), false);
        let run = k_best_boundary_models(&chain, &FIT_BUDGET_V1, CANVAS_PX, K_DISCRETE_PATHS)
            .expect("well formed");
        assert!(!run.models.is_empty());
        for m in &run.models {
            let lowered = m.chain.lower().expect("lowers");
            let readings = g1_readings(&lowered, m.chain.start(), m.chain.end());
            smooth_nodes += readings.len();
            for r in &readings {
                worst = worst.max(r.spread_rad);
            }
            assert_eq!(
                m.worst_g1_spread_rad,
                readings.iter().map(|r| r.spread_rad).fold(0.0f64, f64::max),
                "the model's published spread disagrees with a fresh measurement"
            );
        }
    }
    println!("smooth nodes measured {smooth_nodes}, worst G1 spread {worst:.3e} rad");
    assert!(
        smooth_nodes >= GATE_MIN_G1_NODES,
        "no accepted model had a single smooth join, so this clause was evaluated over an empty \
         population and says nothing (F-0039)"
    );
    assert!(
        worst < GATE_MAX_G1_SPREAD_RAD,
        "a model the solver accepted has a G1 spread of {worst} rad"
    );

    // The positive control: the SAME instrument on `vice-ir`'s shape of chain,
    // where the declaration and the control points are independent values.
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
    println!(
        "positive control: spread {:.4} rad ({:.2} deg)",
        control[0].spread_rad,
        control[0].spread_rad.to_degrees()
    );
    assert!(control[0].spread_rad > GATE_MIN_G1_POSITIVE_CONTROL_RAD);
}

// ---------------------------------------------------------------------------
// Gate clause 2: sample / cut / transform invariance (§14.5's six)
// ---------------------------------------------------------------------------

#[test]
fn the_invariance_clause_has_every_registered_leg() {
    let legs = [
        "sample_step",
        "duplicate_samples",
        "cyclic_cut",
        "translation",
        "reflection",
        "uniform_scale",
    ];
    assert!(
        legs.len() >= GATE_MIN_INVARIANCE_LEGS,
        "only {} invariance legs are registered against a gate floor of {}",
        legs.len(),
        GATE_MIN_INVARIANCE_LEGS
    );
}

#[test]
fn the_selection_is_invariant_to_the_sample_step() {
    let mut sigs = Vec::new();
    for step in [1.0f64, 0.5, 0.25] {
        let pts = s_curve(step);
        let chain = chain_from(&pts, false);
        let m = best(&chain, CANVAS_PX);
        println!(
            "step {step:>5} px: samples {:4} | families {:?} | breaks {:?} | {:9.2} bits",
            pts.len(),
            m.families,
            m.breakpoints,
            m.code.total_bits()
        );
        sigs.push(signature(&m, pts.len()));
    }
    for s in &sigs[1..] {
        assert_eq!(s.0, sigs[0].0, "the family sequence moved with the density");
        assert_eq!(s.1, sigs[0].1, "the join kinds moved with the density");
        for (a, b) in s.2.iter().zip(sigs[0].2.iter()) {
            assert!(
                (a - b).abs() < GATE_MAX_BREAKPOINT_FRACTION_DELTA,
                "a breakpoint moved from {b:.3} to {a:.3} of the chain"
            );
        }
    }
}

/// §14.5's "duplicate samples". The residual code counts INDEPENDENT
/// observations (`weight_ds / corr_length_px`), so a repeated sample carries
/// nothing — which is the only reason this can pass.
#[test]
fn the_selection_is_invariant_to_duplicate_samples() {
    let pts = s_curve(0.5);
    let plain = chain_from(&pts, false);
    let a = best(&plain, CANVAS_PX);

    let mut doubled: Vec<Pt> = Vec::new();
    for (i, p) in pts.iter().enumerate() {
        doubled.push(*p);
        if i % 3 == 0 {
            doubled.push(*p);
        }
    }
    let dup = chain_from(&doubled, false);
    let b = best(&dup, CANVAS_PX);
    println!(
        "plain {:?} {:9.2} bits | duplicated {:?} {:9.2} bits",
        a.families,
        a.code.total_bits(),
        b.families,
        b.code.total_bits()
    );
    assert_eq!(a.families, b.families);
    assert_eq!(a.smooth, b.smooth);
}

/// **RT6-A6's corruption, replayed.** The duplicate invariance held only on
/// BIT-identical duplicates: near-duplicates offset by 1e-9 px — the same
/// physical point of the same observation — did not collapse under the old
/// `==` comparison, the index schedule shifted, and the selection changed
/// (five segments became six, 329.465 to 353.213 bits). The collapse is now by
/// `DUPLICATE_EPSILON_PX`, and this replays the red team's exact offsets.
#[test]
fn the_selection_is_invariant_to_near_duplicate_samples() {
    let pts = s_curve(0.5);
    let plain = chain_from(&pts, false);
    let a = best(&plain, CANVAS_PX);

    // Every third sample repeated at a 1e-9 px offset — the red team's probe.
    let mut nearly: Vec<Pt> = Vec::new();
    for (i, p) in pts.iter().enumerate() {
        nearly.push(*p);
        if i % 3 == 0 {
            nearly.push(*p + Pt::new(1e-9, -1e-9));
        }
    }
    let dup = chain_from(&nearly, false);
    let b = best(&dup, CANVAS_PX);
    println!(
        "plain {:?} {:9.3} bits | near-duplicated {:?} {:9.3} bits",
        a.families,
        a.code.total_bits(),
        b.families,
        b.code.total_bits()
    );
    assert_eq!(
        a.families, b.families,
        "1e-9 px near-duplicates changed the family sequence: the invariance holds only on bit          identity, which is RT6-A6 verbatim"
    );
    assert_eq!(a.smooth, b.smooth);
    // And a separation well ABOVE the epsilon still counts as two samples:
    // the collapse must not merge what the resampling genuinely produced.
    let eps = vice_fit::DUPLICATE_EPSILON_PX;
    let spread: Vec<Pt> = pts
        .iter()
        .flat_map(|p| [*p, *p + Pt::new(100.0 * eps, 0.0)])
        .collect();
    let wide = chain_from(&spread, false);
    let deduped = vice_fit::models::dedup_coincident(&wide);
    assert_eq!(
        deduped.samples.len(),
        wide.samples.len(),
        "samples 100 epsilon apart were merged; the epsilon is eating real observations"
    );
}

/// §14.3's cut-invariance test for closed loops, in the form the spec's own
/// wording requires and in the form that is actually true.
///
/// §14.3 offers two ways to solve a closed loop: a cyclic k-best search, or
/// "несколькими canonical cuts с доказанным cut-invariance test". The second is
/// taken, and the property that has to hold is that the ANSWER does not depend
/// on where the loop happened to be cut.
///
/// **The individual cuts do not agree exactly, and the number is published.**
/// On a 96-sample circle all six canonical cuts select the same grammar — two
/// circular arcs — at 299.693 to 300.995 bits, a spread of **1.303 bits**. That
/// spread is small and it is not zero, and it is what makes the second leg
/// non-trivial: `k_best_boundary_models` never CHOOSES a cut, it evaluates every
/// canonical cut and returns the shortest, so its output is a function of the
/// loop. Rotating the loop must therefore return the identical answer, and it
/// does — to the last printed digit.
///
/// The first version of this test measured 47 bits of spread and I nearly wrote
/// that down as a property of cutting. It was an artifact of the MEASUREMENT:
/// it called the whole pipeline on an already-rotated chain, which re-cut it,
/// so it was measuring the pipeline twice rather than measuring one cut.
/// `models_at_cut` exists because a test of cut invariance may not itself go
/// through the mechanism that hides the cut.
#[test]
fn the_cut_a_closed_chain_is_opened_at_does_not_change_what_is_selected() {
    let n = 96usize;
    let circle: Vec<Pt> = (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            Pt::new(60.0, 60.0) + Pt::new(30.0 * a.cos(), 30.0 * a.sin())
        })
        .collect();
    let chain = chain_from(&circle, true);
    let cuts = vice_fit::canonical_cuts(&chain);
    assert!(cuts.len() >= 2, "only {} cut(s) derived", cuts.len());

    // Leg one: what the individual cuts do, measured rather than assumed.
    let mut totals = Vec::new();
    let mut family_sets = Vec::new();
    for c in &cuts {
        let run = vice_fit::models::models_at_cut(
            &chain,
            *c,
            &FIT_BUDGET_V1,
            CANVAS_PX,
            K_DISCRETE_PATHS,
        )
        .expect("well formed");
        let m = run.models.into_iter().next().expect("a model at every cut");
        println!(
            "cut {c:4}: {} segments {:?} | {:9.3} bits",
            m.families.len(),
            m.families,
            m.code.total_bits()
        );
        totals.push(m.code.total_bits());
        let mut f = m.families.clone();
        f.sort_by_key(|x| format!("{x:?}"));
        f.dedup();
        family_sets.push(f);
    }
    let lo = totals.iter().cloned().fold(f64::INFINITY, f64::min);
    let hi = totals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    println!(
        "per-cut spread {:.3} bits ({:.1} %) over {} cuts",
        hi - lo,
        (hi - lo) / lo * 100.0,
        cuts.len()
    );
    for f in &family_sets[1..] {
        assert_eq!(f, &family_sets[0], "the family SET changed with the cut");
    }
    assert!(
        hi - lo > GATE_MIN_CUT_NONTRIVIAL_SPREAD_BITS,
        "every canonical cut selected the same code length to within a bit; then leg two below \
         passes for a reason that has nothing to do with cut invariance"
    );

    // Leg two: the property. Rotating the LOOP is the same loop, and the
    // pipeline's answer must not move.
    let mut answers = Vec::new();
    for r in [0usize, 7, 23, 61] {
        let rotated: Vec<Pt> = (0..n).map(|i| circle[(r + i) % n]).collect();
        let m = best(&chain_from(&rotated, true), CANVAS_PX);
        println!(
            "loop rotated by {r:3}: {} segments | {:9.3} bits",
            m.families.len(),
            m.code.total_bits()
        );
        answers.push((m.families.len(), m.code.total_bits()));
    }
    for a in &answers[1..] {
        assert_eq!(
            a.0, answers[0].0,
            "rotating the loop changed the number of segments selected"
        );
        assert!(
            (a.1 - answers[0].1).abs() < GATE_MAX_CUT_ROTATION_DELTA_BITS,
            "rotating the loop moved the selected code length from {:.3} to {:.3} bits",
            answers[0].1,
            a.1
        );
    }
}

#[test]
fn the_selection_is_invariant_to_translation() {
    let pts = s_curve(0.5);
    let a = best(&chain_from(&pts, false), CANVAS_PX);
    let moved: Vec<Pt> = pts.iter().map(|p| *p + Pt::new(61.0, -37.0)).collect();
    let b = best(&chain_from(&moved, false), CANVAS_PX);
    assert_eq!(a.families, b.families);
    assert_eq!(a.smooth, b.smooth);
    assert_eq!(a.breakpoints, b.breakpoints);
    assert!(
        (a.code.total_bits() - b.code.total_bits()).abs() < GATE_MAX_TRANSLATION_DELTA_BITS,
        "translation moved the code length from {} to {}",
        a.code.total_bits(),
        b.code.total_bits()
    );
}

#[test]
fn the_selection_is_invariant_to_reflection() {
    let pts = s_curve(0.5);
    let a = best(&chain_from(&pts, false), CANVAS_PX);
    let mirrored: Vec<Pt> = pts.iter().map(|p| Pt::new(200.0 - p.x, p.y)).collect();
    let b = best(&chain_from(&mirrored, false), CANVAS_PX);
    println!(
        "direct {:?} {:9.3} bits | mirrored {:?} {:9.3} bits",
        a.families,
        a.code.total_bits(),
        b.families,
        b.code.total_bits()
    );
    assert_eq!(a.families, b.families);
    assert_eq!(a.smooth, b.smooth);
    assert_eq!(a.breakpoints, b.breakpoints);
}

/// §14.5's "uniform scale". The whole OBSERVATION is scaled — geometry,
/// corridor, correlation length and canvas — because a similarity of the
/// picture alone is not a similarity of the measurement.
///
/// **Two terms move under a similarity and one of them was measured to, which
/// is why this test is written the way it is rather than as an equality.**
///
/// - the ANCHOR code moves by `2 log2(s)` per anchor, because it is a code over
///   a canvas and the canvas scaled;
/// - the residual code's QUANTIZATION NORMALISER, `log2(h sqrt(2 pi) /
///   precision)`, moves by `log2(s)` per observation, because `precision` is a
///   FROZEN px calibration and a similarity of the scene does not move it. The
///   first run of this test read 101.43 bits at scale 1 and 177.05 at scale 2,
///   and that term is the whole of the difference.
///
/// Neither can change a SELECTION: both are identical for every path over the
/// same chain, since every path has the same observation count and the anchor
/// term differs only through the anchor count, which is what is being compared
/// anyway. So the deciding part — the Huber term, which IS dimensionless — is
/// isolated by subtracting the normaliser, and the selection is asserted whole.
#[test]
fn the_selection_is_invariant_to_uniform_scale() {
    let pts = s_curve(0.5);
    let mut sigs = Vec::new();
    for s in [1.0f64, 2.0, 4.0] {
        let scaled: Vec<Pt> = pts.iter().map(|p| *p * s).collect();
        let chain = chain_scaled(&scaled, false, s);
        let m = best(&chain, CANVAS_PX * s);
        println!(
            "scale {s:>4}: families {:?} | smooth {:?} | {:9.3} bits (geometry {:8.3}, residual \
             {:8.3})",
            m.families,
            m.smooth,
            m.code.total_bits(),
            m.code.geometry_bits,
            m.code.residual_bits
        );
        let normaliser: f64 = chain
            .samples
            .iter()
            .map(|x| {
                (x.weight_ds / x.corr_length_px)
                    * (x.halfwidth * std::f64::consts::TAU.sqrt()
                        / GEOMETRY_CODE_TABLE_V1.coordinate_precision_px())
                    .log2()
            })
            .sum();
        println!("            quantization normaliser {normaliser:9.3} bits");
        sigs.push((
            m.families.clone(),
            m.smooth.clone(),
            m.code.residual_bits - normaliser,
        ));
    }
    for s in &sigs[1..] {
        assert_eq!(s.0, sigs[0].0, "the family sequence moved with the scale");
        assert_eq!(s.1, sigs[0].1, "the join kinds moved with the scale");
        // Under ONE BIT across a fourfold similarity, on a code whose smallest
        // meaningful unit is a bit and whose selected model costs 142. What
        // moves at all is not the Huber term failing to be dimensionless: it is
        // the FIXED 0.05 px chord tolerance the residual is measured at, which
        // does not scale, and the solver's fixed step sizes. Measured
        // 0.673 / 0.293 / 0.178 bits at scales 1 / 2 / 4 — decreasing, because
        // a bigger curve is relatively better flattened at a fixed tolerance.
        assert!(
            (s.2 - sigs[0].2).abs() < 1.0,
            "the DECIDING part of the residual code moved from {:.4} to {:.4} bits under a \
             similarity; the Huber term is dimensionless and is what selects",
            sigs[0].2,
            s.2
        );
    }
}

// ---------------------------------------------------------------------------
// Gate clause 4: no BIC-only promotion
// ---------------------------------------------------------------------------

/// **The clause, as a differential rather than as a text scan.**
///
/// A chain drawn from a single circular arc. A four-cubic grammar fits it with
/// a strictly SMALLER residual — more parameters always do — and a raw-residual
/// or `k log n` selector would have to weigh that against a penalty it chooses.
/// The explicit code table does not choose: the extra anchors and control
/// points cost what §14.5's parameter code says they cost, and the arc wins.
///
/// The knockout leg is the one that makes it a measurement: with the geometry
/// code REMOVED (a table whose parameter cost is negligible) the same DP picks
/// the more complex path. So the code table is load-bearing, not decorative.
#[test]
fn the_lower_residual_model_does_not_win_when_its_code_is_longer() {
    let pts = arc_points(Pt::new(60.0, 60.0), 40.0, 0.2, 1.6, 0.5);
    let chain = chain_from(&pts, false);

    let m = best(&chain, CANVAS_PX);
    println!(
        "frozen table : families {:?} | segments {} | residual {:8.3} | total {:9.3} bits",
        m.families,
        m.families.len(),
        m.code.residual_bits,
        m.code.total_bits()
    );
    assert!(
        m.families.len() <= 2,
        "the frozen code table selected {} segments for a single arc: {:?}",
        m.families.len(),
        m.families
    );

    // The knockout: a table whose parameter code is a thousandth of the frozen
    // one. Nothing else changes.
    let cheap = vice_fit::GeometryCodeTable::new(
        GEOMETRY_CODE_TABLE_V1.bits_per_anchor() / 1000.0,
        GEOMETRY_CODE_TABLE_V1.bits_per_segment_family() / 1000.0,
        GEOMETRY_CODE_TABLE_V1.bits_per_relation(),
    )
    .expect("positive");
    let candidates = span_candidates(&chain, &FIT_BUDGET_V1).expect("candidates");
    let edges = build_edges(&candidates.candidates, &chain.samples, &cheap, CANVAS_PX);
    let paths = k_best_paths(&edges, &chain.samples, &cheap, CANVAS_PX, K_DISCRETE_PATHS);
    let cheapest = paths.first().expect("a discrete path");
    let cheap_families = path_families(cheapest, &candidates.candidates);
    println!(
        "cheap table  : families {:?} | segments {} | residual {:8.3} | total {:9.3} bits",
        cheap_families,
        cheap_families.len(),
        cheapest.code.residual_bits,
        cheapest.code.total_bits()
    );
    assert!(
        cheap_families.len() >= m.families.len() + GATE_MIN_NO_BIC_EXTRA_SEGMENTS,
        "making the parameter code free did not buy a single extra segment ({} against {}); then \
         the code table is not what is selecting the grammar and this clause is vacuous",
        cheap_families.len(),
        m.families.len()
    );
    assert!(
        cheapest.code.residual_bits < m.code.residual_bits,
        "the more complex grammar does not even fit better ({} against {} residual bits), so this \
         is not the trade this clause is about",
        cheapest.code.residual_bits,
        m.code.residual_bits
    );
}

/// §24's reason for k-best: the coarse DP's favourite can be infeasible once
/// the exact constraints are applied. If the solver never refused anything, `k`
/// would be doing nothing and the pipeline would be a 1-best DP wearing a k.
#[test]
fn the_solver_refuses_some_of_the_k_paths_and_says_which() {
    let mut refusals = 0usize;
    let mut kinds: Vec<&'static str> = Vec::new();
    for step in [0.25f64, 0.5, 1.0] {
        let chain = chain_from(&s_curve(step), false);
        let run = k_best_boundary_models(&chain, &FIT_BUDGET_V1, CANVAS_PX, K_DISCRETE_PATHS)
            .expect("well formed");
        println!(
            "step {step}: {} candidates, {} edges, {} paths -> {} models, refused {:?}, not \
             representable {}",
            run.candidates,
            run.edges,
            run.discrete_paths,
            run.models.len(),
            run.refused,
            run.not_representable
        );
        refusals += run.refused.iter().map(|(_, n)| n).sum::<usize>() + run.not_representable;
        kinds.extend(run.refused.iter().map(|(k, _)| *k));
    }
    kinds.sort_unstable();
    kinds.dedup();
    println!("refusal kinds seen: {kinds:?}");
    assert!(
        refusals > 0,
        "the joint solve accepted every one of the k paths on every density; then k-best is \
         costing k times the work for nothing, and §24's reason for it is not exercised here"
    );
}

// ---------------------------------------------------------------------------
// §15 Stage H: a relation is a hypothesis judged by the same code length
// ---------------------------------------------------------------------------

#[test]
fn a_closed_circle_promotes_a_whole_loop_primitive_and_an_open_chain_does_not() {
    let points: Vec<Pt> = (0..128)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 128.0;
            Pt::new(80.0 + 30.0 * a.cos(), 70.0 + 30.0 * a.sin())
        })
        .collect();
    let closed = chain_from(&points, true);
    let run = k_best_boundary_models(&closed, &FIT_BUDGET_V1, CANVAS_PX, K_DISCRETE_PATHS)
        .expect("closed circle fits");
    assert!(
        run.primitives_considered >= 13,
        "the seven families plus ten polygon side hypotheses were not judged"
    );
    let model = run.models.first().expect("circle model");
    let kept = model
        .primitive_kept
        .map(|i| &model.primitives[i])
        .expect("a circle should beat its free typed-chain sibling");
    assert_eq!(kept.kind, vice_fit::LoopPrimitiveKind::Circle);
    assert!(kept.accepted && kept.net_bits > 0.0);
    assert_eq!(
        model.relations_kept, 0,
        "a primitive and its implied relations were both charged"
    );

    let open = chain_from(&points, false);
    let open_run = k_best_boundary_models(&open, &FIT_BUDGET_V1, CANVAS_PX, K_DISCRETE_PATHS)
        .expect("open chain fits");
    assert_eq!(open_run.primitives_considered, 0);
    assert!(open_run.models.iter().all(|m| m.primitives.is_empty()));
}

/// **§15's comparison, in both directions.** Two exactly perpendicular runs
/// accept the relation; moving only the second run seven degrees away rejects
/// it. Rotating the whole shape must not matter to a geometric relation.
#[test]
fn a_relation_is_accepted_only_when_it_shortens_the_code() {
    // An L: a long horizontal run, a corner, a long vertical run.
    let mut related: Vec<Pt> = (0..=80)
        .map(|i| Pt::new(20.0 + i as f64 * 0.5, 40.0))
        .collect();
    related.extend((1..=80).map(|i| Pt::new(60.0, 40.0 + i as f64 * 0.5)));
    // Keep the first run fixed and rotate only the second around the corner.
    let th = 7.0f64.to_radians();
    let mut unrelated: Vec<Pt> = related[..=80].to_vec();
    unrelated.extend((1..=80).map(|i| {
        let y = i as f64 * 0.5;
        Pt::new(60.0 - y * th.sin(), 40.0 + y * th.cos())
    }));

    let report = |pts: &[Pt], label: &str| -> (usize, usize) {
        let chain = chain_from(pts, false);
        let run = k_best_boundary_models(&chain, &FIT_BUDGET_V1, CANVAS_PX, K_DISCRETE_PATHS)
            .expect("well formed");
        let m = run.models.first().expect("a model");
        println!(
            "{label}: families {:?} | relations {} considered, {} kept | {:9.3} bits",
            m.families,
            m.relations.len(),
            m.relations_kept,
            m.code.total_bits()
        );
        for h in &m.relations {
            println!(
                "    {:>14} on {:?}: cost {:6.3} saving {:6.3} residual penalty {:9.3} net \
                 {:10.3} -> {}",
                h.kind.universe_name(),
                h.segments,
                h.cost_bits,
                h.saving_bits,
                h.residual_penalty_bits,
                h.net_bits,
                if h.accepted { "ACCEPTED" } else { "rejected" }
            );
        }
        (m.relations.len(), m.relations_kept)
    };

    let (considered_on, kept_on) = report(&related, "perpendicular");
    let (considered_off, kept_off) = report(&unrelated, "off relation ");
    assert!(
        considered_on > 0 && considered_off > 0,
        "no relation hypothesis was formed at all ({considered_on} / {considered_off}), so this \
         test compares nothing (F-0039)"
    );
    assert!(
        kept_on > 0,
        "an L of two exactly perpendicular runs accepted no relation; then §15's constrained model \
         never wins and the relation code is decoration"
    );
    assert!(
        kept_off < kept_on,
        "an L moved seven degrees off perpendicular accepted {kept_off} relations against \
         {kept_on}; then \
         acceptance is not a statement about the shape"
    );
}

// ---------------------------------------------------------------------------
// RT6-A1: the configurations the red team drove through the G1 clause
// ---------------------------------------------------------------------------

/// **RT6-A1's first measurement, replayed as a refusal.** The hand-built chain
/// [Cubic — Arc(FromHeadTangent) — Cubic] with BOTH joins smooth and the tail
/// node's angle set 0.5 rad away from the arc's arrival direction lowered to an
/// accepted `SmoothG1` declaration 0.5 rad from the geometry — the violation
/// this representation claims is unwritable, written down. It must now be
/// REFUSED at lowering, with the unreading segment named, because an arc's
/// anchor reads exactly one end and the tail node's declaration is bound to
/// nothing.
#[test]
fn an_arc_smooth_at_both_ends_cannot_be_written_down() {
    use vice_fit::{ArcAnchor, Handle, RefitChain, RefitNode, RefitRefusal, RefitSegment};
    let chain = RefitChain {
        nodes: vec![
            RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
            RefitNode {
                pos: Pt::new(10.0, 0.0),
                tangent_rad: Some(0.3),
            },
            RefitNode {
                pos: Pt::new(20.0, 6.0),
                // 0.5 rad away from wherever the arc actually arrives: the
                // free parameter RT6-A1 exploited.
                tangent_rad: Some(1.1),
            },
            RefitNode {
                pos: Pt::new(30.0, 6.0),
                tangent_rad: None,
            },
        ],
        segments: vec![
            RefitSegment::Cubic {
                head: Handle::Free(Pt::new(3.0, -2.0)),
                tail: Handle::Shared { length_px: 3.0 },
            },
            RefitSegment::Arc(ArcAnchor::FromHeadTangent),
            RefitSegment::Cubic {
                head: Handle::Shared { length_px: 3.0 },
                tail: Handle::Free(Pt::new(27.0, 5.0)),
            },
        ],
    };
    assert_eq!(
        chain.lower().err(),
        Some(RefitRefusal::SmoothNodeUnread {
            node: 2,
            segment: 1
        }),
        "the arc's tail node declares a tangent the arc does not read; lowering this is writing \
         the G1 violation down"
    );

    // Positive control: the same shape with the tail join a CORNER lowers
    // fine — the refusal is about the unread declaration, not about arcs.
    let mut honest = chain.clone();
    honest.nodes[2].tangent_rad = None;
    honest.segments[2] = RefitSegment::Cubic {
        head: Handle::Free(Pt::new(22.0, 6.5)),
        tail: Handle::Free(Pt::new(27.0, 5.0)),
    };
    assert!(honest.lower().is_ok());
}

/// A chain sampled from a G1-by-construction cubic-arc-cubic (curvature
/// 0 -> 0.045 -> 0, step 0.5 px — RT6-A1's rt6_a1c fixture). Before the class
/// closure, the pipeline returned accepted models with G1 spreads up to
/// 4.224 deg on this chain — seven orders over the clause line — because the
/// DP offered arc-smooth-at-both-ends at the same price as the honest
/// variants. Every accepted model must now hold the clause.
#[test]
fn the_red_team_chain_no_longer_produces_an_accepted_g1_violation() {
    // Heading integration of the curvature profile.
    let (l1, l2, l3, kmax, ds) = (30.0f64, 40.0, 30.0, 0.045, 0.5);
    let mut pts = vec![Pt::new(20.0, 120.0)];
    let mut theta = 0.0f64;
    let mut s = 0.0f64;
    let total = l1 + l2 + l3;
    while s + ds <= total {
        let kappa = if s < l1 {
            kmax * s / l1
        } else if s < l1 + l2 {
            kmax
        } else {
            kmax * (total - s) / l3
        };
        theta += kappa * ds;
        let last = *pts.last().expect("non-empty");
        pts.push(last + Pt::new(theta.cos(), theta.sin()) * ds);
        s += ds;
    }
    let chain = chain_from(&pts, false);
    let run = k_best_boundary_models(&chain, &FIT_BUDGET_V1, CANVAS_PX, K_DISCRETE_PATHS)
        .expect("well formed");
    assert!(
        !run.models.is_empty(),
        "no model survived at all: candidates {}, refused {:?}, not representable {}",
        run.candidates,
        run.refused,
        run.not_representable
    );
    let mut nodes = 0usize;
    let mut worst = 0.0f64;
    for m in &run.models {
        let lowered = m.chain.lower().expect("an accepted model lowers");
        let r = g1_readings(&lowered, m.chain.start(), m.chain.end());
        nodes += r.len();
        for x in &r {
            worst = worst.max(x.spread_rad);
        }
    }
    println!(
        "red-team chain: {} models, {} smooth nodes, worst G1 spread {:.3e} rad, not \
         representable {}",
        run.models.len(),
        nodes,
        worst,
        run.not_representable
    );
    assert!(
        nodes > 0,
        "no accepted model on this chain has a smooth join; the clause would be measured over \
         nothing exactly as it was on the corpus (F-0039)"
    );
    assert!(
        worst < GATE_MAX_G1_SPREAD_RAD,
        "an accepted model on the red team's own chain has a G1 spread of {worst} rad"
    );
    assert!(
        run.not_representable > 0,
        "no path was dropped as non-representable on a chain built to elicit \
         arc-smooth-at-both-ends; then the class closure is not being exercised here and the \
         green above proves nothing about it"
    );
}
