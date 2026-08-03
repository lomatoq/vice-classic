//! M11 compact whole-canvas gradient IR.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vice_geom::Pt;

use crate::{Canvas, LinearRgb};

pub const GRADIENT_SCENE_SCHEMA: &str = "vice-classic/gradient-scene/v1";
pub const MAX_GRADIENT_STOPS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GradientStop {
    pub offset: f64,
    pub color: LinearRgb,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum GradientPaint {
    Solid {
        color: LinearRgb,
    },
    Linear {
        start: Pt,
        end: Pt,
        stops: Vec<GradientStop>,
    },
    Radial {
        center: Pt,
        radius_px: f64,
        stops: Vec<GradientStop>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GradientScene {
    pub canvas: Canvas,
    pub paint: GradientPaint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedGradientScene(GradientScene);

impl ValidatedGradientScene {
    pub fn new(scene: GradientScene) -> Result<Self, GradientIrError> {
        validate(&scene)?;
        Ok(Self(scene))
    }

    pub fn scene(&self) -> &GradientScene {
        &self.0
    }

    pub fn into_inner(self) -> GradientScene {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GradientIrError {
    #[error("gradient canvas has zero extent")]
    EmptyCanvas,
    #[error("gradient color {index} is outside finite linear RGB")]
    InvalidColor { index: usize },
    #[error("gradient geometry is non-finite or degenerate")]
    InvalidGeometry,
    #[error("gradient has {count} stops; expected 2..={limit}")]
    InvalidStopCount { count: usize, limit: usize },
    #[error("gradient stops must begin at 0, end at 1 and be nondecreasing")]
    InvalidStopOffsets,
    #[error("gradient offset {offset} has more than two discontinuity sides")]
    InvalidDiscontinuity { offset: f64 },
    #[error("gradient offset {offset} repeats the same color on both discontinuity sides")]
    RedundantDiscontinuity { offset: f64 },
    #[error("gradient serialization failed: {detail}")]
    Serialization { detail: String },
}

fn validate(scene: &GradientScene) -> Result<(), GradientIrError> {
    if scene.canvas.width_px == 0 || scene.canvas.height_px == 0 {
        return Err(GradientIrError::EmptyCanvas);
    }
    match &scene.paint {
        GradientPaint::Solid { color } => validate_color(*color, 0),
        GradientPaint::Linear { start, end, stops } => {
            if !valid_point(*start) || !valid_point(*end) || start.dist_sq(*end) <= 1e-12 {
                return Err(GradientIrError::InvalidGeometry);
            }
            validate_stops(stops)
        }
        GradientPaint::Radial {
            center,
            radius_px,
            stops,
        } => {
            if !valid_point(*center) || !radius_px.is_finite() || *radius_px <= 0.0 {
                return Err(GradientIrError::InvalidGeometry);
            }
            validate_stops(stops)
        }
    }
}

fn valid_point(point: Pt) -> bool {
    point.is_finite() && !point.has_negative_zero()
}

fn validate_color(color: LinearRgb, index: usize) -> Result<(), GradientIrError> {
    if color
        .components()
        .into_iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
    {
        Ok(())
    } else {
        Err(GradientIrError::InvalidColor { index })
    }
}

fn validate_stops(stops: &[GradientStop]) -> Result<(), GradientIrError> {
    if !(2..=MAX_GRADIENT_STOPS).contains(&stops.len()) {
        return Err(GradientIrError::InvalidStopCount {
            count: stops.len(),
            limit: MAX_GRADIENT_STOPS,
        });
    }
    if stops[0].offset != 0.0
        || stops[stops.len() - 1].offset != 1.0
        || stops.iter().any(|stop| {
            !stop.offset.is_finite()
                || stop.offset.is_sign_negative()
                || !(0.0..=1.0).contains(&stop.offset)
        })
        || stops.windows(2).any(|pair| pair[0].offset > pair[1].offset)
    {
        return Err(GradientIrError::InvalidStopOffsets);
    }
    for (index, stop) in stops.iter().enumerate() {
        validate_color(stop.color, index)?;
    }
    let mut run = 1usize;
    for pair in stops.windows(2) {
        if pair[0].offset == pair[1].offset {
            if pair[0].color == pair[1].color {
                return Err(GradientIrError::RedundantDiscontinuity {
                    offset: pair[0].offset,
                });
            }
            run += 1;
            if run > 2 {
                return Err(GradientIrError::InvalidDiscontinuity {
                    offset: pair[0].offset,
                });
            }
        } else {
            run = 1;
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct GradientEnvelope<'a> {
    schema: &'static str,
    scene: &'a GradientScene,
}

pub fn gradient_scene_bytes(scene: &ValidatedGradientScene) -> Result<Vec<u8>, GradientIrError> {
    serde_json::to_vec(&GradientEnvelope {
        schema: GRADIENT_SCENE_SCHEMA,
        scene: scene.scene(),
    })
    .map_err(|error| GradientIrError::Serialization {
        detail: error.to_string(),
    })
}

pub fn gradient_scene_digest_sha256(
    scene: &ValidatedGradientScene,
) -> Result<String, GradientIrError> {
    Ok(hex::encode(Sha256::digest(gradient_scene_bytes(scene)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red() -> LinearRgb {
        LinearRgb::new(1.0, 0.0, 0.0)
    }

    fn blue() -> LinearRgb {
        LinearRgb::new(0.0, 0.0, 1.0)
    }

    #[test]
    fn a_hard_stop_is_valid_and_canonical() {
        let scene = ValidatedGradientScene::new(GradientScene {
            canvas: Canvas {
                width_px: 32,
                height_px: 16,
            },
            paint: GradientPaint::Linear {
                start: Pt::new(0.0, 8.0),
                end: Pt::new(32.0, 8.0),
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: red(),
                    },
                    GradientStop {
                        offset: 0.5,
                        color: red(),
                    },
                    GradientStop {
                        offset: 0.5,
                        color: blue(),
                    },
                    GradientStop {
                        offset: 1.0,
                        color: blue(),
                    },
                ],
            },
        })
        .unwrap();
        assert_eq!(gradient_scene_digest_sha256(&scene).unwrap().len(), 64);
    }

    #[test]
    fn three_sides_at_one_discontinuity_are_rejected() {
        let scene = GradientScene {
            canvas: Canvas {
                width_px: 8,
                height_px: 8,
            },
            paint: GradientPaint::Radial {
                center: Pt::new(4.0, 4.0),
                radius_px: 4.0,
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: blue(),
                    },
                    GradientStop {
                        offset: 0.5,
                        color: red(),
                    },
                    GradientStop {
                        offset: 0.5,
                        color: blue(),
                    },
                    GradientStop {
                        offset: 0.5,
                        color: red(),
                    },
                    GradientStop {
                        offset: 1.0,
                        color: blue(),
                    },
                ],
            },
        };
        assert!(matches!(
            ValidatedGradientScene::new(scene),
            Err(GradientIrError::InvalidDiscontinuity { .. })
        ));
    }
}
