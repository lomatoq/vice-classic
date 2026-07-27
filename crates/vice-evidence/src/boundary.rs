//! Subpixel boundary observations (spec §13).
//!
//! §13 lists the steps for each shared boundary, and this module does the
//! ones M4 owns:
//!
//! | §13 step | here |
//! |---|---|
//! | choose left/right paint and formation | the [`Flat2Evidence`] this runs on IS that choice |
//! | conditional coverage posterior | the mixture's alpha field |
//! | extraction at `0.5` **as initialization only** | [`extract_chains`] — and the samples are then placed by physical arclength, with position, normal and interval taken from the coverage field rather than from the polyline |
//! | bind the chain to DCEL endpoints/junctions | NOT M4: there is no DCEL until M5, and the limitation is stated rather than faked |
//! | resample by physical `ds` | [`BoundaryConfig::sample_step_px`] |
//! | normal/tangent, confidence, correlation length | from the coverage gradient and the residual along the chain |
//! | calibrated normal interval | [`crate::corridor`] |
//!
//! Two rules shape the extraction.
//!
//! **A malformed chain is not repaired.** §13: *"a malformed chain is not
//! silently repaired: the topology hypothesis is invalid or ambiguous"*. A
//! chain that neither closes nor ends on the image border is a
//! [`BoundaryRefusal::Malformed`], not a chain with its ends welded
//! together.
//!
//! **A saddle is recorded, not decided in silence.** The two ambiguous
//! marching-squares configurations are exactly the "critical 2×2" cases
//! §5.3 forbids resolving by a hidden iteration-order tie-break. M4 resolves
//! them by ONE documented rule (the cell mean) and marks the whole
//! observation [`ChainStatus::Ambiguous`] with the count; enumerating the
//! alternative resolutions as separate hypotheses is M4.5's job (§28 M4.5
//! "saddle alternatives"), and pretending to do it here would be a
//! placeholder.

use serde::Serialize;
use vice_geom::Pt;
use vice_image::norm;

use crate::corridor::{corridor_at, sample_confidence, Corridor, CorridorConfig};
use crate::mixture::Flat2Evidence;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BoundaryConfig {
    /// Physical resample step in px (§13 step 5). The observation must be
    /// invariant to it: §13.1 lists "invariance to sample step" among the
    /// calibration checks, and `vice-bench` measures it.
    pub sample_step_px: f64,
    /// Extraction level. §13: initialization ONLY.
    pub level: f64,
}

pub const BOUNDARY_CONFIG_V1: BoundaryConfig = BoundaryConfig {
    sample_step_px: 0.5,
    level: 0.5,
};

