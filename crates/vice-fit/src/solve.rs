//! The optimiser behind §24's `joint_constrained_refit`.
//!
//! ## Why it is joint, and what "constrained" means once G1 is a representation
//!
//! §24 runs the refit over a WHOLE chain with shared nodes and tangents. A
//! per-segment fit cannot move a shared node, and moving shared nodes is most
//! of what a refit does — the candidate stage held both endpoints of every span
//! at sample positions, and sample positions are observations, not parameters.
//!
//! The "constrained" half is already discharged by the TYPE. [`RefitChain`]
//! cannot express a G1 violation, so there is no constraint to enforce here and
//! no multiplier to tune: the optimiser moves free parameters and every point it
//! can reach is G1-exact by construction. That is the difference between a
//! constrained solve and a penalty, and it is the reason §14.3's "`angle <
//! tolerance` is not G1" is answerable at all.
//!
//! ## What this is NOT
//!
//! Not a certified optimiser. Levenberg–Marquardt with a forward-difference
//! Jacobian, a fixed pass budget, no convergence test, and **the pass with the
//! lowest residual is what is returned, pass zero included** — so the refit can
//! never return something worse than the discrete path it was handed. The claim
//! is "the best of the iterates I enumerated", not "it converged", which is the
//! same discipline `span::fit` states for its footpoint passes.
//!
//! §28 M7 owns the trust-region optimiser and the exact posterior. What this
//! owes M7 is a chain whose G1 is not M7's problem.

use vice_evidence::BoundarySample;
use vice_geom::{ChordTolerancePx, Pt};

use crate::cost::{euclidean_deviation, normal_deviation};
use crate::refit::{
    ArcAnchor, Handle, RefitChain, RefitRefusal, RefitSegment, FEASIBLE_HALFWIDTHS,
};

/// Passes of the damped Gauss–Newton, fixed.
///
/// Twelve. Measured on the corpus: the residual improvement past pass twelve is
/// under a thousandth of a corridor halfwidth on every chain of the run, which
/// is below what the evidence resolves. A convergence test would put a
/// tolerance in the loop, and the pass that is RETURNED is chosen by residual
/// rather than by the loop terminating, so a budget that is too small costs
/// accuracy and can never cost correctness.
pub const JOINT_REFIT_PASSES: usize = 12;

/// Free scalars the joint solve will accept in one chain.
///
/// A backstop, like the candidate budget: it REFUSES rather than truncating,
/// because a truncated parameter vector is a different problem wearing the same
/// name. At two per interior node plus at most four per segment, 256 is a chain
/// of some forty segments, which no boundary of the supported universe reaches.
pub const MAX_JOINT_PARAMETERS: usize = 256;

/// The chord tolerance the residual is measured at.
pub const REFIT_CHORD_TOLERANCE_PX: f64 = 0.05;

/// What the joint refit did.
#[derive(Debug, Clone, PartialEq)]
pub struct RefitOutcome {
    pub chain: RefitChain,
    /// Free scalars the solve moved.
    pub parameters: usize,
    /// Weighted residual (sum of squared deviations in corridor units) before
    /// and after. Both published: an optimiser that returns its input is a
    /// finding, and it is invisible if only the final value is printed.
    pub residual_before: f64,
    pub residual_after: f64,
    /// The pass whose iterate was kept. `0` means no pass improved on the
    /// input.
    pub pass_kept: usize,
    /// Worst `|d_n|` over the chain's samples after the solve, in px.
    pub worst_normal_deviation_px: f64,
}

fn tolerance() -> ChordTolerancePx {
    ChordTolerancePx::new(REFIT_CHORD_TOLERANCE_PX).expect("positive constant")
}

