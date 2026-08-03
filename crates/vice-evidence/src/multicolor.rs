//! Deterministic multicolour palette proposals (spec v1.3 section 9.3, M8).
//!
//! This module deliberately produces a BEAM, not a winner.  Its score is a
//! proposal-ordering surrogate made from an explicit palette code length,
//! interior reconstruction, and spatial coherence.  `requires_exact_rerender`
//! is always true: the production selector must alternate these hypotheses
//! with the visible partition and compare them in the common pixel likelihood.

use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_image::ObservationTensor;
use vice_ir::LinearRgb;

use crate::interior::InteriorConfidence;
use crate::palette::color::{encode_u8, opaque_color};
use crate::palette::{find_modes, PaletteConfig};
use crate::support::NOT_A_LIKELIHOOD;

pub const MULTICOLOR_SCHEMA: &str = "vice-classic/multicolor-palette/v1";
const TRANSPARENT_LABEL: u16 = u16::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MulticolorConfig {
    pub schema: &'static str,
    pub min_faces: usize,
    pub max_faces: usize,
    pub beam_width: usize,
    /// Fixed physical description length of one opaque RGB paint.
    pub paint_code_bits: f64,
    /// Relative ordering weight of a disagreeing four-neighbour edge.
    pub coherence_edge_bits: f64,
}

