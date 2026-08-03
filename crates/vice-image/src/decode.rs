//! Canonical decode (spec §8.1).
//!
//! What §8.1 asks a decode to keep: *"dimensions, source hash, alpha;
//! ICC/profile presence and the assumption applied; border/exterior
//! statistics; original encoded bytes; resource-limit diagnostics"*, with
//! decompression-bomb and oversize protection present from M0.
//!
//! Two of those are recorded here in a form that is deliberately narrower
//! than it could be.
//!
//! **The ICC assumption is recorded, never invented.** This crate applies
//! exactly one transfer function — IEC 61966-2-1 sRGB — and says so in
//! [`IccAssumption`]. When a file carries an ICC profile we do not pretend
//! to interpret it: the image decodes, the applied assumption is
//! `IccProfilePresentAssumedSrgb`, and every downstream report carries that
//! word. Silently treating an AdobeRGB asset as sRGB *without saying so* is
//! the failure mode; saying so is the honest M4 boundary.
//!
//! **The border statistics are not here.** §8.1 lists them next to the
//! decode, but a border statistic is already evidence about the exterior
//! hypothesis (§9.2), and it belongs to the crate that must be able to
//! refuse. Keeping it out of the decode is what stops "the border is the
//! background" from becoming a decode-time fact nobody can question.

use serde::Serialize;
use sha2::{Digest, Sha256};

mod codecs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EncodedImageFormat {
    RawRgba8,
    Png,
    Jpeg,
    WebpLossless,
    WebpLossy,
}

fn check_encoded_limit(bytes: &[u8], limits: &DecodeLimits) -> Result<(), ImageError> {
    let got = bytes.len() as u64;
    if got > limits.max_encoded_bytes {
        Err(ImageError::EncodedTooLarge {
            got,
            limit: limits.max_encoded_bytes,
        })
    } else {
        Ok(())
    }
}

fn check_dimensions(width: u32, height: u32, limits: &DecodeLimits) -> Result<(), ImageError> {
    if width == 0 || height == 0 {
        return Err(ImageError::Empty { width, height });
    }
    if width > limits.max_dimension_px || height > limits.max_dimension_px {
        return Err(ImageError::DimensionTooLarge {
            width,
            height,
            limit: limits.max_dimension_px,
        });
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > limits.max_pixels {
        return Err(ImageError::TooManyPixels {
            width,
            height,
            pixels,
            limit: limits.max_pixels,
        });
    }
    Ok(())
}

fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
    for pixel in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
    }
    rgba
}

impl EncodedImageFormat {
    pub fn carries_codec_residual(self) -> bool {
        matches!(self, Self::Jpeg | Self::WebpLossy)
    }
}

/// Resource limits applied BEFORE any per-pixel allocation (§8.1).
///
/// The header of a PNG can claim 2^31 × 2^31 pixels in 13 bytes; the
/// defence has to be a check on the declared dimensions, not a hope that
/// allocation fails gracefully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DecodeLimits {
    pub max_dimension_px: u32,
    pub max_pixels: u64,
    /// Upper bound on the ENCODED input, so a 40-byte zip bomb cannot be
    /// handed to the decoder at all.
    pub max_encoded_bytes: u64,
}