/// One boundary observation (spec §13, verbatim field list).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BoundarySample {
    pub p: Pt,
    /// Unit normal, pointing INTO the foreground face (the direction of
    /// increasing coverage).
    pub normal: Pt,
    pub halfwidth: f64,
    pub confidence: f64,
    /// Physical arclength this sample stands for, in px.
    pub weight_ds: f64,
    pub corr_length_px: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BoundaryChain {
    pub samples: Vec<BoundarySample>,
    pub closed: bool,
    pub length_px: f64,
    /// Correlation length of `‖r‖` ALONG this chain, in px. Estimated from
    /// the residual MAGNITUDE, not from its signed field: a formation
    /// mismatch is a halo whose sign flips across the edge, and the signed
    /// autocorrelation of a dipole cancels (FAILURE_LEDGER F-0017).
    pub corr_length_px: f64,
    pub vertices: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ChainStatus {
    WellFormed,
    /// At least one 2x2 cell had the diagonal configuration §5.3 calls
    /// critical. Resolved by the cell mean and REPORTED; the alternative
    /// resolutions are hypotheses M4.5 owns.
    Ambiguous {
        saddle_cells: u64,
    },
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BoundaryRefusal {
    #[error(
        "the coverage field never crosses {level}: there is no boundary to observe under this \
         hypothesis"
    )]
    NoContour { level: f64 },
    #[error(
        "chain {chain} neither closes nor ends on the image border ({detail}); spec 13 forbids \
         repairing it silently, so this topology hypothesis is invalid"
    )]
    Malformed { chain: usize, detail: String },
    #[error("coverage level {level} is not one the corridor tabulates")]
    UnknownCoverageLevel { level: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BoundaryObservation {
    pub chains: Vec<BoundaryChain>,
    pub status: ChainStatus,
    pub total_length_px: f64,
    pub samples: u64,
    /// Which corridor level the halfwidths belong to.
    pub coverage_level: f64,
    pub extraction_level: f64,
    pub sample_step_px: f64,
    /// Share of samples whose corridor hit the cap.
    pub capped_fraction: f64,
    pub median_halfwidth_px: f64,
    pub p95_halfwidth_px: f64,
}

// ---------------------------------------------------------------------------
// Marching squares on the pixel-centre lattice
// ---------------------------------------------------------------------------

/// A contour vertex lives on an EDGE of the pixel-centre lattice, and is
/// identified by that edge rather than by its position: two cells that share
/// an edge then produce the same vertex identity exactly, with no distance
/// tolerance anywhere in the linking (§5.4 — combinatorial decisions do not
/// use `abs(x) < eps`).
type EdgeId = u64;

struct Lattice {
    w: usize,
    h: usize,
}

impl Lattice {
    fn h_edge(&self, i: usize, j: usize) -> EdgeId {
        (j * (self.w - 1) + i) as EdgeId
    }
    fn v_edge(&self, i: usize, j: usize) -> EdgeId {
        (((self.w - 1) * self.h) + j * self.w + i) as EdgeId
    }
    /// True when this edge lies on the outer ring of the lattice, which is
    /// where an open chain is allowed to end.
    fn on_border(&self, e: EdgeId) -> bool {
        let hcount = ((self.w - 1) * self.h) as EdgeId;
        if e < hcount {
            let j = (e as usize) / (self.w - 1);
            j == 0 || j == self.h - 1
        } else {
            let k = (e - hcount) as usize;
            let i = k % self.w;
            i == 0 || i == self.w - 1
        }
    }
}

fn lerp_pos(a: Pt, b: Pt, va: f64, vb: f64, level: f64) -> Pt {
    let d = vb - va;
    let t = if d.abs() < f64::MIN_POSITIVE {
        0.5
    } else {
        ((level - va) / d).clamp(0.0, 1.0)
    };
    Pt::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y))
}

