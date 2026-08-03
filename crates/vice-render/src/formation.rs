//! M9 global formation renderer: broader PSFs and resize chains.

use vice_geom::Pt;
use vice_ir::{PixelFilter, ResizeChain, Segment, ValidatedScene, VectorScene};

use crate::{render_partition, PartitionRender, RenderError, RenderOptions};

#[derive(Debug, thiserror::Error)]
pub enum FormationRenderError {
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error(transparent)]
    Scene(#[from] vice_ir::SceneError),
    #[error("resize chain dimensions overflow")]
    DimensionOverflow,
}

/// Render one scene under its global filter and an M9 global resize chain.
/// Geometry is re-rendered at the chain's work resolution; no raster is
/// relabelled as a high-resolution render after the fact.
pub fn render_partition_formed(
    scene: &ValidatedScene,
    options: &RenderOptions,
    resize: ResizeChain,
) -> Result<PartitionRender, FormationRenderError> {
    let target_w = scene.scene().canvas.width_px;
    let target_h = scene.scene().canvas.height_px;
    let (work_w, work_h) = match resize {
        ResizeChain::None => (target_w, target_h),
        ResizeChain::DownFrom2x => (
            target_w
                .checked_mul(2)
                .ok_or(FormationRenderError::DimensionOverflow)?,
            target_h
                .checked_mul(2)
                .ok_or(FormationRenderError::DimensionOverflow)?,
        ),
        ResizeChain::UpFromHalf => (target_w.div_ceil(2), target_h.div_ceil(2)),
    };
    let sx = f64::from(work_w) / f64::from(target_w);
    let sy = f64::from(work_h) / f64::from(target_h);
    let filter = scene.scene().formation.pixel_filter;
    let work_scene = scaled_box_scene(scene.scene(), work_w, work_h, sx, sy)?;
    let mut render = render_partition(&work_scene, options)?;
    apply_filter(
        &mut render,
        filter,
        sx,
        sy,
        work_scene.graph().exterior.index(),
    );
    if (work_w, work_h) != (target_w, target_h) {
        render = resize_partition(render, target_w, target_h, resize);
    }
    recompose(&mut render, &scene.scene().graph.faces);
    Ok(render)
}

fn scaled_box_scene(
    source: &VectorScene,
    width: u32,
    height: u32,
    sx: f64,
    sy: f64,
) -> Result<ValidatedScene, vice_ir::SceneError> {
    let mut scene = source.clone();
    scene.canvas.width_px = width;
    scene.canvas.height_px = height;
    scene.formation.pixel_filter = PixelFilter::Box;
    for vertex in &mut scene.graph.vertices {
        vertex.pos = scale_point(vertex.pos, sx, sy);
    }
    for boundary in &mut scene.graph.boundaries {
        for node in &mut boundary.curve.interior_nodes {
            node.pos = scale_point(node.pos, sx, sy);
        }
        for segment in &mut boundary.curve.segments {
            scale_segment(segment, sx, sy);
        }
    }
    ValidatedScene::new(scene)
}

fn scale_point(point: Pt, sx: f64, sy: f64) -> Pt {
    Pt::new(point.x * sx, point.y * sy)
}

fn scale_segment(segment: &mut Segment, sx: f64, sy: f64) {
    match segment {
        Segment::Line => {}
        Segment::CircularArc {
            radius_px,
            large_arc,
            ccw,
        } => {
            let (radius_px, large_arc, ccw) = (*radius_px, *large_arc, *ccw);
            if sx == sy {
                *segment = Segment::CircularArc {
                    radius_px: radius_px * sx,
                    large_arc,
                    ccw,
                };
            } else {
                // Odd target dimensions make the half-resolution work scale
                // anisotropic.  A transformed circle is then an ellipse;
                // retaining a circular arc would move the boundary.
                *segment = Segment::EllipticArc {
                    rx_px: radius_px * sx,
                    ry_px: radius_px * sy,
                    x_axis_rotation_rad: 0.0,
                    large_arc,
                    ccw,
                };
            }
        }
        Segment::EllipticArc { rx_px, ry_px, .. } => {
            *rx_px *= sx;
            *ry_px *= sy;
        }
        Segment::Quad { ctrl } => *ctrl = scale_point(*ctrl, sx, sy),
        Segment::Cubic { ctrl1, ctrl2 } => {
            *ctrl1 = scale_point(*ctrl1, sx, sy);
            *ctrl2 = scale_point(*ctrl2, sx, sy);
        }
    }
}

fn apply_filter(
    render: &mut PartitionRender,
    filter: PixelFilter,
    sx: f64,
    sy: f64,
    exterior: usize,
) {
    if filter == PixelFilter::Box {
        return;
    }
    let kernel_x = kernel(filter, sx);
    let kernel_y = kernel(filter, sy);
    let width = render.width_px as usize;
    let height = render.height_px as usize;
    for (face, coverage) in render.face_coverage.iter_mut().enumerate() {
        let outside = if face == exterior { 1.0 } else { 0.0 };
        *coverage = convolve_separable(coverage, width, height, &kernel_x, &kernel_y, outside);
    }
}

fn kernel(filter: PixelFilter, scale: f64) -> Vec<f64> {
    match filter {
        PixelFilter::Box => vec![1.0],
        PixelFilter::Triangle => {
            let radius = (2.0 * scale).ceil().max(1.0) as i32;
            normalize_kernel(
                (-radius..=radius)
                    .map(|offset| 1.0 - f64::from(offset.abs()) / f64::from(radius + 1))
                    .collect(),
            )
        }
        PixelFilter::Gaussian { sigma_px } => {
            let sigma = (sigma_px * scale).max(1e-6);
            let radius = (3.0 * sigma).ceil().max(1.0) as i32;
            normalize_kernel(
                (-radius..=radius)
                    .map(|offset| (-0.5 * (f64::from(offset) / sigma).powi(2)).exp())
                    .collect(),
            )
        }
    }
}

