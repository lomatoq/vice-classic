use vice_geom::Pt;
use vice_ir::{
    Canvas, CurveChain, LinearRgb, Paint, Segment, StrokeCap, StrokeEdge, StrokeEdgeId, StrokeJoin,
    StrokeJunction, StrokeScene, StrokeVertex, StrokeVertexId, StrokeVertexStyle,
    ValidatedStrokeScene,
};
use vice_render::{render_stroke_scene, StrokeRenderError};

fn ink() -> Paint {
    Paint::OpaqueSolid(LinearRgb::new(0.0, 0.0, 0.0))
}

fn paper() -> Paint {
    Paint::OpaqueSolid(LinearRgb::new(1.0, 1.0, 1.0))
}

fn line(cap: StrokeCap) -> ValidatedStrokeScene {
    ValidatedStrokeScene::new(StrokeScene {
        canvas: Canvas {
            width_px: 20,
            height_px: 12,
        },
        vertices: vec![
            StrokeVertex {
                position: Pt::new(5.0, 6.0),
                style: StrokeVertexStyle::Cap(cap),
            },
            StrokeVertex {
                position: Pt::new(15.0, 6.0),
                style: StrokeVertexStyle::Cap(cap),
            },
        ],
        edges: vec![StrokeEdge {
            id: StrokeEdgeId(0),
            start: StrokeVertexId(0),
            end: StrokeVertexId(1),
            centerline: CurveChain::single(Segment::Line),
            width_px: 4.0,
            paint: ink(),
        }],
        background: paper(),
    })
    .unwrap()
}

fn bent(join: StrokeJoin) -> ValidatedStrokeScene {
    ValidatedStrokeScene::new(StrokeScene {
        canvas: Canvas {
            width_px: 20,
            height_px: 20,
        },
        vertices: vec![
            StrokeVertex {
                position: Pt::new(3.0, 12.0),
                style: StrokeVertexStyle::Cap(StrokeCap::Butt),
            },
            StrokeVertex {
                position: Pt::new(10.0, 12.0),
                style: StrokeVertexStyle::Join(join),
            },
            StrokeVertex {
                position: Pt::new(14.0, 5.0),
                style: StrokeVertexStyle::Cap(StrokeCap::Butt),
            },
        ],
        edges: vec![
            StrokeEdge {
                id: StrokeEdgeId(0),
                start: StrokeVertexId(0),
                end: StrokeVertexId(1),
                centerline: CurveChain::single(Segment::Line),
                width_px: 4.0,
                paint: ink(),
            },
            StrokeEdge {
                id: StrokeEdgeId(1),
                start: StrokeVertexId(1),
                end: StrokeVertexId(2),
                centerline: CurveChain::single(Segment::Line),
                width_px: 4.0,
                paint: ink(),
            },
        ],
        background: paper(),
    })
    .unwrap()
}

#[test]
fn cap_and_join_families_are_load_bearing_and_deterministic() {
    let butt = render_stroke_scene(&line(StrokeCap::Butt)).unwrap();
    let round = render_stroke_scene(&line(StrokeCap::Round)).unwrap();
    let square = render_stroke_scene(&line(StrokeCap::Square)).unwrap();
    assert_ne!(butt.straight_srgb8, round.straight_srgb8);
    assert_ne!(round.straight_srgb8, square.straight_srgb8);

    let bevel = render_stroke_scene(&bent(StrokeJoin::Bevel)).unwrap();
    let round_join = render_stroke_scene(&bent(StrokeJoin::Round)).unwrap();
    let miter = render_stroke_scene(&bent(StrokeJoin::Miter { limit: 4.0 })).unwrap();
    assert_ne!(bevel.straight_srgb8, round_join.straight_srgb8);
    assert_ne!(bevel.straight_srgb8, miter.straight_srgb8);
    assert_eq!(
        miter,
        render_stroke_scene(&bent(StrokeJoin::Miter { limit: 4.0 })).unwrap()
    );
}

#[test]
fn a_three_way_junction_is_one_explicit_shared_hub() {
    let scene = ValidatedStrokeScene::new(StrokeScene {
        canvas: Canvas {
            width_px: 20,
            height_px: 20,
        },
        vertices: vec![
            StrokeVertex {
                position: Pt::new(10.0, 10.0),
                style: StrokeVertexStyle::Junction(StrokeJunction::Round),
            },
            StrokeVertex {
                position: Pt::new(3.0, 10.0),
                style: StrokeVertexStyle::Cap(StrokeCap::Round),
            },
            StrokeVertex {
                position: Pt::new(17.0, 10.0),
                style: StrokeVertexStyle::Cap(StrokeCap::Round),
            },
            StrokeVertex {
                position: Pt::new(10.0, 3.0),
                style: StrokeVertexStyle::Cap(StrokeCap::Round),
            },
        ],
        edges: (0..3)
            .map(|index| StrokeEdge {
                id: StrokeEdgeId(index),
                start: StrokeVertexId(0),
                end: StrokeVertexId(index + 1),
                centerline: CurveChain::single(Segment::Line),
                width_px: 3.0,
                paint: ink(),
            })
            .collect(),
        background: paper(),
    })
    .unwrap();
    let render = render_stroke_scene(&scene).unwrap();
    let center = (10 * render.width_px as usize + 10) * 4;
    assert_eq!(&render.straight_srgb8[center..center + 4], &[0, 0, 0, 255]);
}

#[test]
fn valid_but_excessive_supersampling_work_is_refused_before_rendering() {
    let scene = ValidatedStrokeScene::new(StrokeScene {
        canvas: Canvas {
            width_px: 2048,
            height_px: 2048,
        },
        vertices: vec![
            StrokeVertex {
                position: Pt::new(1.0, 1.0),
                style: StrokeVertexStyle::Cap(StrokeCap::Butt),
            },
            StrokeVertex {
                position: Pt::new(2047.0, 1.0),
                style: StrokeVertexStyle::Cap(StrokeCap::Butt),
            },
        ],
        edges: vec![StrokeEdge {
            id: StrokeEdgeId(0),
            start: StrokeVertexId(0),
            end: StrokeVertexId(1),
            centerline: CurveChain::single(Segment::Line),
            width_px: 4096.0,
            paint: ink(),
        }],
        background: paper(),
    })
    .unwrap();
    assert!(matches!(
        render_stroke_scene(&scene),
        Err(StrokeRenderError::WorkLimit {
            stage: "supersampling",
            ..
        })
    ));
}
