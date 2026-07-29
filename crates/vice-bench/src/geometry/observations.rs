//! Raster-derived Stage-F observations bound to canonical GT face loops.
//!
//! GT supplies only the intervention labels (family sequence, breakpoints) and
//! the scoring reference. The chain fitted by every arm comes from the real
//! render → decode → Flat2 evidence → boundary-observation path.

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

/// Three certified, development-only witnesses for the exact M6 claims the
/// broad corpus does not happen to place in its development split:
/// heterogeneous span families and representable smooth joints. They still
/// enter through the independent raster and production Stage-F extractor;
/// no authored point is handed to the fitter.
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
    Ok(vec![
        build("m6-witness/mixed-bezier", &mixed_points, &mixed_segments)?,
        build(
            "m6-witness/line-cubic-cornered",
            &line_cubic_points,
            &line_cubic_segments,
        )?,
        build("m6-witness/four-arc-circle", &arc_points, &arc_segments)?,
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
            loops.push(TruthLoop {
                face: face_index,
                loop_index,
                polyline,
                breakpoint_points,
                families,
            });
        }
    }
    Ok(loops)
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
    Ok(RasterBoundObservation {
        fixture_id: format!(
            "{scene_id}/face:{}/loop:{}/stage-f-chain:{chain_index}",
            truth.face, truth.loop_index
        ),
        scene_id: scene_id.to_string(),
        boundary_id: truth.face,
        chain: BoundaryChain {
            samples,
            closed: true,
            length_px,
            corr_length_px: chain.corr_length_px,
            vertices: chain.vertices,
        },
        truth: truth.polyline.clone(),
        gt_families: truth.families.clone(),
        gt_breakpoints: breakpoint_indices,
        stage_f_truth_match_px: match_px,
        render_cell: render_cell.to_string(),
    })
}
