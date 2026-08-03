//! Deterministic M10 centerline renderer.

use vice_geom::flatten::ChordTolerancePx;
use vice_geom::predicates::closed_segments_intersect;
use vice_geom::{Pt, Vec2};
use vice_ir::color::{linear_to_srgb_u8, premultiply, LinearRgba, PremulRgba};
use vice_ir::{
    Paint, StrokeCap, StrokeJoin, StrokeVertexId, StrokeVertexStyle, ValidatedStrokeScene,
};

use crate::domain::NumericDomain;
use crate::mesh::flatten_segment;
use crate::MAX_COVERAGE_ELEMENTS;

pub const STROKE_RENDER_SCHEMA: &str = "vice-classic/stroke-render/v1";
pub const STROKE_SUPERSAMPLE_SIDE: u32 = 8;
const MAX_STROKE_EDGES: usize = 4096;
const MAX_FLATTENED_POINTS: usize = 1 << 20;
const TILE_SIDE: u32 = 16;
const MAX_TILE_REFERENCES: u64 = 1 << 22;
const MAX_SAMPLE_OBJECT_TESTS: u64 = 1 << 27;
const MAX_INTERSECTION_TESTS: u64 = 1 << 24;

#[derive(Debug, Clone, PartialEq)]
pub struct StrokeRender {
    pub schema: &'static str,
    pub width_px: u32,
    pub height_px: u32,
    pub samples_per_axis: u32,
    pub composite: Vec<PremulRgba>,
    pub straight_srgb8: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StrokeRenderError {
    #[error("stroke canvas {width}x{height} is outside the render domain")]
    CanvasDomain { width: u32, height: u32 },
    #[error("stroke render requires {elements} channel elements, over limit {limit}")]
    ResourceLimit { elements: u64, limit: u64 },
    #[error("stroke graph has {edges} edges, over limit {limit}")]
    EdgeLimit { edges: usize, limit: usize },
    #[error("stroke centerline coordinate is outside the render domain")]
    CoordinateDomain,
    #[error("stroke edge {edge} segment {segment} could not be flattened: {detail}")]
    Flatten {
        edge: usize,
        segment: usize,
        detail: String,
    },
    #[error("stroke tessellation exceeds the point budget")]
    TessellationLimit,
    #[error("stroke edge {a} crosses edge {b} without a graph junction")]
    UnrepresentedJunction { a: usize, b: usize },
    #[error("stroke {stage} requires {operations} bounded operations, over limit {limit}")]
    WorkLimit {
        stage: &'static str,
        operations: u64,
        limit: u64,
    },
}

#[derive(Debug, Clone)]
struct FlatEdge {
    start: StrokeVertexId,
    end: StrokeVertexId,
    points: Vec<Pt>,
    radius: f64,
    paint: Paint,
}

#[derive(Debug, Clone)]
enum Marker {
    Circle {
        center: Pt,
        radius: f64,
        paint: Paint,
    },
    Polygon {
        points: Vec<Pt>,
        paint: Paint,
    },
}

struct TileIndex {
    tiles_x: usize,
    edge_ids: Vec<Vec<usize>>,
    marker_ids: Vec<Vec<usize>>,
}

pub fn render_stroke_scene(
    scene: &ValidatedStrokeScene,
) -> Result<StrokeRender, StrokeRenderError> {
    let scene = scene.scene();
    let domain = NumericDomain::default();
    if scene.canvas.width_px > domain.max_canvas_dim_px
        || scene.canvas.height_px > domain.max_canvas_dim_px
    {
        return Err(StrokeRenderError::CanvasDomain {
            width: scene.canvas.width_px,
            height: scene.canvas.height_px,
        });
    }
    let elements = u64::from(scene.canvas.width_px)
        .saturating_mul(u64::from(scene.canvas.height_px))
        .saturating_mul(4);
    if elements > MAX_COVERAGE_ELEMENTS {
        return Err(StrokeRenderError::ResourceLimit {
            elements,
            limit: MAX_COVERAGE_ELEMENTS,
        });
    }
    if scene.edges.len() > MAX_STROKE_EDGES {
        return Err(StrokeRenderError::EdgeLimit {
            edges: scene.edges.len(),
            limit: MAX_STROKE_EDGES,
        });
    }
    if scene.vertices.iter().any(|vertex| {
        !domain.contains_coord(vertex.position.x) || !domain.contains_coord(vertex.position.y)
    }) {
        return Err(StrokeRenderError::CoordinateDomain);
    }
    let edges = flatten_edges(scene)?;
    reject_unrepresented_crossings(&edges)?;
    let markers = build_markers(scene, &edges);
    let index = build_tile_index(
        scene.canvas.width_px,
        scene.canvas.height_px,
        &edges,
        &markers,
    )?;
    let pixels = scene.canvas.width_px as usize * scene.canvas.height_px as usize;
    let background = paint_premul(scene.background);
    let mut composite = vec![background; pixels];
    let side = STROKE_SUPERSAMPLE_SIDE;
    let inv_samples = 1.0 / f64::from(side * side);
    for y in 0..scene.canvas.height_px {
        for x in 0..scene.canvas.width_px {
            let mut sum = PremulRgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            };
            for sy in 0..side {
                for sx in 0..side {
                    let point = Pt::new(
                        f64::from(x) + (f64::from(sx) + 0.5) / f64::from(side),
                        f64::from(y) + (f64::from(sy) + 0.5) / f64::from(side),
                    );
                    let tile = (y / TILE_SIDE) as usize * index.tiles_x + (x / TILE_SIDE) as usize;
                    let paint = sample_paint(
                        point,
                        scene.background,
                        &edges,
                        &markers,
                        &index.edge_ids[tile],
                        &index.marker_ids[tile],
                    );
                    let value = paint_premul(paint);
                    sum.r += value.r * inv_samples;
                    sum.g += value.g * inv_samples;
                    sum.b += value.b * inv_samples;
                    sum.a += value.a * inv_samples;
                }
            }
            composite[y as usize * scene.canvas.width_px as usize + x as usize] = sum;
        }
    }
    let straight_srgb8 = composite_to_srgb8(&composite);
    Ok(StrokeRender {
        schema: STROKE_RENDER_SCHEMA,
        width_px: scene.canvas.width_px,
        height_px: scene.canvas.height_px,
        samples_per_axis: side,
        composite,
        straight_srgb8,
    })
}

