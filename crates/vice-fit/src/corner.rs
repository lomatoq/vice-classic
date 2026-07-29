//! §14.1 corner proposals: a saliency per sample, and the anchors the span
//! schedule hangs its dyadic ladder on.
//!
//! ## "Это proposal confidence, не hard label" — and what that costs to honour
//!
//! §14.1's last sentence is the whole design constraint. A corner saliency that
//! is thresholded into "this is a corner" has decided the grammar before the DP
//! has seen any evidence, and the threshold then carries the decision. So the
//! saliency here does exactly two things, and neither of them is a decision:
//!
//! 1. it selects the samples the SCHEDULE anchors its ladder on, so that a long
//!    span ENDING at a likely corner exists to be offered — a proposal;
//! 2. it is published per sample so the corner structure a run saw is readable.
//!
//! **It does not enter the code length and it does not enter the cost.** Corner
//! versus smooth is decided by [`crate::code`] and the DP: a corner costs the
//! same join code as a smooth node and frees the tangent constraint, so it wins
//! exactly where the residual pays for it. Feeding the saliency into the
//! objective would be the arbitrary lambda §14.5 forbids ("нельзя просто
//! подкрутить произвольные lambdas на test").
//!
//! ## What the saliency is made of
//!
//! §14.1 names four ingredients. Three are computed here and the fourth is not
//! available at this stage, which is said rather than quietly dropped:
//!
//! | §14.1 ingredient | here |
//! |---|---|
//! | multiscale signed turning | [`CornerProposal::turning_rad`], over a dyadic ladder of half-widths |
//! | curvature persistence | [`CornerProposal::persistent_turning_rad`] — the MINIMUM of \|turning\| over the scales |
//! | line-intersection support | [`CornerProposal::line_support_rms_px`] and [`CornerProposal::intersection_offset_px`] |
//! | stability по topology/formation hypotheses | **NOT COMPUTED.** `BoundaryChain` is one hypothesis: `observe_boundaries` is called on one chosen evidence set, and this crate is handed the result. Stability across hypotheses needs the envelope, and the envelope is not threaded to Stage G. Limitation 63 |
//!
//! The persistence is the discriminating one and the reason is arithmetic. On a
//! circular arc of radius `R` sampled at step `d`, the turning across `±k`
//! samples is about `2kd/R`, so it VANISHES with the scale. At a corner of
//! interior turn `theta` the turning across `±k` samples is about `theta` at
//! every scale. Taking the minimum over scales therefore separates "curved" from
//! "kinked" without a curvature threshold: a curve is what reads small at the
//! finest scale, a corner is what reads large at all of them.

use serde::Serialize;
use vice_evidence::BoundarySample;
use vice_geom::Pt;

/// One sample's corner evidence. Every field is published; the last is a
/// confidence and is used only to PROPOSE.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CornerProposal {
    pub sample: usize,
    /// Signed turning across `±1` sample, radians, in `(-pi, pi]`.
    pub turning_rad: f64,
    /// `min` over the dyadic half-widths of `|turning|`, radians. The
    /// curvature-persistence signal: small on any smooth arc, large only where
    /// the turning survives coarsening.
    pub persistent_turning_rad: f64,
    /// RMS distance, in px, of the samples on the two sides from the two
    /// straight lines fitted to them. Small when the corner really is two
    /// straight pieces.
    pub line_support_rms_px: f64,
    /// Distance, in px, from this sample to the intersection of those two
    /// lines. `f64::INFINITY` when they are parallel.
    pub intersection_offset_px: f64,
    /// Proposal confidence in `[0, 1]`.
    ///
    /// `(persistent_turning / pi) * 1/(1 + (rms + offset)/h)`. Monotone in the
    /// persistence, attenuated by how badly two straight lines explain the
    /// neighbourhood, normalised by the sample's own corridor halfwidth so the
    /// attenuation is in units of what the evidence can resolve. **No
    /// threshold appears in it**, and nothing downstream compares it against
    /// one.
    pub saliency: f64,
}

/// Half-widths, in samples, at which the turning is measured.
///
/// A dyadic ladder, generated rather than listed: `1, 2, 4, …` while the window
/// fits inside the chain. Listing them would be the literal F-0048 Q1 asks
/// about, and the next finding ("what about half-width 16") would be answered
/// by appending one.
pub fn turning_scales(n_samples: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut k = 1usize;
    while 2 * k < n_samples {
        out.push(k);
        k *= 2;
    }
    out
}

