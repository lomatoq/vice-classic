//! Span candidates: one typed primitive fitted to one interval of a chain,
//! and the §14.4 proposal cost that orders them.
//!
//! ## These are PROPOSALS, and the word is load-bearing
//!
//! §14.4 says the boundary integral is used "для candidate ordering" and is
//! explicitly not added to the final pixel posterior. Nothing in this module
//! decides geometry. The final parameters come from the joint constrained
//! refit (§14.3, §28 M6 bullet 4, **NOT STARTED**), which is why the fits here
//! are least squares in the family's own linear parameters rather than
//! certified optimisations.
//!
//! The one place that is not closed form is the Bezier PARAMETERISATION, and
//! the honest account of it is in [`PARAMETER_REFINEMENT_PASSES`]: a fixed
//! number of footpoint passes, keeping the best of them by geometric residual,
//! leaving a floor of about 0.56 px on a chain drawn from an exact cubic.
//! **That floor is measured and asserted rather than argued away**, it is a
//! property of the candidate stage and not of the family, and its owner is
//! bullet 4. This paragraph said "a fit that would need an iteration is
//! REFUSED rather than approximated" until the iteration was added — a
//! sentence in a doc comment does not survive the code changing under it
//! (F-0015), so it is corrected here rather than left to read well.
//!
//! ## The family set is a literal, and that is said here rather than below
//!
//! F-0048 Q1 asks whether the mechanism contains a literal enumerating its
//! subjects. [`FITTED_FAMILIES`] is one. The cheapest bypass is exact and
//! costs nothing to state: **one family for which nobody wrote a fitter.**
//! Two things narrow it, neither closes it:
//!
//! - the set is checked against the DECLARED universe from the `vice-bench`
//!   side, where both are visible, so an admissible family with no fitter and
//!   no declared exception reddens rather than passing in silence;
//! - [`ChainCandidates::families_present`] is MEASURED per chain rather than
//!   declared, so a family that stops producing candidates shows up as an
//!   absent name instead of as nothing at all.
//!
//! That is the same mitigation M5's edit shapes ended at, at the same honest
//! strength: visible, not closed.

use serde::Serialize;
use vice_evidence::BoundarySample;
use vice_geom::{ChordTolerancePx, Pt};
use vice_ir::Segment;

use crate::schedule::Support;

/// The families this crate fits.
///
/// §14.2 lists line, circular arc, quad, cubic, optional biarc, and
/// "ellipse/clothoid только после targeted evidence". The elliptic arc is
/// admissible in the declared universe and is deliberately NOT fitted here:
/// see [`FAMILIES_DELIBERATELY_NOT_FITTED`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SpanFamily {
    Line,
    CircularArc,
    Quad,
    Cubic,
}

/// The literal. Named as a literal in the module docs, with its bypass price.
pub const FITTED_FAMILIES: [SpanFamily; 4] = [
    SpanFamily::Line,
    SpanFamily::CircularArc,
    SpanFamily::Quad,
    SpanFamily::Cubic,
];

/// Families admissible in the declared model universe that this crate does
/// NOT fit, each with the reason — a list of DECISIONS, not a list of
/// oversights, and the default on the other side is "must have a fitter".
///
/// The names are the universe's own strings, so the `vice-bench` judge
/// compares like with like.
pub const FAMILIES_DELIBERATELY_NOT_FITTED: [(&str, &str); 1] = [(
    "elliptic_arc",
    "spec 14.2: ellipse/clothoid only after TARGETED EVIDENCE. A five-parameter family fitted \
     speculatively on every support at every scale would win on residual against the four-\
     parameter cubic wherever the boundary is not an ellipse, and the MDL code length that would \
     charge it for those parameters is spec 28 M6 bullet 5 and is NOT STARTED. Owner: the \
     milestone that delivers the code table",
)];

