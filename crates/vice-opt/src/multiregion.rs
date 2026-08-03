//! Per-face opaque paint fitting and exact-score alternation for M8.

use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_image::ObservationTensor;
use vice_ir::color::{linear_to_srgb_encoded, srgb_encoded_to_linear};
use vice_ir::ValidatedScene;
use vice_ir::{BlendSpace, FaceId, LinearRgb};
use vice_render::{
    render_partition, render_partition_roi, PartitionRender, PixelRect, RenderOptions,
};

use crate::likelihood::{
    score_full_resolution, score_full_resolution_scope, BlockLikelihoodConfig, LikelihoodError,
    PriorCodeLengths,
};
use crate::trust_region::{Rect, ScoreScope};
use crate::universe::{model_universe_hash, SupportedModelUniverseV1};

pub const MULTIREGION_PAINT_SCHEMA: &str = "vice-classic/multiregion-paint-fit/v1";

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MultiregionPaintConfig {
    pub schema: &'static str,
    pub ridge_relative: f64,
    pub min_face_support_px: f64,
    pub paint_code_bits: f64,
}

pub const MULTIREGION_PAINT_CONFIG_V1: MultiregionPaintConfig = MultiregionPaintConfig {
    schema: MULTIREGION_PAINT_SCHEMA,
    ridge_relative: 1e-10,
    min_face_support_px: 1e-3,
    paint_code_bits: 24.0,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FacePaintFit {
    pub face: FaceId,
    pub linear_rgb: LinearRgb,
    pub quantized_srgb8: [u8; 3],
    pub coverage_support_px: f64,
    pub code_length_bits: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaintFit {
    pub schema: &'static str,
    pub paints: Vec<FacePaintFit>,
    /// Weighted least-squares diagnostic used only to seed exact likelihood.
    pub proposal_residual: f64,
    pub total_paint_code_bits: f64,
    pub requires_exact_rerender: bool,
    pub digest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PaintFitError {
    #[error("paint-fit configuration is malformed")]
    InvalidConfig,
    #[error("observation and partition dimensions disagree")]
    DimensionMismatch,
    #[error("transparent exterior face {face:?} does not exist")]
    UnknownTransparentExterior { face: FaceId },
    #[error("face {face:?} has insufficient visible support ({support_px} px)")]
    InsufficientFaceSupport { face: FaceId, support_px: f64 },
    #[error("the per-face paint normal equations are singular")]
    SingularNormalEquations,
    #[error("paint fit produced a non-finite value")]
    NonFinite,
    #[error("paint-fit evidence weights are malformed")]
    InvalidWeights,
    #[error("fixed paint table does not exactly cover every opaque face")]
    FixedPaintMismatch,
}

/// Jointly fit all opaque face colours against the common visible partition.
/// Boundary pixels contribute once through their full area-fraction row; no
/// pairwise edge mixture or per-face crop is constructed.
pub fn fit_opaque_face_paints(
    observation: &ObservationTensor,
    render: &PartitionRender,
    transparent_exterior: Option<FaceId>,
    cfg: &MultiregionPaintConfig,
) -> Result<PaintFit, PaintFitError> {
    fit_opaque_face_paints_impl(observation, render, transparent_exterior, None, cfg)
}

pub fn fit_opaque_face_paints_weighted(
    observation: &ObservationTensor,
    render: &PartitionRender,
    transparent_exterior: Option<FaceId>,
    evidence_weights: &[f64],
    cfg: &MultiregionPaintConfig,
) -> Result<PaintFit, PaintFitError> {
    fit_opaque_face_paints_impl(
        observation,
        render,
        transparent_exterior,
        Some(evidence_weights),
        cfg,
    )
}

#[path = "multiregion/fixed.rs"]
mod fixed;
pub use fixed::score_fixed_opaque_face_paints;

fn fit_opaque_face_paints_impl(
    observation: &ObservationTensor,
    render: &PartitionRender,
    transparent_exterior: Option<FaceId>,
    evidence_weights: Option<&[f64]>,
    cfg: &MultiregionPaintConfig,
) -> Result<PaintFit, PaintFitError> {
    if cfg.schema != MULTIREGION_PAINT_SCHEMA
        || !cfg.ridge_relative.is_finite()
        || !cfg.min_face_support_px.is_finite()
        || !cfg.paint_code_bits.is_finite()
        || cfg.ridge_relative < 0.0
        || cfg.min_face_support_px <= 0.0
        || cfg.paint_code_bits < 0.0
    {
        return Err(PaintFitError::InvalidConfig);
    }
    let n = observation.len();
    if evidence_weights.is_some_and(|weights| {
        weights.len() != n
            || weights
                .iter()
                .any(|weight| !weight.is_finite() || *weight < 0.0)
            || !weights.iter().any(|weight| *weight > 0.0)
    }) {
        return Err(PaintFitError::InvalidWeights);
    }
    if render.width_px != observation.width_px()
        || render.height_px != observation.height_px()
        || render.composite.len() != n
        || render.face_coverage.is_empty()
        || render.face_coverage.iter().any(|v| v.len() != n)
    {
        return Err(PaintFitError::DimensionMismatch);
    }
    if let Some(face) = transparent_exterior {
        if face.0 as usize >= render.face_coverage.len() {
            return Err(PaintFitError::UnknownTransparentExterior { face });
        }
    }
    let active = (0..render.face_coverage.len())
        .filter(|face| transparent_exterior.map(|f| f.0 as usize) != Some(*face))
        .collect::<Vec<_>>();
    let m = active.len();
    let support = active
        .iter()
        .map(|&face| render.face_coverage[face].iter().sum::<f64>())
        .collect::<Vec<_>>();
    for (&face, &coverage_support_px) in active.iter().zip(&support) {
        if !coverage_support_px.is_finite() || coverage_support_px < cfg.min_face_support_px {
            return Err(PaintFitError::InsufficientFaceSupport {
                face: FaceId(face as u32),
                support_px: coverage_support_px,
            });
        }
    }

    let mut normal = vec![vec![vec![0.0f64; m]; m]; 3];
    let mut rhs = vec![vec![0.0f64; m]; 3];
    for pixel in 0..n {
        let evidence_weight = evidence_weights.map_or(1.0, |weights| weights[pixel]);
        let q = observation.quantization_halfwidth(pixel);
        for ch in 0..3 {
            let weight = evidence_weight / q[ch].max(1.0 / 510.0).powi(2);
            for (j, &face_j) in active.iter().enumerate() {
                let a = render.face_coverage[face_j][pixel];
                rhs[ch][j] += weight * a * observation.premul(pixel)[ch];
                for (k, &face_k) in active.iter().enumerate() {
                    normal[ch][j][k] += weight * a * render.face_coverage[face_k][pixel];
                }
            }
        }
    }
    for matrix in &mut normal {
        let diagonal_scale = (0..m).map(|i| matrix[i][i]).fold(0.0f64, f64::max);
        for (i, row) in matrix.iter_mut().enumerate() {
            row[i] += cfg.ridge_relative * diagonal_scale.max(1.0);
        }
    }
    let solutions = (0..3)
        .map(|ch| solve(normal[ch].clone(), rhs[ch].clone()))
        .collect::<Result<Vec<_>, _>>()?;

    let mut paints = Vec::with_capacity(m);
    for (j, &face) in active.iter().enumerate() {
        let observed = [solutions[0][j], solutions[1][j], solutions[2][j]];
        if observed.iter().any(|v| !v.is_finite()) {
            return Err(PaintFitError::NonFinite);
        }
        let to_linear = |v: f64| match observation.blend_space() {
            BlendSpace::LinearLight => v.clamp(0.0, 1.0),
            BlendSpace::EncodedSrgb => srgb_encoded_to_linear(v.clamp(0.0, 1.0)),
        };
        let linear_rgb = LinearRgb::new(
            to_linear(observed[0]),
            to_linear(observed[1]),
            to_linear(observed[2]),
        );
        let quantized_srgb8 = [linear_rgb.r, linear_rgb.g, linear_rgb.b].map(|v| {
            (linear_to_srgb_encoded(v) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        });
        paints.push(FacePaintFit {
            face: FaceId(face as u32),
            linear_rgb,
            quantized_srgb8,
            coverage_support_px: support[j],
            code_length_bits: cfg.paint_code_bits,
        });
    }

    let mut proposal_residual = 0.0;
    for pixel in 0..n {
        let evidence_weight = evidence_weights.map_or(1.0, |weights| weights[pixel]);
        for ch in 0..3 {
            let predicted = paints
                .iter()
                .map(|paint| {
                    let linear = [paint.linear_rgb.r, paint.linear_rgb.g, paint.linear_rgb.b][ch];
                    let observed = match observation.blend_space() {
                        BlendSpace::LinearLight => linear,
                        BlendSpace::EncodedSrgb => linear_to_srgb_encoded(linear),
                    };
                    render.face_coverage[paint.face.0 as usize][pixel] * observed
                })
                .sum::<f64>();
            let q = observation.quantization_halfwidth(pixel)[ch].max(1.0 / 510.0);
            proposal_residual +=
                evidence_weight * ((predicted - observation.premul(pixel)[ch]) / q).powi(2);
        }
    }
    if !proposal_residual.is_finite() {
        return Err(PaintFitError::NonFinite);
    }
    let total_paint_code_bits = cfg.paint_code_bits * paints.len() as f64;
    let digest_sha256 = paint_digest(&paints);
    Ok(PaintFit {
        schema: MULTIREGION_PAINT_SCHEMA,
        paints,
        proposal_residual,
        total_paint_code_bits,
        requires_exact_rerender: true,
        digest_sha256,
    })
}

fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Result<Vec<f64>, PaintFitError> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n)
            .max_by(|&i, &j| {
                a[i][col]
                    .abs()
                    .partial_cmp(&a[j][col].abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| j.cmp(&i))
            })
            .ok_or(PaintFitError::SingularNormalEquations)?;
        if !a[pivot][col].is_finite() || a[pivot][col].abs() <= 1e-18 {
            return Err(PaintFitError::SingularNormalEquations);
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let divisor = a[col][col];
        for value in &mut a[col][col..] {
            *value /= divisor;
        }
        b[col] /= divisor;
        for row in 0..n {
            if row == col {
                continue;
            }
            let factor = a[row][col];
            let pivot_tail = a[col][col..].to_vec();
            for (value, pivot_value) in a[row][col..].iter_mut().zip(pivot_tail) {
                *value -= factor * pivot_value;
            }
            b[row] -= factor * b[col];
        }
    }
    Ok(b)
}

fn paint_digest(paints: &[FacePaintFit]) -> String {
    let mut h = Sha256::new();
    h.update(MULTIREGION_PAINT_SCHEMA.as_bytes());
    for paint in paints {
        h.update(paint.face.0.to_le_bytes());
        h.update(paint.quantized_srgb8);
        h.update(paint.coverage_support_px.to_bits().to_le_bytes());
    }
    hex::encode(h.finalize())
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AlternationConfig {
    pub max_rounds: u32,
    pub min_exact_improvement_bits: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AlternationCandidate {
    pub id: String,
    pub universe_hash: String,
    pub palette_digest: String,
    pub partition_digest: String,
    pub paint_digest: String,
    pub exact_total_bits: f64,
    pub exact_rerendered: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AlternationTraceRow {
    pub round: u32,
    pub parent_id: String,
    pub proposed: u64,
    pub accepted_id: Option<String>,
    pub exact_improvement_bits: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AlternationResult {
    pub winner: AlternationCandidate,
    pub trace: Vec<AlternationTraceRow>,
    pub converged: bool,
    pub exhausted: bool,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AlternationError {
    #[error("alternation configuration is malformed")]
    InvalidConfig,
    #[error("M8 alternation cannot reuse a candidate/calibration from universe {found}")]
    StaleUniverse { found: String },
    #[error("candidate {id} has no finite exact rerender score")]
    MissingExactRerender { id: String },
    #[error("the refinement step refused: {0}")]
    Refinement(String),
}

/// Bounded deterministic palette -> partition -> paint alternation.  The
/// callback may use proposals internally, but every returned candidate must
/// carry a common exact rerender score under the M8 universe identity.
pub fn run_exact_alternation<F>(
    initial: AlternationCandidate,
    cfg: AlternationConfig,
    mut refine: F,
) -> Result<AlternationResult, AlternationError>
where
    F: FnMut(&AlternationCandidate, u32) -> Result<Vec<AlternationCandidate>, String>,
{
    if cfg.max_rounds == 0
        || !cfg.min_exact_improvement_bits.is_finite()
        || cfg.min_exact_improvement_bits < 0.0
    {
        return Err(AlternationError::InvalidConfig);
    }
    let expected_universe = model_universe_hash(&SupportedModelUniverseV1::m8());
    validate_candidate(&initial, &expected_universe)?;
    let mut winner = initial;
    let mut trace = Vec::new();
    for round in 0..cfg.max_rounds {
        let mut proposed = refine(&winner, round).map_err(AlternationError::Refinement)?;
        for candidate in &proposed {
            validate_candidate(candidate, &expected_universe)?;
        }
        proposed.sort_by(|a, b| {
            a.exact_total_bits
                .partial_cmp(&b.exact_total_bits)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.partition_digest.cmp(&b.partition_digest))
                .then(a.paint_digest.cmp(&b.paint_digest))
                .then(a.palette_digest.cmp(&b.palette_digest))
                .then(a.id.cmp(&b.id))
        });
        let best = proposed.first().cloned();
        let improvement = best
            .as_ref()
            .map(|candidate| winner.exact_total_bits - candidate.exact_total_bits)
            .unwrap_or(0.0);
        let accepted = best.filter(|_| improvement > cfg.min_exact_improvement_bits);
        trace.push(AlternationTraceRow {
            round,
            parent_id: winner.id.clone(),
            proposed: proposed.len() as u64,
            accepted_id: accepted.as_ref().map(|c| c.id.clone()),
            exact_improvement_bits: improvement,
        });
        if let Some(candidate) = accepted {
            winner = candidate;
        } else {
            return Ok(AlternationResult {
                winner,
                trace,
                converged: true,
                exhausted: false,
            });
        }
    }
    Ok(AlternationResult {
        winner,
        trace,
        converged: false,
        exhausted: true,
    })
}

fn validate_candidate(
    candidate: &AlternationCandidate,
    expected_universe: &str,
) -> Result<(), AlternationError> {
    if candidate.universe_hash != expected_universe {
        return Err(AlternationError::StaleUniverse {
            found: candidate.universe_hash.clone(),
        });
    }
    if !candidate.exact_rerendered || !candidate.exact_total_bits.is_finite() {
        return Err(AlternationError::MissingExactRerender {
            id: candidate.id.clone(),
        });
    }
    Ok(())
}

pub const M8_ROI_CERTIFICATE_SCHEMA: &str = "vice-classic/m8-exact-roi-certificate/v1";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExactRoiTransactionCertificate {
    pub schema: &'static str,
    pub affected_pixels: u64,
    pub roi: Rect,
    pub halo_px: u32,
    pub parent_full_bits: f64,
    pub child_full_bits: f64,
    pub parent_roi_bits: f64,
    pub child_roi_bits: f64,
    pub full_delta_bits: f64,
    pub roi_delta_bits: f64,
    pub roi_render_matches_full_slice: bool,
    pub preference_matches_full: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ExactRoiCertificateError {
    #[error("an exact ROI transaction needs at least one in-canvas affected pixel")]
    InvalidAffectedPixels,
    #[error("parent and child canvases differ")]
    CanvasMismatch,
    #[error(transparent)]
    Render(#[from] vice_render::RenderError),
    #[error(transparent)]
    Likelihood(#[from] LikelihoodError),
    #[error("ROI render differs from the corresponding full-render slice")]
    RoiRenderMismatch,
    #[error("ROI posterior delta {roi_delta_bits} differs from full delta {full_delta_bits}")]
    PosteriorDifferential {
        roi_delta_bits: f64,
        full_delta_bits: f64,
    },
}

/// Certify one affected-scope transaction against a complete rerender.
///
/// The ROI is derived from explicit affected pixel identities, not from a
/// caller-supplied optimistic crop. Both partition bytes and likelihood
/// preference are compared to the full court before the certificate exists.
#[allow(clippy::too_many_arguments)]
pub fn certify_exact_roi_transaction(
    parent: &ValidatedScene,
    child: &ValidatedScene,
    observed: &vice_image::CanonicalImage,
    affected_pixels: &[u64],
    render_options: &RenderOptions,
    likelihood: BlockLikelihoodConfig,
    parent_priors: PriorCodeLengths,
    child_priors: PriorCodeLengths,
    halo_px: u32,
) -> Result<ExactRoiTransactionCertificate, ExactRoiCertificateError> {
    if parent.scene().canvas != child.scene().canvas {
        return Err(ExactRoiCertificateError::CanvasMismatch);
    }
    let canvas = parent.scene().canvas;
    let width = u64::from(canvas.width_px);
    let pixels = width.saturating_mul(u64::from(canvas.height_px));
    let affected = affected_pixels
        .iter()
        .copied()
        .filter(|pixel| *pixel < pixels)
        .collect::<std::collections::BTreeSet<_>>();
    if affected.is_empty() || affected.len() != affected_pixels.len() {
        return Err(ExactRoiCertificateError::InvalidAffectedPixels);
    }
    let min_x = affected.iter().map(|pixel| pixel % width).min().unwrap() as u32;
    let max_x = affected.iter().map(|pixel| pixel % width).max().unwrap() as u32;
    let min_y = affected.iter().map(|pixel| pixel / width).min().unwrap() as u32;
    let max_y = affected.iter().map(|pixel| pixel / width).max().unwrap() as u32;
    let roi = Rect {
        x0: min_x,
        y0: min_y,
        x1: max_x + 1,
        y1: max_y + 1,
    };
    let pixel_roi = PixelRect {
        x0: roi.x0,
        y0: roi.y0,
        x1: roi.x1,
        y1: roi.y1,
    };
    let parent_full_render = render_partition(parent, render_options)?;
    let child_full_render = render_partition(child, render_options)?;
    let parent_roi_render = render_partition_roi(parent, render_options, pixel_roi)?;
    let child_roi_render = render_partition_roi(child, render_options, pixel_roi)?;
    if !roi_matches_full(&parent_full_render, &parent_roi_render)
        || !roi_matches_full(&child_full_render, &child_roi_render)
    {
        return Err(ExactRoiCertificateError::RoiRenderMismatch);
    }
    let scope = ScoreScope {
        roi: Some(roi),
        halo_px,
        global: false,
    };
    let parent_full = score_full_resolution(
        parent.scene(),
        observed,
        &parent_full_render,
        likelihood,
        parent_priors,
    )?;
    let child_full = score_full_resolution(
        child.scene(),
        observed,
        &child_full_render,
        likelihood,
        child_priors,
    )?;
    let parent_roi = score_full_resolution_scope(
        parent.scene(),
        observed,
        &parent_full_render,
        likelihood,
        parent_priors,
        scope,
    )?;
    let child_roi = score_full_resolution_scope(
        child.scene(),
        observed,
        &child_full_render,
        likelihood,
        child_priors,
        scope,
    )?;
    let full_delta_bits = child_full.total_bits - parent_full.total_bits;
    let roi_delta_bits = child_roi.total_bits - parent_roi.total_bits;
    let tolerance = 1e-9 * (1.0 + full_delta_bits.abs().max(roi_delta_bits.abs()));
    if (full_delta_bits - roi_delta_bits).abs() > tolerance {
        return Err(ExactRoiCertificateError::PosteriorDifferential {
            roi_delta_bits,
            full_delta_bits,
        });
    }
    Ok(ExactRoiTransactionCertificate {
        schema: M8_ROI_CERTIFICATE_SCHEMA,
        affected_pixels: affected_pixels.len() as u64,
        roi,
        halo_px,
        parent_full_bits: parent_full.total_bits,
        child_full_bits: child_full.total_bits,
        parent_roi_bits: parent_roi.total_bits,
        child_roi_bits: child_roi.total_bits,
        full_delta_bits,
        roi_delta_bits,
        roi_render_matches_full_slice: true,
        preference_matches_full: full_delta_bits.total_cmp(&0.0) == roi_delta_bits.total_cmp(&0.0),
    })
}

fn roi_matches_full(full: &PartitionRender, roi: &vice_render::RoiRender) -> bool {
    if full.face_coverage.len() != roi.face_coverage.len() {
        return false;
    }
    let width = full.width_px as usize;
    let roi_width = roi.rect.width() as usize;
    for (full_face, roi_face) in full.face_coverage.iter().zip(&roi.face_coverage) {
        let expected = (roi.rect.y0 as usize..roi.rect.y1 as usize)
            .flat_map(|y| {
                let start = y * width + roi.rect.x0 as usize;
                full_face[start..start + roi_width].iter().copied()
            })
            .collect::<Vec<_>>();
        if expected != *roi_face {
            return false;
        }
    }
    let expected_composite = (roi.rect.y0 as usize..roi.rect.y1 as usize)
        .flat_map(|y| {
            let start = y * width + roi.rect.x0 as usize;
            full.composite[start..start + roi_width].iter().copied()
        })
        .collect::<Vec<_>>();
    expected_composite == roi.composite
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_ir::color::PremulRgba;

    fn observation_and_partition() -> (ObservationTensor, PartitionRender) {
        use vice_image::{CanonicalImage, IccAssumption};
        let image = CanonicalImage::from_straight_srgb8(
            3,
            1,
            vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255],
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap();
        let observation = ObservationTensor::of(&image, BlendSpace::LinearLight);
        let render = PartitionRender {
            width_px: 3,
            height_px: 1,
            face_coverage: vec![
                vec![1.0, 0.0, 0.0],
                vec![0.0, 1.0, 0.0],
                vec![0.0, 0.0, 1.0],
            ],
            composite: vec![
                PremulRgba {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0
                };
                3
            ],
        };
        (observation, render)
    }

    #[test]
    fn jointly_recovers_three_face_paints_and_quantizes_them() {
        let (o, r) = observation_and_partition();
        let fit = fit_opaque_face_paints(&o, &r, None, &MULTIREGION_PAINT_CONFIG_V1).unwrap();
        assert_eq!(fit.paints.len(), 3);
        assert_eq!(
            fit.paints
                .iter()
                .map(|p| p.quantized_srgb8)
                .collect::<Vec<_>>(),
            vec![[255, 0, 0], [0, 255, 0], [0, 0, 255]]
        );
        assert!(fit.requires_exact_rerender);
        assert_eq!(fit.total_paint_code_bits, 72.0);
    }

    fn candidate(id: &str, bits: f64) -> AlternationCandidate {
        AlternationCandidate {
            id: id.into(),
            universe_hash: model_universe_hash(&SupportedModelUniverseV1::m8()),
            palette_digest: format!("p-{id}"),
            partition_digest: format!("g-{id}"),
            paint_digest: format!("c-{id}"),
            exact_total_bits: bits,
            exact_rerendered: true,
        }
    }

    #[test]
    fn alternation_accepts_only_exact_improvement_and_then_converges() {
        let result = run_exact_alternation(
            candidate("a", 20.0),
            AlternationConfig {
                max_rounds: 4,
                min_exact_improvement_bits: 0.1,
            },
            |parent, _| {
                Ok(if parent.id == "a" {
                    vec![candidate("b", 10.0), candidate("c", 15.0)]
                } else {
                    vec![candidate("worse", 11.0)]
                })
            },
        )
        .unwrap();
        assert_eq!(result.winner.id, "b");
        assert!(result.converged && !result.exhausted);
        assert_eq!(result.trace.len(), 2);
    }

    #[test]
    fn stale_flat2_universe_and_surrogate_only_candidates_are_refused() {
        let mut stale = candidate("stale", 1.0);
        stale.universe_hash = model_universe_hash(&SupportedModelUniverseV1::m7());
        assert!(matches!(
            run_exact_alternation(
                stale,
                AlternationConfig {
                    max_rounds: 1,
                    min_exact_improvement_bits: 0.0
                },
                |_, _| Ok(vec![])
            ),
            Err(AlternationError::StaleUniverse { .. })
        ));
        let mut proxy = candidate("proxy", 1.0);
        proxy.exact_rerendered = false;
        assert!(matches!(
            run_exact_alternation(
                proxy,
                AlternationConfig {
                    max_rounds: 1,
                    min_exact_improvement_bits: 0.0
                },
                |_, _| Ok(vec![])
            ),
            Err(AlternationError::MissingExactRerender { .. })
        ));
    }
}