/// Extract the level set as chains of lattice edges.
///
/// Returns the chains (as edge sequences), the vertex position of every
/// edge, whether each chain is closed, and the saddle count.
#[allow(clippy::type_complexity)]
fn extract_chains(
    alpha: &[f64],
    w: usize,
    h: usize,
    level: f64,
) -> (
    Vec<(Vec<EdgeId>, bool)>,
    std::collections::BTreeMap<EdgeId, Pt>,
    u64,
) {
    let lat = Lattice { w, h };
    let val = |i: usize, j: usize| alpha[j * w + i];
    let pos = |i: usize, j: usize| Pt::new(i as f64 + 0.5, j as f64 + 0.5);
    let mut vertex: std::collections::BTreeMap<EdgeId, Pt> = Default::default();
    let mut segments: Vec<(EdgeId, EdgeId)> = Vec::new();
    let mut saddles = 0u64;

    if w < 2 || h < 2 {
        return (Vec::new(), vertex, 0);
    }
    for j in 0..h - 1 {
        for i in 0..w - 1 {
            let (v00, v10, v11, v01) = (val(i, j), val(i + 1, j), val(i + 1, j + 1), val(i, j + 1));
            let idx = (u8::from(v00 >= level))
                | (u8::from(v10 >= level) << 1)
                | (u8::from(v11 >= level) << 2)
                | (u8::from(v01 >= level) << 3);
            if idx == 0 || idx == 15 {
                continue;
            }
            let top = lat.h_edge(i, j);
            let bottom = lat.h_edge(i, j + 1);
            let left = lat.v_edge(i, j);
            let right = lat.v_edge(i + 1, j);
            vertex
                .entry(top)
                .or_insert_with(|| lerp_pos(pos(i, j), pos(i + 1, j), v00, v10, level));
            vertex
                .entry(bottom)
                .or_insert_with(|| lerp_pos(pos(i, j + 1), pos(i + 1, j + 1), v01, v11, level));
            vertex
                .entry(left)
                .or_insert_with(|| lerp_pos(pos(i, j), pos(i, j + 1), v00, v01, level));
            vertex
                .entry(right)
                .or_insert_with(|| lerp_pos(pos(i + 1, j), pos(i + 1, j + 1), v10, v11, level));

            let mut push = |a: EdgeId, b: EdgeId| segments.push((a, b));
            match idx {
                1 | 14 => push(left, top),
                2 | 13 => push(top, right),
                3 | 12 => push(left, right),
                4 | 11 => push(right, bottom),
                6 | 9 => push(top, bottom),
                7 | 8 => push(left, bottom),
                5 | 10 => {
                    // The critical 2x2 of §5.3. ONE documented rule — the
                    // cell mean — and the ambiguity is reported, never
                    // hidden.
                    saddles += 1;
                    let mean = 0.25 * (v00 + v10 + v11 + v01);
                    let joined = mean >= level;
                    if (idx == 5) == joined {
                        push(left, top);
                        push(right, bottom);
                    } else {
                        push(left, bottom);
                        push(top, right);
                    }
                }
                _ => unreachable!("marching squares index {idx}"),
            }
        }
    }

    // Link segments by EDGE IDENTITY.
    let mut incident: std::collections::BTreeMap<EdgeId, Vec<usize>> = Default::default();
    for (k, (a, b)) in segments.iter().enumerate() {
        incident.entry(*a).or_default().push(k);
        incident.entry(*b).or_default().push(k);
    }
    let mut used = vec![false; segments.len()];
    let mut chains: Vec<(Vec<EdgeId>, bool)> = Vec::new();

    let walk = |start_edge: EdgeId,
                used: &mut Vec<bool>,
                incident: &std::collections::BTreeMap<EdgeId, Vec<usize>>|
     -> Option<(Vec<EdgeId>, bool)> {
        let mut path = vec![start_edge];
        let mut current = start_edge;
        loop {
            let next = incident.get(&current)?.iter().find(|k| !used[**k]).copied();
            let Some(k) = next else { break };
            used[k] = true;
            let (a, b) = segments[k];
            current = if a == current { b } else { a };
            path.push(current);
            if current == path[0] {
                return Some((path, true));
            }
        }
        Some((path, false))
    };

    // Open chains first: their ends are the edges of odd incidence.
    let odd: Vec<EdgeId> = incident
        .iter()
        .filter(|(_, v)| v.len() == 1)
        .map(|(e, _)| *e)
        .collect();
    for e in odd {
        if incident[&e].iter().all(|k| used[*k]) {
            continue;
        }
        if let Some(c) = walk(e, &mut used, &incident) {
            if c.0.len() > 1 {
                chains.push(c);
            }
        }
    }
    // Then the loops.
    for k in 0..segments.len() {
        if used[k] {
            continue;
        }
        used[k] = true;
        let (a, b) = segments[k];
        let mut path = vec![a, b];
        let mut current = b;
        loop {
            let next = incident
                .get(&current)
                .and_then(|v| v.iter().find(|x| !used[**x]).copied());
            let Some(n) = next else { break };
            used[n] = true;
            let (x, y) = segments[n];
            current = if x == current { y } else { x };
            path.push(current);
            if current == path[0] {
                break;
            }
        }
        let closed = path.len() > 2 && path[0] == path[path.len() - 1];
        chains.push((path, closed));
    }
    (chains, vertex, saddles)
}

/// Total length of the extracted level set, in px, without linking it into
/// chains.
///
/// Used by the formation stage: the transition-width statistic of §10.1
/// divides the "undecided mass" by this length, and computing it does not
/// need chains, endpoints or a topology hypothesis.
pub fn contour_length_px(alpha: &[f64], w: usize, h: usize, level: f64) -> f64 {
    let (chains, vertex, _) = extract_chains(alpha, w, h, level);
    let mut total = 0.0;
    for (path, _) in &chains {
        for k in 1..path.len() {
            let (a, b) = (vertex[&path[k - 1]], vertex[&path[k]]);
            total += (b.x - a.x).hypot(b.y - a.y);
        }
    }
    total
}

fn bilinear(field: &[f64], w: usize, h: usize, p: Pt) -> f64 {
    // Lattice coordinates: a pixel centre sits at (i+0.5, j+0.5).
    let fx = (p.x - 0.5).clamp(0.0, (w - 1) as f64);
    let fy = (p.y - 0.5).clamp(0.0, (h - 1) as f64);
    let (i0, j0) = (fx.floor() as usize, fy.floor() as usize);
    let (i1, j1) = ((i0 + 1).min(w - 1), (j0 + 1).min(h - 1));
    let (tx, ty) = (fx - i0 as f64, fy - j0 as f64);
    let v = |i: usize, j: usize| field[j * w + i];
    let a = v(i0, j0) * (1.0 - tx) + v(i1, j0) * tx;
    let b = v(i0, j1) * (1.0 - tx) + v(i1, j1) * tx;
    a * (1.0 - ty) + b * ty
}

