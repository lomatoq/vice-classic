//! M11 bounded gradient geometry and stop proposals from observed pixels.

use std::collections::BTreeSet;

use serde::Serialize;
use vice_geom::{Pt, Vec2};
use vice_image::CanonicalImage;
use vice_ir::color::srgb_u8_to_linear;
use vice_ir::{
    Canvas, GradientPaint, GradientScene, GradientStop, LinearRgb, ValidatedGradientScene,
};

pub const GRADIENT_EVIDENCE_SCHEMA: &str = "vice-classic/gradient-evidence/v1";
const PROFILE_BINS: usize = 64;
const RADIAL_GRID_SIDE: usize = 5;
const MAX_CENTER_SAMPLES: usize = 65_536;
const MAX_GRADIENT_PIXELS: u64 = 8_000_000;
const MAX_PROPOSAL_WORK: u64 = 96_000_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GradientEvidenceReport {
    pub schema: &'static str,
    pub source_sha256: String,
    pub width_px: u32,
    pub height_px: u32,
    pub linear_direction: [f64; 2],
    pub radial_center: [f64; 2],
    pub detected_linear_discontinuities: u64,
    pub detected_radial_discontinuities: u64,
    pub stop_budgets: Vec<u64>,
    pub candidate_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GradientProposal {
    pub report: GradientEvidenceReport,
    pub candidates: Vec<ValidatedGradientScene>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GradientEvidenceRefusal {
    #[error("M11 opaque-gradient lane does not accept non-opaque source pixels")]
    NonOpaqueSource,
    #[error("gradient proposal work {operations} exceeds bounded limit {limit}")]
    WorkLimit { operations: u64, limit: u64 },
    #[error("gradient proposal could not construct valid compact geometry: {detail}")]
    InvalidProposal { detail: String },
}

pub fn propose_gradients(
    image: &CanonicalImage,
) -> Result<GradientProposal, GradientEvidenceRefusal> {
    if image
        .straight_srgb8()
        .chunks_exact(4)
        .any(|pixel| pixel[3] != 255)
    {
        return Err(GradientEvidenceRefusal::NonOpaqueSource);
    }
    let width = image.width_px() as usize;
    let height = image.height_px() as usize;
    let pixels = width as u64 * height as u64;
    if pixels > MAX_GRADIENT_PIXELS {
        return Err(GradientEvidenceRefusal::WorkLimit {
            operations: pixels,
            limit: MAX_GRADIENT_PIXELS,
        });
    }
    let colors = image
        .straight_srgb8()
        .chunks_exact(4)
        .map(|pixel| {
            LinearRgb::new(
                srgb_u8_to_linear(pixel[0]),
                srgb_u8_to_linear(pixel[1]),
                srgb_u8_to_linear(pixel[2]),
            )
        })
        .collect::<Vec<_>>();
    let budgets: &[usize] = if pixels <= 1_000_000 {
        &[2, 4, 8]
    } else if pixels <= 4_000_000 {
        &[2, 4]
    } else {
        &[4]
    };
    let candidate_count = 1 + 2 * budgets.len() as u64;
    let operations = pixels.saturating_mul(candidate_count);
    if operations > MAX_PROPOSAL_WORK {
        return Err(GradientEvidenceRefusal::WorkLimit {
            operations,
            limit: MAX_PROPOSAL_WORK,
        });
    }
    let direction = usable_direction(
        estimate_linear_direction(&colors, width, height),
        width,
        height,
    );
    let (linear_start, linear_end) = projected_endpoints(direction, width, height);
    let linear_start = round_point_64(linear_start);
    let linear_end = round_point_64(linear_end);
    let linear_offsets = offsets_linear(&colors, width, linear_start, linear_end);
    let linear_profile = profile(&colors, &linear_offsets);
    drop(linear_offsets);
    let linear_jumps = discontinuities(&linear_profile);

    let radial_center = round_point_64(estimate_radial_center(&colors, width, height));
    let radial_radius =
        round_64(max_pixel_center_distance(radial_center, width, height).max(1.0 / 64.0));
    let radial_offsets = offsets_radial(width, height, radial_center, radial_radius);
    let radial_profile = profile(&colors, &radial_offsets);
    drop(radial_offsets);
    let radial_jumps = discontinuities(&radial_profile);

    let canvas = Canvas {
        width_px: image.width_px(),
        height_px: image.height_px(),
    };
    let mut candidates = Vec::with_capacity(candidate_count as usize);
    let mut identities = BTreeSet::new();
    push_unique(
        &mut candidates,
        &mut identities,
        validated(GradientScene {
            canvas,
            paint: GradientPaint::Solid {
                color: mean_color(&colors),
            },
        })?,
    )?;
    for &budget in budgets {
        push_unique(
            &mut candidates,
            &mut identities,
            validated(GradientScene {
                canvas,
                paint: GradientPaint::Linear {
                    start: linear_start,
                    end: linear_end,
                    stops: stops(&linear_profile, &linear_jumps, budget),
                },
            })?,
        )?;
        push_unique(
            &mut candidates,
            &mut identities,
            validated(GradientScene {
                canvas,
                paint: GradientPaint::Radial {
                    center: radial_center,
                    radius_px: radial_radius,
                    stops: stops(&radial_profile, &radial_jumps, budget),
                },
            })?,
        )?;
    }
    Ok(GradientProposal {
        report: GradientEvidenceReport {
            schema: GRADIENT_EVIDENCE_SCHEMA,
            source_sha256: image.source_sha256().into(),
            width_px: image.width_px(),
            height_px: image.height_px(),
            linear_direction: [direction.x, direction.y],
            radial_center: [radial_center.x, radial_center.y],
            detected_linear_discontinuities: linear_jumps.len() as u64,
            detected_radial_discontinuities: radial_jumps.len() as u64,
            stop_budgets: budgets.iter().map(|value| *value as u64).collect(),
            candidate_count: candidates.len() as u64,
        },
        candidates,
    })
}

fn validated(scene: GradientScene) -> Result<ValidatedGradientScene, GradientEvidenceRefusal> {
    ValidatedGradientScene::new(scene).map_err(|error| GradientEvidenceRefusal::InvalidProposal {
        detail: error.to_string(),
    })
}

fn push_unique(
    candidates: &mut Vec<ValidatedGradientScene>,
    identities: &mut BTreeSet<String>,
    candidate: ValidatedGradientScene,
) -> Result<(), GradientEvidenceRefusal> {
    let identity = vice_ir::gradient_scene_digest_sha256(&candidate).map_err(|error| {
        GradientEvidenceRefusal::InvalidProposal {
            detail: error.to_string(),
        }
    })?;
    if identities.insert(identity) {
        candidates.push(candidate);
    }
    Ok(())
}

fn estimate_linear_direction(colors: &[LinearRgb], width: usize, height: usize) -> Vec2 {
    let mut gx = [0.0; 3];
    let mut gy = [0.0; 3];
    let mut xx = 0.0;
    let mut yy = 0.0;
    let mean = mean_color(colors).components();
    let cx = width as f64 * 0.5;
    let cy = height as f64 * 0.5;
    for (index, color) in colors.iter().enumerate() {
        let x = (index % width) as f64 + 0.5 - cx;
        let y = (index / width) as f64 + 0.5 - cy;
        xx += x * x;
        yy += y * y;
        for channel in 0..3 {
            let delta = color.components()[channel] - mean[channel];
            gx[channel] += x * delta;
            gy[channel] += y * delta;
        }
    }
    if xx > 0.0 {
        gx.iter_mut().for_each(|value| *value /= xx);
    }
    if yy > 0.0 {
        gy.iter_mut().for_each(|value| *value /= yy);
    }
    let a = gx.iter().map(|value| value * value).sum::<f64>();
    let b = gx.iter().zip(gy).map(|(x, y)| x * y).sum::<f64>();
    let d = gy.iter().map(|value| value * value).sum::<f64>();
    let angle = 0.5 * (2.0 * b).atan2(a - d);
    let direction = Vec2::new(angle.cos(), angle.sin());
    if a + d <= 1e-18 {
        Vec2::new(1.0, 0.0)
    } else if direction.x < 0.0 || (direction.x == 0.0 && direction.y < 0.0) {
        direction * -1.0
    } else {
        direction
    }
}

fn usable_direction(direction: Vec2, width: usize, height: usize) -> Vec2 {
    let extent = direction.x.abs() * width.saturating_sub(1) as f64
        + direction.y.abs() * height.saturating_sub(1) as f64;
    if extent > 1e-12 {
        direction
    } else if width > 1 || height == 1 {
        Vec2::new(1.0, 0.0)
    } else {
        Vec2::new(0.0, 1.0)
    }
}

fn projected_endpoints(direction: Vec2, width: usize, height: usize) -> (Pt, Pt) {
    let corners = [
        Pt::new(0.5, 0.5),
        Pt::new(width as f64 - 0.5, 0.5),
        Pt::new(0.5, height as f64 - 0.5),
        Pt::new(width as f64 - 0.5, height as f64 - 0.5),
    ];
    let min = corners
        .iter()
        .map(|point| point.x * direction.x + point.y * direction.y)
        .fold(f64::INFINITY, f64::min);
    let max = corners
        .iter()
        .map(|point| point.x * direction.x + point.y * direction.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let center = Pt::new(width as f64 * 0.5, height as f64 * 0.5);
    let center_projection = center.x * direction.x + center.y * direction.y;
    if max - min <= 1e-12 {
        (center - direction * 0.5, center + direction * 0.5)
    } else {
        (
            center + direction * (min - center_projection),
            center + direction * (max - center_projection),
        )
    }
}

fn offsets_linear(colors: &[LinearRgb], width: usize, start: Pt, end: Pt) -> Vec<f64> {
    let axis = end - start;
    colors
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let point = Pt::new((index % width) as f64 + 0.5, (index / width) as f64 + 0.5);
            ((point - start).dot(axis) / axis.length_sq()).clamp(0.0, 1.0)
        })
        .collect()
}

fn estimate_radial_center(colors: &[LinearRgb], width: usize, height: usize) -> Pt {
    let stride = colors.len().div_ceil(MAX_CENTER_SAMPLES).max(1);
    let mut best = (
        f64::INFINITY,
        Pt::new(width as f64 * 0.5, height as f64 * 0.5),
    );
    for gy in 0..RADIAL_GRID_SIDE {
        for gx in 0..RADIAL_GRID_SIDE {
            let center = Pt::new(
                0.5 + (width.saturating_sub(1)) as f64 * gx as f64 / (RADIAL_GRID_SIDE - 1) as f64,
                0.5 + (height.saturating_sub(1)) as f64 * gy as f64 / (RADIAL_GRID_SIDE - 1) as f64,
            );
            let radius = max_pixel_center_distance(center, width, height).max(1.0 / 64.0);
            let mut sums = [[0.0; 3]; 16];
            let mut counts = [0u64; 16];
            for index in (0..colors.len()).step_by(stride) {
                let point = Pt::new((index % width) as f64 + 0.5, (index / width) as f64 + 0.5);
                let bin = ((point.dist(center) / radius).clamp(0.0, 1.0) * 15.0).round() as usize;
                for (slot, value) in sums[bin].iter_mut().zip(colors[index].components()) {
                    *slot += value;
                }
                counts[bin] += 1;
            }
            let mut error = 0.0;
            for index in (0..colors.len()).step_by(stride) {
                let point = Pt::new((index % width) as f64 + 0.5, (index / width) as f64 + 0.5);
                let bin = ((point.dist(center) / radius).clamp(0.0, 1.0) * 15.0).round() as usize;
                if counts[bin] == 0 {
                    continue;
                }
                for (sum, value) in sums[bin].iter().zip(colors[index].components()) {
                    let delta = value - sum / counts[bin] as f64;
                    error += delta * delta;
                }
            }
            if error < best.0 {
                best = (error, center);
            }
        }
    }
    best.1
}

fn max_pixel_center_distance(center: Pt, width: usize, height: usize) -> f64 {
    [
        Pt::new(0.5, 0.5),
        Pt::new(width as f64 - 0.5, 0.5),
        Pt::new(0.5, height as f64 - 0.5),
        Pt::new(width as f64 - 0.5, height as f64 - 0.5),
    ]
    .into_iter()
    .map(|point| point.dist(center))
    .fold(0.0, f64::max)
}

fn offsets_radial(width: usize, height: usize, center: Pt, radius: f64) -> Vec<f64> {
    (0..width * height)
        .map(|index| {
            let point = Pt::new((index % width) as f64 + 0.5, (index / width) as f64 + 0.5);
            (point.dist(center) / radius).clamp(0.0, 1.0)
        })
        .collect()
}

fn profile(colors: &[LinearRgb], offsets: &[f64]) -> Vec<LinearRgb> {
    let mut sums = vec![[0.0; 3]; PROFILE_BINS];
    let mut counts = vec![0u64; PROFILE_BINS];
    for (color, offset) in colors.iter().zip(offsets) {
        let bin = (offset * (PROFILE_BINS - 1) as f64).round() as usize;
        for (slot, value) in sums[bin].iter_mut().zip(color.components()) {
            *slot += value;
        }
        counts[bin] += 1;
    }
    let mut output = vec![LinearRgb::new(0.0, 0.0, 0.0); PROFILE_BINS];
    for bin in 0..PROFILE_BINS {
        if counts[bin] > 0 {
            output[bin] = LinearRgb::new(
                sums[bin][0] / counts[bin] as f64,
                sums[bin][1] / counts[bin] as f64,
                sums[bin][2] / counts[bin] as f64,
            );
        } else {
            let nearest = (0..PROFILE_BINS)
                .filter(|other| counts[*other] > 0)
                .min_by_key(|other| other.abs_diff(bin))
                .unwrap_or(0);
            output[bin] = if counts[nearest] == 0 {
                mean_color(colors)
            } else {
                LinearRgb::new(
                    sums[nearest][0] / counts[nearest] as f64,
                    sums[nearest][1] / counts[nearest] as f64,
                    sums[nearest][2] / counts[nearest] as f64,
                )
            };
        }
    }
    output
}

fn discontinuities(profile: &[LinearRgb]) -> Vec<usize> {
    let deltas = profile
        .windows(2)
        .map(|pair| color_distance(pair[0], pair[1]))
        .collect::<Vec<_>>();
    let mut sorted = deltas.clone();
    sorted.sort_by(f64::total_cmp);
    let threshold = (sorted[sorted.len() / 2] * 6.0).max(0.08);
    deltas
        .iter()
        .enumerate()
        .filter_map(|(index, delta)| (*delta > threshold).then_some(index + 1))
        .take(4)
        .collect()
}

fn stops(profile: &[LinearRgb], jumps: &[usize], budget: usize) -> Vec<GradientStop> {
    if budget >= 4 {
        if let Some(&jump) = jumps.first() {
            let offset = round_offset((jump as f64 - 0.5) / (PROFILE_BINS - 1) as f64);
            return vec![
                GradientStop {
                    offset: 0.0,
                    color: endpoint_color(profile, 0.0),
                },
                GradientStop {
                    offset,
                    color: profile[jump - 1],
                },
                GradientStop {
                    offset,
                    color: profile[jump],
                },
                GradientStop {
                    offset: 1.0,
                    color: endpoint_color(profile, 1.0),
                },
            ];
        }
    }
    if budget == 2 {
        let (first, last) = regression_endpoints(profile);
        return vec![
            GradientStop {
                offset: 0.0,
                color: first,
            },
            GradientStop {
                offset: 1.0,
                color: last,
            },
        ];
    }
    (0..budget)
        .map(|index| {
            let offset = index as f64 / (budget - 1) as f64;
            GradientStop {
                offset: round_offset(offset),
                color: endpoint_color(profile, offset),
            }
        })
        .collect()
}

fn regression_endpoints(profile: &[LinearRgb]) -> (LinearRgb, LinearRgb) {
    let mean_x = 0.5;
    let variance_x = profile
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let x = index as f64 / (profile.len() - 1) as f64;
            (x - mean_x) * (x - mean_x)
        })
        .sum::<f64>();
    let mean = mean_color(profile).components();
    let mut slope = [0.0; 3];
    for (index, color) in profile.iter().enumerate() {
        let x = index as f64 / (profile.len() - 1) as f64;
        for channel in 0..3 {
            slope[channel] += (x - mean_x) * (color.components()[channel] - mean[channel]);
        }
    }
    for value in &mut slope {
        *value /= variance_x;
    }
    let endpoint = |sign: f64| {
        LinearRgb::new(
            (mean[0] + sign * 0.5 * slope[0]).clamp(0.0, 1.0),
            (mean[1] + sign * 0.5 * slope[1]).clamp(0.0, 1.0),
            (mean[2] + sign * 0.5 * slope[2]).clamp(0.0, 1.0),
        )
    };
    (endpoint(-1.0), endpoint(1.0))
}

