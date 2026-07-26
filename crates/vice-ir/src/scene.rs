//! Scene container and the minimal global formation hypothesis
//! (spec v1.3 §6.1, §10.1, §16.2).
//!
//! [`GlobalFormationHypothesis`] carries exactly the M4-minimal family in
//! type form: blend space, one GLOBAL analytic pixel filter (per-edge
//! kernels are forbidden, §16.2), 8-bit quantization, transparent-or-opaque
//! exterior. M1 defines and serializes these types; estimating them from
//! pixels is M4.
//!
//! Deviation from the §6.1 sketch (ADR-0005): no `SceneProvenance` field —
//! nothing produces provenance in M1 and speculative fields are forbidden
//! (§32 rule 7).

use serde::{Deserialize, Serialize};

use crate::color::BlendSpace;
use crate::graph::PlanarGraph;

/// Canvas of a `W × H` image: the box `[0, W] × [0, H]`
/// (see `vice-geom::coords::canvas_box`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Canvas {
    pub width_px: u32,
    pub height_px: u32,
}

/// Global analytic coverage filter family (spec §10.1). The kernel is
/// global for the whole image; per-edge blur is forbidden.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PixelFilter {
    /// Unit box filter (exact pixel-area coverage).
    Box,
    /// Triangle (tent) filter.
    Triangle,
    /// Small isotropic Gaussian; `sigma_px > 0`, finite.
    Gaussian { sigma_px: f64 },
}

/// Output quantization model of the observed raster (spec §10.1: 8-bit in
/// the first family).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum QuantizationModel {
    Uint8,
}

/// How the world outside the visible partition is composited in the
/// observation (spec §10.1: transparent or opaque exterior).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ExteriorModel {
    Transparent,
    Opaque,
}

/// The minimal global image-formation hypothesis (spec §16.2).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalFormationHypothesis {
    pub blend_space: BlendSpace,
    pub pixel_filter: PixelFilter,
    pub quantization: QuantizationModel,
    pub exterior: ExteriorModel,
}

/// A visible planar scene: canvas, shared planar graph, global formation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorScene {
    pub canvas: Canvas,
    pub graph: PlanarGraph,
    pub formation: GlobalFormationHypothesis,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formation_types_compose() {
        let f = GlobalFormationHypothesis {
            blend_space: BlendSpace::LinearLight,
            pixel_filter: PixelFilter::Gaussian { sigma_px: 0.6 },
            quantization: QuantizationModel::Uint8,
            exterior: ExteriorModel::Transparent,
        };
        let scene = VectorScene {
            canvas: Canvas {
                width_px: 64,
                height_px: 64,
            },
            graph: PlanarGraph::empty(),
            formation: f,
        };
        assert_eq!(
            scene.formation.pixel_filter,
            PixelFilter::Gaussian { sigma_px: 0.6 }
        );
    }
}
