//! Raster-derived Stage-F observations bound to canonical GT face loops.
//!
//! GT supplies only the intervention labels (family sequence, breakpoints) and
//! the scoring reference. The automatic arms preserve the raw Stage-F cut and
//! orientation. Forced arms may cyclicly reindex those same physical samples
//! solely to attach the intervention labels; no authored point is fitted.

use std::collections::BTreeSet;

use vice_evidence::analysis::{analyze_full, ANALYSIS_CONFIG_V1};
use vice_evidence::boundary::{observe_boundaries, BOUNDARY_CONFIG_V1};
use vice_evidence::corridor::CORRIDOR_CONFIG_V1;
use vice_evidence::{BoundaryChain, BoundarySample};
use vice_geom::{ChordTolerancePx, Pt};
use vice_image::{CanonicalImage, IccAssumption};
use vice_ir::{ExteriorModel, HalfEdgeId, LinearRgb, Paint, Segment};

use super::{
    flatten_truth_segment, GeometryExclusion, GeometryOracleConfig, RasterBoundObservation,
};
use crate::gt::build::SceneBuilder;
use crate::gt::corpus::all_groups;
use crate::gt::degradation::{matrix_v1, render_cell, ResizeChain};
use crate::gt::grammar::{flat2_formation, AUTHORING_CANVAS_PX};
use crate::gt::raster::{Psf, RasterProfile};
use crate::gt::split::{Split, SPLIT_POLICY_V1};
use crate::gt::{AuthoredTruth, GtScene};

#[derive(Clone)]
struct TruthLoop {
    face: usize,
    loop_index: usize,
    polyline: Vec<Pt>,
    breakpoint_points: Vec<Pt>,
    families: Vec<vice_fit::SpanFamily>,
    gt_chain: Option<vice_fit::RefitChain>,
}

pub(super) struct ObservationPopulation {
    pub source_groups: usize,
    pub scenes: usize,
    pub attempted: usize,
    pub observations: Vec<RasterBoundObservation>,
    pub exclusions: Vec<GeometryExclusion>,
}

