//! Bounded JPEG and WebP decode implementations.

use super::*;

pub(super) fn decode_jpeg(
    bytes: &[u8],
    limits: &DecodeLimits,
) -> Result<CanonicalImage, ImageError> {
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;

    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGBA)
        .set_max_width(limits.max_dimension_px as usize)
        .set_max_height(limits.max_dimension_px as usize)
        .set_use_unsafe(false)
        .set_strict_mode(false);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(bytes, options);
    decoder.decode_headers().map_err(|error| ImageError::Jpeg {
        detail: error.to_string(),
    })?;
    let (width, height) = decoder.dimensions().ok_or_else(|| ImageError::Jpeg {
        detail: "decoder returned no dimensions".into(),
    })?;
    let width_px = u32::try_from(width).map_err(|_| ImageError::DimensionTooLarge {
        width: u32::MAX,
        height: u32::try_from(height).unwrap_or(u32::MAX),
        limit: limits.max_dimension_px,
    })?;
    let height_px = u32::try_from(height).map_err(|_| ImageError::DimensionTooLarge {
        width: width_px,
        height: u32::MAX,
        limit: limits.max_dimension_px,
    })?;
    check_dimensions(width_px, height_px, limits)?;
    let icc = if decoder.icc_profile().is_some() {
        IccAssumption::IccProfilePresentAssumedSrgb
    } else {
        IccAssumption::NoProfileAssumedSrgb
    };
    let srgb8 = decoder.decode().map_err(|error| ImageError::Jpeg {
        detail: error.to_string(),
    })?;
    CanonicalImage::from_decoded(
        width_px,
        height_px,
        srgb8,
        false,
        icc,
        bytes,
        EncodedImageFormat::Jpeg,
    )
}

pub(super) fn decode_webp(
    bytes: &[u8],
    limits: &DecodeLimits,
) -> Result<CanonicalImage, ImageError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut decoder = image_webp::WebPDecoder::new(cursor).map_err(|error| ImageError::Webp {
        detail: error.to_string(),
    })?;
    decoder.set_memory_limit(limits.max_encoded_bytes as usize);
    if decoder.is_animated() {
        return Err(ImageError::AnimatedWebp);
    }
    let (width_px, height_px) = decoder.dimensions();
    check_dimensions(width_px, height_px, limits)?;
    let source_had_alpha = decoder.has_alpha();
    let lossy = decoder.is_lossy();
    let icc = if decoder
        .icc_profile()
        .map_err(|error| ImageError::Webp {
            detail: error.to_string(),
        })?
        .is_some()
    {
        IccAssumption::IccProfilePresentAssumedSrgb
    } else {
        IccAssumption::NoProfileAssumedSrgb
    };
    let size = decoder
        .output_buffer_size()
        .ok_or(ImageError::TooManyPixels {
            width: width_px,
            height: height_px,
            pixels: u64::from(width_px) * u64::from(height_px),
            limit: limits.max_pixels,
        })?;
    let mut decoded = vec![0; size];
    decoder
        .read_image(&mut decoded)
        .map_err(|error| ImageError::Webp {
            detail: error.to_string(),
        })?;
    let srgb8 = if source_had_alpha {
        decoded
    } else {
        rgb_to_rgba(&decoded)
    };
    CanonicalImage::from_decoded(
        width_px,
        height_px,
        srgb8,
        source_had_alpha,
        icc,
        bytes,
        if lossy {
            EncodedImageFormat::WebpLossy
        } else {
            EncodedImageFormat::WebpLossless
        },
    )
}
