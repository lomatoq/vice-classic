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
    apply_compound_transaction, optimize_best_deterministic, BlockSpec, CompoundTransaction,
    OptimizationResult, PriorCodeLengths, SceneMutation, ScoreScope, TransactionKind,
    TrustRegionProblem,
};
use vice_render::PartitionRender;
use vice_topology::{audit, signature, Dcel, Labelling};
use vice_verify::{topology_signature_sha256, BoundaryBinding};

use crate::config::CoreConfig;

#[derive(Debug, Clone)]
pub(crate) struct TopologyArm {
    pub class: String,
    pub dcel: Dcel,
    /// Evidence-chain index -> canonical DCEL boundary index.
    pub chain_to_boundary: Vec<usize>,
    pub dcel_boundary_sha256: Vec<String>,
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

pub(crate) fn topology_arms(
    evidence: &Flat2Evidence,
    chains: &[BoundaryChain],
) -> Result<Vec<TopologyArm>, String> {
    let labelling = Labelling::new(
        evidence.width_px() as usize,
        evidence.height_px() as usize,
        evidence
            .alpha_field()
            .iter()
            .map(|value| *value >= 0.5)
            .collect(),
    );
    let mut arms = Vec::new();
    for connectivity in vice_ir::ComplementaryConnectivity::arms() {
        let sig = signature(&labelling, connectivity);
        let dcel = Dcel::assemble(labelling.clone(), connectivity);
        audit(&dcel).map_err(|error| error.to_string())?;
        if dcel
            .boundaries()
            .iter()
            .any(|boundary| boundary.start != boundary.end)
            || dcel.vertices().len() != dcel.boundaries().len()
        {
            continue;
        }
        let chain_to_boundary = bind_chains_to_dcel(chains, &dcel)?;
        let dcel_boundary_sha256 = dcel
            .boundaries()
            .iter()
            .map(digest)
            .collect::<Result<Vec<_>, _>>()?;
        arms.push(TopologyArm {
            class: format!(
                "fg{}-bg{}:{}",
                sig.foreground_connectivity, sig.background_connectivity, sig.digest
            ),
            dcel,
            chain_to_boundary,
            dcel_boundary_sha256,
        });
    }
    arms.sort_by(|left, right| left.class.cmp(&right.class));
    arms.dedup_by(|left, right| left.class == right.class);
    if arms.is_empty() {
        Err("no audited closed-boundary DCEL arm matched every observed chain".into())
    } else {
        Ok(arms)
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
        .fold(0.0f64, f64::max);
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

fn paint(scene: &VectorScene, face: FaceId) -> Result<LinearRgb, String> {
    match scene.graph.faces[face.index()].paint {
        Paint::OpaqueSolid(color) => Ok(color),
        Paint::TransparentExterior => Err("optimizer paint face is transparent".into()),
    }
}

struct PaintProblem<'a> {
    scene: &'a VectorScene,
    render: &'a PartitionRender,
    observed: &'a vice_image::CanonicalImage,
    likelihood: vice_opt::BlockLikelihoodConfig,
    priors: PriorCodeLengths,
    layout: PaintLayout,
    export_decimal_places: u32,
    apron_width_px: f64,
    exact_cache: RefCell<BTreeMap<Vec<u64>, f64>>,
}

impl PaintProblem<'_> {
    fn materialize(&self, parameters: &[f64]) -> Result<VectorScene, String> {
        let want = if self.layout.background.is_empty() {
            3
        } else {
            6
        };
        if parameters.len() != want {
            return Err("paint parameter arity".into());
        }
        let mut scene = self.scene.clone();
        let foreground =
            Paint::OpaqueSolid(LinearRgb::new(parameters[0], parameters[1], parameters[2]));
        for face in &self.layout.foreground {
            scene.graph.faces[face.index()].paint = foreground;
        }
        if !self.layout.background.is_empty() {
            let background =
                Paint::OpaqueSolid(LinearRgb::new(parameters[3], parameters[4], parameters[5]));
            for face in &self.layout.background {
                scene.graph.faces[face.index()].paint = background;
            }
        }
        Ok(scene)
    }
}

impl TrustRegionProblem for PaintProblem<'_> {
    fn surrogate_bits(
        &self,
        parameters: &[f64],
        _scope: ScoreScope,
        _token: vice_opt::EvaluationToken,
    ) -> Result<f64, String> {
        let scene = self.materialize(parameters)?;
        vice_opt::score_full_resolution(
            &scene,
            self.observed,
            self.render,
            self.likelihood,
            self.priors,
        )
        .map(|score| score.total_bits)
        .map_err(|error| error.to_string())
    }

