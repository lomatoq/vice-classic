//! Local two-sided evidence-corridor feasibility.
//!
//! Every deviation stays paired with the halfwidth at the same physical
//! location. A maximum deviation and a maximum halfwidth from different
//! samples are not a corridor witness.

use vice_evidence::BoundarySample;
use vice_geom::Pt;

use crate::cost::{euclidean_deviation, normal_deviation};
use crate::refit::FEASIBLE_HALFWIDTHS;

/// The location and its OWN allowance that maximize corridor violation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CorridorReading {
    pub deviation_px: f64,
    pub allowed_px: f64,
    pub ratio: f64,
}

impl CorridorReading {
    fn invalid() -> Self {
        Self {
            deviation_px: f64::INFINITY,
            allowed_px: 0.0,
            ratio: f64::INFINITY,
        }
    }

    pub fn feasible(self) -> bool {
        self.ratio.is_finite() && self.ratio <= 1.0
    }
}

fn reading(deviation_px: f64, halfwidth_px: f64) -> CorridorReading {
    let allowed_px = FEASIBLE_HALFWIDTHS * halfwidth_px;
    CorridorReading {
        deviation_px,
        allowed_px,
        ratio: deviation_px / allowed_px,
    }
}

pub(crate) fn worse(a: CorridorReading, b: CorridorReading) -> CorridorReading {
    if b.ratio > a.ratio {
        b
    } else {
        a
    }
}

/// Every Stage-F sample is compared with the allowance calibrated for that
/// same sample.
pub(crate) fn evidence_to_model_corridor(
    poly: &[Pt],
    samples: &[BoundarySample],
) -> CorridorReading {
    if poly.len() < 2 || samples.is_empty() {
        return CorridorReading::invalid();
    }
    samples
        .iter()
        .map(|sample| {
            let deviation = normal_deviation(sample.p, sample.normal, poly)
                .map_or_else(|| euclidean_deviation(sample.p, poly), f64::abs);
            reading(deviation, sample.halfwidth)
        })
        .fold(
            CorridorReading {
                deviation_px: 0.0,
                allowed_px: FEASIBLE_HALFWIDTHS * samples[0].halfwidth,
                ratio: 0.0,
            },
            worse,
        )
}

fn point_segment_projection(point: Pt, a: Pt, b: Pt) -> (f64, f64) {
    let d = b - a;
    let length_sq = d.length_sq();
    let t = if length_sq > 0.0 && length_sq.is_finite() {
        ((point - a).dot(d) / length_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((point - (a + d * t)).length(), t)
}

fn local_reverse_reading(point: Pt, samples: &[BoundarySample], closed: bool) -> CorridorReading {
    if samples.is_empty() {
        return CorridorReading::invalid();
    }
    if samples.len() == 1 {
        return reading((point - samples[0].p).length(), samples[0].halfwidth);
    }

    let pairs = samples.len() - 1 + usize::from(closed);
    let mut nearest: Option<(f64, f64)> = None;
    for i in 0..pairs {
        let j = (i + 1) % samples.len();
        let (distance, t) = point_segment_projection(point, samples[i].p, samples[j].p);
        let halfwidth = samples[i].halfwidth + t * (samples[j].halfwidth - samples[i].halfwidth);
        match nearest {
            None => nearest = Some((distance, halfwidth)),
            Some((best_distance, best_halfwidth))
                if distance < best_distance
                    || (distance == best_distance && halfwidth < best_halfwidth) =>
            {
                nearest = Some((distance, halfwidth));
            }
            _ => {}
        }
    }
    nearest.map_or_else(CorridorReading::invalid, |(distance, halfwidth)| {
        reading(distance, halfwidth)
    })
}

/// Every delivered polyline point is compared with the halfwidth interpolated
/// at its closest projection on the ordered Stage-F evidence. A closed model
/// also checks the evidence seam.
pub(crate) fn model_to_evidence_corridor(
    poly: &[Pt],
    samples: &[BoundarySample],
) -> CorridorReading {
    if poly.len() < 2 || samples.len() < 2 {
        return CorridorReading::invalid();
    }
    let closed = poly.first() == poly.last();
    poly.iter()
        .map(|point| local_reverse_reading(*point, samples, closed))
        .fold(
            CorridorReading {
                deviation_px: 0.0,
                allowed_px: FEASIBLE_HALFWIDTHS * samples[0].halfwidth,
                ratio: 0.0,
            },
            worse,
        )
}