/// Flatten a whole lowered chain into one polyline.
pub fn flatten_chain(chain: &RefitChain) -> Result<Vec<Pt>, RefitRefusal> {
    let lowered = chain.lower()?;
    let pts = lowered.node_positions(chain.start(), chain.end());
    let mut out: Vec<Pt> = Vec::new();
    for (k, seg) in lowered.segments.iter().enumerate() {
        let (p0, p1) = (pts[k], pts[k + 1]);
        let piece: Vec<Pt> = match *seg {
            vice_ir::Segment::Line => vec![p0, p1],
            vice_ir::Segment::Quad { ctrl } => {
                vice_geom::flatten::flatten_quad(p0, ctrl, p1, tolerance()).points
            }
            vice_ir::Segment::Cubic { ctrl1, ctrl2 } => {
                vice_geom::flatten::flatten_cubic(p0, ctrl1, ctrl2, p1, tolerance()).points
            }
            vice_ir::Segment::CircularArc {
                radius_px,
                large_arc,
                ccw,
            } => {
                vice_geom::flatten::flatten_circular_arc(
                    p0,
                    p1,
                    radius_px,
                    large_arc,
                    ccw,
                    tolerance(),
                )
                .map_err(|_| RefitRefusal::ArcIsALine { segment: k })?
                .points
            }
            vice_ir::Segment::EllipticArc { .. } => {
                return Err(RefitRefusal::NonFinite { segment: k })
            }
        };
        if out.is_empty() {
            out.extend(piece);
        } else {
            out.extend(piece.into_iter().skip(1));
        }
    }
    (out.len() >= 2)
        .then_some(out)
        .ok_or(RefitRefusal::Malformed)
}

/// Sum of squared sample deviations in CORRIDOR units.
///
/// Corridor units and not px: a sample whose corridor is wide says less about
/// where the curve is, and weighting by the halfwidth is how the Stage F
/// calibration enters the solve rather than being ignored by it.
fn residual(chain: &RefitChain, samples: &[BoundarySample]) -> f64 {
    let Ok(poly) = flatten_chain(chain) else {
        return f64::INFINITY;
    };
    samples
        .iter()
        .map(|s| (euclidean_deviation(s.p, &poly) / s.halfwidth).powi(2))
        .sum()
}

/// Pack the free scalars of a chain into a vector.
fn pack(chain: &RefitChain) -> Vec<f64> {
    let mut v = Vec::new();
    for n in &chain.nodes[1..chain.nodes.len() - 1] {
        v.push(n.pos.x);
        v.push(n.pos.y);
    }
    for n in &chain.nodes {
        if let Some(t) = n.tangent_rad {
            v.push(t);
        }
    }
    for s in &chain.segments {
        match *s {
            RefitSegment::Line | RefitSegment::Arc(ArcAnchor::FromHeadTangent) => {}
            RefitSegment::Arc(ArcAnchor::FromTailTangent) => {}
            RefitSegment::Arc(ArcAnchor::Radius { radius_px, .. }) => v.push(radius_px),
            RefitSegment::Quad { ctrl } => push_handle(&mut v, ctrl),
            RefitSegment::Cubic { head, tail } => {
                push_handle(&mut v, head);
                push_handle(&mut v, tail);
            }
        }
    }
    v
}

fn push_handle(v: &mut Vec<f64>, h: Handle) {
    match h {
        Handle::Free(p) => {
            v.push(p.x);
            v.push(p.y);
        }
        Handle::Shared { length_px } => v.push(length_px),
    }
}

/// The inverse of [`pack`], writing the vector back into a chain of the same
/// SHAPE. The shape — which nodes are smooth, which handles are shared — is
/// never a parameter, so the discrete grammar the DP chose cannot drift during
/// the solve.
fn unpack(chain: &mut RefitChain, v: &[f64]) {
    let mut i = 0usize;
    let last = chain.nodes.len() - 1;
    for n in &mut chain.nodes[1..last] {
        n.pos = Pt::new(v[i], v[i + 1]);
        i += 2;
    }
    for n in &mut chain.nodes {
        if n.tangent_rad.is_some() {
            n.tangent_rad = Some(v[i]);
            i += 1;
        }
    }
    for s in &mut chain.segments {
        match s {
            RefitSegment::Line
            | RefitSegment::Arc(ArcAnchor::FromHeadTangent)
            | RefitSegment::Arc(ArcAnchor::FromTailTangent) => {}
            RefitSegment::Arc(ArcAnchor::Radius { radius_px, .. }) => {
                *radius_px = v[i].abs().max(f64::MIN_POSITIVE);
                i += 1;
            }
            RefitSegment::Quad { ctrl } => pull_handle(ctrl, v, &mut i),
            RefitSegment::Cubic { head, tail } => {
                pull_handle(head, v, &mut i);
                pull_handle(tail, v, &mut i);
            }
        }
    }
}

