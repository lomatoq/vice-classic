use std::collections::{BTreeSet, VecDeque};

use vice_geom::{Pt, Vec2};
use vice_ir::{
    Canvas, ChainNode, CurveChain, JoinKind, Paint, Segment, StrokeCap, StrokeEdge, StrokeEdgeId,
    StrokeJoin, StrokeJunction, StrokeScene, StrokeVertex, StrokeVertexId, StrokeVertexStyle,
    ValidatedStrokeScene,
};

use super::LineArtRefusal;

const MAX_GRAPH_ITEMS: usize = 4096;
const SIMPLIFICATION_TOLERANCE_PX: f64 = 0.35;

pub(super) struct Topology {
    pub vertices: Vec<TopologyVertex>,
    pub edges: Vec<TopologyEdge>,
}

pub(super) struct TopologyVertex {
    pub position: Pt,
    pub degree: usize,
    pub cap_boundary: Option<Pt>,
    pub cap_inward: Option<Vec2>,
}

pub(super) struct TopologyEdge {
    pub start: usize,
    pub end: usize,
    pub points: Vec<Pt>,
    pub width_px: f64,
}

pub(super) fn trace(
    skeleton: &[bool],
    foreground: &[bool],
    distance_squared: &[f64],
    width: usize,
    height: usize,
) -> Result<Topology, LineArtRefusal> {
    let mut node_pixels = skeleton
        .iter()
        .enumerate()
        .map(|(index, value)| *value && neighbors(index, skeleton, width, height).len() != 2)
        .collect::<Vec<_>>();
    add_cycle_anchors(skeleton, width, height, &mut node_pixels);
    let clusters = node_clusters(&node_pixels, width, height);
    if clusters.is_empty() {
        return Err(LineArtRefusal::IsolatedDot);
    }
    let mut owner = vec![None; skeleton.len()];
    let mut vertices = Vec::with_capacity(clusters.len());
    for (cluster, pixels) in clusters.iter().enumerate() {
        for &pixel in pixels {
            owner[pixel] = Some(cluster);
        }
        let (sum_x, sum_y) = pixels.iter().fold((0.0, 0.0), |sum, pixel| {
            let point = pixel_center(*pixel, width);
            (sum.0 + point.x, sum.1 + point.y)
        });
        vertices.push(TopologyVertex {
            position: Pt::new(sum_x / pixels.len() as f64, sum_y / pixels.len() as f64),
            degree: 0,
            cap_boundary: None,
            cap_inward: None,
        });
    }
    let mut visited = BTreeSet::new();
    let mut edges = Vec::new();
    for (cluster, pixels) in clusters.iter().enumerate() {
        for &pixel in pixels {
            for next in neighbors(pixel, skeleton, width, height) {
                if owner[next] == Some(cluster) || visited.contains(&link(pixel, next)) {
                    continue;
                }
                let edge = trace_edge(
                    cluster,
                    pixel,
                    next,
                    skeleton,
                    distance_squared,
                    width,
                    height,
                    &owner,
                    &vertices,
                    &mut visited,
                )?;
                edges.push(edge);
                if edges.len() > MAX_GRAPH_ITEMS {
                    return Err(LineArtRefusal::GraphBudget);
                }
            }
        }
    }
    if edges.is_empty() || vertices.len() > MAX_GRAPH_ITEMS {
        return Err(LineArtRefusal::IsolatedDot);
    }
    edges = split_bends(&mut vertices, edges)?;
    for edge in &edges {
        if edge.start == edge.end {
            vertices[edge.start].degree += 2;
        } else {
            vertices[edge.start].degree += 1;
            vertices[edge.end].degree += 1;
        }
    }
    if vertices.iter().any(|vertex| vertex.degree == 0) {
        return Err(LineArtRefusal::Graph {
            detail: "unconnected skeleton node cluster".into(),
        });
    }
    locate_cap_boundaries(&mut vertices, &edges, foreground, width, height);
    Ok(Topology { vertices, edges })
}