impl SpanFamily {
    /// The name this family carries in the declared model universe
    /// (`vice_bench::universe::ir_segment_family`).
    ///
    /// Exhaustive on purpose: a new variant that nobody named stops
    /// compiling, which is the compiler acting as the judge rather than a
    /// reviewer noticing.
    pub fn universe_name(self) -> &'static str {
        match self {
            SpanFamily::Line => "line",
            SpanFamily::CircularArc => "circular_arc",
            SpanFamily::Quad => "quadratic_bezier",
            SpanFamily::Cubic => "cubic_bezier",
        }
    }

    /// Free parameters the fit solves for, with both endpoints held.
    ///
    /// This is the quantity §14.5's parameter code is charged on
    /// (`log2(range / calibrated precision)` per parameter). It is published
    /// here because the DP will need it and because a family's cost in bits is
    /// a property of the family rather than of a table someone maintains
    /// beside it. **No code length is computed anywhere in this crate**: §28
    /// M6 bullet 5 is NOT STARTED and `[geometry_code_table]` is still a
    /// placeholder that nothing may gate on.
    pub fn free_parameters(self) -> usize {
        match self {
            SpanFamily::Line => 0,
            SpanFamily::CircularArc => 1,
            SpanFamily::Quad => 2,
            SpanFamily::Cubic => 4,
        }
    }
}

/// One fitted candidate: which samples it spans, what it is, and what it
/// costs to propose.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpanCandidate {
    pub support: Support,
    pub family: SpanFamily,
    pub segment: Segment,
    /// §14.4's `C_proposal = integral rho(d_n(s)/h(s)) ds`, in px (the loss is
    /// dimensionless, `ds` is arclength), and its parts.
    ///
    /// `d_n` is the deviation along the sample NORMAL, which is what §14.4
    /// writes. Until C302 this crate integrated the EUCLIDEAN deviation, a
    /// lower bound that is tight where a candidate is good and loose where it
    /// is bad — STATUS_M6 limitation 58. `crate::cost` carries the correction
    /// and the ratio that measures what it was worth.
    pub cost: crate::cost::ProposalCost,
}

impl SpanCandidate {
    /// §14.4's integral, in px.
    pub fn proposal_cost_px(&self) -> f64 {
        self.cost.cost_px
    }

    /// The largest `|d_n|` over the support, in px — the deviation a
    /// feasibility test against the corridor must use.
    pub fn max_deviation_px(&self) -> f64 {
        self.cost.max_normal_deviation_px
    }
}

/// Why a family produced no candidate on a support.
///
/// Counted rather than swallowed: a family that silently stops fitting looks
/// exactly like a family nobody asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum NoFit {
    /// The support's samples are collinear to within the arithmetic, so the
    /// circle fit has no centre on the bisector. The line family covers it.
    ArcIsDegenerate,
    /// The cubic normal equations are singular on this support (the
    /// parameterisation collapsed). Refused rather than pseudo-inverted: a
    /// pseudo-inverse would return a plausible curve chosen by the solver
    /// rather than by the data.
    CubicIsSingular,
    /// A fitted control point or radius came out non-finite.
    NonFiniteFit,
}

/// The chord tolerance the deviation measurement flattens candidates at.
///
/// A tenth of a pixel: an order below the `median halfwidth <= 0.35 px`
/// §13.1 targets for a clean corridor, so the flattening error does not
/// materially enter a deviation that is compared against a halfwidth. The
/// flattener certifies its own bound, and [`flattening_error_px`] republishes
/// it per candidate so this sentence is checkable rather than believed.
pub const DEVIATION_CHORD_TOLERANCE_PX: f64 = 0.1;

fn tolerance() -> ChordTolerancePx {
    ChordTolerancePx::new(DEVIATION_CHORD_TOLERANCE_PX).expect("positive constant")
}

/// The certified upper bound on how far the flattened polyline used for the
/// deviation measurement can be from the true curve.
pub fn flattening_error_px(segment: &Segment, p0: Pt, p1: Pt) -> f64 {
    flatten(segment, p0, p1).map_or(0.0, |f| f.1)
}

/// Flatten a segment to a polyline plus the flattener's certified deviation
/// bound. `None` for families this crate does not fit.
pub(crate) fn flatten(segment: &Segment, p0: Pt, p1: Pt) -> Option<(Vec<Pt>, f64)> {
    match *segment {
        Segment::Line => Some((vec![p0, p1], 0.0)),
        Segment::Quad { ctrl } => {
            let f = vice_geom::flatten::flatten_quad(p0, ctrl, p1, tolerance());
            Some((f.points, f.max_deviation_px))
        }
        Segment::Cubic { ctrl1, ctrl2 } => {
            let f = vice_geom::flatten::flatten_cubic(p0, ctrl1, ctrl2, p1, tolerance());
            Some((f.points, f.max_deviation_px))
        }
        Segment::CircularArc {
            radius_px,
            large_arc,
            ccw,
        } => {
            let f = vice_geom::flatten::flatten_circular_arc(
                p0,
                p1,
                radius_px,
                large_arc,
                ccw,
                tolerance(),
            );
            f.map(|f| (f.points, f.max_deviation_px)).ok()
        }
        Segment::EllipticArc { .. } => None,
    }
}