/// Signed turning at `i` across `±k` samples, radians in `(-pi, pi]`.
///
/// `None` when the window leaves the chain, or when either arm has zero length
/// — a turning between a direction and nothing is not zero, it is absent
/// (F-0075).
pub fn signed_turning(samples: &[BoundarySample], i: usize, k: usize) -> Option<f64> {
    let (lo, hi) = (i.checked_sub(k)?, i + k);
    if hi >= samples.len() {
        return None;
    }
    let back = samples[i].p - samples[lo].p;
    let fwd = samples[hi].p - samples[i].p;
    if back.length_sq() <= 0.0 || fwd.length_sq() <= 0.0 {
        return None;
    }
    Some(back.cross(fwd).atan2(back.dot(fwd)))
}

pub(crate) fn cyclic_turning(samples: &[BoundarySample], i: usize, k: usize) -> Option<f64> {
    let n = samples.len();
    if n < 3 || k == 0 || 2 * k >= n {
        return None;
    }
    let back = samples[i].p - samples[(i + n - k) % n].p;
    let forward = samples[(i + k) % n].p - samples[i].p;
    if back.length_sq() <= 0.0 || forward.length_sq() <= 0.0 {
        return None;
    }
    Some(back.cross(forward).atan2(back.dot(forward)))
}

/// Least-squares line through points, as (unit direction, point on the line),
/// plus the RMS orthogonal residual. `None` for fewer than two points.
fn fit_line(pts: &[Pt]) -> Option<(Pt, Pt, f64)> {
    if pts.len() < 2 {
        return None;
    }
    let n = pts.len() as f64;
    let mut c = Pt::new(0.0, 0.0);
    for p in pts {
        c += *p;
    }
    c = c * (1.0 / n);
    // Principal direction of the scatter, closed form for 2x2.
    let (mut sxx, mut sxy, mut syy) = (0.0f64, 0.0f64, 0.0f64);
    for p in pts {
        let d = *p - c;
        sxx += d.x * d.x;
        sxy += d.x * d.y;
        syy += d.y * d.y;
    }
    let theta = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let dir = Pt::new(theta.cos(), theta.sin());
    if !dir.is_finite() {
        return None;
    }
    let normal = Pt::new(-dir.y, dir.x);
    let ss: f64 = pts.iter().map(|p| (*p - c).dot(normal).powi(2)).sum();
    Some((dir, c, (ss / n).sqrt()))
}

/// Intersection of two lines given as (direction, point). `None` when parallel.
fn line_intersection(a: (Pt, Pt), b: (Pt, Pt)) -> Option<Pt> {
    let det = a.0.cross(b.0);
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    let t = (b.1 - a.1).cross(b.0) / det;
    let p = a.1 + a.0 * t;
    p.is_finite().then_some(p)
}

/// Corner proposals for every interior sample of a chain.
///
/// One entry per sample that has a `±1` window inside the chain; the ends of an
/// open chain have no turning and are absent rather than zero.
pub fn corner_proposals(samples: &[BoundarySample]) -> Vec<CornerProposal> {
    let n = samples.len();
    let scales = turning_scales(n);
    let mut out = Vec::new();
    for i in 0..n {
        let Some(turning) = signed_turning(samples, i, 1) else {
            continue;
        };
        let mut persistent = turning.abs();
        for k in &scales {
            match signed_turning(samples, i, *k) {
                Some(t) => persistent = persistent.min(t.abs()),
                // A scale whose window leaves the chain says nothing; it must
                // not be read as "the turning vanished there", which is what
                // treating a missing value as zero would do.
                None => continue,
            }
        }

        // Two straight lines over the widest window that fits, both sides.
        let arm = scales.last().copied().unwrap_or(1);
        let lo = i.saturating_sub(arm);
        let hi = (i + arm).min(n - 1);
        let left: Vec<Pt> = samples[lo..=i].iter().map(|s| s.p).collect();
        let right: Vec<Pt> = samples[i..=hi].iter().map(|s| s.p).collect();
        let (rms, offset) = match (fit_line(&left), fit_line(&right)) {
            (Some(a), Some(b)) => {
                let rms = 0.5 * (a.2 + b.2);
                let off = line_intersection((a.0, a.1), (b.0, b.1))
                    .map_or(f64::INFINITY, |p| (p - samples[i].p).length());
                (rms, off)
            }
            _ => (f64::INFINITY, f64::INFINITY),
        };

        let h = samples[i].halfwidth;
        let attenuation = if h > 0.0 && (rms + offset).is_finite() {
            1.0 / (1.0 + (rms + offset) / h)
        } else {
            0.0
        };
        let saliency = (persistent / std::f64::consts::PI).clamp(0.0, 1.0) * attenuation;

        out.push(CornerProposal {
            sample: i,
            turning_rad: turning,
            persistent_turning_rad: persistent,
            line_support_rms_px: rms,
            intersection_offset_px: offset,
            saliency,
        });
    }
    out
}