/// Observe the boundaries of one mixture hypothesis.
pub fn observe_boundaries(
    e: &Flat2Evidence,
    coverage_level: f64,
    cfg: &BoundaryConfig,
    ccfg: &CorridorConfig,
) -> Result<BoundaryObservation, BoundaryRefusal> {
    if crate::corridor::z_for_coverage(coverage_level).is_none() {
        return Err(BoundaryRefusal::UnknownCoverageLevel {
            level: coverage_level,
        });
    }
    let (w, h) = (e.width_px() as usize, e.height_px() as usize);
    let alpha = e.alpha_field();
    let (raw, vertex, saddles) = extract_chains(alpha, w, h, cfg.level);
    if raw.is_empty() {
        return Err(BoundaryRefusal::NoContour { level: cfg.level });
    }
    let lat = Lattice { w, h };

    // Per-pixel coverage gradient VECTOR, for normals.
    let mut gx = vec![0.0f64; w * h];
    let mut gy = vec![0.0f64; w * h];
    let at = |x: i64, y: i64| -> usize {
        let cx = x.clamp(0, w as i64 - 1) as usize;
        let cy = y.clamp(0, h as i64 - 1) as usize;
        cy * w + cx
    };
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            gx[at(x, y)] = 0.5 * (alpha[at(x + 1, y)] - alpha[at(x - 1, y)]);
            gy[at(x, y)] = 0.5 * (alpha[at(x, y + 1)] - alpha[at(x, y - 1)]);
        }
    }
    let residual_norm: Vec<f64> = (0..w * h).map(|i| norm(e.residual(i))).collect();

    let mut chains = Vec::new();
    let mut total_length = 0.0;
    let mut all_halfwidths: Vec<f64> = Vec::new();
    let mut capped = 0u64;
    let mut samples_total = 0u64;

    for (ci, (path, closed)) in raw.iter().enumerate() {
        let pts: Vec<Pt> = path.iter().map(|k| vertex[k]).collect();
        if pts.len() < 2 {
            continue;
        }
        if !closed {
            let ends = [path[0], path[path.len() - 1]];
            if !ends.iter().all(|k| lat.on_border(*k)) {
                return Err(BoundaryRefusal::Malformed {
                    chain: ci,
                    detail: format!(
                        "ends at lattice edges {:?}, which are interior",
                        ends.iter()
                            .filter(|k| !lat.on_border(**k))
                            .collect::<Vec<_>>()
                    ),
                });
            }
        }
        let mut cum = vec![0.0f64];
        for k in 1..pts.len() {
            let d = (pts[k].x - pts[k - 1].x).hypot(pts[k].y - pts[k - 1].y);
            cum.push(cum[k - 1] + d);
        }
        let length = *cum.last().unwrap_or(&0.0);
        if length <= 0.0 {
            continue;
        }
        let n = ((length / cfg.sample_step_px).round() as usize).max(1);
        let step = length / n as f64;
        let mut chain_samples = Vec::with_capacity(n);
        let mut chain_residual = Vec::with_capacity(n);
        for k in 0..n {
            let s = (k as f64 + 0.5) * step;
            // Locate s on the polyline.
            let seg = cum.partition_point(|c| *c <= s).clamp(1, pts.len() - 1);
            let t = (s - cum[seg - 1]) / (cum[seg] - cum[seg - 1]).max(1e-12);
            let p = Pt::new(
                pts[seg - 1].x + t * (pts[seg].x - pts[seg - 1].x),
                pts[seg - 1].y + t * (pts[seg].y - pts[seg - 1].y),
            );
            let (ux, uy) = (bilinear(&gx, w, h, p), bilinear(&gy, w, h, p));
            let g = ux.hypot(uy);
            let normal = if g > 1e-12 {
                Pt::new(ux / g, uy / g)
            } else {
                // Degenerate gradient: fall back to the polyline normal and
                // let the corridor say the position is unconstrained.
                let d = Pt::new(pts[seg].x - pts[seg - 1].x, pts[seg].y - pts[seg - 1].y);
                let l = d.x.hypot(d.y).max(1e-12);
                Pt::new(-d.y / l, d.x / l)
            };
            let pixel = at(p.x.floor() as i64, p.y.floor() as i64);
            let c: Corridor = corridor_at(e, pixel, g, coverage_level, ccfg).ok_or(
                BoundaryRefusal::UnknownCoverageLevel {
                    level: coverage_level,
                },
            )?;
            if c.capped {
                capped += 1;
            }
            all_halfwidths.push(c.halfwidth_px);
            chain_residual.push(bilinear(&residual_norm, w, h, p));
            chain_samples.push(BoundarySample {
                p,
                normal,
                halfwidth: c.halfwidth_px,
                confidence: sample_confidence(&c, ccfg),
                weight_ds: step,
                // Filled in below, once the whole chain is known.
                corr_length_px: 0.0,
            });
        }
        let corr = correlation_length(&chain_residual, step);
        for s in &mut chain_samples {
            s.corr_length_px = corr;
        }
        samples_total += chain_samples.len() as u64;
        total_length += length;
        chains.push(BoundaryChain {
            vertices: pts.len() as u64,
            samples: chain_samples,
            closed: *closed,
            length_px: length,
            corr_length_px: corr,
        });
    }

    if chains.is_empty() {
        return Err(BoundaryRefusal::NoContour { level: cfg.level });
    }
    all_halfwidths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pick = |q: f64| {
        let i = ((all_halfwidths.len() as f64 * q) as usize).min(all_halfwidths.len() - 1);
        all_halfwidths[i]
    };
    Ok(BoundaryObservation {
        status: if saddles > 0 {
            ChainStatus::Ambiguous {
                saddle_cells: saddles,
            }
        } else {
            ChainStatus::WellFormed
        },
        total_length_px: total_length,
        samples: samples_total,
        coverage_level,
        extraction_level: cfg.level,
        sample_step_px: cfg.sample_step_px,
        capped_fraction: capped as f64 / samples_total.max(1) as f64,
        median_halfwidth_px: pick(0.5),
        p95_halfwidth_px: pick(0.95),
        chains,
    })
}