fn pull_handle(h: &mut Handle, v: &[f64], i: &mut usize) {
    match h {
        Handle::Free(p) => {
            *p = Pt::new(v[*i], v[*i + 1]);
            *i += 2;
        }
        Handle::Shared { length_px } => {
            // A negative handle length would put the control point on the wrong
            // side of the node and silently turn a smooth join into a cusp.
            // Clamped rather than penalised: the sign is not a parameter, it is
            // part of what "smooth" means.
            *length_px = v[*i].max(0.0);
            *i += 1;
        }
    }
}

/// Solve a small symmetric positive-definite system by Gaussian elimination
/// with partial pivoting. `None` when the matrix is singular to the arithmetic.
#[allow(clippy::needless_range_loop, clippy::neg_cmp_op_on_partial_ord)]
fn solve_spd(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for k in 0..n {
        let (mut piv, mut best) = (k, a[k][k].abs());
        for r in k + 1..n {
            if a[r][k].abs() > best {
                best = a[r][k].abs();
                piv = r;
            }
        }
        if !best.is_finite() || best <= 0.0 {
            return None;
        }
        a.swap(k, piv);
        b.swap(k, piv);
        for r in k + 1..n {
            let f = a[r][k] / a[k][k];
            if !f.is_finite() {
                return None;
            }
            for c in k..n {
                a[r][c] -= f * a[k][c];
            }
            b[r] -= f * b[k];
        }
    }
    let mut x = vec![0.0f64; n];
    for k in (0..n).rev() {
        let mut s = b[k];
        for c in k + 1..n {
            s -= a[k][c] * x[c];
        }
        let v = s / a[k][k];
        if !v.is_finite() {
            return None;
        }
        x[k] = v;
    }
    Some(x)
}