/// Chord-length parameterisation of a support: `t` in `[0, 1]`, one per
/// sample.
///
/// Chord length rather than sample index, because the samples are already
/// resampled by PHYSICAL arclength (`weight_ds`) and an index parameterisation
/// would silently re-weight the fit by sampling density — the resampling
/// dependence §14.4 says the cost must not have.
fn chord_parameters(samples: &[BoundarySample], support: Support) -> Vec<f64> {
    let pts = &samples[support.lo()..=support.hi()];
    let mut acc = Vec::with_capacity(pts.len());
    let mut total = 0.0f64;
    acc.push(0.0);
    for w in pts.windows(2) {
        total += (w[1].p - w[0].p).length();
        acc.push(total);
    }
    if total <= 0.0 {
        // Every sample at one point: fall back to a uniform parameterisation
        // so the caller's fit is still defined. The candidate it produces will
        // be judged by its deviation like any other.
        let n = pts.len().max(2) - 1;
        return (0..pts.len()).map(|i| i as f64 / n as f64).collect();
    }
    acc.iter().map(|s| s / total).collect()
}

/// How many times the Bezier parameterisation is re-estimated from the curve
/// it produced.
///
/// The closed-form least squares below solves for control points GIVEN a
/// parameter per sample, and that parameter is not known: chord length is an
/// estimate of it. Measured, the estimate alone leaves about 2 px on a chain
/// sampled from an exact cubic — a curve the family reproduces perfectly and
/// therefore should score at zero. A candidate stage whose ordering is wrong
/// by 2 px on the family's own shape is not ordering candidates.
///
/// So the parameter is re-estimated by projecting each sample onto the fitted
/// curve, and the fit repeated. Four passes, fixed, with no convergence test
/// and no tolerance: this is not an optimiser and does not claim to be one.
/// **The result kept is the pass with the lowest geometric residual, pass zero
/// included**, so refinement can never return something worse than the
/// chord-length fit. That construction is what makes a bounded iteration safe
/// to state — the claim is "the best of five fits I enumerated", not "it
/// converged".
///
/// The exact fit is the joint constrained refit of §14.3 over a whole chain
/// with shared nodes and tangents, which is §28 M6 bullet 4 and is NOT STARTED.
pub const PARAMETER_REFINEMENT_PASSES: usize = 8;

/// Fit one family to one support.
///
/// A family that cannot be fitted returns `Err(NoFit)` rather than a plausible
/// curve chosen by the solver.
pub fn fit(
    samples: &[BoundarySample],
    support: Support,
    family: SpanFamily,
) -> Result<Segment, NoFit> {
    let p0 = samples[support.lo()].p;
    let p1 = samples[support.hi()].p;
    let q: Vec<Pt> = samples[support.lo()..=support.hi()]
        .iter()
        .map(|s| s.p)
        .collect();

    let seg = match family {
        SpanFamily::Line => Segment::Line,
        SpanFamily::CircularArc => fit_circular_arc(&q, p0, p1)?,
        SpanFamily::Quad | SpanFamily::Cubic => {
            let mut best = solve_bezier(family, &chord_parameters(samples, support), &q, p0, p1)?;
            let mut best_residual = geometric_residual(&q, &best, p0, p1);
            for _ in 0..PARAMETER_REFINEMENT_PASSES {
                let Some(t) = reproject(&q, &best, p0, p1) else {
                    break;
                };
                let Ok(cand) = solve_bezier(family, &t, &q, p0, p1) else {
                    break;
                };
                let r = geometric_residual(&q, &cand, p0, p1);
                if r < best_residual {
                    best_residual = r;
                    best = cand;
                }
            }
            best
        }
    };

    if !segment_is_finite(&seg) {
        return Err(NoFit::NonFiniteFit);
    }
    Ok(seg)
}