/// RDP breakpoints are semantic stroke joins, not anonymous curve-chain nodes.
fn split_bends(
    vertices: &mut Vec<TopologyVertex>,
    edges: Vec<TopologyEdge>,
) -> Result<Vec<TopologyEdge>, LineArtRefusal> {
    let mut output = Vec::new();
    for edge in edges {
        if edge.points.len() <= 2 {
            output.push(edge);
            continue;
        }
        let mut vertex_ids = Vec::with_capacity(edge.points.len());
        vertex_ids.push(edge.start);
        for point in &edge.points[1..edge.points.len() - 1] {
            if vertices.len() >= MAX_GRAPH_ITEMS {
                return Err(LineArtRefusal::GraphBudget);
            }
            let id = vertices.len();
            vertices.push(TopologyVertex {
                position: *point,
                degree: 0,
                cap_boundary: None,
                cap_inward: None,
            });
            vertex_ids.push(id);
        }
        vertex_ids.push(edge.end);
        for pair in vertex_ids.windows(2) {
            output.push(TopologyEdge {
                start: pair[0],
                end: pair[1],
                points: vec![vertices[pair[0]].position, vertices[pair[1]].position],
                width_px: edge.width_px,
            });
            if output.len() > MAX_GRAPH_ITEMS {
                return Err(LineArtRefusal::GraphBudget);
            }
        }
    }
    Ok(output)
}

fn locate_cap_boundaries(
    vertices: &mut [TopologyVertex],
    edges: &[TopologyEdge],
    foreground: &[bool],
    width: usize,
    height: usize,
) {
    for (vertex_index, vertex) in vertices.iter_mut().enumerate() {
        if vertex.degree != 1 {
            continue;
        }
        let Some(edge) = edges
            .iter()
            .find(|edge| edge.start == vertex_index || edge.end == vertex_index)
        else {
            continue;
        };
        let inward = if edge.start == vertex_index {
            normalize(edge.points[1] - vertex.position)
        } else {
            normalize(edge.points[edge.points.len() - 2] - vertex.position)
        };
        let outward = inward * -1.0;
        let origin = vertex.position;
        let mut inside = 0.0;
        let mut outside = 0.25;
        let search_limit = 2.0 * edge.width_px + 4.0;
        while outside <= search_limit
            && mask_contains(origin + outward * outside, foreground, width, height)
        {
            inside = outside;
            outside += 0.25;
        }
        if outside > search_limit {
            continue;
        }
        for _ in 0..16 {
            let middle = 0.5 * (inside + outside);
            if mask_contains(origin + outward * middle, foreground, width, height) {
                inside = middle;
            } else {
                outside = middle;
            }
        }
        vertex.cap_boundary = Some(origin + outward * outside);
        vertex.cap_inward = Some(inward);
    }
}

fn mask_contains(point: Pt, foreground: &[bool], width: usize, height: usize) -> bool {
    if point.x < 0.0 || point.y < 0.0 || point.x >= width as f64 || point.y >= height as f64 {
        return false;
    }
    foreground[point.y.floor() as usize * width + point.x.floor() as usize]
}

fn normalize(vector: Vec2) -> Vec2 {
    let length = vector.length();
    if length == 0.0 {
        Vec2::ZERO
    } else {
        vector * (1.0 / length)
    }
}