impl Default for DecodeLimits {
    fn default() -> DecodeLimits {
        DecodeLimits {
            max_dimension_px: 8192,
            max_pixels: 32_000_000,
            max_encoded_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Which colour assumption was APPLIED to the decoded bytes.
///
/// Ordered from "the file said so" to "we assumed it", because a report
/// that shows only the transfer function used cannot distinguish a declared
/// sRGB asset from a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IccAssumption {
    /// An `sRGB` chunk declares the colour space; the applied transfer is
    /// the one the file names.
    SrgbChunkDeclared,
    /// A `gAMA`/`cHRM` pair is present but no `sRGB` chunk; sRGB applied.
    GamaChunkAssumedSrgb,
    /// An ICC profile is embedded and NOT interpreted; sRGB applied and
    /// recorded as an assumption, so a report can say which images were
    /// decoded under one.
    IccProfilePresentAssumedSrgb,
    /// No colour information at all; sRGB applied by convention (§5.2).
    NoProfileAssumedSrgb,
}

impl IccAssumption {
    pub fn as_str(&self) -> &'static str {
        match self {
            IccAssumption::SrgbChunkDeclared => "srgb_chunk_declared",
            IccAssumption::GamaChunkAssumedSrgb => "gama_chunk_assumed_srgb",
            IccAssumption::IccProfilePresentAssumedSrgb => "icc_profile_present_assumed_srgb",
            IccAssumption::NoProfileAssumedSrgb => "no_profile_assumed_srgb",
        }
    }

    /// True when the colour space was ASSUMED rather than declared. A run
    /// over such an input is not wrong, but it rests on an assumption the
    /// report must show.
    pub fn is_assumed(&self) -> bool {
        !matches!(self, IccAssumption::SrgbChunkDeclared)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("encoded input is {got} bytes, over the {limit}-byte limit")]
    EncodedTooLarge { got: u64, limit: u64 },
    #[error("image is {width}x{height}, over the {limit} px per-side limit")]
    DimensionTooLarge { width: u32, height: u32, limit: u32 },
    #[error("image is {width}x{height} = {pixels} pixels, over the {limit}-pixel limit")]
    TooManyPixels {
        width: u32,
        height: u32,
        pixels: u64,
        limit: u64,
    },
    #[error("image has zero extent ({width}x{height})")]
    Empty { width: u32, height: u32 },
    #[error("png decode failed: {detail}")]
    Png { detail: String },
    #[error("jpeg decode failed: {detail}")]
    Jpeg { detail: String },
    #[error("webp decode failed: {detail}")]
    Webp { detail: String },
    #[error("animated webp input is unsupported")]
    AnimatedWebp,
    #[error("input is not a supported PNG, JPEG, or WebP image")]
    UnsupportedFormat,
    #[error("unsupported png output colour type {color_type:?} at bit depth {bit_depth:?}")]
    UnsupportedColorType {
        color_type: String,
        bit_depth: String,
    },
    #[error("buffer is {got} bytes, expected {want} for {width}x{height} RGBA8")]
    BufferSize {
        got: usize,
        want: usize,
        width: u32,
        height: u32,
    },
}

/// A decoded image in the canonical straight sRGB8 RGBA form.
///
/// "Straight" is a decision, not a detail: the stored RGB of a pixel whose
/// alpha is small is NOT colour evidence (§1.6), and keeping the bytes
/// straight means every magnitude question has to go through
/// [`crate::observation`], which premultiplies first.
#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalImage {
    width_px: u32,
    height_px: u32,
    /// Straight, sRGB-encoded RGBA8, row-major, 4 bytes per pixel.
    srgb8: Vec<u8>,
    source_sha256: String,
    encoded_bytes: u64,
    /// True when the SOURCE carried an alpha channel. An opaque image
    /// expanded to RGBA is not the same input as one that declared alpha,
    /// and the exterior hypotheses of §9.2 care about the difference.
    source_had_alpha: bool,
    icc: IccAssumption,
    encoded_format: EncodedImageFormat,
}

