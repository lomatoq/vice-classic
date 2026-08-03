mod common;

use vice_ir::{PixelFilter, ResizeChain, ValidatedScene};
use vice_render::{render_partition_formed, RenderOptions};

fn scene(filter: PixelFilter) -> ValidatedScene {
    let mut raw = common::rect_scene(17, 15, 4.25, 3.5, 12.25, 11.5, common::red()).into_inner();
    raw.formation.pixel_filter = filter;
    ValidatedScene::new(raw).unwrap()
}

fn assert_partition(render: &vice_render::PartitionRender) {
    for pixel in 0..render.width_px as usize * render.height_px as usize {
        let sum = render
            .face_coverage
            .iter()
            .map(|face| face[pixel])
            .sum::<f64>();
        assert!((sum - 1.0).abs() < 1e-9, "partition sum {sum}");
        assert!(render
            .face_coverage
            .iter()
            .all(|face| (-1e-9..=1.0 + 1e-9).contains(&face[pixel])));
    }
}

#[test]
fn broader_global_psf_changes_pixels_but_preserves_the_partition() {
    let options = RenderOptions::default();
    let box_render =
        render_partition_formed(&scene(PixelFilter::Box), &options, ResizeChain::None).unwrap();
    let gaussian = render_partition_formed(
        &scene(PixelFilter::Gaussian { sigma_px: 1.5 }),
        &options,
        ResizeChain::None,
    )
    .unwrap();
    assert_partition(&gaussian);
    assert_ne!(gaussian.face_coverage, box_render.face_coverage);
    assert!(gaussian.face_coverage[1][3 * 17 + 3] > box_render.face_coverage[1][3 * 17 + 3]);
}

#[test]
fn both_resize_chains_render_at_work_resolution_and_return_target_dimensions() {
    let options = RenderOptions::default();
    let source = scene(PixelFilter::Triangle);
    let direct = render_partition_formed(&source, &options, ResizeChain::None).unwrap();
    let down = render_partition_formed(&source, &options, ResizeChain::DownFrom2x).unwrap();
    let up = render_partition_formed(&source, &options, ResizeChain::UpFromHalf).unwrap();
    for render in [&down, &up] {
        assert_eq!((render.width_px, render.height_px), (17, 15));
        assert_partition(render);
    }
    assert_ne!(down.face_coverage, direct.face_coverage);
    assert_ne!(up.face_coverage, direct.face_coverage);
    assert_eq!(
        down,
        render_partition_formed(&source, &options, ResizeChain::DownFrom2x).unwrap()
    );
}