pub(super) fn collect(config: &GeometryOracleConfig) -> Result<ObservationPopulation, String> {
    let groups = all_groups()?;
    let cell = matrix_v1()
        .into_iter()
        .find(|cell| {
            cell.size_px == config.render_size_px
                && cell.profile == RasterProfile::ExactClip
                && cell.psf == Psf::Box
                && cell.resize == ResizeChain::None
                && cell.subpixel_dx == 0.0
                && cell.subpixel_dy == 0.0
                && cell.contrast == 1.0
        })
        .ok_or_else(|| "the declared geometry cell is absent from degradation V1".to_string())?;
    let render_cell_id = cell.id();

    let mut source_groups = 0usize;
    let mut scenes = 0usize;
    let mut attempted = 0usize;
    let mut observations = Vec::new();
    let mut exclusions = Vec::new();

    for group in groups {
        if SPLIT_POLICY_V1.split_of_group(&group) != Split::Development {
            continue;
        }
        source_groups += 1;
        let Some(scene) = group.scenes.first() else {
            continue;
        };
        scenes += 1;
        collect_scene(
            scene,
            group.scenes.len(),
            &cell,
            &render_cell_id,
            config,
            &mut attempted,
            &mut observations,
            &mut exclusions,
        )?;
    }
    for witness in geometry_witnesses()? {
        source_groups += 1;
        scenes += 1;
        collect_scene(
            &witness,
            1,
            &cell,
            &render_cell_id,
            config,
            &mut attempted,
            &mut observations,
            &mut exclusions,
        )?;
    }

    observations.sort_by(|a, b| a.fixture_id.cmp(&b.fixture_id));
    exclusions.sort_by(|a, b| a.fixture_id.cmp(&b.fixture_id).then(a.stage.cmp(b.stage)));
    Ok(ObservationPopulation {
        source_groups,
        scenes,
        attempted,
        observations,
        exclusions,
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_scene(
    scene: &GtScene,
    equivalence_members: usize,
    cell: &crate::gt::degradation::DegradationCell,
    render_cell_id: &str,
    config: &GeometryOracleConfig,
    attempted: &mut usize,
    observations: &mut Vec<RasterBoundObservation>,
    exclusions: &mut Vec<GeometryExclusion>,
) -> Result<(), String> {
    let fixture = render_cell(scene, cell, equivalence_members)?;
    let image = CanonicalImage::from_straight_srgb8(
        fixture.width_px,
        fixture.height_px,
        fixture.rgba8,
        true,
        IccAssumption::NoProfileAssumedSrgb,
    )
    .map_err(|error| error.to_string())?;
    let Some(evidence) = analyze_full(&image, &ANALYSIS_CONFIG_V1, None).chosen else {
        exclusions.push(GeometryExclusion {
            fixture_id: scene.id().to_string(),
            stage: "stage_f_evidence",
            reason: "Flat2 analysis produced no chosen hypothesis".to_string(),
        });
        return Ok(());
    };
    let observed =
        match observe_boundaries(&evidence, 0.95, &BOUNDARY_CONFIG_V1, &CORRIDOR_CONFIG_V1) {
            Ok(observation) => observation,
            Err(error) => {
                exclusions.push(GeometryExclusion {
                    fixture_id: scene.id().to_string(),
                    stage: "stage_f_boundary",
                    reason: error.to_string(),
                });
                return Ok(());
            }
        };
    let truths = truth_loops(scene, config)?;
    let mut used_truths = BTreeSet::new();
    for (chain_index, chain) in observed.chains.into_iter().enumerate() {
        *attempted += 1;
        let fixture_id = format!("{}/stage-f-chain:{chain_index}", scene.id());
        if !chain.closed {
            exclusions.push(GeometryExclusion {
                fixture_id,
                stage: "bind_stage_f_loop",
                reason: "the observed chain is open; no closed face-loop intervention".into(),
            });
            continue;
        }
        let Some((truth_index, match_px)) = truths
            .iter()
            .enumerate()
            .filter(|(index, _)| !used_truths.contains(index))
            .filter_map(|(index, truth)| {
                loop_match_error(&chain, &truth.polyline).map(|error| (index, error))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
        else {
            exclusions.push(GeometryExclusion {
                fixture_id,
                stage: "bind_stage_f_loop",
                reason: "no supported GT face loop can be compared".into(),
            });
            continue;
        };
        if match_px > config.max_stage_f_truth_match_px {
            exclusions.push(GeometryExclusion {
                fixture_id,
                stage: "bind_stage_f_loop",
                reason: format!(
                    "nearest GT face loop is {match_px:.6} px away, above {} px",
                    config.max_stage_f_truth_match_px
                ),
            });
            continue;
        }
        used_truths.insert(truth_index);
        let truth = &truths[truth_index];
        match bind_chain(
            chain,
            truth,
            scene.id(),
            chain_index,
            match_px,
            render_cell_id,
        ) {
            Ok(bound) => observations.push(bound),
            Err(reason) => exclusions.push(GeometryExclusion {
                fixture_id,
                stage: "bind_stage_f_loop",
                reason,
            }),
        }
    }
    Ok(())
}

/// Four certified, development-only witnesses for the exact M6 claims the
/// broad corpus does not happen to place in its development split:
/// heterogeneous span families, representable smooth joints and a smooth
/// cyclic cubic seam. They still enter through the independent raster and
/// production Stage-F extractor; no authored point is handed to the fitter.
fn geometry_witnesses() -> Result<Vec<GtScene>, String> {
    let paint = Paint::OpaqueSolid(LinearRgb {
        r: 0.08,
        g: 0.42,
        b: 0.78,
    });
    let build = |id: &str, points: &[Pt], segments: &[Segment]| -> Result<GtScene, String> {
        let mut builder = SceneBuilder::new(
            AUTHORING_CANVAS_PX,
            AUTHORING_CANVAS_PX,
            flat2_formation(ExteriorModel::Transparent),
        );
        let face = builder.add_face(paint);
        builder
            .add_ring(points, segments, face, SceneBuilder::EXTERIOR)
            .map_err(|error| error.to_string())?;
        GtScene::new(
            id,
            id,
            builder.build().map_err(|error| error.to_string())?,
            AuthoredTruth::new(
                "M6 raster witness with heterogeneous families and exact tangent joins",
                &[],
            ),
            Vec::new(),
        )
        .map_err(|error| error.to_string())
    };

    let mixed_points = [
        Pt::new(216.0, 128.0),
        Pt::new(128.0, 216.0),
        Pt::new(40.0, 128.0),
        Pt::new(128.0, 40.0),
    ];
    let mixed_segments = [
        Segment::Quad {
            ctrl: Pt::new(216.0, 216.0),
        },
        Segment::Cubic {
            ctrl1: Pt::new(80.0, 216.0),
            ctrl2: Pt::new(40.0, 176.0),
        },
        Segment::Quad {
            ctrl: Pt::new(40.0, 40.0),
        },
        Segment::Cubic {
            ctrl1: Pt::new(176.0, 40.0),
            ctrl2: Pt::new(216.0, 80.0),
        },
    ];

    let line_cubic_points = [
        Pt::new(64.0, 64.0),
        Pt::new(192.0, 64.0),
        Pt::new(192.0, 192.0),
        Pt::new(64.0, 192.0),
    ];
    let line_cubic_segments = [
        Segment::Line,
        Segment::Cubic {
            ctrl1: Pt::new(224.0, 96.0),
            ctrl2: Pt::new(224.0, 160.0),
        },
        Segment::Line,
        Segment::Line,
    ];
    let arc_points = [
        Pt::new(208.0, 128.0),
        Pt::new(128.0, 208.0),
        Pt::new(48.0, 128.0),
        Pt::new(128.0, 48.0),
    ];
    let arc_segments: [Segment; 4] = std::array::from_fn(|_| Segment::CircularArc {
        radius_px: 80.0,
        large_arc: false,
        ccw: true,
    });
    let cubic_loop_points = [
        Pt::new(220.0, 128.0),
        Pt::new(128.0, 204.0),
        Pt::new(36.0, 128.0),
        Pt::new(128.0, 52.0),
    ];
    let cubic_loop_segments = [
        Segment::Cubic {
            ctrl1: Pt::new(220.0, 170.0),
            ctrl2: Pt::new(179.0, 204.0),
        },
        Segment::Cubic {
            ctrl1: Pt::new(77.0, 204.0),
            ctrl2: Pt::new(36.0, 170.0),
        },
        Segment::Cubic {
            ctrl1: Pt::new(36.0, 86.0),
            ctrl2: Pt::new(77.0, 52.0),
        },
        Segment::Cubic {
            ctrl1: Pt::new(179.0, 52.0),
            ctrl2: Pt::new(220.0, 86.0),
        },
    ];
    Ok(vec![
        build("m6-witness/mixed-bezier", &mixed_points, &mixed_segments)?,
        build(
            "m6-witness/line-cubic-cornered",
            &line_cubic_points,
            &line_cubic_segments,
        )?,
        build("m6-witness/four-arc-circle", &arc_points, &arc_segments)?,
        build(
            "m6-witness/smooth-cubic-loop",
            &cubic_loop_points,
            &cubic_loop_segments,
        )?,
    ])
}

fn truth_loops(
    scene: &crate::gt::GtScene,
    config: &GeometryOracleConfig,
) -> Result<Vec<TruthLoop>, String> {
    let graph = scene.scene().graph();
    let scale = f64::from(config.render_size_px) / f64::from(AUTHORING_CANVAS_PX);
    let transform = |point: Pt| point * scale;
    let tolerance =
        ChordTolerancePx::new(config.truth_chord_tolerance_px / scale.max(f64::MIN_POSITIVE))
            .ok_or_else(|| "invalid truth chord tolerance".to_string())?;
    let mut loops = Vec::new();
    for (face_index, face) in graph.faces.iter().enumerate() {
        if face_index == graph.exterior.index() {
            continue;
        }
        for (loop_index, &start_half_edge) in face.loops.iter().enumerate() {
            let mut current = start_half_edge;
            let mut visited = BTreeSet::new();
            let mut loop_half_edges = Vec::new();
            let mut polyline = Vec::new();
            let mut breakpoint_points = Vec::new();
            let mut families = Vec::new();
            loop {
                if !visited.insert(current.index()) {
                    if current != start_half_edge {
                        return Err(format!(
                            "{} face {face_index} loop {loop_index} repeats half-edge {} before closure",
                            scene.id(),
                            current.index()
                        ));
                    }
                    break;
                }
                loop_half_edges.push(current);
                append_half_edge(
                    graph,
                    current,
                    tolerance,
                    &mut polyline,
                    &mut breakpoint_points,
                    &mut families,
                )?;
                current = graph.half_edges[current.index()].next;
            }
            if polyline.len() < 3 || families.is_empty() {
                continue;
            }
            for point in &mut polyline {
                *point = transform(*point);
            }
            for point in &mut breakpoint_points {
                *point = transform(*point);
            }
            let gt_chain = lift_truth_loop(graph, &loop_half_edges, scale);
            loops.push(TruthLoop {
                face: face_index,
                loop_index,
                polyline,
                breakpoint_points,
                families,
                gt_chain,
            });
        }
    }
    Ok(loops)
}

fn lift_truth_loop(
    graph: &vice_ir::PlanarGraph,
    half_edges: &[HalfEdgeId],
    scale: f64,
) -> Option<vice_fit::RefitChain> {
    let mut combined: Option<vice_fit::RefitChain> = None;
    for &half_edge_id in half_edges {
        let half_edge = graph.half_edges[half_edge_id.index()];
        let boundary = &graph.boundaries[half_edge.boundary.index()];
        let start = graph.vertices[boundary.start_vertex.index()].pos;
        let end = graph.vertices[boundary.end_vertex.index()].pos;
        let closure_join = (boundary.start_vertex == boundary.end_vertex)
            .then_some(boundary.closure_join)
            .flatten();
        let mut piece = vice_fit::refit_chain_from_ir(
            start,
            end,
            &boundary.curve,
            closure_join,
            half_edge.forward,
        )
        .ok()?;
        if let Some(chain) = &mut combined {
            let shared = chain.nodes.last_mut()?;
            let piece_start = piece.nodes.first_mut()?;
            if shared.pos != piece_start.pos {
                return None;
            }
            // A join between graph boundaries is a graph vertex. In the
            // canonical GT corpus it is a corner unless an explicit relation
            // says otherwise; never invent a smooth parameter while lifting.
            shared.tangent_rad = None;
            piece_start.tangent_rad = None;
            chain.nodes.extend(piece.nodes.into_iter().skip(1));
            chain.segments.extend(piece.segments);
        } else {
            combined = Some(piece);
        }
    }
    let mut chain = combined?;
    if chain.nodes.first()?.pos != chain.nodes.last()?.pos {
        return None;
    }
    scale_refit_chain(&mut chain, scale);
    chain.lower_boundary_geometry().ok()?;
    Some(chain)
}

fn scale_refit_chain(chain: &mut vice_fit::RefitChain, scale: f64) {
    for node in &mut chain.nodes {
        node.pos = node.pos * scale;
    }
    for segment in &mut chain.segments {
        match segment {
            vice_fit::RefitSegment::Line
            | vice_fit::RefitSegment::Arc(
                vice_fit::ArcAnchor::FromHeadTangent | vice_fit::ArcAnchor::FromTailTangent,
            ) => {}
            vice_fit::RefitSegment::Arc(vice_fit::ArcAnchor::Radius { radius_px, .. }) => {
                *radius_px *= scale;
            }
            vice_fit::RefitSegment::Quad { ctrl } => scale_handle(ctrl, scale),
            vice_fit::RefitSegment::Cubic { head, tail } => {
                scale_handle(head, scale);
                scale_handle(tail, scale);
            }
        }
    }
}

fn scale_handle(handle: &mut vice_fit::Handle, scale: f64) {
    match handle {
        vice_fit::Handle::Free(point) => *point = *point * scale,
        vice_fit::Handle::Shared { length_px } => *length_px *= scale,
    }
}

fn append_half_edge(
    graph: &vice_ir::PlanarGraph,
    half_edge_id: HalfEdgeId,
    tolerance: ChordTolerancePx,
    polyline: &mut Vec<Pt>,
    breakpoint_points: &mut Vec<Pt>,
    families: &mut Vec<vice_fit::SpanFamily>,
) -> Result<(), String> {
    let half_edge = graph.half_edges[half_edge_id.index()];
    let boundary = &graph.boundaries[half_edge.boundary.index()];
    let start = graph.vertices[boundary.start_vertex.index()].pos;
    let end = graph.vertices[boundary.end_vertex.index()].pos;
    let nodes = boundary.curve.node_positions(start, end);
    let mut pieces = Vec::new();
    for (index, segment) in boundary.curve.segments.iter().enumerate() {
        let family = family_of(segment)?;
        let points = flatten_truth_segment(segment, nodes[index], nodes[index + 1], tolerance)?;
        pieces.push((family, points));
    }
    if !half_edge.forward {
        pieces.reverse();
        for (_, points) in &mut pieces {
            points.reverse();
        }
    }
    for (piece_index, (family, points)) in pieces.into_iter().enumerate() {
        if !families.is_empty() || piece_index > 0 {
            breakpoint_points.push(points[0]);
        }
        if polyline.is_empty() {
            polyline.extend(points);
        } else {
            polyline.extend(points.into_iter().skip(1));
        }
        families.push(family);
    }
    Ok(())
}

fn family_of(segment: &Segment) -> Result<vice_fit::SpanFamily, String> {
    match segment {
        Segment::Line => Ok(vice_fit::SpanFamily::Line),
        Segment::CircularArc { .. } => Ok(vice_fit::SpanFamily::CircularArc),
        Segment::Quad { .. } => Ok(vice_fit::SpanFamily::Quad),
        Segment::Cubic { .. } => Ok(vice_fit::SpanFamily::Cubic),
        Segment::EllipticArc { .. } => Err("elliptic arc is outside the M6 fit universe".into()),
    }
}

fn loop_match_error(chain: &BoundaryChain, truth: &[Pt]) -> Option<f64> {
    if chain.samples.len() < 3 || truth.len() < 3 {
        return None;
    }
    let observed: Vec<Pt> = chain.samples.iter().map(|sample| sample.p).collect();
    let a = observed
        .iter()
        .map(|point| vice_fit::cost::euclidean_deviation(*point, truth))
        .fold(0.0f64, f64::max);
    let b = truth
        .iter()
        .map(|point| vice_fit::cost::euclidean_deviation(*point, &observed))
        .fold(0.0f64, f64::max);
    Some(a.max(b))
}

fn bind_chain(
    chain: BoundaryChain,
    truth: &TruthLoop,
    scene_id: &str,
    chain_index: usize,
    match_px: f64,
    render_cell: &str,
) -> Result<RasterBoundObservation, String> {
    let (forced_chain, breakpoint_indices) = align_forced_chain(&chain, truth)?;
    let mut raw_points: Vec<(u64, u64)> = chain
        .samples
        .iter()
        .map(|sample| (sample.p.x.to_bits(), sample.p.y.to_bits()))
        .collect();
    let mut forced_points: Vec<(u64, u64)> = forced_chain
        .samples
        .iter()
        .map(|sample| (sample.p.x.to_bits(), sample.p.y.to_bits()))
        .collect();
    raw_points.sort_unstable();
    forced_points.sort_unstable();
    if raw_points != forced_points {
        return Err("GT label alignment changed the Stage-F sample population".to_string());
    }
    Ok(RasterBoundObservation {
        fixture_id: format!(
            "{scene_id}/face:{}/loop:{}/stage-f-chain:{chain_index}",
            truth.face, truth.loop_index
        ),
        scene_id: scene_id.to_string(),
        boundary_id: truth.face,
        chain,
        forced_chain,
        truth: truth.polyline.clone(),
        gt_chain: truth.gt_chain.clone(),
        gt_families: truth.families.clone(),
        gt_breakpoints: breakpoint_indices,
        stage_f_truth_match_px: match_px,
        render_cell: render_cell.to_string(),
    })
}

fn align_forced_chain(
    chain: &BoundaryChain,
    truth: &TruthLoop,
) -> Result<(BoundaryChain, Vec<usize>), String> {
    let start = truth.polyline[0];
    let cut = chain
        .samples
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (a.p - start)
                .length_sq()
                .total_cmp(&(b.p - start).length_sq())
        })
        .map(|(index, _)| index)
        .ok_or_else(|| "empty Stage-F chain".to_string())?;
    let n = chain.samples.len();
    let mut samples: Vec<BoundarySample> = (0..n)
        .map(|index| chain.samples[(cut + index) % n])
        .collect();
    if n > 2 && truth.polyline.len() > 2 {
        let forward = (samples[1].p - truth.polyline[1]).length_sq();
        let reverse = (samples[n - 1].p - truth.polyline[1]).length_sq();
        if reverse < forward {
            let mut reversed = Vec::with_capacity(n);
            let mut first = samples[0];
            first.normal = first.normal * -1.0;
            reversed.push(first);
            reversed.extend(samples[1..].iter().rev().map(|sample| {
                let mut sample = *sample;
                sample.normal = sample.normal * -1.0;
                sample
            }));
            samples = reversed;
        }
    }

    let mut breakpoint_indices = Vec::new();
    for point in &truth.breakpoint_points {
        let index = samples
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                (a.p - *point)
                    .length_sq()
                    .total_cmp(&(b.p - *point).length_sq())
            })
            .map(|(index, _)| index)
            .ok_or_else(|| "no Stage-F samples for a GT breakpoint".to_string())?;
        if index > 0 && index < samples.len() {
            breakpoint_indices.push(index);
        }
    }
    breakpoint_indices.sort_unstable();
    breakpoint_indices.dedup();
    if breakpoint_indices.len() + 1 != truth.families.len() {
        return Err(format!(
            "{} GT spans collapsed to {} distinct Stage-F breakpoints",
            truth.families.len(),
            breakpoint_indices.len()
        ));
    }
    let length_px: f64 = samples
        .windows(2)
        .map(|pair| (pair[1].p - pair[0].p).length())
        .sum::<f64>()
        + (samples[0].p - samples[samples.len() - 1].p).length();
    Ok((
        BoundaryChain {
            samples,
            closed: true,
            length_px,
            corr_length_px: chain.corr_length_px,
            vertices: chain.vertices,
        },
        breakpoint_indices,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_multi_boundary_gt_loop_lifts_as_the_whole_loop() {
        let mut builder = SceneBuilder::new(16, 16, flat2_formation(ExteriorModel::Transparent));
        let face = builder.add_face(Paint::OpaqueSolid(LinearRgb {
            r: 0.2,
            g: 0.4,
            b: 0.6,
        }));
        builder
            .add_polygon_ring(
                &[
                    Pt::new(2.0, 2.0),
                    Pt::new(14.0, 2.0),
                    Pt::new(14.0, 14.0),
                    Pt::new(2.0, 14.0),
                ],
                face,
                SceneBuilder::EXTERIOR,
            )
            .unwrap();
        let scene = builder.build().unwrap();
        let graph = scene.graph();
        let start = graph.faces[face].loops[0];
        let mut half_edges = Vec::new();
        let mut current = start;
        loop {
            half_edges.push(current);
            current = graph.half_edges[current.index()].next;
            if current == start {
                break;
            }
        }
        let lifted = lift_truth_loop(graph, &half_edges, 1.0).expect("whole loop lifts");
        assert_eq!(lifted.segments.len(), 4);
        assert_eq!(lifted.nodes.len(), 5);
        assert_eq!(
            lifted.nodes.first().unwrap().pos,
            lifted.nodes.last().unwrap().pos
        );
        assert_eq!(vice_fit::solve::flatten_chain(&lifted).unwrap().len(), 5);
    }

    #[test]
    fn gt_label_alignment_does_not_recut_the_automatic_stage_f_chain() {
        let points = [
            Pt::new(10.0, 10.0),
            Pt::new(0.0, 10.0),
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 0.0),
        ];
        let samples = points
            .iter()
            .map(|point| BoundarySample {
                p: *point,
                normal: Pt::new(0.0, 1.0),
                halfwidth: 0.35,
                confidence: 1.0,
                weight_ds: 10.0,
                corr_length_px: 1.0,
            })
            .collect();
        let raw = BoundaryChain {
            samples,
            closed: true,
            length_px: 40.0,
            corr_length_px: 1.0,
            vertices: 4,
        };
        let truth = TruthLoop {
            face: 1,
            loop_index: 0,
            polyline: vec![
                Pt::new(0.0, 0.0),
                Pt::new(10.0, 0.0),
                Pt::new(10.0, 10.0),
                Pt::new(0.0, 10.0),
            ],
            breakpoint_points: Vec::new(),
            families: vec![vice_fit::SpanFamily::Line],
            gt_chain: None,
        };

        let bound = bind_chain(raw.clone(), &truth, "scene", 0, 0.0, "cell").expect("binds labels");
        assert_eq!(bound.chain, raw, "automatic evidence was GT-reindexed");
        assert_ne!(
            bound.forced_chain.samples[0].p, bound.chain.samples[0].p,
            "fixture must exercise a different GT label seam"
        );
        assert_eq!(bound.forced_chain.samples[0].p, truth.polyline[0]);
    }
}
