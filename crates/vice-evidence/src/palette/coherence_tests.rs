use super::*;
use crate::interior::{interior_confidence, INTERIOR_CONFIG_V1};
use vice_image::{CanonicalImage, IccAssumption};
use vice_ir::BlendSpace;

fn img_from(size: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> CanonicalImage {
    let mut px = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&f(x, y));
        }
    }
    CanonicalImage::from_straight_srgb8(size, size, px, true, IccAssumption::NoProfileAssumedSrgb)
        .unwrap()
}

fn propose(img: &CanonicalImage) -> Flat2Proposals {
    let tensor = ObservationTensor::of(img, BlendSpace::LinearLight);
    let interior = interior_confidence(&tensor, &INTERIOR_CONFIG_V1);
    propose_flat2(
        &tensor,
        &interior,
        &img.border_indices(),
        &PALETTE_CONFIG_V1,
    )
}

#[test]
fn coherent_small_opaque_components_survive_the_global_share_floor() {
    let img = img_from(64, |x, y| {
        let small_face = [(9, 9), (29, 17), (45, 43)]
            .iter()
            .any(|&(x0, y0)| (x0..x0 + 4).contains(&x) && (y0..y0 + 4).contains(&y));
        if small_face {
            [225, 35, 45, 255]
        } else {
            [20, 25, 70, 255]
        }
    });
    let proposals = propose(&img);
    assert!(proposals.refusal.is_none(), "{:?}", proposals.refusal);
    assert_eq!(
        proposals.modes.len(),
        2,
        "coherent minority face: {:?}",
        proposals.modes
    );
    assert!(
        proposals
            .modes
            .iter()
            .any(|mode| mode.weight / (64.0 * 64.0) < 0.02),
        "the control must exercise the sub-share path: {:?}",
        proposals.modes
    );
}

#[test]
fn coherent_sub_core_opaque_face_uses_a_bounded_colour_interval() {
    let img = img_from(64, |x, y| {
        let tiny_face = [(9, 9), (29, 17), (45, 43)]
            .iter()
            .any(|&(x0, y0)| (x0..x0 + 2).contains(&x) && (y0..y0 + 2).contains(&y));
        if tiny_face {
            [225, 35, 45, 255]
        } else {
            [20, 25, 70, 255]
        }
    });
    let proposals = propose(&img);
    assert!(proposals.refusal.is_none(), "{:?}", proposals.refusal);
    assert_eq!(
        proposals.modes.len(),
        1,
        "the control must have no second core mode"
    );
    assert_eq!(proposals.hypotheses.len(), 3);
    assert!(proposals.hypotheses[0].foreground.is_interval());
    assert!(matches!(
        proposals.hypotheses[2].background,
        BackgroundHypothesis::OpaqueFace(colour) if colour.is_interval()
    ));
}

#[test]
fn isolated_sub_share_colour_pixels_remain_noise() {
    let img = img_from(64, |x, y| {
        let isolated = (0..12).any(|i| x == 3 + i * 5 && y == 5 + (i * 7) % 53);
        if isolated {
            [225, 35, 45, 255]
        } else {
            [20, 25, 70, 255]
        }
    });
    let proposals = propose(&img);
    assert_eq!(
        proposals.modes.len(),
        1,
        "isolated noise must not become a face"
    );
    assert_eq!(proposals.refusal, Some(PaletteRefusal::SingleUniformFace));
}