fn flatten_edges(scene: &vice_ir::StrokeScene) -> Result<Vec<FlatEdge>, StrokeRenderError> {
    let tolerance = ChordTolerancePx::new(1.0 / 64.0).expect("static tolerance");
    let mut total_points = 0usize;
    let mut output = Vec::with_capacity(scene.edges.len());
    for (edge_index, edge) in scene.edges.iter().enumerate() {
        let start = scene.vertices[edge.start.index()].position;
        let end = scene.vertices[edge.end.index()].position;
        let nodes = edge.centerline.node_positions(start, end);
        let mut points = Vec::new();
        for (segment_index, segment) in edge.centerline.segments.iter().enumerate() {
            let flat = flatten_segment(
                segment,
                nodes[segment_index],
                nodes[segment_index + 1],
                tolerance,
            )
            .map_err(|error| StrokeRenderError::Flatten {
                edge: edge_index,
                segment: segment_index,
                detail: error.to_string(),
            })?;
            let skip = usize::from(segment_index > 0);
            points.extend_from_slice(&flat.points[skip..]);
        }
        total_points = total_points.saturating_add(points.len());
        if total_points > MAX_FLATTENED_POINTS {
            return Err(StrokeRenderError::TessellationLimit);
        }
        output.push(FlatEdge {
            start: edge.start,
            end: edge.end,
            points,
            radius: 0.5 * edge.width_px,
            paint: edge.paint,
        });
    }
    Ok(output)
}