/// Closed-loop counterpart of [`corner_proposals`].
///
/// Every turning and both line-support arms wrap across the input seam, so a
/// cyclic shift produces proposals at the same physical samples.
pub(crate) fn cyclic_corner_proposals(samples: &[BoundarySample]) -> Vec<CornerProposal> {
    let n = samples.len();
    let scales = turning_scales(n);
    let arm = scales.last().copied().unwrap_or(1);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let Some(turning) = cyclic_turning(samples, i, 1) else {
            continue;
        };
        let mut persistent = turning.abs();
        for &scale in &scales {
            if let Some(value) = cyclic_turning(samples, i, scale) {
                persistent = persistent.min(value.abs());
            }
        }
        let left: Vec<Pt> = (0..=arm)
            .map(|offset| samples[(i + n - arm + offset) % n].p)
            .collect();
        let right: Vec<Pt> = (0..=arm)
            .map(|offset| samples[(i + offset) % n].p)
            .collect();
        let (rms, offset) = match (fit_line(&left), fit_line(&right)) {
            (Some(a), Some(b)) => {
                let rms = 0.5 * (a.2 + b.2);
                let offset = line_intersection((a.0, a.1), (b.0, b.1))
                    .map_or(f64::INFINITY, |point| (point - samples[i].p).length());
                (rms, offset)
            }
            _ => (f64::INFINITY, f64::INFINITY),
        };
        let halfwidth = samples[i].halfwidth;
        let attenuation = if halfwidth > 0.0 && (rms + offset).is_finite() {
            1.0 / (1.0 + (rms + offset) / halfwidth)
        } else {
            0.0
        };
        out.push(CornerProposal {
            sample: i,
            turning_rad: turning,
            persistent_turning_rad: persistent,
            line_support_rms_px: rms,
            intersection_offset_px: offset,
            saliency: (persistent / std::f64::consts::PI).clamp(0.0, 1.0) * attenuation,
        });
    }
    out
}