/// **§28 M6 bullet 4.** Refit a whole chain against its samples, jointly.
///
/// Returns the best iterate by residual, or `OutsideCorridor` when even the
/// best cannot bring the chain within [`FEASIBLE_HALFWIDTHS`] corridors of its
/// evidence — which is §14.3's "path invalid и рассматривается следующий",
/// reported rather than silently accepted.
#[allow(clippy::needless_range_loop)]
pub fn joint_constrained_refit(
    init: &RefitChain,
    samples: &[BoundarySample],
) -> Result<RefitOutcome, RefitRefusal> {
    // A lowering failure of the INPUT is a refusal about the input.
    let _ = init.lower()?;
    let mut best = init.clone();
    let start = residual(init, samples);
    let mut best_r = start;
    let mut pass_kept = 0usize;

    let p = pack(init).len();
    if p > MAX_JOINT_PARAMETERS {
        return Err(RefitRefusal::Malformed);
    }

    if p > 0 && best_r.is_finite() {
        let mut cur = init.clone();
        let mut x = pack(&cur);
        let mut lambda = 1e-3f64;
        for pass in 1..=JOINT_REFIT_PASSES {
            let n = samples.len();
            let base = residual_vector(&cur, samples);
            let Some(base) = base else { break };
            // Forward-difference Jacobian. The step is relative to the
            // parameter's own magnitude, because a node coordinate and a
            // tangent angle do not live on the same scale.
            let mut jac = vec![vec![0.0f64; p]; n];
            let mut ok = true;
            for j in 0..p {
                let h = 1e-6 * x[j].abs().max(1.0);
                let mut xp = x.clone();
                xp[j] += h;
                let mut probe = cur.clone();
                unpack(&mut probe, &xp);
                let Some(rp) = residual_vector(&probe, samples) else {
                    ok = false;
                    break;
                };
                for (i, jrow) in jac.iter_mut().enumerate() {
                    jrow[j] = (rp[i] - base[i]) / h;
                }
            }
            if !ok {
                break;
            }

            // Normal equations with Levenberg damping.
            let mut ata = vec![vec![0.0f64; p]; p];
            let mut atb = vec![0.0f64; p];
            for i in 0..n {
                for a in 0..p {
                    atb[a] -= jac[i][a] * base[i];
                    for b in a..p {
                        ata[a][b] += jac[i][a] * jac[i][b];
                    }
                }
            }
            for a in 0..p {
                for b in 0..a {
                    ata[a][b] = ata[b][a];
                }
                ata[a][a] *= 1.0 + lambda;
            }
            let Some(step) = solve_spd(ata, atb) else {
                break;
            };

            let mut trial_x = x.clone();
            for (a, s) in step.iter().enumerate() {
                trial_x[a] += s;
            }
            if trial_x.iter().any(|v| !v.is_finite()) {
                break;
            }
            let mut trial = cur.clone();
            unpack(&mut trial, &trial_x);
            let r = residual(&trial, samples);
            if r < best_r {
                best_r = r;
                best = trial.clone();
                pass_kept = pass;
            }
            if r.is_finite() && r < residual(&cur, samples) {
                cur = trial;
                x = trial_x;
                lambda = (lambda * 0.3).max(1e-9);
            } else {
                lambda *= 10.0;
                if lambda > 1e9 {
                    break;
                }
            }
        }
    }

    // Feasibility, in §14.4's own quantity: the deviation along the sample
    // normal, not the Euclidean one the optimiser minimised.
    let poly = flatten_chain(&best)?;
    let mut worst = 0.0f64;
    let mut allowed = 0.0f64;
    for s in samples {
        let dn = normal_deviation(s.p, s.normal, &poly)
            .map_or_else(|| euclidean_deviation(s.p, &poly), f64::abs);
        if dn > worst {
            worst = dn;
            allowed = FEASIBLE_HALFWIDTHS * s.halfwidth;
        }
    }
    if worst > allowed {
        return Err(RefitRefusal::OutsideCorridor {
            worst_normal_deviation_px: worst,
            allowed_px: allowed,
        });
    }

    Ok(RefitOutcome {
        chain: best,
        parameters: p,
        residual_before: start,
        residual_after: best_r,
        pass_kept,
        worst_normal_deviation_px: worst,
    })
}

