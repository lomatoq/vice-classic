//! Input resource limits (M0 gate: "input resource limits").
//!
//! PNG inputs are validated against explicit limits BEFORE any baseline is
//! invoked on them: file size is checked from metadata before reading, and
//! dimensions are parsed from the IHDR header without decoding pixel data,
//! so a decompression bomb is rejected without decompression.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResourceLimits {
    /// Maximum input PNG file size in bytes.
    pub max_input_bytes: u64,
    /// Maximum PNG width or height in pixels.
    pub max_png_dimension: u32,
    /// Timeout for one baseline invocation on one input, seconds.
    pub run_timeout_secs: u64,
    /// Timeout for one baseline build, seconds.
    pub build_timeout_secs: u64,
    /// Maximum size of a single declared output artifact, bytes.
    pub max_output_bytes: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        ResourceLimits {
            max_input_bytes: 8 * 1024 * 1024,
            max_png_dimension: 4096,
            run_timeout_secs: 300,
            build_timeout_secs: 3600,
            max_output_bytes: 32 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum InputError {
    #[error("file too large: {bytes} bytes > limit {max}")]
    FileTooLarge { bytes: u64, max: u64 },
    #[error("not a PNG: bad signature")]
    BadSignature,
    #[error("truncated PNG header")]
    Truncated,
    #[error("corrupt IHDR chunk")]
    CorruptIhdr,
    #[error("dimensions {width}x{height} exceed per-axis limit {max}")]
    DimensionsExceeded { width: u32, height: u32, max: u32 },
    #[error("zero width or height")]
    ZeroDimension,
    #[error("io: {0}")]
    Io(String),
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct PngInfo {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub color_type: u8,
}

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// Validate an in-memory PNG prefix (signature + IHDR) against limits.
pub fn validate_png_bytes(bytes: &[u8], limits: &ResourceLimits) -> Result<PngInfo, InputError> {
    if bytes.len() as u64 > limits.max_input_bytes {
        return Err(InputError::FileTooLarge {
            bytes: bytes.len() as u64,
            max: limits.max_input_bytes,
        });
    }
    if bytes.len() < 8 {
        return Err(InputError::Truncated);
    }
    if bytes[..8] != PNG_SIGNATURE {
        return Err(InputError::BadSignature);
    }
    // First chunk: 4-byte big-endian length, 4-byte type "IHDR", 13 data bytes.
    if bytes.len() < 8 + 8 + 13 {
        return Err(InputError::Truncated);
    }
    let len = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    if &bytes[12..16] != b"IHDR" || len != 13 {
        return Err(InputError::CorruptIhdr);
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    let bit_depth = bytes[24];
    let color_type = bytes[25];
    if width == 0 || height == 0 {
        return Err(InputError::ZeroDimension);
    }
    if width > limits.max_png_dimension || height > limits.max_png_dimension {
        return Err(InputError::DimensionsExceeded {
            width,
            height,
            max: limits.max_png_dimension,
        });
    }
    Ok(PngInfo {
        width,
        height,
        bit_depth,
        color_type,
    })
}

/// Validate a PNG file on disk. Size is checked from metadata before reading.
pub fn validate_png_file(path: &Path, limits: &ResourceLimits) -> Result<PngInfo, InputError> {
    let meta = std::fs::metadata(path).map_err(|e| InputError::Io(e.to_string()))?;
    if meta.len() > limits.max_input_bytes {
        return Err(InputError::FileTooLarge {
            bytes: meta.len(),
            max: limits.max_input_bytes,
        });
    }
    let bytes = std::fs::read(path).map_err(|e| InputError::Io(e.to_string()))?;
    validate_png_bytes(&bytes, limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(width: u32, height: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&PNG_SIGNATURE);
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&width.to_be_bytes());
        v.extend_from_slice(&height.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]); // depth, color, comp, filter, interlace
        v
    }

    #[test]
    fn accepts_valid_header() {
        let info = validate_png_bytes(&header(64, 32), &ResourceLimits::default()).unwrap();
        assert_eq!((info.width, info.height), (64, 32));
        assert_eq!((info.bit_depth, info.color_type), (8, 6));
    }

    #[test]
    fn rejects_bad_signature() {
        let mut h = header(4, 4);
        h[0] = 0x00;
        assert_eq!(
            validate_png_bytes(&h, &ResourceLimits::default()),
            Err(InputError::BadSignature)
        );
    }

    #[test]
    fn rejects_truncated() {
        let h = header(4, 4);
        assert_eq!(
            validate_png_bytes(&h[..12], &ResourceLimits::default()),
            Err(InputError::Truncated)
        );
    }

    #[test]
    fn rejects_oversized_dimensions_without_decoding() {
        // A "decompression bomb" header: 1e9 x 1e9. Only 33 bytes on disk;
        // must be rejected from the header alone.
        let h = header(1_000_000_000, 1_000_000_000);
        match validate_png_bytes(&h, &ResourceLimits::default()) {
            Err(InputError::DimensionsExceeded { max, .. }) => assert_eq!(max, 4096),
            other => panic!("expected DimensionsExceeded, got {other:?}"),
        }
    }

    #[test]
    fn rejects_zero_dimension() {
        assert_eq!(
            validate_png_bytes(&header(0, 7), &ResourceLimits::default()),
            Err(InputError::ZeroDimension)
        );
    }

    #[test]
    fn rejects_oversized_file() {
        let limits = ResourceLimits {
            max_input_bytes: 16,
            ..ResourceLimits::default()
        };
        let h = header(4, 4);
        assert!(matches!(
            validate_png_bytes(&h, &limits),
            Err(InputError::FileTooLarge { .. })
        ));
    }
}
