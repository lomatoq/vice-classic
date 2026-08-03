//! Deterministic M11 compact gradient renderer.

use vice_geom::Pt;
use vice_ir::color::{linear_to_srgb_u8, premultiply, LinearRgba, PremulRgba};
use vice_ir::{GradientPaint, GradientStop, LinearRgb, ValidatedGradientScene};

use crate::domain::NumericDomain;
use crate::MAX_COVERAGE_ELEMENTS;

pub const GRADIENT_RENDER_SCHEMA: &str = "vice-classic/gradient-render/v1";

#[derive(Debug, Clone, PartialEq)]
pub struct GradientRender {
    pub schema: &'static str,
    pub width_px: u32,
    pub height_px: u32,
    pub composite: Vec<PremulRgba>,
    pub straight_srgb8: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GradientRenderError {
    #[error("gradient canvas {width}x{height} is outside the render domain")]
    CanvasDomain { width: u32, height: u32 },
    #[error("gradient render requires {elements} channel elements, over limit {limit}")]
    ResourceLimit { elements: u64, limit: u64 },
}

pub fn render_gradient_scene(
    scene: &ValidatedGradientScene,
) -> Result<GradientRender, GradientRenderError> {
    let scene = scene.scene();
    let domain = NumericDomain::default();
    if scene.canvas.width_px > domain.max_canvas_dim_px
        || scene.canvas.height_px > domain.max_canvas_dim_px
    {
        return Err(GradientRenderError::CanvasDomain {
            width: scene.canvas.width_px,
            height: scene.canvas.height_px,
        });
    }
    let elements = u64::from(scene.canvas.width_px)
        .saturating_mul(u64::from(scene.canvas.height_px))
        .saturating_mul(4);
    if elements > MAX_COVERAGE_ELEMENTS {
        return Err(GradientRenderError::ResourceLimit {
            elements,
            limit: MAX_COVERAGE_ELEMENTS,
        });
    }
    let pixels = scene.canvas.width_px as usize * scene.canvas.height_px as usize;
    let mut composite = Vec::with_capacity(pixels);
    let mut straight_srgb8 = Vec::with_capacity(pixels * 4);
    for y in 0..scene.canvas.height_px {
        for x in 0..scene.canvas.width_px {
            let point = Pt::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            let color = sample(&scene.paint, point);
            composite.push(premultiply(LinearRgba {
                r: color.r,
                g: color.g,
                b: color.b,
                a: 1.0,
            }));
            straight_srgb8.extend_from_slice(&[
                linear_to_srgb_u8(color.r),
                linear_to_srgb_u8(color.g),
                linear_to_srgb_u8(color.b),
                255,
            ]);
        }
    }
    Ok(GradientRender {
        schema: GRADIENT_RENDER_SCHEMA,
        width_px: scene.canvas.width_px,
        height_px: scene.canvas.height_px,
        composite,
        straight_srgb8,
    })
}

fn sample(paint: &GradientPaint, point: Pt) -> LinearRgb {
    match paint {
        GradientPaint::Solid { color } => *color,
        GradientPaint::Linear { start, end, stops } => {
            let axis = *end - *start;
            let offset = ((point - *start).dot(axis) / axis.length_sq()).clamp(0.0, 1.0);
            sample_stops(stops, offset)
        }
        GradientPaint::Radial {
            center,
            radius_px,
            stops,
        } => sample_stops(stops, (point.dist(*center) / radius_px).clamp(0.0, 1.0)),
    }
}

fn sample_stops(stops: &[GradientStop], offset: f64) -> LinearRgb {
    let upper = stops.partition_point(|stop| stop.offset <= offset);
    if upper == 0 {
        return stops[0].color;
    }
    if upper == stops.len() {
        return stops[stops.len() - 1].color;
    }
    let left = stops[upper - 1];
    let right = stops[upper];
    let span = right.offset - left.offset;
    if span <= 0.0 {
        return right.color;
    }
    let t = (offset - left.offset) / span;
    LinearRgb::new(
        left.color.r + (right.color.r - left.color.r) * t,
        left.color.g + (right.color.g - left.color.g) * t,
        left.color.b + (right.color.b - left.color.b) * t,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_ir::{Canvas, GradientScene};

    fn stop(offset: f64, r: f64, b: f64) -> GradientStop {
        GradientStop {
            offset,
            color: LinearRgb::new(r, 0.0, b),
        }
    }

    #[test]
    fn duplicate_stops_render_a_real_discontinuity() {
        let scene = ValidatedGradientScene::new(GradientScene {
            canvas: Canvas {
                width_px: 4,
                height_px: 1,
            },
            paint: GradientPaint::Linear {
                start: Pt::new(0.0, 0.5),
                end: Pt::new(4.0, 0.5),
                stops: vec![
                    stop(0.0, 1.0, 0.0),
                    stop(0.5, 1.0, 0.0),
                    stop(0.5, 0.0, 1.0),
                    stop(1.0, 0.0, 1.0),
                ],
            },
        })
        .unwrap();
        let rendered = render_gradient_scene(&scene).unwrap();
        assert_eq!(&rendered.straight_srgb8[4..8], &[255, 0, 0, 255]);
        assert_eq!(&rendered.straight_srgb8[8..12], &[0, 0, 255, 255]);
        assert_eq!(rendered, render_gradient_scene(&scene).unwrap());
    }
}