fn endpoint_color(profile: &[LinearRgb], offset: f64) -> LinearRgb {
    let position = offset * (profile.len() - 1) as f64;
    let left = position.floor() as usize;
    let right = position.ceil() as usize;
    if left == right {
        return profile[left];
    }
    let t = position - left as f64;
    let a = profile[left];
    let b = profile[right];
    LinearRgb::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}

fn mean_color(colors: &[LinearRgb]) -> LinearRgb {
    let sum = colors.iter().fold([0.0; 3], |mut sum, color| {
        for (slot, value) in sum.iter_mut().zip(color.components()) {
            *slot += value;
        }
        sum
    });
    LinearRgb::new(
        sum[0] / colors.len() as f64,
        sum[1] / colors.len() as f64,
        sum[2] / colors.len() as f64,
    )
}

fn color_distance(a: LinearRgb, b: LinearRgb) -> f64 {
    let delta = [a.r - b.r, a.g - b.g, a.b - b.b];
    delta.iter().map(|value| value * value).sum::<f64>().sqrt()
}

fn round_64(value: f64) -> f64 {
    (value * 64.0).round() / 64.0
}

fn round_point_64(point: Pt) -> Pt {
    Pt::new(round_64(point.x), round_64(point.y))
}

fn round_offset(value: f64) -> f64 {
    (value * 4096.0).round() / 4096.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use vice_image::IccAssumption;

    #[test]
    fn a_hard_linear_step_produces_duplicate_stops() {
        let mut rgba = Vec::new();
        for _y in 0..8 {
            for x in 0..32 {
                let value = if x < 16 { 0 } else { 255 };
                rgba.extend_from_slice(&[value, 0, 255 - value, 255]);
            }
        }
        let image = CanonicalImage::from_straight_srgb8(
            32,
            8,
            rgba,
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap();
        let proposal = propose_gradients(&image).unwrap();
        assert!(proposal.report.detected_linear_discontinuities > 0);
        assert!(proposal
            .candidates
            .iter()
            .any(|candidate| match &candidate.scene().paint {
                GradientPaint::Linear { stops, .. } => stops
                    .windows(2)
                    .any(|pair| pair[0].offset == pair[1].offset),
                _ => false,
            }));
        let identities = proposal
            .candidates
            .iter()
            .map(vice_ir::gradient_scene_digest_sha256)
            .collect::<Result<BTreeSet<_>, _>>()
            .unwrap();
        assert_eq!(identities.len(), proposal.candidates.len());
        assert_eq!(
            proposal.report.candidate_count as usize,
            proposal.candidates.len()
        );
    }

    #[test]
    fn nonopaque_pixels_are_a_typed_refusal() {
        let image = CanonicalImage::from_straight_srgb8(
            1,
            1,
            vec![255, 0, 0, 128],
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap();
        assert_eq!(
            propose_gradients(&image),
            Err(GradientEvidenceRefusal::NonOpaqueSource)
        );
    }
}
