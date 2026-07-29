//! §14.4's proposal integral, measured along the sample NORMAL.
//!
//! ## What this module replaces, and why the replacement is not an improvement
//! ## but a correction
//!
//! §14.4 writes
//!
//! ```text
//! C_proposal = integral rho(d_n(s) / h(s)) ds
//! ```
//!
//! and `d_n` is the deviation ALONG THE NORMAL. The first version of this crate
//! used the EUCLIDEAN distance from the sample to the candidate, which is the
//! minimum over all directions and is therefore a LOWER BOUND on `d_n`. STATUS_M6
//! limitation 58 recorded how loose: on the corpus the angle between the
//! closest-point direction and the sample normal reached **90.000 degrees at a
//! deviation of 1.503 px** — four times the clean-corridor median halfwidth, so
//! not a sample sitting on the curve and not rounding. A lower bound that is
//! tight where a candidate is good and loose where it is bad **flatters bad
//! candidates**, which is the one direction of error a candidate ORDERING cannot
//! absorb.
//!
//! So the quantity is now the spec's. [`normal_deviation`] intersects the
//! sample's normal LINE with the flattened candidate and takes the crossing
//! nearest the sample. There is no search window and no cap: the criterion is
//! "does the normal line meet this candidate at all", which is answered by the
//! candidate's own extent rather than by a number someone chose.
//!
//! ## What happens when the normal line misses
//!
//! §14.4's integrand at such a sample is not merely large, it does not exist:
//! the corridor at that sample says nothing about where that curve is. The
//! answer here is a REFUSAL of the candidate ([`CostRefusal::NormalLineMisses`]),
//! not a saturation value, because a saturation value is a number chosen to let
//! an inadmissible candidate keep competing. A candidate that some sample of its
//! own support cannot see along its own normal is not a candidate for that
//! support.
//!
//! That refusal is COUNTED per family and per chain rather than swallowed
//! (`ChainCandidates::no_costs`), because "this family never wins here" and
//! "this family is being thrown away by the cost stage" are different facts.

use serde::Serialize;
use vice_evidence::BoundarySample;
use vice_geom::Pt;
use vice_ir::Segment;

use crate::schedule::Support;
use crate::span::flatten;

/// The §14.4 integral over one support, plus the quantities that say what it
/// is made of.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ProposalCost {
    /// `integral rho(d_n / h) ds`, dimensionless loss times px of arclength.
    pub cost_px: f64,
    /// Largest `|d_n|` over the support's samples, in px. **This is the
    /// spec's deviation**; it is the one a feasibility test against the
    /// corridor must use.
    pub max_normal_deviation_px: f64,
    /// Largest Euclidean distance from a sample to the flattened candidate, in
    /// px. Kept because it is what a FLATTENER's certificate bounds, and
    /// because publishing both is what makes the next field a measurement.
    pub max_euclidean_deviation_px: f64,
    /// The largest `|d_n| / d_euclidean` over the samples where the Euclidean
    /// deviation is at least [`MATERIAL_DEVIATION_FRACTION`] of the corridor
    /// halfwidth — or `None` when NO sample of the support was material.
    ///
    /// **This is the size of the error the previous cost carried**, now
    /// measured on the same population rather than bounded by an argument. It
    /// is `>= 1` by construction, and a value near 1 means the two agree.
    ///
    /// An `Option`, and the history is RT6-A5: these were two `f64`s
    /// initialised to `1.0` and `0.0` — the LOWER boundary of the range and a
    /// legal deviation value — so a support whose every sample had a true
    /// ratio of 2.0 but sat below the material threshold published "1.0 at
    /// 0.0 px", indistinguishable from perfect agreement. F-0085 (a value at
    /// the exact boundary of the instrument's range is a saturation reading)
    /// applied to the OTHER end of the range, and F-0075 (an absence written
    /// as a value that elsewhere means zero) in the same two fields. Absence
    /// is now unwritable as a number.
    pub worst_ratio: Option<RatioReading>,
    /// How many samples of the support were material — the population the
    /// ratio was measured over, published so `None` above reads as "measured
    /// over nothing" rather than as silence.
    pub material_samples: usize,
}

