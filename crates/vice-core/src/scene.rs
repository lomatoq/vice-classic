use std::time::Instant;

use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_evidence::{BackgroundHypothesis, BoundaryChain, Flat2Evidence};
use vice_fit::{BoundaryModel, LoweredBoundaryGeometry, SelectedBoundaryGeometry};
use vice_geom::Pt;
use vice_ir::{
    Boundary, BoundaryId, Canvas, ChainNode, CurveChain, Face, FaceId, GraphVertex, HalfEdge,
    HalfEdgeId, JoinKind, Paint, PlanarGraph, Segment, VectorScene, VertexId,
};
use vice_topology::{audit, Dcel};
use vice_verify::{topology_signature_sha256, BoundaryBinding};

use crate::types::{TopologyArmRefusal, TopologyArmTrace, TopologyRuntimeSummary};

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
    pub runtime: TopologyRuntimeSummary,
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

#[derive(Debug, Clone, Copy)]
struct SegmentBounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl SegmentBounds {
    fn of_segment(a: Pt, b: Pt) -> Self {
        Self {
            min_x: a.x.min(b.x),
            min_y: a.y.min(b.y),
            max_x: a.x.max(b.x),
            max_y: a.y.max(b.y),
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    fn distance_sq(self, point: Pt) -> f64 {
        let dx = if point.x < self.min_x {
            self.min_x - point.x
        } else if point.x > self.max_x {
            point.x - self.max_x
        } else {
            0.0
        };
        let dy = if point.y < self.min_y {
            self.min_y - point.y
        } else if point.y > self.max_y {
            point.y - self.max_y
        } else {
            0.0
        };
        dx.mul_add(dx, dy * dy)
    }
}

#[derive(Debug)]
enum SegmentNode {
    Leaf {
        bounds: SegmentBounds,
        segments: Vec<(Pt, Pt)>,
    },
    Branch {
        bounds: SegmentBounds,
        left: Box<SegmentNode>,
        right: Box<SegmentNode>,
    },
}

impl SegmentNode {
    const LEAF_SEGMENTS: usize = 8;

    fn bounds(&self) -> SegmentBounds {
        match self {
            Self::Leaf { bounds, .. } | Self::Branch { bounds, .. } => *bounds,
        }
    }

    fn build(mut segments: Vec<(Pt, Pt)>) -> Self {
        let bounds = segments
            .iter()
            .map(|(a, b)| SegmentBounds::of_segment(*a, *b))
            .reduce(SegmentBounds::union)
            .expect("a segment index is never empty");
        if segments.len() <= Self::LEAF_SEGMENTS {
            return Self::Leaf { bounds, segments };
        }
        let split_x = bounds.max_x - bounds.min_x >= bounds.max_y - bounds.min_y;
        segments.sort_by(|(a0, a1), (b0, b1)| {
            let ac = if split_x { a0.x + a1.x } else { a0.y + a1.y };
            let bc = if split_x { b0.x + b1.x } else { b0.y + b1.y };
            ac.total_cmp(&bc)
                .then_with(|| a0.x.total_cmp(&b0.x))
                .then_with(|| a0.y.total_cmp(&b0.y))
                .then_with(|| a1.x.total_cmp(&b1.x))
                .then_with(|| a1.y.total_cmp(&b1.y))
        });
        let right = segments.split_off(segments.len() / 2);
        Self::Branch {
            bounds,
            left: Box::new(Self::build(segments)),
            right: Box::new(Self::build(right)),
        }
    }

    fn nearest(&self, point: Pt, best: &mut f64) {
        if self.bounds().distance_sq(point) >= *best * *best {
            return;
        }
        match self {
            Self::Leaf { segments, .. } => {
                for (a, b) in segments {
                    let distance = point_segment_distance(point, *a, *b);
                    *best = (*best).min(distance);
                }
            }
            Self::Branch { left, right, .. } => {
                let left_distance = left.bounds().distance_sq(point);
                let right_distance = right.bounds().distance_sq(point);
                if left_distance <= right_distance {
                    left.nearest(point, best);
                    right.nearest(point, best);
                } else {
                    right.nearest(point, best);
                    left.nearest(point, best);
                }
            }
        }
    }
}

#[derive(Debug)]
struct PolylineIndex(Option<SegmentNode>);

impl PolylineIndex {
    fn new(polyline: &[Pt]) -> Self {
        let segments = polyline
            .windows(2)
            .map(|segment| (segment[0], segment[1]))
            .collect::<Vec<_>>();
        Self((!segments.is_empty()).then(|| SegmentNode::build(segments)))
    }

    fn distance(&self, point: Pt) -> f64 {
        let Some(root) = &self.0 else {
            return f64::INFINITY;
        };
        let mut best = f64::INFINITY;
        root.nearest(point, &mut best);
        best
    }
}

fn chain_boundary_distance(chain: &BoundaryChain, boundary: &vice_topology::Boundary) -> f64 {
    let path: Vec<Pt> = boundary
        .path
        .iter()
        .map(|&(x, y)| Pt::new(f64::from(x), f64::from(y)))
        .collect();
    let path_index = PolylineIndex::new(&path);
    let forward = chain
        .samples
        .iter()
        .map(|sample| path_index.distance(sample.p))
        .fold(0.0f64, f64::max);
    let evidence = closed_support(chain);
    let evidence_index = PolylineIndex::new(&evidence);
    let reverse = path
        .iter()
        .map(|point| evidence_index.distance(*point))
        .fold(0.0f64, f64::max);
    forward.max(reverse)
}

fn bind_chains_to_dcel(chains: &[BoundaryChain], dcel: &Dcel) -> Result<Vec<usize>, String> {
    if chains.len() != dcel.boundaries().len() {
        let boundaries = dcel
            .boundaries()
            .iter()
            .enumerate()
            .map(|(index, boundary)| {
                let min_x = boundary.path.iter().map(|point| point.0).min().unwrap_or(0);
                let max_x = boundary.path.iter().map(|point| point.0).max().unwrap_or(0);
                let min_y = boundary.path.iter().map(|point| point.1).min().unwrap_or(0);
                let max_y = boundary.path.iter().map(|point| point.1).max().unwrap_or(0);
                format!("{index}:[{min_x},{min_y}]..[{max_x},{max_y}]")
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "{} observed chains cannot bind bijectively to {} DCEL boundaries ({boundaries})",
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
    let stage_started = Instant::now();
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
    let envelope_proposal_ms = stage_started
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    let envelope_hypotheses = proposal
        .envelope
        .hypotheses
        .len()
        .try_into()
        .unwrap_or(u64::MAX);
    let stage_started = Instant::now();
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
        runtime: TopologyRuntimeSummary {
            envelope_proposal_ms,
            arm_materialization_ms: stage_started
                .elapsed()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
            envelope_hypotheses,
            materialized_arms: seen.len().try_into().unwrap_or(u64::MAX),
        },
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
        + vice_fit::BINDING_CERTIFICATION_CHORD_TOLERANCE_PX_V1
        + vice_fit::BINDING_RELATION_RESCUE_MARGIN_PX_V1;
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

#[cfg(test)]
mod spatial_index_tests {
    use super::*;

    fn brute(point: Pt, polyline: &[Pt]) -> f64 {
        polyline
            .windows(2)
            .map(|segment| point_segment_distance(point, segment[0], segment[1]))
            .fold(f64::INFINITY, f64::min)
    }

    #[test]
    fn segment_index_is_exactly_equivalent_to_brute_force_distance() {
        let polylines = [
            vec![
                Pt::new(0.0, 0.0),
                Pt::new(8.0, 0.0),
                Pt::new(8.0, 6.0),
                Pt::new(0.0, 6.0),
                Pt::new(0.0, 0.0),
            ],
            (0..40)
                .map(|index| {
                    let x = f64::from(index) * 0.25;
                    Pt::new(x, (x * 0.7).sin() * 2.0 + 3.0)
                })
                .collect(),
        ];
        for polyline in &polylines {
            let index = PolylineIndex::new(polyline);
            for yi in -8..=32 {
                for xi in -8..=48 {
                    let point = Pt::new(f64::from(xi) * 0.25, f64::from(yi) * 0.25);
                    assert_eq!(
                        index.distance(point).to_bits(),
                        brute(point, polyline).to_bits()
                    );
                }
            }
        }
    }
}

mod continuous;
pub(crate) use continuous::optimize_continuous;