fn reject_unrepresented_crossings(edges: &[FlatEdge]) -> Result<(), StrokeRenderError> {
    let mut tests = 0u64;
    for a in 0..edges.len() {
        for b in a + 1..edges.len() {
            let shared = [edges[a].start, edges[a].end]
                .into_iter()
                .find(|vertex| *vertex == edges[b].start || *vertex == edges[b].end);
            for (left_index, left) in edges[a].points.windows(2).enumerate() {
                for (right_index, right) in edges[b].points.windows(2).enumerate() {
                    tests = tests.saturating_add(1);
                    if tests > MAX_INTERSECTION_TESTS {
                        return Err(StrokeRenderError::WorkLimit {
                            stage: "intersection validation",
                            operations: tests,
                            limit: MAX_INTERSECTION_TESTS,
                        });
                    }
                    if !closed_segments_intersect(left[0], left[1], right[0], right[1]) {
                        continue;
                    }
                    if shared.is_some_and(|vertex| {
                        intersection_is_only_shared_endpoint(
                            &edges[a],
                            left_index,
                            &edges[b],
                            right_index,
                            vertex,
                        )
                    }) {
                        continue;
                    }
                    return Err(StrokeRenderError::UnrepresentedJunction { a, b });
                }
            }
        }
    }
    Ok(())
}

fn build_tile_index(
    width: u32,
    height: u32,
    edges: &[FlatEdge],
    markers: &[Marker],
) -> Result<TileIndex, StrokeRenderError> {
    let tiles_x = width.div_ceil(TILE_SIDE) as usize;
    let tiles_y = height.div_ceil(TILE_SIDE) as usize;
    let tile_count = tiles_x.saturating_mul(tiles_y);
    let mut index = TileIndex {
        tiles_x,
        edge_ids: vec![Vec::new(); tile_count],
        marker_ids: vec![Vec::new(); tile_count],
    };
    let mut references = 0u64;
    for (id, edge) in edges.iter().enumerate() {
        let bounds = bounds_for_points(&edge.points, edge.radius);
        insert_bounds(
            bounds,
            width,
            height,
            tiles_x,
            id,
            &mut index.edge_ids,
            &mut references,
        )?;
    }
    for (id, marker) in markers.iter().enumerate() {
        let bounds = match marker {
            Marker::Circle { center, radius, .. } => (
                *center - Vec2::new(*radius, *radius),
                *center + Vec2::new(*radius, *radius),
            ),
            Marker::Polygon { points, .. } => bounds_for_points(points, 0.0),
        };
        insert_bounds(
            bounds,
            width,
            height,
            tiles_x,
            id,
            &mut index.marker_ids,
            &mut references,
        )?;
    }
    let mut operations = 0u64;
    for tile_y in 0..tiles_y {
        for tile_x in 0..tiles_x {
            let tile = tile_y * tiles_x + tile_x;
            let pixels_x = (width - tile_x as u32 * TILE_SIDE).min(TILE_SIDE);
            let pixels_y = (height - tile_y as u32 * TILE_SIDE).min(TILE_SIDE);
            let objects = index.edge_ids[tile].len() + index.marker_ids[tile].len();
            operations = operations.saturating_add(
                u64::from(pixels_x)
                    .saturating_mul(u64::from(pixels_y))
                    .saturating_mul(u64::from(STROKE_SUPERSAMPLE_SIDE).pow(2))
                    .saturating_mul(objects as u64),
            );
        }
    }
    if operations > MAX_SAMPLE_OBJECT_TESTS {
        return Err(StrokeRenderError::WorkLimit {
            stage: "supersampling",
            operations,
            limit: MAX_SAMPLE_OBJECT_TESTS,
        });
    }
    Ok(index)
}

fn bounds_for_points(points: &[Pt], padding: f64) -> (Pt, Pt) {
    let mut min = points[0];
    let mut max = points[0];
    for point in &points[1..] {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
    }
    (
        min - Vec2::new(padding, padding),
        max + Vec2::new(padding, padding),
    )
}

#[allow(clippy::too_many_arguments)]
fn insert_bounds(
    bounds: (Pt, Pt),
    width: u32,
    height: u32,
    tiles_x: usize,
    id: usize,
    buckets: &mut [Vec<usize>],
    references: &mut u64,
) -> Result<(), StrokeRenderError> {
    let x0 = bounds.0.x.floor().max(0.0).min(f64::from(width)) as u32;
    let y0 = bounds.0.y.floor().max(0.0).min(f64::from(height)) as u32;
    let x1 = bounds.1.x.ceil().max(0.0).min(f64::from(width)) as u32;
    let y1 = bounds.1.y.ceil().max(0.0).min(f64::from(height)) as u32;
    if x0 >= x1 || y0 >= y1 {
        return Ok(());
    }
    for tile_y in y0 / TILE_SIDE..=(y1 - 1) / TILE_SIDE {
        for tile_x in x0 / TILE_SIDE..=(x1 - 1) / TILE_SIDE {
            *references = references.saturating_add(1);
            if *references > MAX_TILE_REFERENCES {
                return Err(StrokeRenderError::WorkLimit {
                    stage: "spatial indexing",
                    operations: *references,
                    limit: MAX_TILE_REFERENCES,
                });
            }
            buckets[tile_y as usize * tiles_x + tile_x as usize].push(id);
        }
    }
    Ok(())
}