fn normalize_kernel(mut weights: Vec<f64>) -> Vec<f64> {
    let sum = weights.iter().sum::<f64>();
    for weight in &mut weights {
        *weight /= sum;
    }
    weights
}

fn convolve_separable(
    source: &[f64],
    width: usize,
    height: usize,
    kernel_x: &[f64],
    kernel_y: &[f64],
    outside: f64,
) -> Vec<f64> {
    let mut horizontal = vec![0.0; source.len()];
    let rx = kernel_x.len() as isize / 2;
    for y in 0..height {
        for x in 0..width {
            horizontal[y * width + x] = kernel_x
                .iter()
                .enumerate()
                .map(|(index, weight)| {
                    let sample_x = x as isize + index as isize - rx;
                    let value = if (0..width as isize).contains(&sample_x) {
                        source[y * width + sample_x as usize]
                    } else {
                        outside
                    };
                    weight * value
                })
                .sum();
        }
    }
    let mut output = vec![0.0; source.len()];
    let ry = kernel_y.len() as isize / 2;
    for y in 0..height {
        for x in 0..width {
            output[y * width + x] = kernel_y
                .iter()
                .enumerate()
                .map(|(index, weight)| {
                    let sample_y = y as isize + index as isize - ry;
                    let value = if (0..height as isize).contains(&sample_y) {
                        horizontal[sample_y as usize * width + x]
                    } else {
                        outside
                    };
                    weight * value
                })
                .sum();
        }
    }
    output
}

fn resize_partition(
    mut source: PartitionRender,
    target_w: u32,
    target_h: u32,
    chain: ResizeChain,
) -> PartitionRender {
    let source_w = source.width_px as usize;
    let source_h = source.height_px as usize;
    let target_w_usize = target_w as usize;
    let target_h_usize = target_h as usize;
    for plane in &mut source.face_coverage {
        *plane = match chain {
            ResizeChain::DownFrom2x => downsample_2x(plane, source_w, source_h),
            ResizeChain::UpFromHalf => {
                bilinear_resize(plane, source_w, source_h, target_w_usize, target_h_usize)
            }
            ResizeChain::None => unreachable!("dimensions differ only for a resize chain"),
        };
    }
    source.width_px = target_w;
    source.height_px = target_h;
    source.composite.resize(
        target_w_usize * target_h_usize,
        vice_ir::color::PremulRgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
    );
    source
}

fn downsample_2x(source: &[f64], width: usize, height: usize) -> Vec<f64> {
    let target_w = width / 2;
    let target_h = height / 2;
    let mut output = vec![0.0; target_w * target_h];
    for y in 0..target_h {
        for x in 0..target_w {
            output[y * target_w + x] = [
                source[(2 * y) * width + 2 * x],
                source[(2 * y) * width + 2 * x + 1],
                source[(2 * y + 1) * width + 2 * x],
                source[(2 * y + 1) * width + 2 * x + 1],
            ]
            .iter()
            .sum::<f64>()
                / 4.0;
        }
    }
    output
}

fn bilinear_resize(
    source: &[f64],
    width: usize,
    height: usize,
    target_w: usize,
    target_h: usize,
) -> Vec<f64> {
    let mut output = vec![0.0; target_w * target_h];
    for y in 0..target_h {
        let fy = ((y as f64 + 0.5) * height as f64 / target_h as f64 - 0.5)
            .clamp(0.0, (height - 1) as f64);
        let y0 = fy.floor() as usize;
        let y1 = (y0 + 1).min(height - 1);
        let ty = fy.fract();
        for x in 0..target_w {
            let fx = ((x as f64 + 0.5) * width as f64 / target_w as f64 - 0.5)
                .clamp(0.0, (width - 1) as f64);
            let x0 = fx.floor() as usize;
            let x1 = (x0 + 1).min(width - 1);
            let tx = fx.fract();
            let top = source[y0 * width + x0] * (1.0 - tx) + source[y0 * width + x1] * tx;
            let bottom = source[y1 * width + x0] * (1.0 - tx) + source[y1 * width + x1] * tx;
            output[y * target_w + x] = top * (1.0 - ty) + bottom * ty;
        }
    }
    output
}

fn recompose(render: &mut PartitionRender, faces: &[vice_ir::Face]) {
    let pixels = render.width_px as usize * render.height_px as usize;
    render.composite.clear();
    render.composite.resize(
        pixels,
        vice_ir::color::PremulRgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
    );
    for (face, coverage) in faces.iter().zip(&render.face_coverage) {
        let paint = crate::partition::premul_of_paint(face.paint);
        for (pixel, alpha) in render.composite.iter_mut().zip(coverage) {
            pixel.r += alpha * paint.r;
            pixel.g += alpha * paint.g;
            pixel.b += alpha * paint.b;
            pixel.a += alpha * paint.a;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anisotropic_resize_turns_a_circle_into_the_exact_ellipse() {
        let mut segment = Segment::CircularArc {
            radius_px: 4.0,
            large_arc: true,
            ccw: false,
        };
        scale_segment(&mut segment, 8.0 / 17.0, 7.0 / 15.0);
        assert_eq!(
            segment,
            Segment::EllipticArc {
                rx_px: 32.0 / 17.0,
                ry_px: 28.0 / 15.0,
                x_axis_rotation_rad: 0.0,
                large_arc: true,
                ccw: false,
            }
        );
    }
}