/// The closed-form least-squares control points for a GIVEN parameterisation.
fn solve_bezier(family: SpanFamily, t: &[f64], q: &[Pt], p0: Pt, p1: Pt) -> Result<Segment, NoFit> {
    match family {
        SpanFamily::Quad => {
            // B(t) = (1-t)^2 p0 + 2t(1-t) c + t^2 p1. Linear in c, so the
            // least-squares control point is a ratio of two sums.
            let (mut num, mut den) = (Pt::new(0.0, 0.0), 0.0f64);
            for (ti, qi) in t.iter().zip(q.iter()) {
                let w = 2.0 * ti * (1.0 - ti);
                let r = *qi - p0 * ((1.0 - ti) * (1.0 - ti)) - p1 * (ti * ti);
                num += r * w;
                den += w * w;
            }
            if !(den.is_finite() && den > 0.0) {
                return Err(NoFit::CubicIsSingular);
            }
            Ok(Segment::Quad {
                ctrl: num * (1.0 / den),
            })
        }
        SpanFamily::Cubic => {
            // B(t) = (1-t)^3 p0 + 3t(1-t)^2 c1 + 3t^2(1-t) c2 + t^3 p1.
            // Linear in (c1, c2); the 2x2 normal equations are scalar because
            // the basis weights are scalar.
            let (mut saa, mut sab, mut sbb) = (0.0f64, 0.0f64, 0.0f64);
            let (mut sar, mut sbr) = (Pt::new(0.0, 0.0), Pt::new(0.0, 0.0));
            for (ti, qi) in t.iter().zip(q.iter()) {
                let u = 1.0 - ti;
                let a = 3.0 * ti * u * u;
                let b = 3.0 * ti * ti * u;
                let r = *qi - p0 * (u * u * u) - p1 * (ti * ti * ti);
                saa += a * a;
                sab += a * b;
                sbb += b * b;
                sar += r * a;
                sbr += r * b;
            }
            let det = saa * sbb - sab * sab;
            // Scale-aware singularity test: the determinant of a 2x2 Gram
            // matrix is bounded by the product of its diagonal, so comparing
            // against that product is a CONDITION test rather than an absolute
            // epsilon on a quantity whose magnitude depends on the support
            // length (F-0014: the limit I called f64 was my formula's
            // conditioning).
            if !(det.is_finite() && det > 1e-12 * saa * sbb) {
                return Err(NoFit::CubicIsSingular);
            }
            let c1 = (sar * sbb - sbr * sab) * (1.0 / det);
            let c2 = (sbr * saa - sar * sab) * (1.0 / det);
            Ok(Segment::Cubic {
                ctrl1: c1,
                ctrl2: c2,
            })
        }
        SpanFamily::Line | SpanFamily::CircularArc => Err(NoFit::NonFiniteFit),
    }
}

/// Sum of squared distances from the samples to the curve, on the flattened
/// polyline.
///
/// GEOMETRIC, not algebraic: it is the quantity refinement reduces, and
/// comparing two fits by their own parameterisations would compare them on two
/// different rulers.
fn geometric_residual(q: &[Pt], segment: &Segment, p0: Pt, p1: Pt) -> f64 {
    let Some((poly, _)) = flatten(segment, p0, p1) else {
        return f64::INFINITY;
    };
    q.iter()
        .map(|qi| {
            let d = crate::cost::euclidean_deviation(*qi, &poly);
            d * d
        })
        .sum()
}

/// Re-estimate each sample's curve parameter by projecting it onto the fitted
/// curve.
///
/// `flatten_quad` and `flatten_cubic` evaluate at UNIFORM `t` (`i / n`), so an
/// index into the flattened polyline IS a parameter, with no arclength
/// integral in between. That is read off their implementations rather than
/// assumed — and if they ever flatten adaptively, this returns a wrong
/// parameter and the best-of-passes rule in [`fit`] falls back to pass zero
/// rather than returning something worse.
fn reproject(q: &[Pt], segment: &Segment, p0: Pt, p1: Pt) -> Option<Vec<f64>> {
    let (poly, _) = flatten(segment, p0, p1)?;
    let n = poly.len().checked_sub(1)?;
    if n == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(q.len());
    for qi in q {
        let mut best = f64::INFINITY;
        let mut best_t = 0.0f64;
        for (k, w) in poly.windows(2).enumerate() {
            let (a, b) = (w[0], w[1]);
            let ab = b - a;
            let len_sq = ab.length_sq();
            let s = if len_sq <= 0.0 {
                0.0
            } else {
                ((*qi - a).dot(ab) / len_sq).clamp(0.0, 1.0)
            };
            let d = (a + ab * s - *qi).length();
            if d < best {
                best = d;
                best_t = (k as f64 + s) / n as f64;
            }
        }
        if !best_t.is_finite() {
            return None;
        }
        out.push(best_t);
    }
    Some(out)
}