/// One measured worst ratio: the value and the Euclidean deviation at which it
/// occurred. The second is what makes the first interpretable: a ratio of 40
/// at 0.001 px and at 1.5 px are the same number and different findings
/// (F-0085).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RatioReading {
    pub ratio: f64,
    pub at_deviation_px: f64,
}

/// Why a candidate has no §14.4 cost.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "cost_refusal", rename_all = "snake_case")]
pub enum CostRefusal {
    /// A malformed observation was supplied directly to this public low-level
    /// entry point.
    Input { refusal: crate::FitRefusal },
    /// The support names samples outside the supplied slice.
    SupportOutOfRange {
        lo: usize,
        hi: usize,
        samples: usize,
    },
    /// The sample's normal line does not meet the candidate anywhere, so
    /// §14.4's integrand at that sample does not exist. The index is into the
    /// chain, not into the support.
    NormalLineMisses { sample: usize },
    /// The family is one this crate does not flatten, or the flattening
    /// produced fewer than two points.
    NotFlattenable,
    /// A non-finite intermediate. Refused rather than skipped: a sample
    /// dropped from an integral changes the integral.
    NonFinite { sample: usize },
}

/// The deviation, as a fraction of the corridor halfwidth, below which a
/// sample is not asked how its normal deviation compares with its Euclidean
/// one.
///
/// A quarter, and it now guards a REPORTED RATIO rather than the cost itself —
/// which is the difference from the previous design, where the same constant
/// was load-bearing for the instrument that measured the approximation. The
/// cost integrates every sample of the support with no threshold at all.
///
/// What it hides: a large normal-to-Euclidean ratio at a sample whose
/// deviation is under a quarter of its corridor. Under the Huber loss such a
/// sample contributes at most `0.0625 * ds` against a knee of `1.0`, and the
/// ratio there is dominated by the direction of a closest point that is one ulp
/// away — the saturation F-0085 records.
pub const MATERIAL_DEVIATION_FRACTION: f64 = 0.25;

/// The robust loss of §14.4. Huber with unit knee: the argument is already
/// normalised by the corridor halfwidth, so "one" means "one corridor", and a
/// sample a corridor away is where quadratic growth should stop.
pub fn rho(u: f64) -> f64 {
    let a = u.abs();
    if a <= 1.0 {
        a * a
    } else {
        2.0 * a - 1.0
    }
}

/// Signed deviation of `p` from `poly` along the unit direction `n`: the `t`
/// of smallest magnitude with `p + t n` on the polyline.
///
/// `None` when the normal LINE (both directions, unbounded) meets no segment of
/// the polyline. No window and no cap — the extent of the search is the extent
/// of the candidate.
///
/// The determinant test is against exact zero rather than an epsilon: a
/// polyline segment exactly parallel to the normal contributes no crossing, and
/// a segment nearly parallel to it contributes a crossing far away, which is a
/// true statement about that candidate rather than a numerical accident to be
/// filtered. An epsilon here would be a threshold deciding which candidates are
/// visible.
pub fn normal_deviation(p: Pt, n: Pt, poly: &[Pt]) -> Option<f64> {
    let mut best: Option<f64> = None;
    for w in poly.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = b - a;
        let det = n.cross(d);
        if det == 0.0 || !det.is_finite() {
            continue;
        }
        let m = p - a;
        let s = n.cross(m) / det;
        if !(0.0..=1.0).contains(&s) {
            continue;
        }
        let t = d.cross(m) / det;
        if !t.is_finite() {
            continue;
        }
        if best.is_none_or(|x: f64| t.abs() < x.abs()) {
            best = Some(t);
        }
    }
    best
}

/// Euclidean distance from a point to a polyline.
pub fn euclidean_deviation(p: Pt, poly: &[Pt]) -> f64 {
    let mut best = f64::INFINITY;
    for w in poly.windows(2) {
        let (a, b) = (w[0], w[1]);
        let ab = b - a;
        let len_sq = ab.length_sq();
        let t = if len_sq <= 0.0 {
            0.0
        } else {
            ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0)
        };
        best = best.min((a + ab * t - p).length());
    }
    best
}