#[allow(clippy::too_many_arguments)]
fn trace_edge(
    start_cluster: usize,
    start_pixel: usize,
    first_pixel: usize,
    skeleton: &[bool],
    distance_squared: &[f64],
    width: usize,
    height: usize,
    owner: &[Option<usize>],
    vertices: &[TopologyVertex],
    visited: &mut BTreeSet<(usize, usize)>,
) -> Result<TopologyEdge, LineArtRefusal> {
    let mut points = vec![vertices[start_cluster].position];
    let mut widths = vec![pixel_width(distance_squared[start_pixel])];
    let mut previous = start_pixel;
    let mut current = first_pixel;
    visited.insert(link(previous, current));
    let end_cluster = loop {
        if let Some(cluster) = owner[current] {
            points.push(vertices[cluster].position);
            widths.push(pixel_width(distance_squared[current]));
            break cluster;
        }
        points.push(pixel_center(current, width));
        widths.push(pixel_width(distance_squared[current]));
        let next = neighbors(current, skeleton, width, height)
            .into_iter()
            .filter(|pixel| *pixel != previous)
            .min()
            .ok_or_else(|| LineArtRefusal::Graph {
                detail: format!("centerline ended outside a node at pixel {current}"),
            })?;
        visited.insert(link(current, next));
        previous = current;
        current = next;
    };
    points.dedup();
    if points.len() < 2 || (points.len() == 2 && points[0] == points[1]) {
        return Err(LineArtRefusal::IsolatedDot);
    }
    let points = if start_cluster == end_cluster {
        points
    } else {
        simplify(&points, SIMPLIFICATION_TOLERANCE_PX)
    };
    widths.sort_by(f64::total_cmp);
    let width_px = round_64(widths[widths.len() / 2].max(1.0));
    Ok(TopologyEdge {
        start: start_cluster,
        end: end_cluster,
        points,
        width_px,
    })
}

fn add_cycle_anchors(skeleton: &[bool], width: usize, height: usize, node_pixels: &mut [bool]) {
    let mut seen = vec![false; skeleton.len()];
    for seed in 0..skeleton.len() {
        if !skeleton[seed] || seen[seed] {
            continue;
        }
        let mut queue = VecDeque::from([seed]);
        seen[seed] = true;
        let mut component = Vec::new();
        let mut has_node = false;
        while let Some(pixel) = queue.pop_front() {
            component.push(pixel);
            has_node |= node_pixels[pixel];
            for next in neighbors(pixel, skeleton, width, height) {
                if !seen[next] {
                    seen[next] = true;
                    queue.push_back(next);
                }
            }
        }
        if !has_node {
            if component.len() == 1 {
                continue;
            }
            node_pixels[*component.iter().min().expect("nonempty component")] = true;
        }
    }
}

fn node_clusters(node_pixels: &[bool], width: usize, height: usize) -> Vec<Vec<usize>> {
    let mut seen = vec![false; node_pixels.len()];
    let mut output = Vec::new();
    for seed in 0..node_pixels.len() {
        if !node_pixels[seed] || seen[seed] {
            continue;
        }
        let mut queue = VecDeque::from([seed]);
        let mut cluster = Vec::new();
        seen[seed] = true;
        while let Some(pixel) = queue.pop_front() {
            cluster.push(pixel);
            for next in neighbors(pixel, node_pixels, width, height) {
                if !seen[next] {
                    seen[next] = true;
                    queue.push_back(next);
                }
            }
        }
        cluster.sort_unstable();
        output.push(cluster);
    }
    output
}

fn neighbors(index: usize, pixels: &[bool], width: usize, height: usize) -> Vec<usize> {
    let x = index % width;
    let y = index / width;
    let mut output = Vec::with_capacity(8);
    for dy in -1isize..=1 {
        for dx in -1isize..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = x as isize + dx;
            let ny = y as isize + dy;
            if nx >= 0 && ny >= 0 && nx < width as isize && ny < height as isize {
                let next = ny as usize * width + nx as usize;
                if pixels[next] {
                    output.push(next);
                }
            }
        }
    }
    output
}

fn pixel_center(index: usize, width: usize) -> Pt {
    Pt::new((index % width) as f64 + 0.5, (index / width) as f64 + 0.5)
}

fn pixel_width(distance_squared: f64) -> f64 {
    2.0 * (distance_squared.sqrt() - 0.5).max(0.5)
}