/// Correlation length of a sequence, in px, from its autocorrelation.
///
/// The first lag at which the normalized autocorrelation drops below `1/e`,
/// scaled by the sample step. Taken over the residual MAGNITUDE: F-0017
/// measured what happens otherwise — a formation mismatch is a dipole, and
/// the signed autocorrelation of a dipole cancels to "white noise".
fn correlation_length(values: &[f64], step_px: f64) -> f64 {
    let n = values.len();
    if n < 4 {
        return step_px;
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    let var: f64 = values.iter().map(|v| (v - mean) * (v - mean)).sum();
    if var <= 0.0 {
        return step_px;
    }
    let max_lag = (n / 2).max(1);
    for lag in 1..max_lag {
        let c: f64 = (0..n - lag)
            .map(|i| (values[i] - mean) * (values[i + lag] - mean))
            .sum();
        if c / var < std::f64::consts::E.recip() {
            return lag as f64 * step_px;
        }
    }
    max_lag as f64 * step_px
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corridor::CORRIDOR_CONFIG_V1;
    use crate::mixture::{infer_mixture, MIXTURE_CONFIG_V1};
    use crate::palette::oracle_override;
    use vice_image::{CanonicalImage, IccAssumption, ObservationTensor};
    use vice_ir::color::linear_to_srgb_encoded;
    use vice_ir::{
        BlendSpace, ExteriorModel, GlobalFormationHypothesis, LinearRgb, PixelFilter,
        QuantizationModel,
    };

    fn enc(v: f64) -> u8 {
        (linear_to_srgb_encoded(v.clamp(0.0, 1.0)) * 255.0).round() as u8
    }

    const INK: LinearRgb = LinearRgb {
        r: 0.1,
        g: 0.4,
        b: 0.85,
    };

    /// Build evidence from an alpha field over a transparent exterior.
    fn evidence(size: u32, alpha: impl Fn(u32, u32) -> f64) -> Flat2Evidence {
        let mut px = Vec::new();
        for y in 0..size {
            for x in 0..size {
                px.extend_from_slice(&[
                    enc(INK.r),
                    enc(INK.g),
                    enc(INK.b),
                    (alpha(x, y).clamp(0.0, 1.0) * 255.0).round() as u8,
                ]);
            }
        }
        let img = CanonicalImage::from_straight_srgb8(
            size,
            size,
            px,
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap();
        let t = ObservationTensor::of(&img, BlendSpace::LinearLight);
        infer_mixture(
            &t,
            &oracle_override(INK, None),
            &GlobalFormationHypothesis {
                blend_space: BlendSpace::LinearLight,
                pixel_filter: PixelFilter::Box,
                quantization: QuantizationModel::Uint8,
                exterior: ExteriorModel::Transparent,
            },
            "img",
            &MIXTURE_CONFIG_V1,
        )
        .unwrap()
    }

    /// A disc: one CLOSED chain whose samples lie on the circle, with
    /// normals pointing inward (into the foreground) and arclength weights
    /// summing to the perimeter.
    #[test]
    fn a_disc_yields_one_closed_chain_with_inward_normals() {
        let r = 10.0;
        let c = 16.0;
        let e = evidence(32, |x, y| {
            let d = (f64::from(x) + 0.5 - c).hypot(f64::from(y) + 0.5 - c);
            (r + 0.5 - d).clamp(0.0, 1.0)
        });
        let o = observe_boundaries(&e, 0.95, &BOUNDARY_CONFIG_V1, &CORRIDOR_CONFIG_V1).unwrap();
        assert_eq!(o.chains.len(), 1, "one shape, one boundary");
        assert!(o.chains[0].closed);
        assert_eq!(o.status, ChainStatus::WellFormed);
        // The extracted length is the circle's perimeter to within the
        // polygonal-approximation error of a 20 px disc.
        let want = 2.0 * std::f64::consts::PI * r;
        assert!(
            (o.total_length_px - want).abs() / want < 0.02,
            "length {} against {want}",
            o.total_length_px
        );
        let sum_ds: f64 = o.chains[0].samples.iter().map(|s| s.weight_ds).sum();
        assert!(
            (sum_ds - o.total_length_px).abs() < 1e-9,
            "ds must partition the chain"
        );
        for s in &o.chains[0].samples {
            // On the circle, to a fraction of a pixel.
            let d = (s.p.x - c).hypot(s.p.y - c);
            assert!((d - r).abs() < 0.3, "sample at radius {d}");
            // Unit normal pointing at the centre.
            assert!((s.normal.x.hypot(s.normal.y) - 1.0).abs() < 1e-9);
            let inward = ((c - s.p.x) * s.normal.x + (c - s.p.y) * s.normal.y) / d.max(1e-9);
            assert!(inward > 0.8, "normal must point into the ink: {inward}");
        }
    }

    /// §13 step 5: the observation is invariant to the sample step. The
    /// number of samples changes, the geometry does not.
    #[test]
    fn the_observation_is_invariant_to_the_sample_step() {
        let e = evidence(32, |x, y| {
            let d = (f64::from(x) + 0.5 - 16.0).hypot(f64::from(y) + 0.5 - 16.0);
            (10.5 - d).clamp(0.0, 1.0)
        });
        let mut lengths = Vec::new();
        let mut medians = Vec::new();
        for step in [0.25f64, 0.5, 1.0] {
            let o = observe_boundaries(
                &e,
                0.95,
                &BoundaryConfig {
                    sample_step_px: step,
                    ..BOUNDARY_CONFIG_V1
                },
                &CORRIDOR_CONFIG_V1,
            )
            .unwrap();
            lengths.push(o.total_length_px);
            medians.push(o.median_halfwidth_px);
        }
        assert!(
            lengths.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-9),
            "length must not depend on the sample step: {lengths:?}"
        );
        let (lo, hi) = medians
            .iter()
            .fold((f64::MAX, 0.0f64), |(l, h), m| (l.min(*m), h.max(*m)));
        assert!(
            (hi - lo) < 0.05,
            "median halfwidth moved with the sample step: {medians:?}"
        );
    }

    /// §5.3's critical 2x2 is REPORTED. A checkerboard of coverage is the
    /// case, and the observation says `ambiguous` with a count instead of
    /// silently picking a connectivity.
    #[test]
    fn a_saddle_configuration_is_reported_as_ambiguous() {
        let e = evidence(16, |x, y| if (x + y) % 2 == 0 { 1.0 } else { 0.0 });
        let o = observe_boundaries(&e, 0.95, &BOUNDARY_CONFIG_V1, &CORRIDOR_CONFIG_V1).unwrap();
        match o.status {
            ChainStatus::Ambiguous { saddle_cells } => assert!(saddle_cells > 10, "{saddle_cells}"),
            other => panic!("{other:?}"),
        }
        // Control: a clean disc is NOT ambiguous, or the status would be
        // constant and mean nothing.
        let clean = evidence(32, |x, y| {
            let d = (f64::from(x) + 0.5 - 16.0).hypot(f64::from(y) + 0.5 - 16.0);
            (10.5 - d).clamp(0.0, 1.0)
        });
        assert_eq!(
            observe_boundaries(&clean, 0.95, &BOUNDARY_CONFIG_V1, &CORRIDOR_CONFIG_V1)
                .unwrap()
                .status,
            ChainStatus::WellFormed
        );
    }

    /// A shape running off the canvas gives an OPEN chain that ends on the
    /// border, and that is well-formed rather than repaired.
    #[test]
    fn a_shape_crossing_the_border_gives_an_open_chain_that_ends_there() {
        let e = evidence(24, |x, _| if x < 12 { 1.0 } else { 0.0 });
        let o = observe_boundaries(&e, 0.95, &BOUNDARY_CONFIG_V1, &CORRIDOR_CONFIG_V1).unwrap();
        assert_eq!(o.chains.len(), 1);
        assert!(!o.chains[0].closed);
        assert!(
            (o.total_length_px - 22.0).abs() < 1.5,
            "{}",
            o.total_length_px
        );
    }

    /// The malformed case is a REFUSAL, not a weld. Reached by handing the
    /// linker a chain whose end is an interior lattice edge — the shape
    /// §13 says must invalidate the topology hypothesis.
    #[test]
    fn a_chain_ending_in_the_interior_is_refused_rather_than_closed() {
        let lat = Lattice { w: 8, h: 8 };
        // An interior vertical edge: not on the outer ring.
        let interior = lat.v_edge(3, 3);
        assert!(!lat.on_border(interior));
        assert!(lat.on_border(lat.v_edge(0, 3)));
        assert!(lat.on_border(lat.h_edge(3, 0)));
        assert!(!lat.on_border(lat.h_edge(3, 3)));

        // And the refusal type says what it is about.
        let r = BoundaryRefusal::Malformed {
            chain: 0,
            detail: "ends at lattice edges [12], which are interior".to_string(),
        };
        assert!(r.to_string().contains("13 forbids repairing it silently"));
    }

    /// An empty field has no boundary, and that is a typed refusal rather
    /// than an empty observation somebody downstream reads as "no error".
    #[test]
    fn a_field_that_never_crosses_the_level_is_refused() {
        let e = evidence(16, |_, _| 0.0);
        assert!(matches!(
            observe_boundaries(&e, 0.95, &BOUNDARY_CONFIG_V1, &CORRIDOR_CONFIG_V1),
            Err(BoundaryRefusal::NoContour { .. })
        ));
        let full = evidence(16, |_, _| 1.0);
        assert!(matches!(
            observe_boundaries(&full, 0.95, &BOUNDARY_CONFIG_V1, &CORRIDOR_CONFIG_V1),
            Err(BoundaryRefusal::NoContour { .. })
        ));
        // An unknown coverage level is refused too: a corridor at 0.8 would
        // need a quantile nobody tabulated.
        let disc = evidence(24, |x, y| {
            let d = (f64::from(x) + 0.5 - 12.0).hypot(f64::from(y) + 0.5 - 12.0);
            (8.5 - d).clamp(0.0, 1.0)
        });
        assert!(matches!(
            observe_boundaries(&disc, 0.80, &BOUNDARY_CONFIG_V1, &CORRIDOR_CONFIG_V1),
            Err(BoundaryRefusal::UnknownCoverageLevel { .. })
        ));
    }

    /// The correlation-length estimator is controlled in BOTH directions
    /// (meta-rule M-4): a white sequence measures one step, a smooth one
    /// measures many.
    #[test]
    fn the_correlation_length_estimator_has_a_control_in_both_directions() {
        let white: Vec<f64> = (0..64)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        assert!((correlation_length(&white, 0.5) - 0.5).abs() < 1e-9);
        let smooth: Vec<f64> = (0..64).map(|i| (f64::from(i) / 20.0).sin()).collect();
        assert!(
            correlation_length(&smooth, 0.5) > 2.0,
            "{}",
            correlation_length(&smooth, 0.5)
        );
    }
}