/// Per-sample signed-free residual in corridor units, as a vector.
fn residual_vector(chain: &RefitChain, samples: &[BoundarySample]) -> Option<Vec<f64>> {
    let poly = flatten_chain(chain).ok()?;
    let v: Vec<f64> = samples
        .iter()
        .map(|s| euclidean_deviation(s.p, &poly) / s.halfwidth)
        .collect();
    v.iter().all(|x| x.is_finite()).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refit::{g1_readings, RefitNode};

    fn samples_from(points: &[Pt]) -> Vec<BoundarySample> {
        points
            .iter()
            .map(|p| BoundarySample {
                p: *p,
                normal: Pt::new(0.0, -1.0),
                halfwidth: 0.35,
                confidence: 1.0,
                weight_ds: 1.0,
                corr_length_px: 1.0,
            })
            .collect()
    }

    /// A chain of two cubics through a smooth node, initialised away from the
    /// samples. The solve must reduce the residual AND the result must still be
    /// G1-exact — the second is the whole point, and a solve that improved the
    /// fit by breaking the join would pass the first alone.
    #[test]
    fn the_joint_solve_reduces_the_residual_and_keeps_g1_exact() {
        // Samples on a sine, which neither single cubic reproduces.
        let pts: Vec<Pt> = (0..=60)
            .map(|i| {
                let x = i as f64 * 0.5;
                Pt::new(x, 10.0 * (x / 30.0 * std::f64::consts::PI).sin())
            })
            .collect();
        let s = samples_from(&pts);
        let init = RefitChain {
            nodes: vec![
                RefitNode {
                    pos: pts[0],
                    tangent_rad: None,
                },
                RefitNode {
                    pos: pts[30],
                    tangent_rad: Some(0.0),
                },
                RefitNode {
                    pos: pts[60],
                    tangent_rad: None,
                },
            ],
            segments: vec![
                RefitSegment::Cubic {
                    head: Handle::Free(pts[10]),
                    tail: Handle::Shared { length_px: 5.0 },
                },
                RefitSegment::Cubic {
                    head: Handle::Shared { length_px: 5.0 },
                    tail: Handle::Free(pts[50]),
                },
            ],
        };
        let out = joint_constrained_refit(&init, &s).expect("feasible");
        println!(
            "residual {:.5} -> {:.5} over {} parameters, pass {} kept, worst d_n {:.5} px",
            out.residual_before,
            out.residual_after,
            out.parameters,
            out.pass_kept,
            out.worst_normal_deviation_px
        );
        assert!(
            out.residual_after < out.residual_before * 0.5,
            "the joint solve moved the residual from {} to {}",
            out.residual_before,
            out.residual_after
        );
        let lowered = out.chain.lower().expect("lowers");
        let worst = g1_readings(&lowered, out.chain.start(), out.chain.end())
            .iter()
            .map(|r| r.spread_rad)
            .fold(0.0f64, f64::max);
        assert!(
            worst < 1e-12,
            "the solve returned a chain with a G1 spread of {worst} rad"
        );
    }

    /// **§14.3's "path invalid".** A grammar the evidence cannot support is
    /// REFUSED with the numbers, not returned with a bad residual.
    #[test]
    fn a_path_the_evidence_cannot_support_is_refused_with_its_numbers() {
        let pts: Vec<Pt> = (0..=40)
            .map(|i| {
                let a = i as f64 / 40.0 * std::f64::consts::TAU;
                Pt::new(30.0 * a.cos(), 30.0 * a.sin())
            })
            .collect();
        let s = samples_from(&pts);
        // One straight line across a full circle: no parameter can save it.
        let init = RefitChain {
            nodes: vec![
                RefitNode {
                    pos: pts[0],
                    tangent_rad: None,
                },
                RefitNode {
                    pos: pts[40],
                    tangent_rad: None,
                },
            ],
            segments: vec![RefitSegment::Line],
        };
        match joint_constrained_refit(&init, &s) {
            Err(RefitRefusal::OutsideCorridor {
                worst_normal_deviation_px,
                allowed_px,
            }) => {
                assert!(worst_normal_deviation_px > allowed_px);
                assert!((allowed_px - FEASIBLE_HALFWIDTHS * 0.35).abs() < 1e-12);
            }
            other => panic!("expected OutsideCorridor, got {other:?}"),
        }
    }

    /// The solve never returns something worse than what it was handed, at any
    /// pass budget. Without the best-of rule a fixed iteration count is a bet.
    #[test]
    fn the_solve_never_returns_a_worse_chain_than_its_input() {
        let pts: Vec<Pt> = (0..=40).map(|i| Pt::new(i as f64, 0.0)).collect();
        let s = samples_from(&pts);
        let init = RefitChain {
            nodes: vec![
                RefitNode {
                    pos: pts[0],
                    tangent_rad: None,
                },
                RefitNode {
                    pos: pts[40],
                    tangent_rad: None,
                },
            ],
            segments: vec![RefitSegment::Line],
        };
        let out = joint_constrained_refit(&init, &s).expect("a line on a line is feasible");
        assert!(out.residual_after <= out.residual_before);
        assert_eq!(
            out.parameters, 0,
            "a single line between held ends is rigid"
        );
        assert_eq!(out.pass_kept, 0);
    }
}