fn intersection_is_only_shared_endpoint(
    left: &FlatEdge,
    left_segment: usize,
    right: &FlatEdge,
    right_segment: usize,
    shared: StrokeVertexId,
) -> bool {
    let left_at_start = left.start == shared && left_segment == 0;
    let left_at_end = left.end == shared && left_segment + 2 == left.points.len();
    let right_at_start = right.start == shared && right_segment == 0;
    let right_at_end = right.end == shared && right_segment + 2 == right.points.len();
    if !(left_at_start || left_at_end) || !(right_at_start || right_at_end) {
        return false;
    }
    let left_away = if left_at_start {
        left.points[left_segment + 1] - left.points[left_segment]
    } else {
        left.points[left_segment] - left.points[left_segment + 1]
    };
    let right_away = if right_at_start {
        right.points[right_segment + 1] - right.points[right_segment]
    } else {
        right.points[right_segment] - right.points[right_segment + 1]
    };
    // Collinear arms pointing into the same ray overlap beyond the junction.
    left_away.cross(right_away).abs() > 1e-12 || left_away.dot(right_away) <= 0.0
}

fn build_markers(scene: &vice_ir::StrokeScene, edges: &[FlatEdge]) -> Vec<Marker> {
    let mut incident = vec![Vec::<(usize, bool)>::new(); scene.vertices.len()];
    for (edge, value) in edges.iter().enumerate() {
        incident[value.start.index()].push((edge, true));
        incident[value.end.index()].push((edge, false));
    }
    let mut markers = Vec::new();
    for (vertex_index, vertex) in scene.vertices.iter().enumerate() {
        let arms = &incident[vertex_index];
        let paint = edges[arms[0].0].paint;
        let radius = arms
            .iter()
            .map(|(edge, _)| edges[*edge].radius)
            .fold(0.0, f64::max);
        match vertex.style {
            StrokeVertexStyle::Cap(StrokeCap::Round)
            | StrokeVertexStyle::Join(StrokeJoin::Round)
            | StrokeVertexStyle::Junction(_) => markers.push(Marker::Circle {
                center: vertex.position,
                radius,
                paint,
            }),
            StrokeVertexStyle::Cap(StrokeCap::Square) => {
                let (edge, at_start) = arms[0];
                let tangent = endpoint_tangent(&edges[edge], at_start);
                markers.push(square_cap(vertex.position, tangent, radius, paint));
            }
            StrokeVertexStyle::Cap(StrokeCap::Butt) => {}
            StrokeVertexStyle::Join(join) => {
                let first = endpoint_tangent(&edges[arms[0].0], arms[0].1);
                let second = endpoint_tangent(&edges[arms[1].0], arms[1].1);
                markers.extend(join_markers(
                    vertex.position,
                    first,
                    second,
                    radius,
                    paint,
                    join,
                ));
            }
        }
    }
    markers
}

fn endpoint_tangent(edge: &FlatEdge, at_start: bool) -> Vec2 {
    let (a, b) = if at_start {
        (edge.points[0], edge.points[1])
    } else {
        (
            *edge.points.last().expect("validated edge"),
            edge.points[edge.points.len() - 2],
        )
    };
    normalize(b - a)
}

fn square_cap(center: Pt, tangent: Vec2, radius: f64, paint: Paint) -> Marker {
    let normal = Vec2::new(-tangent.y, tangent.x);
    Marker::Polygon {
        points: vec![
            center + normal * radius,
            center - normal * radius,
            center - normal * radius - tangent * radius,
            center + normal * radius - tangent * radius,
        ],
        paint,
    }
}

