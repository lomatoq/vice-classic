use vice_image::CanonicalImage;
use vice_ir::color::srgb_u8_to_linear;
use vice_ir::{LinearRgb, Paint};

use super::LineArtRefusal;

pub(super) struct MaskEvidence {
    pub width: usize,
    pub height: usize,
    pub threshold: u8,
    pub foreground: Vec<bool>,
    pub distance_squared: Vec<f64>,
    pub foreground_paint: Paint,
    pub background_paint: Paint,
}

pub(super) fn measure(image: &CanonicalImage) -> Result<MaskEvidence, LineArtRefusal> {
    let width = image.width_px() as usize;
    let height = image.height_px() as usize;
    let border = image.border_indices();
    let background_rgba = channel_medians(image, &border);
    let background_premul = premul_codes(background_rgba);
    let mut distances = Vec::with_capacity(image.pixel_count());
    let mut histogram = [0u64; 256];
    for index in 0..image.pixel_count() {
        let pixel = premul_codes(image.pixel(index));
        let sum = pixel
            .iter()
            .zip(background_premul)
            .map(|(a, b)| {
                let delta = f64::from(*a) - f64::from(b);
                delta * delta
            })
            .sum::<f64>();
        let distance = (0.5 * sum.sqrt()).round().clamp(0.0, 255.0) as u8;
        histogram[distance as usize] += 1;
        distances.push(distance);
    }
    if histogram[0] == image.pixel_count() as u64 {
        return Err(LineArtRefusal::NoContrast);
    }
    let threshold = otsu_threshold(&histogram);
    let foreground = distances
        .iter()
        .map(|distance| *distance > threshold)
        .collect::<Vec<_>>();
    let count = foreground.iter().filter(|pixel| **pixel).count();
    if count == 0 || count == foreground.len() {
        return Err(LineArtRefusal::DegenerateForeground);
    }
    let foreground_indices = foreground
        .iter()
        .enumerate()
        .filter_map(|(index, value)| value.then_some(index))
        .collect::<Vec<_>>();
    let foreground_rgba = channel_medians(image, &foreground_indices);
    let foreground_paint = Paint::OpaqueSolid(LinearRgb::new(
        srgb_u8_to_linear(foreground_rgba[0]),
        srgb_u8_to_linear(foreground_rgba[1]),
        srgb_u8_to_linear(foreground_rgba[2]),
    ));
    let background_paint = if background_rgba[3] == 0 {
        Paint::TransparentExterior
    } else {
        Paint::OpaqueSolid(LinearRgb::new(
            srgb_u8_to_linear(background_rgba[0]),
            srgb_u8_to_linear(background_rgba[1]),
            srgb_u8_to_linear(background_rgba[2]),
        ))
    };
    let distance_squared = squared_distance_transform(&foreground, width, height);
    Ok(MaskEvidence {
        width,
        height,
        threshold,
        foreground,
        distance_squared,
        foreground_paint,
        background_paint,
    })
}

fn channel_medians(image: &CanonicalImage, indices: &[usize]) -> [u8; 4] {
    let mut channels = [
        Vec::with_capacity(indices.len()),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    ];
    for &index in indices {
        let pixel = image.pixel(index);
        for channel in 0..4 {
            channels[channel].push(pixel[channel]);
        }
    }
    let mut output = [0; 4];
    for channel in 0..4 {
        channels[channel].sort_unstable();
        output[channel] = channels[channel][channels[channel].len() / 2];
    }
    output
}

fn premul_codes(pixel: [u8; 4]) -> [u8; 4] {
    let alpha = u16::from(pixel[3]);
    [
        ((u16::from(pixel[0]) * alpha + 127) / 255) as u8,
        ((u16::from(pixel[1]) * alpha + 127) / 255) as u8,
        ((u16::from(pixel[2]) * alpha + 127) / 255) as u8,
        pixel[3],
    ]
}

fn otsu_threshold(histogram: &[u64; 256]) -> u8 {
    let total = histogram.iter().sum::<u64>();
    let total_sum = histogram
        .iter()
        .enumerate()
        .map(|(value, count)| value as f64 * *count as f64)
        .sum::<f64>();
    let mut background_count = 0u64;
    let mut background_sum = 0.0;
    let mut best_threshold = 0u8;
    let mut best_variance = -1.0;
    for (threshold, count) in histogram.iter().enumerate().take(255) {
        background_count += count;
        background_sum += threshold as f64 * *count as f64;
        let foreground_count = total - background_count;
        if background_count == 0 || foreground_count == 0 {
            continue;
        }
        let mean_background = background_sum / background_count as f64;
        let mean_foreground = (total_sum - background_sum) / foreground_count as f64;
        let variance = background_count as f64
            * foreground_count as f64
            * (mean_background - mean_foreground).powi(2);
        if variance > best_variance {
            best_variance = variance;
            best_threshold = threshold as u8;
        }
    }
    best_threshold
}

fn squared_distance_transform(mask: &[bool], width: usize, height: usize) -> Vec<f64> {
    const FAR: f64 = 1e20;
    let mut vertical = vec![0.0; mask.len()];
    for x in 0..width {
        let column = (0..height)
            .map(|y| if mask[y * width + x] { FAR } else { 0.0 })
            .collect::<Vec<_>>();
        let transformed = edt_1d(&column);
        for y in 0..height {
            vertical[y * width + x] = transformed[y];
        }
    }
    let mut output = vec![0.0; mask.len()];
    for y in 0..height {
        let transformed = edt_1d(&vertical[y * width..(y + 1) * width]);
        output[y * width..(y + 1) * width].copy_from_slice(&transformed);
    }
    output
}

fn edt_1d(values: &[f64]) -> Vec<f64> {
    let n = values.len();
    let mut sites = vec![0usize; n];
    let mut boundaries = vec![0.0; n + 1];
    let mut k = 0usize;
    sites[0] = 0;
    boundaries[0] = f64::NEG_INFINITY;
    boundaries[1] = f64::INFINITY;
    for q in 1..n {
        let mut intersection;
        loop {
            let p = sites[k];
            intersection = ((values[q] + (q * q) as f64) - (values[p] + (p * p) as f64))
                / (2.0 * (q as f64 - p as f64));
            if intersection > boundaries[k] || k == 0 {
                break;
            }
            k -= 1;
        }
        if intersection <= boundaries[k] {
            k = 0;
        } else {
            k += 1;
        }
        sites[k] = q;
        boundaries[k] = intersection;
        boundaries[k + 1] = f64::INFINITY;
    }
    k = 0;
    (0..n)
        .map(|q| {
            while boundaries[k + 1] < q as f64 {
                k += 1;
            }
            let delta = q as f64 - sites[k] as f64;
            delta * delta + values[sites[k]]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_transform_reads_the_physical_halfwidth() {
        let mask = vec![false, true, true, true, false];
        let distance = squared_distance_transform(&mask, 5, 1);
        assert_eq!(distance, vec![0.0, 1.0, 4.0, 1.0, 0.0]);
    }

    #[test]
    fn otsu_separates_a_two_level_population() {
        let mut histogram = [0; 256];
        histogram[0] = 90;
        histogram[200] = 10;
        assert!(otsu_threshold(&histogram) < 200);
    }
}
