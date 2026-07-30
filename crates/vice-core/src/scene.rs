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
    TrustRegionConfig, TrustRegionProblem,
};
use vice_render::PartitionRender;
use vice_topology::{audit, signature, Dcel, Labelling};
use vice_verify::{topology_signature_sha256, BoundaryBinding};

#[derive(Debug, Clone)]
pub(crate) struct TopologyArm {
    pub class: String,
    pub dcel_boundary_sha256: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PaintLayout {
    pub foreground: FaceId,
    pub background: Option<FaceId>,
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

pub(crate) fn topology_arms(evidence: &Flat2Evidence) -> Result<Vec<TopologyArm>, String> {
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
        if sig.components != 1 || sig.holes != 0 {
            continue;
        }
        let dcel = Dcel::assemble(labelling.clone(), connectivity);
        audit(&dcel).map_err(|error| error.to_string())?;
        if dcel.boundaries().len() != 1 || dcel.boundaries()[0].start != dcel.boundaries()[0].end {
            continue;
        }
        arms.push(TopologyArm {
            class: format!(
                "fg{}-bg{}:{}",
                sig.foreground_connectivity, sig.background_connectivity, sig.digest
            ),
            dcel_boundary_sha256: digest(&dcel.boundaries()[0])?,
        });
    }
    arms.sort_by(|left, right| left.class.cmp(&right.class));
    arms.dedup_by(|left, right| left.class == right.class);
    if arms.is_empty() {
        Err("no audited one-component/no-hole DCEL arm".into())
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

fn lower_model(model: &BoundaryModel) -> Result<(Pt, LoweredBoundaryGeometry), String> {
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
    let polyline = model
        .geometry
        .flatten()
        .map_err(|error| format!("{error:?}"))?;
    Ok(if signed_area(&polyline) < 0.0 {
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
    arm: &TopologyArm,
) -> Result<BoundaryBinding, String> {
    let topology = topology_signature_sha256(scene).map_err(|error| error.to_string())?;
    let tube = chain
        .samples
        .iter()
        .map(|sample| sample.halfwidth)
        .fold(0.0f64, f64::max);
    BoundaryBinding::new_observed(
        digest(chain)?,
        arm.dcel_boundary_sha256.clone(),
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
    chain: &BoundaryChain,
    model: &BoundaryModel,
    arm: &TopologyArm,
    formation: vice_ir::GlobalFormationHypothesis,
) -> Result<SceneCandidate, String> {
    let (start, geometry) = lower_model(model)?;
    let foreground = Paint::OpaqueSolid(evidence.hypothesis.foreground.center());
    match evidence.hypothesis.background {
        BackgroundHypothesis::TransparentExterior => {
            let scene = VectorScene {
                canvas,
                graph: PlanarGraph {
                    exterior: FaceId(0),
                    vertices: vec![GraphVertex { pos: start }],
                    boundaries: vec![Boundary {
                        left_face: FaceId(1),
                        right_face: FaceId(0),
                        start_vertex: VertexId(0),
                        end_vertex: VertexId(0),
                        closure_join: geometry.closure_join,
                        curve: geometry.curve,
                    }],
                    half_edges: vec![
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
                    ],
                    faces: vec![
                        Face {
                            loops: vec![HalfEdgeId(1)],
                            paint: Paint::TransparentExterior,
                        },
                        Face {
                            loops: vec![HalfEdgeId(0)],
                            paint: foreground,
                        },
                    ],
                },
                formation,
            };
            let bindings = vec![observed_binding(&scene, BoundaryId(0), chain, arm)?];
            vice_ir::validate_scene(&scene).map_err(|error| error.to_string())?;
            Ok(SceneCandidate {
                scene,
                bindings,
                paint_layout: PaintLayout {
                    foreground: FaceId(1),
                    background: None,
                },
            })
        }
        BackgroundHypothesis::OpaqueFace(background) => {
            let scene = VectorScene {
                canvas,
                graph: PlanarGraph {
                    exterior: FaceId(0),
                    vertices: vec![
                        GraphVertex {
                            pos: Pt::new(0.0, 0.0),
                        },
                        GraphVertex { pos: start },
                    ],
                    boundaries: vec![
                        Boundary {
                            left_face: FaceId(1),
                            right_face: FaceId(0),
                            start_vertex: VertexId(0),
                            end_vertex: VertexId(0),
                            closure_join: Some(JoinKind::Corner),
                            curve: canvas_curve(canvas),
                        },
                        Boundary {
                            left_face: FaceId(2),
                            right_face: FaceId(1),
                            start_vertex: VertexId(1),
                            end_vertex: VertexId(1),
                            closure_join: geometry.closure_join,
                            curve: geometry.curve,
                        },
                    ],
                    half_edges: vec![
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
                        HalfEdge {
                            boundary: BoundaryId(1),
                            forward: true,
                            twin: HalfEdgeId(3),
                            next: HalfEdgeId(2),
                            face: FaceId(2),
                        },
                        HalfEdge {
                            boundary: BoundaryId(1),
                            forward: false,
                            twin: HalfEdgeId(2),
                            next: HalfEdgeId(3),
                            face: FaceId(1),
                        },
                    ],
                    faces: vec![
                        Face {
                            loops: vec![HalfEdgeId(1)],
                            paint: Paint::TransparentExterior,
                        },
                        Face {
                            loops: vec![HalfEdgeId(0), HalfEdgeId(3)],
                            paint: Paint::OpaqueSolid(background.center()),
                        },
                        Face {
                            loops: vec![HalfEdgeId(2)],
                            paint: foreground,
                        },
                    ],
                },
                formation,
            };
            vice_ir::validate_scene(&scene).map_err(|error| error.to_string())?;
            let topology = topology_signature_sha256(&scene).map_err(|error| error.to_string())?;
            let canvas_support = vec![
                Pt::new(0.0, 0.0),
                Pt::new(f64::from(canvas.width_px), 0.0),
                Pt::new(f64::from(canvas.width_px), f64::from(canvas.height_px)),
                Pt::new(0.0, f64::from(canvas.height_px)),
                Pt::new(0.0, 0.0),
            ];
            let bindings = vec![
                BoundaryBinding::new_canvas_closure(
                    canvas,
                    BoundaryId(0),
                    topology,
                    canvas_support,
                )
                .map_err(|error| error.to_string())?,
                observed_binding(&scene, BoundaryId(1), chain, arm)?,
            ];
            Ok(SceneCandidate {
                scene,
                bindings,
                paint_layout: PaintLayout {
                    foreground: FaceId(2),
                    background: Some(FaceId(1)),
                },
            })
        }
    }
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
}

impl PaintProblem<'_> {
    fn materialize(&self, parameters: &[f64]) -> Result<VectorScene, String> {
        let want = if self.layout.background.is_some() {
            6
        } else {
            3
        };
        if parameters.len() != want {
            return Err("paint parameter arity".into());
        }
        let mut scene = self.scene.clone();
        scene.graph.faces[self.layout.foreground.index()].paint =
            Paint::OpaqueSolid(LinearRgb::new(parameters[0], parameters[1], parameters[2]));
        if let Some(background) = self.layout.background {
            scene.graph.faces[background.index()].paint =
                Paint::OpaqueSolid(LinearRgb::new(parameters[3], parameters[4], parameters[5]));
        }
        Ok(scene)
    }
}

impl TrustRegionProblem for PaintProblem<'_> {
    fn exact_bits(
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
    likelihood: vice_opt::BlockLikelihoodConfig,
    priors: PriorCodeLengths,
    config: TrustRegionConfig,
) -> Result<(SceneCandidate, OptimizationResult), String> {
    let foreground = paint(&candidate.scene, candidate.paint_layout.foreground)?;
    let mut initial = foreground.components().to_vec();
    if let Some(background) = candidate.paint_layout.background {
        initial.extend_from_slice(&paint(&candidate.scene, background)?.components());
    }
    let problem = PaintProblem {
        scene: &candidate.scene,
        render: fixed_render,
        observed,
        likelihood,
        priors,
        layout: candidate.paint_layout,
    };
    let mut blocks = vec![BlockSpec {
        name: "foreground_paint".into(),
        parameter_indices: vec![0, 1, 2],
        scales: vec![1.0; 3],
        max_radius: 4.0 / 255.0,
        scope: ScoreScope::FULL,
    }];
    if candidate.paint_layout.background.is_some() {
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
    let result = optimize_best_deterministic(&problem, starts, &blocks, config)
        .map_err(|error| error.to_string())?;
    let optimized = problem.materialize(&result.parameters)?;
    let mut mutations = vec![SceneMutation::ReplaceFacePaint {
        face: candidate.paint_layout.foreground,
        paint: optimized.graph.faces[candidate.paint_layout.foreground.index()].paint,
    }];
    if let Some(background) = candidate.paint_layout.background {
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