pub const MULTICOLOR_CONFIG_V1: MulticolorConfig = MulticolorConfig {
    schema: MULTICOLOR_SCHEMA,
    min_faces: 3,
    max_faces: 16,
    beam_width: 8,
    paint_code_bits: 24.0,
    coherence_edge_bits: 0.25,
};

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PaletteScore {
    pub palette_code_bits: f64,
    pub reconstruction_bits: f64,
    pub spatial_coherence_bits: f64,
    pub total_proposal_bits: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MulticolorHypothesis {
    pub id: String,
    /// Canonical encoded-RGB order; independent of discovery/weight order.
    pub colors: Vec<LinearRgb>,
    /// Row-major canonical palette index; `u16::MAX` means alpha ~= 0 and
    /// therefore carries no RGB evidence.
    pub assignments: Vec<u16>,
    pub score: PaletteScore,
    pub digest_sha256: String,
    pub requires_exact_rerender: bool,
    pub surrogate_role: &'static str,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MulticolorRefusal {
    #[error("multicolour M8 needs at least {required} supported opaque modes, found {found}")]
    TooFewSupportedModes { required: usize, found: usize },
    #[error("the configured multicolour palette range is malformed")]
    MalformedConfig,
    #[error("the palette cardinality exceeds the canonical u16 label space")]
    LabelSpaceExhausted,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MulticolorProposals {
    pub schema: &'static str,
    pub hypotheses: Vec<MulticolorHypothesis>,
    pub opaque_modes_seen: usize,
    pub transparent_pixels: u64,
    pub refusal: Option<MulticolorRefusal>,
    /// Makes it impossible to mistake this ordering score for the production
    /// likelihood described in section 10.2.
    pub surrogate_role: &'static str,
}

fn color_key(c: LinearRgb) -> [u8; 3] {
    [encode_u8(c.r), encode_u8(c.g), encode_u8(c.b)]
}

fn encoded_distance_sq(a: LinearRgb, b: LinearRgb) -> f64 {
    color_key(a)
        .into_iter()
        .zip(color_key(b))
        .map(|(x, y)| {
            let d = f64::from(x) - f64::from(y);
            d * d
        })
        .sum()
}

fn canonical_digest(width: u32, height: u32, colors: &[LinearRgb], labels: &[u16]) -> String {
    let mut h = Sha256::new();
    h.update(MULTICOLOR_SCHEMA.as_bytes());
    h.update(width.to_le_bytes());
    h.update(height.to_le_bytes());
    h.update((colors.len() as u32).to_le_bytes());
    for color in colors {
        h.update(color_key(*color));
    }
    for label in labels {
        h.update(label.to_le_bytes());
    }
    hex::encode(h.finalize())
}

/// Produce every supported palette cardinality, retain a deterministic beam,
/// and leave the final decision to exact rerender/posterior comparison.
pub fn propose_multicolor(
    tensor: &ObservationTensor,
    interior: &InteriorConfidence,
    border: &[usize],
    palette_cfg: &PaletteConfig,
    cfg: &MulticolorConfig,
) -> MulticolorProposals {
    let transparent_pixels = (0..tensor.len())
        .filter(|&i| !interior.gives_rgb_evidence(i))
        .count() as u64;
    let (mut modes, _) = find_modes(tensor, interior, border, palette_cfg);
    // Opaque AA mixtures form long one-pixel chromatic bands. The Flat2
    // mode finder correctly retains spatially coherent thin features, but
    // M8 palette cardinality must not reinterpret those bands as authored
    // paints. A supported M8 paint needs a two-dimensional interior core;
    // thin authored colours remain a typed refusal until their bounded paint
    // interval can be distinguished from formation mixing.
    let mode_court = modes.clone();
    modes.retain(|mode| mode_has_area_core(tensor, interior, mode.color, &mode_court, palette_cfg));
    let base = |hypotheses, refusal| MulticolorProposals {
        schema: MULTICOLOR_SCHEMA,
        hypotheses,
        opaque_modes_seen: modes.len(),
        transparent_pixels,
        refusal,
        surrogate_role: NOT_A_LIKELIHOOD,
    };
    if cfg.min_faces < 3
        || cfg.min_faces > cfg.max_faces
        || cfg.beam_width == 0
        || !cfg.paint_code_bits.is_finite()
        || !cfg.coherence_edge_bits.is_finite()
        || cfg.paint_code_bits < 0.0
        || cfg.coherence_edge_bits < 0.0
    {
        return base(Vec::new(), Some(MulticolorRefusal::MalformedConfig));
    }
    if cfg.max_faces >= usize::from(TRANSPARENT_LABEL) {
        return base(Vec::new(), Some(MulticolorRefusal::LabelSpaceExhausted));
    }
    if modes.len() < cfg.min_faces {
        return base(
            Vec::new(),
            Some(MulticolorRefusal::TooFewSupportedModes {
                required: cfg.min_faces,
                found: modes.len(),
            }),
        );
    }

    let max_faces = modes.len().min(cfg.max_faces);
    let width = tensor.width_px() as usize;
    let height = tensor.height_px() as usize;
    let mut hypotheses = Vec::new();
    for cardinality in cfg.min_faces..=max_faces {
        let mut colors = modes[..cardinality]
            .iter()
            .map(|mode| mode.color)
            .collect::<Vec<_>>();
        colors.sort_by_key(|c| color_key(*c));

        let mut assignments = Vec::with_capacity(tensor.len());
        for i in 0..tensor.len() {
            if !interior.gives_rgb_evidence(i) {
                assignments.push(TRANSPARENT_LABEL);
                continue;
            }
            let observed = opaque_color(tensor, i);
            let (label, _) = colors
                .iter()
                .enumerate()
                .map(|(label, color)| (label, encoded_distance_sq(observed, *color)))
                .min_by(|a, b| {
                    a.1.partial_cmp(&b.1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.0.cmp(&b.0))
                })
                .expect("a multicolour hypothesis has at least three paints");
            assignments.push(label as u16);
        }
        assignments = spatially_regularize(
            tensor,
            interior,
            &colors,
            assignments,
            cfg.coherence_edge_bits,
        );
        assignments = prune_unsupported_components(
            tensor,
            interior,
            &colors,
            assignments,
            palette_cfg.min_core_weight,
            cfg.coherence_edge_bits,
        );
        let reconstruction_bits = assignments
            .iter()
            .enumerate()
            .filter(|(_, label)| **label != TRANSPARENT_LABEL)
            .map(|(i, label)| {
                interior.weight(i)
                    * encoded_distance_sq(opaque_color(tensor, i), colors[*label as usize])
                    / (255.0 * 255.0)
            })
            .sum::<f64>();

        let mut disagreeing_edges = 0u64;
        for y in 0..height {
            for x in 0..width {
                let i = y * width + x;
                for j in [
                    (x + 1 < width).then_some(i + 1),
                    (y + 1 < height).then_some(i + width),
                ]
                .into_iter()
                .flatten()
                {
                    let a = assignments[i];
                    let b = assignments[j];
                    if a != TRANSPARENT_LABEL && b != TRANSPARENT_LABEL && a != b {
                        disagreeing_edges += 1;
                    }
                }
            }
        }
        let palette_code_bits =
            cfg.paint_code_bits * cardinality as f64 + (cardinality as f64 + 1.0).log2();
        let spatial_coherence_bits = cfg.coherence_edge_bits * disagreeing_edges as f64;
        let total_proposal_bits = palette_code_bits + reconstruction_bits + spatial_coherence_bits;
        let digest = canonical_digest(tensor.width_px(), tensor.height_px(), &colors, &assignments);
        hypotheses.push(MulticolorHypothesis {
            id: format!("M8/palette-k{cardinality}/{}", &digest[..12]),
            colors,
            assignments,
            score: PaletteScore {
                palette_code_bits,
                reconstruction_bits,
                spatial_coherence_bits,
                total_proposal_bits,
            },
            digest_sha256: digest,
            requires_exact_rerender: true,
            surrogate_role: NOT_A_LIKELIHOOD,
        });
    }
    hypotheses.sort_by(|a, b| {
        a.score
            .total_proposal_bits
            .partial_cmp(&b.score.total_proposal_bits)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.digest_sha256.cmp(&b.digest_sha256))
    });
    hypotheses.truncate(cfg.beam_width);
    base(hypotheses, None)
}

fn mode_has_area_core(
    tensor: &ObservationTensor,
    interior: &InteriorConfidence,
    color: LinearRgb,
    modes: &[crate::palette::ColorMode],
    cfg: &PaletteConfig,
) -> bool {
    let width = tensor.width_px() as usize;
    let height = tensor.height_px() as usize;
    let threshold_sq = (f64::from(cfg.bin_codes.max(1)) * 2.0).powi(2) * 3.0;
    let members = (0..tensor.len())
        .filter(|&i| {
            if interior.weight(i) <= 0.0 {
                return false;
            }
            let observed = opaque_color(tensor, i);
            let nearest = modes
                .iter()
                .min_by(|a, b| {
                    encoded_distance_sq(observed, a.color)
                        .total_cmp(&encoded_distance_sq(observed, b.color))
                })
                .expect("mode court is nonempty");
            nearest.color == color && encoded_distance_sq(observed, color) <= threshold_sq
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut core_weight = 0.0;
    for &i in &members {
        let x = i % width;
        let y = i / width;
        let horizontal =
            (x > 0 && members.contains(&(i - 1))) || (x + 1 < width && members.contains(&(i + 1)));
        let vertical = (y > 0 && members.contains(&(i - width)))
            || (y + 1 < height && members.contains(&(i + width)));
        if horizontal && vertical {
            core_weight += interior.weight(i);
        }
    }
    core_weight >= cfg.min_core_weight
}

/// Deterministic Potts refinement of the visible label field. Boundary
/// mixtures over an opaque exterior carry weak paint evidence; classifying
/// them independently fragments one authored face into many RAG faces. The
/// same physical coherence price already present in the proposal score must
/// therefore participate in the proposal itself. Updates are synchronous so
/// scan direction cannot decide a label.
fn spatially_regularize(
    tensor: &ObservationTensor,
    interior: &InteriorConfidence,
    colors: &[LinearRgb],
    mut labels: Vec<u16>,
    coherence_edge_bits: f64,
) -> Vec<u16> {
    let width = tensor.width_px() as usize;
    let height = tensor.height_px() as usize;
    // One synchronous pass removes isolated mixture labels without letting a
    // Potts front erode a genuine small face across repeated iterations.
    for _ in 0..1 {
        let previous = labels.clone();
        let mut changed = false;
        for i in 0..previous.len() {
            if previous[i] == TRANSPARENT_LABEL {
                continue;
            }
            let x = i % width;
            let y = i / width;
            let neighbours = [
                (x > 0).then(|| i - 1),
                (x + 1 < width).then_some(i + 1),
                (y > 0).then(|| i - width),
                (y + 1 < height).then_some(i + width),
            ];
            let observed = opaque_color(tensor, i);
            let best = colors
                .iter()
                .enumerate()
                .map(|(label, color)| {
                    let data = interior.weight(i) * encoded_distance_sq(observed, *color)
                        / (255.0 * 255.0);
                    let spatial = neighbours
                        .iter()
                        .flatten()
                        .filter(|&&neighbour| {
                            previous[neighbour] != TRANSPARENT_LABEL
                                && previous[neighbour] != label as u16
                        })
                        .count() as f64
                        * coherence_edge_bits;
                    (label as u16, data + spatial)
                })
                .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                .expect("M8 has at least three colors")
                .0;
            if labels[i] != best {
                labels[i] = best;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    labels
}

#[allow(clippy::too_many_arguments)]
fn prune_unsupported_components(
    tensor: &ObservationTensor,
    interior: &InteriorConfidence,
    colors: &[LinearRgb],
    mut labels: Vec<u16>,
    min_core_weight: f64,
    coherence_edge_bits: f64,
) -> Vec<u16> {
    use std::collections::{BTreeMap, BTreeSet, VecDeque};

    let width = tensor.width_px() as usize;
    let height = tensor.height_px() as usize;
    let mut seen = vec![false; labels.len()];
    for start in 0..labels.len() {
        let label = labels[start];
        if label == TRANSPARENT_LABEL || seen[start] {
            continue;
        }
        let mut component = Vec::new();
        let mut queue = VecDeque::from([start]);
        seen[start] = true;
        while let Some(i) = queue.pop_front() {
            component.push(i);
            let x = i % width;
            let y = i / width;
            for neighbour in [
                (x > 0).then(|| i - 1),
                (x + 1 < width).then_some(i + 1),
                (y > 0).then(|| i - width),
                (y + 1 < height).then_some(i + width),
            ]
            .into_iter()
            .flatten()
            {
                if !seen[neighbour] && labels[neighbour] == label {
                    seen[neighbour] = true;
                    queue.push_back(neighbour);
                }
            }
        }
        let members = component.iter().copied().collect::<BTreeSet<_>>();
        let core_weight = component
            .iter()
            .copied()
            .filter(|&i| {
                let x = i % width;
                let y = i / width;
                let horizontal = (x > 0 && members.contains(&(i - 1)))
                    || (x + 1 < width && members.contains(&(i + 1)));
                let vertical = (y > 0 && members.contains(&(i - width)))
                    || (y + 1 < height && members.contains(&(i + width)));
                horizontal && vertical
            })
            .map(|i| interior.weight(i))
            .sum::<f64>();
        if core_weight >= min_core_weight {
            continue;
        }

        let mut boundary_counts = BTreeMap::<u16, u64>::new();
        for &i in &component {
            let x = i % width;
            let y = i / width;
            for neighbour in [
                (x > 0).then(|| i - 1),
                (x + 1 < width).then_some(i + 1),
                (y > 0).then(|| i - width),
                (y + 1 < height).then_some(i + width),
            ]
            .into_iter()
            .flatten()
            {
                let other = labels[neighbour];
                if other != label && other != TRANSPARENT_LABEL {
                    *boundary_counts.entry(other).or_default() += 1;
                }
            }
        }
        let replacement = boundary_counts
            .into_iter()
            .map(|(candidate, shared)| {
                let data = component
                    .iter()
                    .map(|&i| {
                        interior.weight(i)
                            * encoded_distance_sq(
                                opaque_color(tensor, i),
                                colors[candidate as usize],
                            )
                            / (255.0 * 255.0)
                    })
                    .sum::<f64>();
                let boundary_reward = coherence_edge_bits * shared as f64;
                (candidate, data - boundary_reward)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            .map(|(candidate, _)| candidate);
        if let Some(replacement) = replacement {
            for pixel in component {
                labels[pixel] = replacement;
            }
        }
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{interior_confidence, INTERIOR_CONFIG_V1, PALETTE_CONFIG_V1};
    use vice_image::{CanonicalImage, IccAssumption};
    use vice_ir::BlendSpace;

    fn stripes(hidden_rgb: [u8; 3]) -> (ObservationTensor, InteriorConfidence, Vec<usize>) {
        let (w, h) = (24u32, 12u32);
        let palette = [[230, 30, 30], [30, 210, 50], [30, 60, 230], [235, 210, 30]];
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                if x == 0 && y == 0 {
                    pixels[i..i + 3].copy_from_slice(&hidden_rgb);
                    pixels[i + 3] = 0;
                } else {
                    pixels[i..i + 3].copy_from_slice(&palette[(x / 6) as usize]);
                    pixels[i + 3] = 255;
                }
            }
        }
        let image = CanonicalImage::from_straight_srgb8(
            w,
            h,
            pixels,
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap();
        let tensor = ObservationTensor::of(&image, BlendSpace::LinearLight);
        let interior = interior_confidence(&tensor, &INTERIOR_CONFIG_V1);
        let border = (0..tensor.len())
            .filter(|&i| {
                let x = i % w as usize;
                let y = i / w as usize;
                x == 0 || y == 0 || x + 1 == w as usize || y + 1 == h as usize
            })
            .collect();
        (tensor, interior, border)
    }

    #[test]
    fn emits_a_deterministic_cardinality_beam_that_still_requires_rerender() {
        let (t, i, b) = stripes([1, 2, 3]);
        let a = propose_multicolor(&t, &i, &b, &PALETTE_CONFIG_V1, &MULTICOLOR_CONFIG_V1);
        let z = propose_multicolor(&t, &i, &b, &PALETTE_CONFIG_V1, &MULTICOLOR_CONFIG_V1);
        assert_eq!(a, z);
        assert!(a.refusal.is_none(), "{:?}", a.refusal);
        assert_eq!(a.opaque_modes_seen, 4);
        assert_eq!(a.hypotheses.len(), 2);
        assert!(a.hypotheses.iter().all(|h| h.requires_exact_rerender));
        assert!(a
            .hypotheses
            .iter()
            .all(|h| h.surrogate_role == NOT_A_LIKELIHOOD));
    }

    #[test]
    fn hidden_rgb_under_zero_alpha_cannot_change_a_palette_artifact() {
        let (ta, ia, ba) = stripes([0, 0, 0]);
        let (tb, ib, bb) = stripes([255, 19, 201]);
        let a = propose_multicolor(&ta, &ia, &ba, &PALETTE_CONFIG_V1, &MULTICOLOR_CONFIG_V1);
        let b = propose_multicolor(&tb, &ib, &bb, &PALETTE_CONFIG_V1, &MULTICOLOR_CONFIG_V1);
        assert_eq!(a.hypotheses, b.hypotheses);
        assert_eq!(a.transparent_pixels, 1);
    }

    #[test]
    fn a_flat2_observation_is_refused_by_the_multicolour_entry_point() {
        let (t, i, b) = stripes([1, 2, 3]);
        let cfg = MulticolorConfig {
            min_faces: 5,
            ..MULTICOLOR_CONFIG_V1
        };
        let got = propose_multicolor(&t, &i, &b, &PALETTE_CONFIG_V1, &cfg);
        assert!(matches!(
            got.refusal,
            Some(MulticolorRefusal::TooFewSupportedModes {
                found: 4,
                required: 5
            })
        ));
        assert!(got.hypotheses.is_empty());
    }
}
