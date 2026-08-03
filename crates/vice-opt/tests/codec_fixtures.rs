use vice_image::{CanonicalImage, DecodeLimits, EncodedImageFormat};
use vice_opt::{calibrated_codec_likelihood_config, measure_codec_residual};

fn gradient() -> Vec<u8> {
    let mut pixels = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16u8 {
        for x in 0..16u8 {
            pixels.extend_from_slice(&[x * 17, y * 17, (x ^ y) * 17, 255]);
        }
    }
    pixels
}

#[test]
fn actual_jpeg_and_lossy_webp_fixtures_measure_their_transform_residuals() {
    let predicted = gradient();
    let fixtures = [
        (
            include_str!("fixtures/m9_jpeg_gradient.hex"),
            EncodedImageFormat::Jpeg,
        ),
        (
            include_str!("fixtures/m9_webp_gradient.hex"),
            EncodedImageFormat::WebpLossy,
        ),
    ];
    for (hex_fixture, expected_format) in fixtures {
        let bytes = hex::decode(hex_fixture.trim()).unwrap();
        let image = CanonicalImage::decode(&bytes, &DecodeLimits::default()).unwrap();
        assert_eq!(image.encoded_format(), expected_format);
        let stats = measure_codec_residual(&image, &predicted).unwrap();
        println!(
            "{expected_format:?}: dc={:.3} ac={:.3} alpha={:.3} blocks={}",
            stats.dc_rms_codes, stats.ac_rms_codes, stats.alpha_rms_codes, stats.blocks
        );
        assert!(stats.dc_rms_codes.is_finite() && stats.dc_rms_codes > 0.0);
        assert!(stats.ac_rms_codes.is_finite() && stats.ac_rms_codes > 0.0);
        assert_eq!(stats.alpha_rms_codes, 0.0);
        assert!(stats.blocks >= 4);
        let config = calibrated_codec_likelihood_config(expected_format);
        assert!(stats.dc_rms_codes <= config.dc_sigma_codes);
        assert!(stats.ac_rms_codes <= config.ac_sigma_codes);
    }
}
