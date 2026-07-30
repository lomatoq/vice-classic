use std::cell::RefCell;
use std::collections::BTreeMap;

use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_evidence::{BackgroundHypothesis, BoundaryChain, Flat2Evidence};
use vice_fit::{BoundaryModel, LoweredBoundaryGeometry, SelectedBoundaryGeometry};
use vice_geom::Pt;
use vice_ir::{
    Boundary, BoundaryId, Canvas, ChainNode, CurveChain, Face, FaceId, GraphVertex, HalfEdge,
    HalfEdgeId, JoinKind, LinearRgb, Paint, PlanarGraph, Segment, VectorScene, VertexId,
};
use vice_opt::{
    apply_compound_transaction_traced, optimize_best_deterministic, BlockSpec, CompoundTransaction,
    OptimizationResult, PriorCodeLengths, SceneMutation, ScoreScope, TransactionApplication,
    TransactionKind, TrustRegionProblem,
};
use vice_render::PartitionRender;
use vice_topology::{audit, Dcel};
use vice_verify::{topology_signature_sha256, BoundaryBinding};

use crate::config::CoreConfig;
use crate::types::{TopologyArmRefusal, TopologyArmTrace};

#[derive(Debug, Clone)]
pub(crate) struct TopologyArm {
    /// Visible combinatorial equivalence class. Distinct event-level
    /// labellings with the same Flat2 component/hole structure remain
    /// separate search arms but share posterior topology mass.
    pub topology_class: String,
    pub class: String,
    pub dcel: Dcel,
    pub chains: Vec<BoundaryChain>,
    pub trace: TopologyArmTrace,
    /// Evidence-chain index -> canonical DCEL boundary index.
    pub chain_to_boundary: Vec<usize>,
    pub dcel_boundary_sha256: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct TopologyArmSet {
    pub proposal: vice_topology::Proposal,
    pub arms: Vec<TopologyArm>,
    pub traces: Vec<TopologyArmTrace>,
    pub refusals: Vec<TopologyArmRefusal>,
}

#[derive(Debug, Clone)]
pub(crate) struct PaintLayout {
    pub foreground: Vec<FaceId>,
    pub background: Vec<FaceId>,
}

#[derive(Debug, Clone)]
pub(crate) struct SceneCandidate {
    pub scene: VectorScene,
    pub bindings: Vec<BoundaryBinding>,
    pub paint_layout: PaintLayout,
}

fn digest<T: Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn point_segment_distance(point: Pt, a: Pt, b: Pt) -> f64 {
    let segment = b - a;
    let length_sq = segment.length_sq();
    if length_sq == 0.0 {
        point.dist(a)
    } else {
        let t = ((point - a).dot(segment) / length_sq).clamp(0.0, 1.0);
        point.dist(a + segment * t)
    }
}

fn point_polyline_distance(point: Pt, polyline: &[Pt]) -> f64 {
    polyline
        .windows(2)
        .map(|segment| point_segment_distance(point, segment[0], segment[1]))
        .fold(f64::INFINITY, f64::min)
}

fn chain_boundary_distance(chain: &BoundaryChain, boundary: &vice_topology::Boundary) -> f64 {
    let path: Vec<Pt> = boundary
        .path
        .iter()
        .map(|&(x, y)| Pt::new(f64::from(x), f64::from(y)))
        .collect();
    let forward = chain
        .samples
        .iter()
        .map(|sample| point_polyline_distance(sample.p, &path))
        .fold(0.0f64, f64::max);
    let evidence = closed_support(chain);
    let reverse = path
        .iter()
        .map(|point| point_polyline_distance(*point, &evidence))
        .fold(0.0f64, f64::max);
    forward.max(reverse)
}

fn bind_chains_to_dcel(chains: &[BoundaryChain], dcel: &Dcel) -> Result<Vec<usize>, String> {
    if chains.len() != dcel.boundaries().len() {
        return Err(format!(
            "{} observed chains cannot bind bijectively to {} DCEL boundaries",
            chains.len(),
            dcel.boundaries().len()
        ));
    }
    let mut pairs = Vec::new();
    for (chain_index, chain) in chains.iter().enumerate() {
        for (boundary_index, boundary) in dcel.boundaries().iter().enumerate() {
            pairs.push((
                chain_boundary_distance(chain, boundary),
                chain_index,
                boundary_index,
            ));
        }
    }
    pairs.sort_by(|left, right| {
        left.0
            .total_cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    let mut mapping = vec![usize::MAX; chains.len()];
    let mut boundary_used = vec![false; dcel.boundaries().len()];
    for (distance, chain, boundary) in pairs {
        if mapping[chain] == usize::MAX && !boundary_used[boundary] {
            let allowance = chains[chain]
                .samples
                .iter()
                .map(|sample| sample.halfwidth)
                .fold(0.0f64, f64::max)
                + 2.0;
            if !distance.is_finite() || distance > allowance {
                return Err(format!(
                    "chain {chain} is {distance:.6}px from DCEL boundary {boundary}, above {allowance:.6}px"
                ));
            }
            mapping[chain] = boundary;
            boundary_used[boundary] = true;
        }
    }
    if mapping.contains(&usize::MAX) || boundary_used.contains(&false) {
        Err("observed-chain/DCEL matching was not bijective".into())
    } else {
        Ok(mapping)
    }
}

pub(crate) fn topology_arms(evidence: &Flat2Evidence) -> TopologyArmSet {
    let proposal = vice_topology::propose(
        &[vice_topology::CoverageObservation {
            palette_id: evidence.hypothesis.id.clone(),
            formation_id: vice_evidence::formation_id(&evidence.formation),
            filter: evidence.formation.pixel_filter,
            filter_identifiable: vice_evidence::filter_is_identifiable(evidence.alpha_field()),
            alpha: evidence.alpha_field(),
            width_px: evidence.width_px() as usize,
            height_px: evidence.height_px() as usize,
        }],
        &vice_topology::TOPOLOGY_CONFIG_V1,
    );
    let mut arms = Vec::new();
    let mut traces = Vec::new();
    let mut refusals = Vec::new();
    let canonical_observation = vice_evidence::observe_boundaries(
        evidence,
        vice_evidence::ANALYSIS_CONFIG_V1.coverage_level,
        &vice_evidence::BOUNDARY_CONFIG_V1,
        &vice_evidence::CORRIDOR_CONFIG_V1,
    )
    .ok()
    .filter(|observation| {
        !observation.chains.is_empty() && observation.chains.iter().all(|chain| chain.closed)
    });
    for hypothesis in &proposal.envelope.hypotheses {
        let connectivity = vice_ir::ComplementaryConnectivity::new(
            if hypothesis.signature.foreground_connectivity == "4" {
                vice_ir::PixelConnectivity::Four
            } else {
                vice_ir::PixelConnectivity::Eight
            },
        );
        let refusal = |detail: String| TopologyArmRefusal {
            topology_class: format!(
                "flat2-components{}-holes{}",
                hypothesis.signature.components, hypothesis.signature.holes
            ),
            signature_sha256: hypothesis.signature.digest.clone(),
            foreground_connectivity: hypothesis.signature.foreground_connectivity.to_string(),
            field: hypothesis.provenance.field,
            saddle: hypothesis.provenance.saddle,
            extraction_level: hypothesis.provenance.level,
            detail,
        };
        let observation = match vice_evidence::observe_boundaries(
            evidence,
            vice_evidence::ANALYSIS_CONFIG_V1.coverage_level,
            &vice_evidence::BoundaryConfig {
                level: hypothesis.provenance.level,
                ..vice_evidence::BOUNDARY_CONFIG_V1
            },
            &vice_evidence::CORRIDOR_CONFIG_V1,
        ) {
            Ok(observation) => observation,
            Err(error) => {
                refusals.push(refusal(format!("boundary observation refused: {error}")));
                continue;
            }
        };
        if observation.chains.is_empty() || observation.chains.iter().any(|chain| !chain.closed) {
            refusals.push(refusal(
                "topology hypothesis did not produce one or more closed observed chains".into(),
            ));
            continue;
        }
        let event_chains = observation.chains;
        let dcel = Dcel::assemble(hypothesis.labelling.clone(), connectivity);
        if let Err(error) = audit(&dcel) {
            refusals.push(refusal(error.to_string()));
            continue;
        }
        if dcel
            .boundaries()
            .iter()
            .any(|boundary| boundary.start != boundary.end)
            || dcel.vertices().len() != dcel.boundaries().len()
        {
            refusals.push(refusal(
                "selective Flat2 core currently requires audited closed self-loop boundaries"
                    .into(),
            ));
            continue;
        }
        let canonical_binding = canonical_observation.as_ref().and_then(|canonical| {
            bind_chains_to_dcel(&canonical.chains, &dcel)
                .ok()
                .map(|binding| (canonical.chains.clone(), binding))
        });
        let (chains, chain_to_boundary, fit_observation_level) =
            if let Some((chains, binding)) = canonical_binding {
                (chains, binding, vice_evidence::BOUNDARY_CONFIG_V1.level)
            } else {
                let binding = match bind_chains_to_dcel(&event_chains, &dcel) {
                    Ok(binding) => binding,
                    Err(error) => {
                        refusals.push(refusal(error));
                        continue;
                    }
                };
                (event_chains, binding, hypothesis.provenance.level)
            };
        let dcel_boundary_sha256 = match dcel
            .boundaries()
            .iter()
            .map(digest)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(digests) => digests,
            Err(error) => {
                refusals.push(refusal(error));
                continue;
            }
        };
        let class = format!(
            "fg{}-bg{}:{}",
            hypothesis.signature.foreground_connectivity,
            hypothesis.signature.background_connectivity,
            hypothesis.signature.digest
        );
        let topology_class = format!(
            "flat2-components{}-holes{}",
            hypothesis.signature.components, hypothesis.signature.holes
        );
        let trace = TopologyArmTrace {
            class: class.clone(),
            topology_class: topology_class.clone(),
            signature_sha256: hypothesis.signature.digest.clone(),
            components: hypothesis.signature.components,
            holes: hypothesis.signature.holes,
            foreground_connectivity: hypothesis.signature.foreground_connectivity.to_string(),
            field: hypothesis.provenance.field,
            saddle: hypothesis.provenance.saddle,
            extraction_level: hypothesis.provenance.level,
            fit_observation_level,
            observed_chains: chains.len(),
            fit_models_per_chain: Vec::new(),
        };
        traces.push(trace.clone());
        arms.push(TopologyArm {
            topology_class,
            class,
            dcel,
            chains,
            trace,
            chain_to_boundary,
            dcel_boundary_sha256,
        });
    }
    let mut sorted_arms = Vec::with_capacity(arms.len());
    let mut sorted_traces = Vec::with_capacity(traces.len());
    let mut seen = std::collections::BTreeSet::new();
    for index in 0..arms.len() {
        if !seen.insert(arms[index].class.clone()) {
            continue;
        }
        sorted_arms.push(arms[index].clone());
        sorted_traces.push(traces[index].clone());
    }
    TopologyArmSet {
        proposal,
        arms: sorted_arms,
        traces: sorted_traces,
        refusals,
    }
}

fn reverse_join(join: JoinKind) -> JoinKind {
    match join {
        JoinKind::Corner => JoinKind::Corner,
        JoinKind::SmoothG1 { tangent_angle_rad } => JoinKind::SmoothG1 {
            tangent_angle_rad: vice_fit::canonical_angle(tangent_angle_rad + std::f64::consts::PI),
        },
    }
}

fn reverse_segment(segment: &Segment) -> Segment {
    match *segment {
        Segment::Line => Segment::Line,
        Segment::CircularArc {
            radius_px,
            large_arc,
            ccw,
        } => Segment::CircularArc {
            radius_px,
            large_arc,
            ccw: !ccw,
        },
        Segment::EllipticArc {
            rx_px,
            ry_px,
            x_axis_rotation_rad,
            large_arc,
            ccw,
        } => Segment::EllipticArc {
            rx_px,
            ry_px,
            x_axis_rotation_rad,
            large_arc,
            ccw: !ccw,
        },
        Segment::Quad { ctrl } => Segment::Quad { ctrl },
        Segment::Cubic { ctrl1, ctrl2 } => Segment::Cubic {
            ctrl1: ctrl2,
            ctrl2: ctrl1,
        },
    }
}

fn reverse_boundary(mut geometry: LoweredBoundaryGeometry) -> LoweredBoundaryGeometry {
    geometry.curve.interior_nodes.reverse();
    for node in &mut geometry.curve.interior_nodes {
        node.join = reverse_join(node.join);
    }
    geometry.curve.segments = geometry
        .curve
        .segments
        .iter()
        .rev()
        .map(reverse_segment)
        .collect();
    geometry.closure_join = geometry.closure_join.map(reverse_join);
    geometry
}

fn signed_area(polyline: &[Pt]) -> f64 {
    if polyline.len() < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    for index in 0..polyline.len() {
        let next = (index + 1) % polyline.len();
        area += polyline[index].x * polyline[next].y - polyline[next].x * polyline[index].y;
    }
    area * 0.5
}

fn lower_model(
    model: &BoundaryModel,
    desired_signed_area: f64,
) -> Result<(Pt, LoweredBoundaryGeometry), String> {
    let (start, lowered) = match &model.geometry {
        SelectedBoundaryGeometry::TypedChain { chain } => (
            chain.start(),
            chain
                .lower_boundary_geometry()
                .map_err(|error| format!("{error:?}"))?,
        ),
        SelectedBoundaryGeometry::LoopPrimitive { kind, geometry, .. } => {
            let lowered = vice_fit::lower_loop_primitive(*kind, *geometry)
                .ok_or_else(|| "native primitive lowering refused".to_string())?;
            (lowered.start, lowered.boundary)
        }
    };
    let node_polygon = lowered.curve.node_positions(start, start);
    let mut orientation_area = signed_area(&node_polygon);
    if orientation_area == 0.0 {
        orientation_area = signed_area(
            &model
                .geometry
                .flatten()
                .map_err(|error| format!("{error:?}"))?,
        );
    }
    if desired_signed_area == 0.0 || orientation_area == 0.0 {
        return Err("DCEL boundary has zero signed area".into());
    }
    Ok(if orientation_area * desired_signed_area < 0.0 {
        (start, reverse_boundary(lowered))
    } else {
        (start, lowered)
    })
}

fn canvas_curve(canvas: Canvas) -> CurveChain {
    CurveChain {
        interior_nodes: vec![
            ChainNode {
                pos: Pt::new(f64::from(canvas.width_px), 0.0),
                join: JoinKind::Corner,
            },
            ChainNode {
                pos: Pt::new(f64::from(canvas.width_px), f64::from(canvas.height_px)),
                join: JoinKind::Corner,
            },
            ChainNode {
                pos: Pt::new(0.0, f64::from(canvas.height_px)),
                join: JoinKind::Corner,
            },
        ],
        segments: vec![Segment::Line; 4],
    }
}

fn closed_support(chain: &BoundaryChain) -> Vec<Pt> {
    let mut points: Vec<Pt> = chain.samples.iter().map(|sample| sample.p).collect();
    if points.len() >= 2 && points.first() != points.last() {
        points.push(points[0]);
    }
    points
}

fn observed_binding(
    scene: &VectorScene,
    boundary: BoundaryId,
    chain: &BoundaryChain,
    dcel_boundary_sha256: String,
) -> Result<BoundaryBinding, String> {
    let topology = topology_signature_sha256(scene).map_err(|error| error.to_string())?;
    let tube = chain
        .samples
        .iter()
        .map(|sample| sample.halfwidth)
        .fold(0.0f64, f64::max)
        + 0.5
            * chain
                .samples
                .iter()
                .map(|sample| sample.weight_ds)
                .fold(0.0f64, f64::max)
        + vice_fit::BINDING_CERTIFICATION_CHORD_TOLERANCE_PX_V1;
    BoundaryBinding::new_observed(
        digest(chain)?,
        dcel_boundary_sha256,
        boundary,
        topology,
        tube,
        closed_support(chain),
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn build_scene_candidate(
    canvas: Canvas,
    evidence: &Flat2Evidence,
    chains: &[BoundaryChain],
    models: &[BoundaryModel],
    arm: &TopologyArm,
    formation: vice_ir::GlobalFormationHypothesis,
) -> Result<SceneCandidate, String> {
    if chains.len() != models.len()
        || chains.len() != arm.chain_to_boundary.len()
        || arm.dcel.boundaries().len() != models.len()
    {
        return Err("scene candidate model/chain/DCEL arity mismatch".into());
    }
    let mut model_by_boundary = vec![usize::MAX; models.len()];
    for (chain_index, &boundary_index) in arm.chain_to_boundary.iter().enumerate() {
        if boundary_index >= model_by_boundary.len()
            || model_by_boundary[boundary_index] != usize::MAX
        {
            return Err("scene candidate chain/DCEL mapping is not bijective".into());
        }
        model_by_boundary[boundary_index] = chain_index;
    }
    if model_by_boundary.contains(&usize::MAX) {
        return Err("scene candidate has an unbound DCEL boundary".into());
    }
    let mut lowered = Vec::with_capacity(models.len());
    for (boundary_index, boundary) in arm.dcel.boundaries().iter().enumerate() {
        let chain_index = model_by_boundary[boundary_index];
        let dcel_path: Vec<Pt> = boundary
            .path
            .iter()
            .map(|&(x, y)| Pt::new(f64::from(x), f64::from(y)))
            .collect();
        lowered.push(lower_model(&models[chain_index], -signed_area(&dcel_path))?);
    }
    let foreground = Paint::OpaqueSolid(evidence.hypothesis.foreground.center());
    let opaque_background = match evidence.hypothesis.background {
        BackgroundHypothesis::TransparentExterior => None,
        BackgroundHypothesis::OpaqueFace(background) => {
            Some(Paint::OpaqueSolid(background.center()))
        }
    };
    let boundary_offset = u32::from(opaque_background.is_some());
    let half_edge_offset = boundary_offset * 2;
    let face_offset = boundary_offset;
    let vertex_offset = boundary_offset;

    let mut vertices = Vec::new();
    let mut boundaries = Vec::new();
    let mut half_edges = Vec::new();
    let mut faces = Vec::new();
    if opaque_background.is_some() {
        vertices.push(GraphVertex {
            pos: Pt::new(0.0, 0.0),
        });
        boundaries.push(Boundary {
            left_face: FaceId(1),
            right_face: FaceId(0),
            start_vertex: VertexId(0),
            end_vertex: VertexId(0),
            closure_join: Some(JoinKind::Corner),
            curve: canvas_curve(canvas),
        });
        half_edges.extend([
            HalfEdge {
                boundary: BoundaryId(0),
                forward: true,
                twin: HalfEdgeId(1),
                next: HalfEdgeId(0),
                face: FaceId(1),
            },
            HalfEdge {
                boundary: BoundaryId(0),
                forward: false,
                twin: HalfEdgeId(0),
                next: HalfEdgeId(1),
                face: FaceId(0),
            },
        ]);
        faces.push(Face {
            loops: vec![HalfEdgeId(1)],
            paint: Paint::TransparentExterior,
        });
    }

    for (boundary_index, boundary) in arm.dcel.boundaries().iter().enumerate() {
        let (start, geometry) = &lowered[boundary_index];
        let vertex = VertexId(boundary_index as u32 + vertex_offset);
        vertices.push(GraphVertex { pos: *start });
        boundaries.push(Boundary {
            left_face: FaceId(boundary.owners.left().0 + face_offset),
            right_face: FaceId(boundary.owners.right().0 + face_offset),
            start_vertex: vertex,
            end_vertex: vertex,
            closure_join: geometry.closure_join,
            curve: geometry.curve.clone(),
        });
    }
    for half_edge in arm.dcel.half_edges() {
        half_edges.push(HalfEdge {
            boundary: BoundaryId(half_edge.boundary().0 + boundary_offset),
            forward: half_edge.is_forward(),
            twin: HalfEdgeId(half_edge.twin().0 + half_edge_offset),
            next: HalfEdgeId(arm.dcel.next(half_edge).0 + half_edge_offset),
            face: FaceId(arm.dcel.face_of(half_edge).0 + face_offset),
        });
    }

    let mut foreground_faces = Vec::new();
    let mut background_faces = Vec::new();
    for (face_index, face) in arm.dcel.faces().iter().enumerate() {
        let mapped = FaceId(face_index as u32 + face_offset);
        let mut loops: Vec<HalfEdgeId> = face
            .loops
            .iter()
            .map(|cycle| HalfEdgeId(cycle[0].0 + half_edge_offset))
            .collect();
        let paint = if face.label {
            foreground_faces.push(mapped);
            foreground
        } else if let Some(background) = opaque_background {
            background_faces.push(mapped);
            background
        } else {
            Paint::TransparentExterior
        };
        if opaque_background.is_some() && face_index == 0 {
            loops.insert(0, HalfEdgeId(0));
        }
        faces.push(Face { loops, paint });
    }

    let scene = VectorScene {
        canvas,
        graph: PlanarGraph {
            exterior: FaceId(0),
            vertices,
            boundaries,
            half_edges,
            faces,
        },
        formation,
    };
    vice_ir::validate_scene(&scene).map_err(|error| error.to_string())?;
    let mut bindings = Vec::with_capacity(scene.graph.boundaries.len());
    if opaque_background.is_some() {
        let topology = topology_signature_sha256(&scene).map_err(|error| error.to_string())?;
        bindings.push(
            BoundaryBinding::new_canvas_closure(
                canvas,
                BoundaryId(0),
                topology,
                vec![
                    Pt::new(0.0, 0.0),
                    Pt::new(f64::from(canvas.width_px), 0.0),
                    Pt::new(f64::from(canvas.width_px), f64::from(canvas.height_px)),
                    Pt::new(0.0, f64::from(canvas.height_px)),
                    Pt::new(0.0, 0.0),
                ],
            )
            .map_err(|error| error.to_string())?,
        );
    }
    for (chain_index, chain) in chains.iter().enumerate() {
        let dcel_boundary = arm.chain_to_boundary[chain_index];
        bindings.push(observed_binding(
            &scene,
            BoundaryId(dcel_boundary as u32 + boundary_offset),
            chain,
            arm.dcel_boundary_sha256[dcel_boundary].clone(),
        )?);
    }
    Ok(SceneCandidate {
        scene,
        bindings,
        paint_layout: PaintLayout {
            foreground: foreground_faces,
            background: background_faces,
        },
    })
}

mod paint;
pub(crate) use paint::optimize_paint;