    fn exact_bits(
        &self,
        parameters: &[f64],
        _scope: ScoreScope,
        _token: vice_opt::EvaluationToken,
    ) -> Result<f64, String> {
        let key: Vec<u64> = parameters.iter().map(|value| value.to_bits()).collect();
        if let Some(bits) = self.exact_cache.borrow().get(&key) {
            return Ok(*bits);
        }
        let scene = self.materialize(parameters)?;
        let plan =
            vice_svg::build_export_plan(&scene, self.export_decimal_places, self.apron_width_px)
                .map_err(|error| error.to_string())?;
        let svg = vice_svg::materialize_svg(&plan, vice_svg::SvgProfile::SeamSafe)
            .map_err(|error| error.to_string())?;
        let witness =
            vice_svg::parse_and_render_independently(&svg).map_err(|error| error.to_string())?;
        let bits = vice_opt::score_serialized_full_resolution(
            &scene,
            self.observed,
            witness.premultiplied_rgba8(),
            witness.width_px(),
            witness.height_px(),
            self.likelihood,
            self.priors,
        )
        .map(|score| score.total_bits)
        .map_err(|error| error.to_string())?;
        self.exact_cache.borrow_mut().insert(key, bits);
        Ok(bits)
    }

    fn project(&self, parameters: &mut [f64], block: &BlockSpec) -> Result<(), String> {
        for &index in &block.parameter_indices {
            parameters[index] = parameters[index].clamp(0.0, 1.0);
        }
        Ok(())
    }
}

pub(crate) fn optimize_paint(
    candidate: SceneCandidate,
    observed: &vice_image::CanonicalImage,
    fixed_render: &PartitionRender,
    priors: PriorCodeLengths,
    config: &CoreConfig,
) -> Result<(SceneCandidate, OptimizationResult), String> {
    let foreground_face = *candidate
        .paint_layout
        .foreground
        .first()
        .ok_or_else(|| "paint optimizer has no foreground face".to_string())?;
    let foreground = paint(&candidate.scene, foreground_face)?;
    let mut initial = foreground.components().to_vec();
    if let Some(&background) = candidate.paint_layout.background.first() {
        initial.extend_from_slice(&paint(&candidate.scene, background)?.components());
    }
    let problem = PaintProblem {
        scene: &candidate.scene,
        render: fixed_render,
        observed,
        likelihood: config.likelihood,
        priors,
        layout: candidate.paint_layout.clone(),
        export_decimal_places: config.export_decimal_places,
        apron_width_px: config.apron_width_px,
        exact_cache: RefCell::new(BTreeMap::new()),
    };
    let mut blocks = vec![BlockSpec {
        name: "foreground_paint".into(),
        parameter_indices: vec![0, 1, 2],
        scales: vec![1.0; 3],
        max_radius: 4.0 / 255.0,
        scope: ScoreScope::FULL,
    }];
    if !candidate.paint_layout.background.is_empty() {
        blocks.push(BlockSpec {
            name: "background_paint".into(),
            parameter_indices: vec![3, 4, 5],
            scales: vec![1.0; 3],
            max_radius: 4.0 / 255.0,
            scope: ScoreScope::FULL,
        });
    }
    let starts = [-1.0, 0.0, 1.0]
        .into_iter()
        .map(|direction| {
            initial
                .iter()
                .map(|value| (value + direction / 255.0).clamp(0.0, 1.0))
                .collect()
        })
        .collect();
    let result = optimize_best_deterministic(&problem, starts, &blocks, config.trust_region)
        .map_err(|error| error.to_string())?;
    let optimized = problem.materialize(&result.parameters)?;
    let mut mutations = candidate
        .paint_layout
        .foreground
        .iter()
        .map(|&face| SceneMutation::ReplaceFacePaint {
            face,
            paint: optimized.graph.faces[face.index()].paint,
        })
        .collect::<Vec<_>>();
    for &background in &candidate.paint_layout.background {
        mutations.push(SceneMutation::ReplaceFacePaint {
            face: background,
            paint: optimized.graph.faces[background.index()].paint,
        });
    }
    let scene = apply_compound_transaction(
        &candidate.scene,
        &CompoundTransaction {
            kind: TransactionKind::PaintChange,
            expected_parent_digest: vice_ir::scene_digest_sha256(&candidate.scene)
                .map_err(|error| error.to_string())?,
            mutations,
        },
    )
    .map_err(|error| error.to_string())?;
    Ok((SceneCandidate { scene, ..candidate }, result))
}