fn join_markers(
    center: Pt,
    first: Vec2,
    second: Vec2,
    radius: f64,
    paint: Paint,
    join: StrokeJoin,
) -> Vec<Marker> {
    if matches!(join, StrokeJoin::Round) {
        return vec![Marker::Circle {
            center,
            radius,
            paint,
        }];
    }
    let n1 = Vec2::new(-first.y, first.x);
    let n2 = Vec2::new(-second.y, second.x);
    [-1.0, 1.0]
        .into_iter()
        .map(|sign| {
            let a = center + n1 * (radius * sign);
            // Both tangents point away from the shared vertex. Their
            // corresponding outline sides therefore use opposite normals.
            let b = center - n2 * (radius * sign);
            let tip = match join {
                StrokeJoin::Miter { limit } => line_intersection(a, first, b, second)
                    .filter(|point| point.dist(center) <= limit * radius)
                    .unwrap_or(center),
                StrokeJoin::Bevel => center,
                StrokeJoin::Round => unreachable!(),
            };
            Marker::Polygon {
                points: vec![a, tip, b],
                paint,
            }
        })
        .collect()
}

fn line_intersection(a: Pt, da: Vec2, b: Pt, db: Vec2) -> Option<Pt> {
    let denominator = da.cross(db);
    if denominator == 0.0 {
        return None;
    }
    let t = (b - a).cross(db) / denominator;
    let point = a + da * t;
    point.is_finite().then_some(point)
}

fn sample_paint(
    point: Pt,
    background: Paint,
    edges: &[FlatEdge],
    markers: &[Marker],
    edge_ids: &[usize],
    marker_ids: &[usize],
) -> Paint {
    let mut paint = background;
    for &id in edge_ids {
        let edge = &edges[id];
        if edge_contains(edge, point) {
            paint = edge.paint;
        }
    }
    for &id in marker_ids {
        let marker = &markers[id];
        match marker {
            Marker::Circle {
                center,
                radius,
                paint: marker_paint,
            } if point.dist_sq(*center) <= radius * radius => paint = *marker_paint,
            Marker::Polygon {
                points,
                paint: marker_paint,
            } if point_in_polygon(point, points) => paint = *marker_paint,
            _ => {}
        }
    }
    paint
}

fn edge_contains(edge: &FlatEdge, point: Pt) -> bool {
    edge.points.windows(2).any(|pair| {
        let delta = pair[1] - pair[0];
        let length_sq = delta.length_sq();
        if length_sq == 0.0 {
            return point.dist_sq(pair[0]) <= edge.radius * edge.radius;
        }
        let t = (point - pair[0]).dot(delta) / length_sq;
        (0.0..=1.0).contains(&t) && point.dist_sq(pair[0] + delta * t) <= edge.radius * edge.radius
    }) || edge.points[1..edge.points.len() - 1]
        .iter()
        .any(|center| point.dist_sq(*center) <= edge.radius * edge.radius)
}

fn point_in_polygon(point: Pt, polygon: &[Pt]) -> bool {
    let mut inside = false;
    let mut previous = polygon[polygon.len() - 1];
    for &current in polygon {
        if ((current.y > point.y) != (previous.y > point.y))
            && point.x
                < (previous.x - current.x) * (point.y - current.y) / (previous.y - current.y)
                    + current.x
        {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

fn normalize(vector: Vec2) -> Vec2 {
    let length = vector.length();
    if length == 0.0 {
        Vec2::ZERO
    } else {
        vector * (1.0 / length)
    }
}

fn paint_premul(paint: Paint) -> PremulRgba {
    match paint {
        Paint::OpaqueSolid(rgb) => premultiply(LinearRgba {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
            a: 1.0,
        }),
        Paint::TransparentExterior => PremulRgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
    }
}

fn composite_to_srgb8(composite: &[PremulRgba]) -> Vec<u8> {
    let mut output = Vec::with_capacity(composite.len() * 4);
    for pixel in composite {
        if pixel.a <= 1e-12 {
            output.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            output.extend_from_slice(&[
                linear_to_srgb_u8(pixel.r / pixel.a),
                linear_to_srgb_u8(pixel.g / pixel.a),
                linear_to_srgb_u8(pixel.b / pixel.a),
                (pixel.a.clamp(0.0, 1.0) * 255.0).round() as u8,
            ]);
        }
    }
    output
}
