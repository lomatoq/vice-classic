use super::*;
use vice_image::IccAssumption;
use vice_ir::color::PremulRgba;
use vice_ir::{
    BlendSpace, Canvas, ExteriorModel, GlobalFormationHypothesis, PixelFilter, PlanarGraph,
};

fn scene(w: u32, h: u32) -> VectorScene {
    VectorScene {
        canvas: Canvas {
            width_px: w,
            height_px: h,
        },
        graph: PlanarGraph::empty(),
        formation: GlobalFormationHypothesis {
            blend_space: BlendSpace::LinearLight,
            pixel_filter: PixelFilter::Box,
            quantization: QuantizationModel::Uint8,
            exterior: ExteriorModel::Transparent,
        },
    }
}

fn cfg() -> BlockLikelihoodConfig {
    BlockLikelihoodConfig::new(2, 2.0, [0.01; 4], 4.0).unwrap()
}

fn priors() -> PriorCodeLengths {
    PriorCodeLengths {
        topology_bits: 1.0,
        geometry_bits: 2.0,
        paint_bits: 3.0,
        relation_bits: -0.5,
        formation_bits: 1.0,
    }
}

#[test]
fn exact_quantized_match_charges_no_pixel_bits_and_iid_is_diagnostic_only() {
    let image = vice_image::CanonicalImage::from_straight_srgb8(
        2,
        2,
        vec![0; 16],
        true,
        IccAssumption::SrgbChunkDeclared,
    )
    .unwrap();
    let render = PartitionRender {
        width_px: 2,
        height_px: 2,
        face_coverage: vec![vec![1.0; 4]],
        composite: vec![
            PremulRgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
            4
        ],
    };
    let got = score_full_resolution(&scene(2, 2), &image, &render, cfg(), priors()).unwrap();
    assert_eq!(got.pixel_bits, 0.0);
    assert_eq!(got.total_bits, 6.5);
    assert_eq!(got.diagnostics.iid_pixel_diagnostic_bits, 0.0);
}

#[test]
fn serialized_svg_bytes_are_the_final_likelihood_prediction() {
    let image = vice_image::CanonicalImage::from_straight_srgb8(
        2,
        2,
        vec![0; 16],
        true,
        IccAssumption::SrgbChunkDeclared,
    )
    .unwrap();
    let got =
        score_serialized_full_resolution(&scene(2, 2), &image, &[0; 16], 2, 2, cfg(), priors())
            .unwrap();
    assert_eq!(got.pixel_bits, 0.0);
    assert_eq!(
        got.diagnostics.prediction_source,
        PredictionSource::SerializedSvgRender
    );
}

#[test]
fn one_constant_error_is_charged_once_per_correlation_block_not_per_pixel() {
    let image = vice_image::CanonicalImage::from_straight_srgb8(
        4,
        2,
        vec![255; 32],
        true,
        IccAssumption::SrgbChunkDeclared,
    )
    .unwrap();
    let render = PartitionRender {
        width_px: 4,
        height_px: 2,
        face_coverage: vec![vec![1.0; 8]],
        composite: vec![
            PremulRgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
            8
        ],
    };
    let got = score_full_resolution(&scene(4, 2), &image, &render, cfg(), priors()).unwrap();
    assert_eq!(got.diagnostics.blocks, 2);
    assert!(got.pixel_bits > 0.0);
    assert!(got.diagnostics.iid_pixel_diagnostic_bits > got.pixel_bits);
}

#[test]
fn block_below_calibrated_support_is_refused() {
    assert!(matches!(
        BlockLikelihoodConfig::new(2, 2.1, [0.01; 4], 4.0),
        Err(LikelihoodError::CorrelationSupport { .. })
    ));
}

#[test]
fn roi_halo_selects_globally_aligned_correlation_blocks() {
    let image = vice_image::CanonicalImage::from_straight_srgb8(
        8,
        2,
        vec![0; 64],
        true,
        IccAssumption::SrgbChunkDeclared,
    )
    .unwrap();
    let render = PartitionRender {
        width_px: 8,
        height_px: 2,
        face_coverage: vec![vec![1.0; 16]],
        composite: vec![
            PremulRgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
            16
        ],
    };
    let local = score_full_resolution_scope(
        &scene(8, 2),
        &image,
        &render,
        cfg(),
        priors(),
        ScoreScope {
            roi: Some(crate::Rect {
                x0: 0,
                y0: 0,
                x1: 1,
                y1: 1,
            }),
            halo_px: 1,
            global: false,
        },
    )
    .unwrap();
    let crossing = score_full_resolution_scope(
        &scene(8, 2),
        &image,
        &render,
        cfg(),
        priors(),
        ScoreScope {
            roi: Some(crate::Rect {
                x0: 0,
                y0: 0,
                x1: 2,
                y1: 1,
            }),
            halo_px: 1,
            global: false,
        },
    )
    .unwrap();
    assert_eq!(local.diagnostics.blocks, 1);
    assert_eq!(crossing.diagnostics.blocks, 2);
}
