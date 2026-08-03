use std::collections::{BTreeMap, BTreeSet};

use super::*;

/// Score an explicit P1 paint assignment without silently refitting it away.
/// The returned object uses the same physical paint pricing and digest as an
/// inferred M8 fit, so the edited scene can re-enter the exact common court.
pub fn score_fixed_opaque_face_paints(
    observation: &ObservationTensor,
    render: &PartitionRender,
    transparent_exterior: Option<FaceId>,
    fixed: &[(FaceId, [u8; 3])],
    cfg: &MultiregionPaintConfig,
) -> Result<PaintFit, PaintFitError> {
    if cfg.schema != MULTIREGION_PAINT_SCHEMA
        || !cfg.ridge_relative.is_finite()
        || !cfg.min_face_support_px.is_finite()
        || !cfg.paint_code_bits.is_finite()
        || cfg.ridge_relative < 0.0
        || cfg.min_face_support_px <= 0.0
        || cfg.paint_code_bits < 0.0
        || render.width_px != observation.width_px()
        || render.height_px != observation.height_px()
        || render.composite.len() != observation.len()
        || render.face_coverage.is_empty()
        || render
            .face_coverage
            .iter()
            .any(|coverage| coverage.len() != observation.len())
    {
        return Err(PaintFitError::InvalidConfig);
    }
    if transparent_exterior.is_some_and(|face| face.index() >= render.face_coverage.len()) {
        return Err(PaintFitError::UnknownTransparentExterior {
            face: transparent_exterior.expect("checked as some"),
        });
    }

    let fixed = fixed.iter().copied().collect::<BTreeMap<_, _>>();
    let expected = (0..render.face_coverage.len())
        .map(|face| FaceId(face as u32))
        .filter(|face| Some(*face) != transparent_exterior)
        .collect::<BTreeSet<_>>();
    if fixed.len() != expected.len() || fixed.keys().copied().collect::<BTreeSet<_>>() != expected {
        return Err(PaintFitError::FixedPaintMismatch);
    }

    let mut paints = Vec::with_capacity(fixed.len());
    for (face, rgb) in fixed {
        let coverage_support_px = render.face_coverage[face.index()].iter().sum::<f64>();
        if !coverage_support_px.is_finite() || coverage_support_px < cfg.min_face_support_px {
            return Err(PaintFitError::InsufficientFaceSupport {
                face,
                support_px: coverage_support_px,
            });
        }
        paints.push(FacePaintFit {
            face,
            linear_rgb: LinearRgb::new(
                srgb_encoded_to_linear(f64::from(rgb[0]) / 255.0),
                srgb_encoded_to_linear(f64::from(rgb[1]) / 255.0),
                srgb_encoded_to_linear(f64::from(rgb[2]) / 255.0),
            ),
            quantized_srgb8: rgb,
            coverage_support_px,
            code_length_bits: cfg.paint_code_bits,
        });
    }

    let mut proposal_residual = 0.0;
    for pixel in 0..observation.len() {
        for channel in 0..3 {
            let predicted = paints
                .iter()
                .map(|paint| {
                    let linear =
                        [paint.linear_rgb.r, paint.linear_rgb.g, paint.linear_rgb.b][channel];
                    let value = match observation.blend_space() {
                        BlendSpace::LinearLight => linear,
                        BlendSpace::EncodedSrgb => linear_to_srgb_encoded(linear),
                    };
                    render.face_coverage[paint.face.index()][pixel] * value
                })
                .sum::<f64>();
            let q = observation.quantization_halfwidth(pixel)[channel].max(1.0 / 510.0);
            proposal_residual += ((predicted - observation.premul(pixel)[channel]) / q).powi(2);
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