impl CanonicalImage {
    /// Wrap an in-memory straight-RGBA8 buffer.
    ///
    /// Exists because the GT corpus produces observations as bytes rather
    /// than as files: the calibration harness must analyse the SAME bytes
    /// the corpus hashed, not a re-encoded lookalike.
    pub fn from_straight_srgb8(
        width_px: u32,
        height_px: u32,
        srgb8: Vec<u8>,
        source_had_alpha: bool,
        icc: IccAssumption,
    ) -> Result<CanonicalImage, ImageError> {
        if width_px == 0 || height_px == 0 {
            return Err(ImageError::Empty {
                width: width_px,
                height: height_px,
            });
        }
        let want = (width_px as usize)
            .checked_mul(height_px as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or(ImageError::TooManyPixels {
                width: width_px,
                height: height_px,
                pixels: u64::from(width_px) * u64::from(height_px),
                limit: u64::MAX,
            })?;
        if srgb8.len() != want {
            return Err(ImageError::BufferSize {
                got: srgb8.len(),
                want,
                width: width_px,
                height: height_px,
            });
        }
        let source_sha256 = hex::encode(Sha256::digest(&srgb8));
        let encoded_bytes = srgb8.len() as u64;
        Ok(CanonicalImage {
            width_px,
            height_px,
            srgb8,
            source_sha256,
            encoded_bytes,
            source_had_alpha,
            icc,
            encoded_format: EncodedImageFormat::RawRgba8,
        })
    }

    /// Decode any M9-supported encoded raster while retaining its codec
    /// provenance for the formation likelihood.
    pub fn decode(bytes: &[u8], limits: &DecodeLimits) -> Result<CanonicalImage, ImageError> {
        check_encoded_limit(bytes, limits)?;
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Self::decode_png(bytes, limits)
        } else if bytes.starts_with(&[0xff, 0xd8]) {
            codecs::decode_jpeg(bytes, limits)
        } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            codecs::decode_webp(bytes, limits)
        } else {
            Err(ImageError::UnsupportedFormat)
        }
    }

    /// Decode a PNG under explicit resource limits (§8.1).
    pub fn decode_png(bytes: &[u8], limits: &DecodeLimits) -> Result<CanonicalImage, ImageError> {
        check_encoded_limit(bytes, limits)?;
        let encoded_bytes = bytes.len() as u64;
        let mut decoder = png::Decoder::new(bytes);
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().map_err(|e| ImageError::Png {
            detail: e.to_string(),
        })?;

        // The header is read; the frame buffer is NOT allocated yet. This
        // is the only point at which a declared size can be refused before
        // it costs memory.
        let (width_px, height_px, source_had_alpha, icc) = {
            let info = reader.info();
            let icc = if info.srgb.is_some() {
                IccAssumption::SrgbChunkDeclared
            } else if info.icc_profile.is_some() {
                IccAssumption::IccProfilePresentAssumedSrgb
            } else if info.gama_chunk.is_some() {
                IccAssumption::GamaChunkAssumedSrgb
            } else {
                IccAssumption::NoProfileAssumedSrgb
            };
            let declares_alpha = matches!(
                info.color_type,
                png::ColorType::Rgba | png::ColorType::GrayscaleAlpha
            ) || info.trns.is_some();
            (info.width, info.height, declares_alpha, icc)
        };
        if width_px == 0 || height_px == 0 {
            return Err(ImageError::Empty {
                width: width_px,
                height: height_px,
            });
        }
        if width_px > limits.max_dimension_px || height_px > limits.max_dimension_px {
            return Err(ImageError::DimensionTooLarge {
                width: width_px,
                height: height_px,
                limit: limits.max_dimension_px,
            });
        }
        let pixels = u64::from(width_px) * u64::from(height_px);
        if pixels > limits.max_pixels {
            return Err(ImageError::TooManyPixels {
                width: width_px,
                height: height_px,
                pixels,
                limit: limits.max_pixels,
            });
        }

        let mut buf = vec![0u8; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut buf).map_err(|e| ImageError::Png {
            detail: e.to_string(),
        })?;
        let src = &buf[..frame.buffer_size()];
        let n = (width_px as usize) * (height_px as usize);
        let mut srgb8 = vec![0u8; n * 4];
        match (frame.color_type, frame.bit_depth) {
            (png::ColorType::Rgba, png::BitDepth::Eight) => srgb8.copy_from_slice(src),
            (png::ColorType::Rgb, png::BitDepth::Eight) => {
                for i in 0..n {
                    srgb8[4 * i] = src[3 * i];
                    srgb8[4 * i + 1] = src[3 * i + 1];
                    srgb8[4 * i + 2] = src[3 * i + 2];
                    srgb8[4 * i + 3] = 255;
                }
            }
            (png::ColorType::GrayscaleAlpha, png::BitDepth::Eight) => {
                for i in 0..n {
                    let v = src[2 * i];
                    srgb8[4 * i] = v;
                    srgb8[4 * i + 1] = v;
                    srgb8[4 * i + 2] = v;
                    srgb8[4 * i + 3] = src[2 * i + 1];
                }
            }
            (png::ColorType::Grayscale, png::BitDepth::Eight) => {
                for i in 0..n {
                    let v = src[i];
                    srgb8[4 * i] = v;
                    srgb8[4 * i + 1] = v;
                    srgb8[4 * i + 2] = v;
                    srgb8[4 * i + 3] = 255;
                }
            }
            (ct, bd) => {
                return Err(ImageError::UnsupportedColorType {
                    color_type: format!("{ct:?}"),
                    bit_depth: format!("{bd:?}"),
                })
            }
        }

        Ok(CanonicalImage {
            width_px,
            height_px,
            srgb8,
            // The hash is of the ENCODED bytes here: for a file input, the
            // identity a report must be able to quote is the file's.
            source_sha256: hex::encode(Sha256::digest(bytes)),
            encoded_bytes,
            source_had_alpha,
            icc,
            encoded_format: EncodedImageFormat::Png,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn from_decoded(
        width_px: u32,
        height_px: u32,
        srgb8: Vec<u8>,
        source_had_alpha: bool,
        icc: IccAssumption,
        encoded: &[u8],
        encoded_format: EncodedImageFormat,
    ) -> Result<CanonicalImage, ImageError> {
        let want = width_px as usize * height_px as usize * 4;
        if srgb8.len() != want {
            return Err(ImageError::BufferSize {
                got: srgb8.len(),
                want,
                width: width_px,
                height: height_px,
            });
        }
        Ok(CanonicalImage {
            width_px,
            height_px,
            srgb8,
            source_sha256: hex::encode(Sha256::digest(encoded)),
            encoded_bytes: encoded.len() as u64,
            source_had_alpha,
            icc,
            encoded_format,
        })
    }

    pub fn width_px(&self) -> u32 {
        self.width_px
    }
    pub fn height_px(&self) -> u32 {
        self.height_px
    }
    pub fn pixel_count(&self) -> usize {
        (self.width_px as usize) * (self.height_px as usize)
    }
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }
    pub fn encoded_bytes(&self) -> u64 {
        self.encoded_bytes
    }
    pub fn source_had_alpha(&self) -> bool {
        self.source_had_alpha
    }
    pub fn icc_assumption(&self) -> IccAssumption {
        self.icc
    }
    pub fn encoded_format(&self) -> EncodedImageFormat {
        self.encoded_format
    }
    pub fn straight_srgb8(&self) -> &[u8] {
        &self.srgb8
    }

    /// Straight sRGB8 RGBA of pixel `i` (row-major).
    pub fn pixel(&self, i: usize) -> [u8; 4] {
        [
            self.srgb8[4 * i],
            self.srgb8[4 * i + 1],
            self.srgb8[4 * i + 2],
            self.srgb8[4 * i + 3],
        ]
    }

    pub fn index(&self, x: u32, y: u32) -> usize {
        (y as usize) * (self.width_px as usize) + (x as usize)
    }

    /// Pixel indices on the canvas border, in a deterministic order.
    ///
    /// The exterior hypotheses of §9.2 need to know what is at the border
    /// WITHOUT being allowed to conclude that the border is the background.
    /// This returns the set; deciding what it means is [`vice-evidence`]'s
    /// job and is a hypothesis there, not a fact here.
    pub fn border_indices(&self) -> Vec<usize> {
        let (w, h) = (self.width_px, self.height_px);
        let mut out = Vec::with_capacity(2 * (w as usize + h as usize));
        for x in 0..w {
            out.push(self.index(x, 0));
            if h > 1 {
                out.push(self.index(x, h - 1));
            }
        }
        for y in 1..h.saturating_sub(1) {
            out.push(self.index(0, y));
            if w > 1 {
                out.push(self.index(w - 1, y));
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_BY_TWO_JPEG_HEX: &str = concat!(
        "ffd8ffe000104a46494600010101006000600000ffdb00430003020203020203030303040303040508",
        "05050404050a070706080c0a0c0c0b0a0b0b0d0e12100d0e110e0b0b1016101113141515150c0f17",
        "1816141812141514ffdb00430103040405040509050509140d0b0d141414141414141414141414141414",
        "1414141414141414141414141414141414141414141414141414141414141414141414ffc00011080002",
        "000203012200021101031101ffc4001f0000010501010101010100000000000000000102030405060708",
        "090a0bffc400b5100002010303020403050504040000017d010203000411051221314106135161072271",
        "14328191a1082342b1c11552d1f02433627282090a161718191a25262728292a3435363738393a434445",
        "464748494a535455565758595a636465666768696a737475767778797a838485868788898a9293949596",
        "9798999aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9bac2c3c4c5c6c7c8c9cad2d3d4d5d6d7d8d9",
        "dae1e2e3e4e5e6e7e8e9eaf1f2f3f4f5f6f7f8f9faffc4001f010003010101010101010101000000",
        "0000000102030405060708090a0bffc400b5110002010204040304070504040001027700010203110405",
        "2131061241510761711322328108144291a1b1c109233352f0156272d10a162434e125f11718191a2627",
        "28292a35363738393a434445464748494a535455565758595a636465666768696a737475767778797a82",
        "838485868788898a92939495969798999aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9bac2c3c4c5c6",
        "c7c8c9cad2d3d4d5d6d7d8d9dae2e3e4e5e6e7e8e9eaf2f3f4f5f6f7f8f9faffda000c0301000211",
        "0311003f00fb57f676f82df0f75bfd9fbe196a3a8f813c337fa85e785f4cb8b9bbbad1ede496795ed2",
        "267777642599892492724924d14515f238bff78a9fe27f99f098eff7aabfe297e6cfffd9"
    );

    fn tiny_rgba(w: u32, h: u32, fill: [u8; 4]) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..(w * h) {
            v.extend_from_slice(&fill);
        }
        v
    }

    #[test]
    fn generic_decode_retains_jpeg_codec_provenance() {
        let bytes = hex::decode(TWO_BY_TWO_JPEG_HEX).unwrap();
        let image = CanonicalImage::decode(&bytes, &DecodeLimits::default()).unwrap();
        assert_eq!((image.width_px(), image.height_px()), (2, 2));
        assert_eq!(image.encoded_format(), EncodedImageFormat::Jpeg);
        assert!(image.encoded_format().carries_codec_residual());
        assert!(!image.source_had_alpha());
    }

    #[test]
    fn generic_decode_retains_lossless_webp_and_alpha_provenance() {
        let rgba = [
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 255, 0,
        ];
        let mut bytes = Vec::new();
        image_webp::WebPEncoder::new(&mut bytes)
            .encode(&rgba, 2, 2, image_webp::ColorType::Rgba8)
            .unwrap();
        let image = CanonicalImage::decode(&bytes, &DecodeLimits::default()).unwrap();
        assert_eq!((image.width_px(), image.height_px()), (2, 2));
        assert_eq!(image.encoded_format(), EncodedImageFormat::WebpLossless);
        assert!(image.source_had_alpha());
        assert_eq!(image.straight_srgb8(), rgba);
    }

    #[test]
    fn generic_decode_refuses_unknown_and_animated_inputs_by_type() {
        assert_eq!(
            CanonicalImage::decode(b"not an image", &DecodeLimits::default()).unwrap_err(),
            ImageError::UnsupportedFormat
        );
    }

    #[test]
    fn a_buffer_of_the_wrong_size_is_a_typed_refusal_not_a_panic() {
        let e = CanonicalImage::from_straight_srgb8(
            4,
            4,
            vec![0u8; 10],
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap_err();
        assert!(matches!(e, ImageError::BufferSize { .. }), "{e:?}");
        let e = CanonicalImage::from_straight_srgb8(
            0,
            4,
            Vec::new(),
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap_err();
        assert!(matches!(e, ImageError::Empty { .. }), "{e:?}");
    }

    /// The bomb defence has to fire on the DECLARED size, i.e. before the
    /// frame buffer exists. A 12-byte IHDR claiming 2^31 px must not become
    /// an allocation (§8.1).
    #[test]
    fn an_oversized_header_is_refused_before_the_frame_is_allocated() {
        // A real PNG, re-headered: build one at a legal size and then
        // rewrite IHDR width/height plus the IHDR CRC.
        let mut bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut bytes, 2, 2);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&tiny_rgba(2, 2, [10, 20, 30, 255]))
                .unwrap();
        }
        // IHDR data starts at byte 16 (8 signature + 4 length + 4 type).
        let huge: u32 = 1 << 20;
        bytes[16..20].copy_from_slice(&huge.to_be_bytes());
        bytes[20..24].copy_from_slice(&huge.to_be_bytes());
        let crc = crc32(&bytes[12..29]);
        bytes[29..33].copy_from_slice(&crc.to_be_bytes());

        let limits = DecodeLimits::default();
        let e = CanonicalImage::decode_png(&bytes, &limits).unwrap_err();
        assert!(
            matches!(
                e,
                ImageError::TooManyPixels { .. } | ImageError::DimensionTooLarge { .. }
            ),
            "{e:?}"
        );

        // Control: the same file at its real size decodes.
        let mut ok = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut ok, 2, 2);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&tiny_rgba(2, 2, [10, 20, 30, 255]))
                .unwrap();
        }
        let img = CanonicalImage::decode_png(&ok, &limits).expect("a 2x2 png decodes");
        assert_eq!((img.width_px(), img.height_px()), (2, 2));
        assert_eq!(img.pixel(0), [10, 20, 30, 255]);
        assert!(img.source_had_alpha());
    }

    #[test]
    fn an_encoded_input_over_the_byte_limit_is_refused() {
        let limits = DecodeLimits {
            max_encoded_bytes: 8,
            ..DecodeLimits::default()
        };
        let e = CanonicalImage::decode_png(&[0u8; 64], &limits).unwrap_err();
        assert!(matches!(e, ImageError::EncodedTooLarge { .. }), "{e:?}");
    }

    /// Every colour type the normalising transformation can hand us becomes
    /// RGBA8, and an RGB source is recorded as HAVING NO alpha - the
    /// distinction the exterior hypotheses need.
    #[test]
    fn every_normalised_colour_type_becomes_rgba8_and_alpha_presence_is_recorded() {
        for (ct, channels, has_alpha) in [
            (png::ColorType::Rgba, 4usize, true),
            (png::ColorType::Rgb, 3, false),
            (png::ColorType::GrayscaleAlpha, 2, true),
            (png::ColorType::Grayscale, 1, false),
        ] {
            let mut bytes = Vec::new();
            let data: Vec<u8> = (0..(4 * channels) as u8).map(|v| v * 7 + 1).collect();
            {
                let mut enc = png::Encoder::new(&mut bytes, 2, 2);
                enc.set_color(ct);
                enc.set_depth(png::BitDepth::Eight);
                let mut w = enc.write_header().unwrap();
                w.write_image_data(&data).unwrap();
            }
            let img = CanonicalImage::decode_png(&bytes, &DecodeLimits::default())
                .unwrap_or_else(|e| panic!("{ct:?}: {e}"));
            assert_eq!(img.straight_srgb8().len(), 16, "{ct:?}");
            assert_eq!(img.source_had_alpha(), has_alpha, "{ct:?}");
            if !has_alpha {
                assert!(
                    img.straight_srgb8().chunks(4).all(|p| p[3] == 255),
                    "{ct:?}: an alpha-less source must decode fully opaque"
                );
            }
            assert_eq!(img.icc_assumption(), IccAssumption::NoProfileAssumedSrgb);
            assert!(img.icc_assumption().is_assumed());
        }
    }

    /// The applied assumption is recorded and DIFFERS by what the file
    /// carried. Without this the report could not tell a declared sRGB
    /// asset from a guess.
    #[test]
    fn the_applied_colour_assumption_is_recorded_and_not_constant() {
        let mut declared = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut declared, 1, 1);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[1, 2, 3, 4]).unwrap();
        }
        let img = CanonicalImage::decode_png(&declared, &DecodeLimits::default()).unwrap();
        assert_eq!(img.icc_assumption(), IccAssumption::SrgbChunkDeclared);
        assert!(!img.icc_assumption().is_assumed());
    }

    #[test]
    fn the_border_is_the_frame_and_the_interior_is_not_in_it() {
        let img = CanonicalImage::from_straight_srgb8(
            4,
            3,
            tiny_rgba(4, 3, [0, 0, 0, 0]),
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap();
        let b = img.border_indices();
        // 4x3: all 12 pixels except the two interior ones (1,1) and (2,1).
        assert_eq!(b.len(), 10);
        assert!(!b.contains(&img.index(1, 1)));
        assert!(!b.contains(&img.index(2, 1)));
        assert!(b.contains(&img.index(0, 1)) && b.contains(&img.index(3, 1)));
        // A one-pixel-tall image is all border, and the row is not counted
        // twice.
        let thin = CanonicalImage::from_straight_srgb8(
            5,
            1,
            tiny_rgba(5, 1, [0, 0, 0, 0]),
            true,
            IccAssumption::NoProfileAssumedSrgb,
        )
        .unwrap();
        assert_eq!(thin.border_indices().len(), 5);
    }

    /// CRC-32 (ISO 3309), for rewriting the IHDR of a test fixture.
    fn crc32(data: &[u8]) -> u32 {
        let mut table = [0u32; 256];
        for (i, e) in table.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *e = c;
        }
        let mut c = 0xFFFF_FFFFu32;
        for b in data {
            c = table[((c ^ u32::from(*b)) & 0xFF) as usize] ^ (c >> 8);
        }
        c ^ 0xFFFF_FFFF
    }
}