/// The samples the span schedule anchors its ladder on.
///
/// A sample is an anchor iff its saliency is a STRICT maximum over the window
/// of `half_window` samples either side. That is a structural criterion, not a
/// threshold and not a top-`k`: it cannot be tuned to admit one more corner,
/// and the count it returns is bounded by `n / (half_window + 1)` by
/// construction, which is what keeps the anchored schedule's size a theorem
/// rather than a hope.
///
/// Samples with zero saliency are excluded, because a flat region has a
/// "strict maximum" only through arithmetic accident and anchoring on it would
/// spend the schedule on nothing.
pub fn corner_anchors(proposals: &[CornerProposal], half_window: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for (idx, p) in proposals.iter().enumerate() {
        // NaN is excluded here as well as zero, and deliberately: an anchor is
        // a place the schedule spends supports on, and a saliency that is not
        // a number is not a reason to spend them.
        if !p.saliency.is_finite() || p.saliency <= 0.0 {
            continue;
        }
        let lo = idx.saturating_sub(half_window);
        let hi = (idx + half_window).min(proposals.len() - 1);
        if proposals[lo..=hi]
            .iter()
            .enumerate()
            .all(|(j, q)| lo + j == idx || q.saliency < p.saliency)
        {
            out.push(p.sample);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_evidence::BoundarySample;

    fn chain(points: &[Pt]) -> Vec<BoundarySample> {
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

    fn arc(radius: f64, sweep: f64, n: usize) -> Vec<Pt> {
        (0..=n)
            .map(|i| {
                let a = sweep * i as f64 / n as f64;
                Pt::new(50.0 + radius * a.cos(), 50.0 + radius * a.sin())
            })
            .collect()
    }

    /// **The discriminating property, with both sides measured.** A right-angle
    /// corner keeps its turning at every scale; an arc of comparable total
    /// turning loses it as the scale shrinks. Without the second half this test
    /// would only say the instrument returns a number.
    #[test]
    fn persistence_separates_a_corner_from_an_arc_of_the_same_total_turn() {
        let mut pts: Vec<Pt> = (0..=24).map(|i| Pt::new(i as f64, 0.0)).collect();
        pts.extend((1..=24).map(|i| Pt::new(24.0, i as f64)));
        let corner = chain(&pts);
        let kp = corner_proposals(&corner);
        let at_corner = kp
            .iter()
            .find(|p| p.sample == 24)
            .expect("the kink is an interior sample");

        // A quarter turn spread over the same number of samples.
        let smooth = chain(&arc(30.0, std::f64::consts::FRAC_PI_2, 48));
        let worst_smooth = corner_proposals(&smooth)
            .iter()
            .map(|p| p.persistent_turning_rad)
            .fold(0.0f64, f64::max);

        println!(
            "corner persistence {:.5} rad (saliency {:.4}) | smoothest-arc worst persistence \
             {:.5} rad",
            at_corner.persistent_turning_rad, at_corner.saliency, worst_smooth
        );
        assert!(
            at_corner.persistent_turning_rad > 1.5,
            "a right-angle kink read {:.5} rad of persistent turning",
            at_corner.persistent_turning_rad
        );
        assert!(
            worst_smooth * 10.0 < at_corner.persistent_turning_rad,
            "an arc of the same total turn read {worst_smooth:.5} rad against the corner's {:.5}; \
             the persistence is not separating shape from kink",
            at_corner.persistent_turning_rad
        );
    }

    /// The line-intersection leg: at a real corner the two sides are straight
    /// and their intersection is AT the sample.
    #[test]
    fn the_two_sides_of_a_corner_are_lines_meeting_at_it() {
        let mut pts: Vec<Pt> = (0..=24).map(|i| Pt::new(i as f64, 0.0)).collect();
        pts.extend((1..=24).map(|i| Pt::new(24.0, i as f64)));
        let kp = corner_proposals(&chain(&pts));
        let at = kp.iter().find(|p| p.sample == 24).expect("interior");
        assert!(
            at.line_support_rms_px < 1e-9,
            "two straight arms fitted with RMS {} px",
            at.line_support_rms_px
        );
        assert!(
            at.intersection_offset_px < 1e-9,
            "the arms intersect {} px from the corner",
            at.intersection_offset_px
        );
        assert!(at.saliency > 0.4, "saliency {}", at.saliency);
    }

    /// Anchors are strict local maxima, so their count is bounded by the window
    /// rather than by a threshold. Asserted on a chain with four corners.
    #[test]
    fn anchors_are_local_maxima_and_their_count_is_bounded_by_the_window() {
        let mut pts = Vec::new();
        for (a, b) in [
            (Pt::new(0.0, 0.0), Pt::new(30.0, 0.0)),
            (Pt::new(30.0, 0.0), Pt::new(30.0, 30.0)),
            (Pt::new(30.0, 30.0), Pt::new(0.0, 30.0)),
            (Pt::new(0.0, 30.0), Pt::new(0.0, 0.0)),
        ] {
            for i in 0..30 {
                pts.push(a + (b - a) * (i as f64 / 30.0));
            }
        }
        let proposals = corner_proposals(&chain(&pts));
        let anchors = corner_anchors(&proposals, 3);
        println!("anchors {anchors:?} of {} samples", pts.len());
        assert!(
            anchors.len() >= 3,
            "a square outline produced {} anchors; the three interior corners are at 30, 60, 90",
            anchors.len()
        );
        assert!(
            anchors.len() <= proposals.len() / 4 + 1,
            "{} anchors over {} proposals exceeds the structural bound of one per window",
            anchors.len(),
            proposals.len()
        );
        for c in [30usize, 60, 90] {
            assert!(
                anchors.iter().any(|a| a.abs_diff(c) <= 1),
                "no anchor within one sample of the corner at {c}: {anchors:?}"
            );
        }
    }

    /// A perfectly smooth arc has corners nowhere, and the anchor rule must not
    /// invent them out of arithmetic noise at a scale of `1e-16` rad.
    #[test]
    fn a_smooth_arc_anchors_far_less_than_a_polygon() {
        let smooth = corner_proposals(&chain(&arc(40.0, 2.0, 120)));
        let poly_pts: Vec<Pt> = {
            let mut v = Vec::new();
            for (a, b) in [
                (Pt::new(0.0, 0.0), Pt::new(30.0, 0.0)),
                (Pt::new(30.0, 0.0), Pt::new(30.0, 30.0)),
                (Pt::new(30.0, 30.0), Pt::new(0.0, 30.0)),
                (Pt::new(0.0, 30.0), Pt::new(0.0, 0.0)),
            ] {
                for i in 0..30 {
                    v.push(a + (b - a) * (i as f64 / 30.0));
                }
            }
            v
        };
        let poly = corner_proposals(&chain(&poly_pts));
        let sal = |v: &[CornerProposal]| v.iter().map(|p| p.saliency).fold(0.0f64, f64::max);
        println!(
            "max saliency: smooth arc {:.6}, square outline {:.6}",
            sal(&smooth),
            sal(&poly)
        );
        assert!(
            sal(&smooth) * 20.0 < sal(&poly),
            "the smooth arc's peak saliency {:.6} is not far below the square's {:.6}",
            sal(&smooth),
            sal(&poly)
        );
    }
}