fn link(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn simplify(points: &[Pt], tolerance: f64) -> Vec<Pt> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((start, end)) = stack.pop() {
        let mut farthest = None;
        for index in start + 1..end {
            let distance = point_segment_distance(points[index], points[start], points[end]);
            if farthest.is_none_or(|(_, best)| distance > best) {
                farthest = Some((index, distance));
            }
        }
        if let Some((index, distance)) = farthest {
            if distance > tolerance {
                keep[index] = true;
                stack.push((start, index));
                stack.push((index, end));
            }
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(point, keep)| keep.then_some(*point))
        .collect()
}

fn point_segment_distance(point: Pt, start: Pt, end: Pt) -> f64 {
    let delta = end - start;
    let length_sq = delta.length_sq();
    if length_sq == 0.0 {
        return point.dist(start);
    }
    let t = ((point - start).dot(delta) / length_sq).clamp(0.0, 1.0);
    point.dist(start + delta * t)
}

fn round_64(value: f64) -> f64 {
    (value * 64.0).round() / 64.0
}

fn round_point_64(point: Pt) -> Pt {
    Pt::new(round_64(point.x), round_64(point.y))
}

pub(super) fn median_edge_width(topology: &Topology) -> f64 {
    let mut widths = topology
        .edges
        .iter()
        .map(|edge| edge.width_px)
        .collect::<Vec<_>>();
    widths.sort_by(f64::total_cmp);
    widths[widths.len() / 2]
}

#[allow(clippy::too_many_arguments)]
pub(super) fn materialize(
    topology: &Topology,
    width_px: u32,
    height_px: u32,
    foreground: Paint,
    background: Paint,
    cap: StrokeCap,
    join: StrokeJoin,
) -> Result<ValidatedStrokeScene, LineArtRefusal> {
    let global_width = median_edge_width(topology);
    let vertices = topology
        .vertices
        .iter()
        .map(|vertex| StrokeVertex {
            position: round_point_64(match (vertex.cap_boundary, vertex.cap_inward) {
                (Some(boundary), Some(inward)) if vertex.degree == 1 => match cap {
                    StrokeCap::Butt => boundary,
                    StrokeCap::Round | StrokeCap::Square => {
                        boundary + inward * (0.5 * global_width)
                    }
                },
                _ => vertex.position,
            }),
            style: match vertex.degree {
                1 => StrokeVertexStyle::Cap(cap),
                2 => StrokeVertexStyle::Join(join),
                _ => StrokeVertexStyle::Junction(StrokeJunction::Round),
            },
        })
        .collect();
    let edges = topology
        .edges
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            let interior_nodes = edge.points[1..edge.points.len() - 1]
                .iter()
                .map(|point| ChainNode {
                    pos: *point,
                    join: JoinKind::Corner,
                })
                .collect::<Vec<_>>();
            StrokeEdge {
                id: StrokeEdgeId(index as u32),
                start: StrokeVertexId(edge.start as u32),
                end: StrokeVertexId(edge.end as u32),
                centerline: CurveChain {
                    segments: vec![Segment::Line; interior_nodes.len() + 1],
                    interior_nodes,
                },
                width_px: global_width,
                paint: foreground,
            }
        })
        .collect();
    ValidatedStrokeScene::new(StrokeScene {
        canvas: Canvas {
            width_px,
            height_px,
        },
        vertices,
        edges,
        background,
    })
    .map_err(|error| LineArtRefusal::Graph {
        detail: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_t_skeleton_traces_three_edges_and_one_junction() {
        let mut skeleton = vec![false; 9 * 9];
        for x in 1..8 {
            skeleton[4 * 9 + x] = true;
        }
        for y in 1..5 {
            skeleton[y * 9 + 4] = true;
        }
        let distance = vec![1.0; skeleton.len()];
        let topology = trace(&skeleton, &skeleton, &distance, 9, 9).unwrap();
        assert_eq!(topology.edges.len(), 3);
        assert_eq!(
            topology.vertices.iter().filter(|v| v.degree == 3).count(),
            1
        );
    }
}