fn segment_is_finite(s: &Segment) -> bool {
    match *s {
        Segment::Line => true,
        Segment::Quad { ctrl } => ctrl.is_finite(),
        Segment::Cubic { ctrl1, ctrl2 } => ctrl1.is_finite() && ctrl2.is_finite(),
        Segment::CircularArc { radius_px, .. } => radius_px.is_finite() && radius_px > 0.0,
        Segment::EllipticArc { .. } => false,
    }
}

/// The circle through both endpoints that best fits the interior samples.
///
/// Closed form, and the reason it is closed form is worth stating: the centre
/// of a circle through `p0` and `p1` lies on their perpendicular bisector, so
/// it has ONE free parameter `s`. Writing the algebraic residual
/// `|q - c|^2 - r^2` with `c = m + s u` and `r^2 = h^2 + s^2` cancels `s^2`
/// exactly and leaves a residual LINEAR in `s`. No iteration, no tolerance,
/// no starting guess.
///
/// This is the algebraic (Kasa) residual rather than the geometric one, which
/// is a real difference: it weights samples far from the circle more heavily.
/// At proposal quality that is acceptable and is the standard trade; what
/// would not be acceptable is calling the result a geometric best fit, so it
/// is not called one.
fn fit_circular_arc(q: &[Pt], p0: Pt, p1: Pt) -> Result<Segment, NoFit> {
    let chord = p1 - p0;
    let chord_len = chord.length();
    if !(chord_len.is_finite() && chord_len > 0.0) {
        return Err(NoFit::ArcIsDegenerate);
    }
    let m = p0 + chord * 0.5;
    let h = chord_len / 2.0;
    // Unit perpendicular to the chord.
    let u = Pt::new(-chord.y / chord_len, chord.x / chord_len);

    let (mut num, mut den) = (0.0f64, 0.0f64);
    for qi in q {
        let d = *qi - m;
        let a = d.length_sq() - h * h;
        let b = d.dot(u);
        num += a * b;
        den += b * b;
    }
    // Collinear to within the arithmetic: every sample sits on the chord, so
    // there is no bisector offset the data prefers. The line family covers it,
    // and inventing a huge radius here would produce a candidate that is a
    // line wearing an arc's parameter count.
    if !(den.is_finite() && den > 0.0 && num.is_finite()) {
        return Err(NoFit::ArcIsDegenerate);
    }
    let s = num / (2.0 * den);
    if !s.is_finite() {
        return Err(NoFit::NonFiniteFit);
    }
    let centre = m + u * s;
    let radius = (h * h + s * s).sqrt();
    if !radius.is_finite() || radius <= 0.0 {
        return Err(NoFit::ArcIsDegenerate);
    }

    // Which of the two arcs between p0 and p1 the samples actually take.
    // Decided by a sample in the interior rather than by a sign convention, so
    // the answer is a property of the data.
    let mid = q[q.len() / 2];
    let ang = |p: Pt| (p.y - centre.y).atan2(p.x - centre.x);
    let two_pi = std::f64::consts::TAU;
    let norm = |a: f64| {
        let mut a = a % two_pi;
        if a < 0.0 {
            a += two_pi;
        }
        a
    };
    let sweep_ccw = norm(ang(p1) - ang(p0));
    let mid_ccw = norm(ang(mid) - ang(p0));
    let ccw = mid_ccw <= sweep_ccw;
    let swept = if ccw { sweep_ccw } else { two_pi - sweep_ccw };

    Ok(Segment::CircularArc {
        radius_px: radius,
        large_arc: swept > std::f64::consts::PI,
        ccw,
    })
}