/// §14.4's proposal integral over one support.
pub fn proposal_cost(
    samples: &[BoundarySample],
    support: Support,
    segment: &Segment,
) -> Result<ProposalCost, CostRefusal> {
    crate::validate_samples(samples).map_err(|refusal| CostRefusal::Input { refusal })?;
    if support.hi() >= samples.len() {
        return Err(CostRefusal::SupportOutOfRange {
            lo: support.lo(),
            hi: support.hi(),
            samples: samples.len(),
        });
    }
    let p0 = samples[support.lo()].p;
    let p1 = samples[support.hi()].p;
    let Some((poly, _bound)) = flatten(segment, p0, p1) else {
        return Err(CostRefusal::NotFlattenable);
    };
    if poly.len() < 2 {
        return Err(CostRefusal::NotFlattenable);
    }

    let mut cost = 0.0f64;
    let mut max_dn = 0.0f64;
    let mut max_de = 0.0f64;
    let mut worst_ratio: Option<RatioReading> = None;
    let mut material_samples = 0usize;

    for (i, s) in samples
        .iter()
        .enumerate()
        .take(support.hi() + 1)
        .skip(support.lo())
    {
        // A closed loop is opened by repeating its seam with zero physical
        // arclength. It constrains the endpoint position but is not another
        // likelihood observation and has no unique corner normal.
        if s.weight_ds == 0.0 {
            continue;
        }
        let Some(dn) = normal_deviation(s.p, s.normal, &poly) else {
            return Err(CostRefusal::NormalLineMisses { sample: i });
        };
        let de = euclidean_deviation(s.p, &poly);
        if !dn.is_finite() || !de.is_finite() {
            return Err(CostRefusal::NonFinite { sample: i });
        }
        cost += rho(dn / s.halfwidth) * s.weight_ds;
        max_dn = max_dn.max(dn.abs());
        max_de = max_de.max(de);
        if de >= MATERIAL_DEVIATION_FRACTION * s.halfwidth && de > 0.0 {
            material_samples += 1;
            let ratio = dn.abs() / de;
            if worst_ratio.is_none_or(|w| ratio > w.ratio) {
                worst_ratio = Some(RatioReading {
                    ratio,
                    at_deviation_px: de,
                });
            }
        }
    }
    if !cost.is_finite() {
        return Err(CostRefusal::NonFinite {
            sample: support.lo(),
        });
    }

    Ok(ProposalCost {
        cost_px: cost,
        max_normal_deviation_px: max_dn,
        max_euclidean_deviation_px: max_de,
        worst_ratio,
        material_samples,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poly(pts: &[(f64, f64)]) -> Vec<Pt> {
        pts.iter().map(|&(x, y)| Pt::new(x, y)).collect()
    }

    /// The normal deviation of a point above a horizontal polyline is its
    /// height, with the sign of the normal — and the Euclidean distance agrees
    /// only because the normal is perpendicular to the polyline here.
    #[test]
    fn a_normal_meeting_the_curve_head_on_agrees_with_the_euclidean_distance() {
        let p = poly(&[(0.0, 0.0), (10.0, 0.0)]);
        let s = Pt::new(5.0, 2.0);
        let n = Pt::new(0.0, -1.0);
        let dn = normal_deviation(s, n, &p).expect("the normal crosses");
        assert!((dn - 2.0).abs() < 1e-12, "dn = {dn}");
        assert!((euclidean_deviation(s, &p) - 2.0).abs() < 1e-12);
    }

    /// **The finding limitation 58 recorded, as a unit fact.** Where the normal
    /// is oblique to the curve the two quantities differ by `1 / cos(theta)`,
    /// and the Euclidean one is the smaller. At 60 degrees the ratio is 2.
    #[test]
    fn an_oblique_normal_reads_larger_than_the_euclidean_distance() {
        let p = poly(&[(-100.0, 0.0), (100.0, 0.0)]);
        let s = Pt::new(0.0, 2.0);
        let theta = std::f64::consts::FRAC_PI_3;
        let n = Pt::new(theta.sin(), -theta.cos());
        let dn = normal_deviation(s, n, &p).expect("the normal crosses");
        let de = euclidean_deviation(s, &p);
        assert!((de - 2.0).abs() < 1e-12, "de = {de}");
        assert!(
            (dn - 4.0).abs() < 1e-9,
            "dn = {dn}, expected 2 / cos(60 deg) = 4"
        );
        assert!(dn > de, "the normal deviation must dominate the Euclidean");
    }

    /// A normal line that meets nothing returns `None` rather than a large
    /// number. The polyline is entirely on one side and parallel to the normal.
    #[test]
    fn a_normal_that_meets_nothing_is_a_miss_not_a_big_number() {
        let p = poly(&[(0.0, 0.0), (0.0, 10.0)]);
        let s = Pt::new(5.0, 20.0);
        let n = Pt::new(0.0, -1.0);
        assert_eq!(normal_deviation(s, n, &p), None);
        // Positive control on the same polyline: a sample whose normal DOES
        // meet it reads a crossing, so `None` is a fact about the geometry and
        // not about the function always failing.
        assert!(normal_deviation(Pt::new(5.0, 5.0), Pt::new(-1.0, 0.0), &p).is_some());
    }

    /// **RT6-A5's probe, kept as the regression test.** A support whose every
    /// sample has a TRUE normal-to-Euclidean ratio of 2.0 (normals at 60
    /// degrees to the curve) but whose deviations all sit below the material
    /// threshold used to publish `1.0 at 0.0 px` — the lower boundary of the
    /// instrument's range, indistinguishable from measured perfect agreement.
    /// It must now publish ABSENCE, with the population that produced it.
    #[test]
    fn a_support_with_no_material_sample_reports_absence_not_perfect_agreement() {
        use vice_evidence::BoundarySample;
        use vice_ir::Segment;

        let theta = std::f64::consts::FRAC_PI_3;
        // Endpoints on the chord, interior samples offset: the Line candidate
        // runs between the two endpoints, so the interior deviation IS `dev`.
        let make = |dev: f64| -> Vec<BoundarySample> {
            (0..8)
                .map(|i| BoundarySample {
                    p: Pt::new(2.0 * i as f64, if i == 0 || i == 7 { 0.0 } else { dev }),
                    normal: Pt::new(theta.sin(), -theta.cos()),
                    halfwidth: 0.35,
                    confidence: 1.0,
                    weight_ds: 2.0,
                    corr_length_px: 1.0,
                })
                .collect()
        };
        let support = crate::schedule::Support::new(0, 7).expect("support");

        // Every deviation at a QUARTER of the material threshold: true ratio
        // 2.0 everywhere, measured nowhere.
        let below = make(0.25 * MATERIAL_DEVIATION_FRACTION * 0.35);
        let c = proposal_cost(&below, support, &Segment::Line).expect("line flattens");
        assert_eq!(
            c.worst_ratio, None,
            "no sample was material; a number here is the saturation reading F-0085 names"
        );
        assert_eq!(c.material_samples, 0);

        // The positive leg: one material deviation, and the reading appears
        // with the true ratio and the deviation it occurred at.
        let above = make(2.0 * MATERIAL_DEVIATION_FRACTION * 0.35);
        let c2 = proposal_cost(&above, support, &Segment::Line).expect("line flattens");
        let r = c2.worst_ratio.expect("material samples exist");
        assert!(c2.material_samples > 0);
        assert!(
            (r.ratio - 2.0).abs() < 1e-9,
            "normals at 60 degrees have d_n / d_e = 2, measured {}",
            r.ratio
        );
        assert!(r.at_deviation_px > 0.0);
    }

    /// The crossing NEAREST the sample is the one taken, on a polyline the
    /// normal line meets twice.
    #[test]
    fn the_nearest_crossing_is_the_one_taken() {
        let p = poly(&[(-1.0, 3.0), (1.0, 3.0), (1.0, -7.0), (-1.0, -7.0)]);
        let s = Pt::new(0.0, 0.0);
        let n = Pt::new(0.0, 1.0);
        let dn = normal_deviation(s, n, &p).expect("crosses twice");
        assert!((dn.abs() - 3.0).abs() < 1e-12, "dn = {dn}, expected |3|");
    }
}
