//! Typed render failures (spec §5.4: typed tolerances and typed rejects —
//! the renderer refuses non-partitions, it never repairs them silently).

use vice_ir::{FaceId, PixelFilter};

use crate::mesh::MeshError;

/// Typed failure of partition rendering.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RenderError {
    #[error(transparent)]
    Mesh(#[from] MeshError),
    #[error(
        "pixel filter {got:?} is not renderable in M2: the M2 renderer computes exact box-filter area coverage; Triangle/Gaussian forward filters arrive with the M4 formation work"
    )]
    UnsupportedPixelFilter { got: PixelFilter },
    #[error(
        "canvas {width_px}x{height_px} with {faces} faces exceeds the render resource limit ({limit_elements} coverage elements)"
    )]
    CanvasTooLarge {
        width_px: u32,
        height_px: u32,
        faces: usize,
        limit_elements: u64,
    },
    #[error(
        "loop {loop_index} of face {face:?}: |signed area| {signed_area_px2} px^2 does not exceed the certified uncertainty {certified_bound_px2} px^2 — orientation cannot be certified at this tessellation budget"
    )]
    UncertifiableLoopOrientation {
        face: FaceId,
        loop_index: usize,
        signed_area_px2: f64,
        certified_bound_px2: f64,
    },
    #[error(
        "exterior face loop {loop_index} has POSITIVE certified signed area {signed_area_px2} px^2; exterior loops must all be negative (face-on-left convention)"
    )]
    ExteriorPositiveLoop {
        loop_index: usize,
        signed_area_px2: f64,
    },
    #[error(
        "face {face:?} has {positive_loops} certified-positive loops out of {total_loops}; a bounded face must have exactly one outer loop"
    )]
    FaceLoopStructure {
        face: FaceId,
        positive_loops: usize,
        total_loops: usize,
    },
    #[error(
        "face {face:?} coverage at pixel ({x},{y}) is {coverage}, outside [0,1] by more than {tolerance} — the faces do not form a partition (overlap or wrong nesting)"
    )]
    PartitionRangeViolation {
        face: FaceId,
        x: u32,
        y: u32,
        coverage: f64,
        tolerance: f64,
    },
    #[error(
        "coverage sum at pixel ({x},{y}) is {sum}, deviating from 1 by more than {tolerance} — hidden gap or overlap in the partition"
    )]
    PartitionSumViolation {
        x: u32,
        y: u32,
        sum: f64,
        tolerance: f64,
    },
    #[error("ROI [{x0},{x1})x[{y0},{y1}) is empty or exceeds the {width_px}x{height_px} canvas")]
    RoiInvalid {
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
        width_px: u32,
        height_px: u32,
    },
}
