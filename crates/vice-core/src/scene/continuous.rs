use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use vice_geom::Pt;
use vice_ir::{BoundaryId, FaceId, JoinKind, LinearRgb, Paint, Segment, VectorScene, VertexId};
use vice_opt::{
    apply_compound_transaction_traced, optimize_best_deterministic, BlockSpec, CompoundTransaction,
    OptimizationResult, PriorCodeLengths, SceneMutation, ScoreScope, TransactionApplication,
    TransactionKind, TrustRegionProblem,
};
use vice_render::PartitionRender;

use super::{PaintLayout, SceneCandidate};
use crate::config::CoreConfig;

const DEPENDENCY_HALO_PX: u32 = 2;
// Large enough to lose every real comparison, small enough that the
// trust-region gradient norm cannot overflow when an infeasible perturbation
// is compared with a finite parent.
const INFEASIBLE_BITS: f64 = 1.0e30;

#[derive(Debug, Clone)]
struct GeometryGroup {
    boundaries: Vec<BoundaryId>,
    center: Pt,
    parameter_indices: [usize; 4],
}

#[derive(Debug, Clone, Copy)]
enum ParameterKind {
    TranslationX { limit: f64 },
    TranslationY { limit: f64 },
    LogScale { limit: f64 },
    Rotation { limit: f64 },
    Paint,
}

struct ContinuousProblem<'a> {
    scene: &'a VectorScene,
    bindings: &'a [vice_verify::BoundaryBinding],
    observed: &'a vice_image::CanonicalImage,
    likelihood: vice_opt::BlockLikelihoodConfig,
    priors: PriorCodeLengths,
    verification: vice_verify::VerificationConfig,
    quantization: vice_verify::QuantizationPolicy,
    geometry_groups: Vec<GeometryGroup>,
    parameter_kinds: Vec<ParameterKind>,
    layout: PaintLayout,
    foreground_paint_offset: usize,
    background_paint_offset: Option<usize>,
    export_decimal_places: u32,
    apron_width_px: f64,
    surrogate_cache: RefCell<BTreeMap<Vec<u64>, f64>>,
    exact_cache: RefCell<BTreeMap<Vec<u64>, f64>>,
    fixed_mesh_signatures: RefCell<BTreeMap<u64, Vec<usize>>>,
}

fn rotate_angle(angle: f64, delta: f64) -> f64 {
    let mut value = (angle + delta) % std::f64::consts::TAU;
    if value <= -std::f64::consts::PI {
        value += std::f64::consts::TAU;
    } else if value > std::f64::consts::PI {
        value -= std::f64::consts::TAU;
    }
    value
}

fn transform_point(point: Pt, center: Pt, dx: f64, dy: f64, scale: f64, angle: f64) -> Pt {
    let relative = point - center;
    let cos = angle.cos();
    let sin = angle.sin();
    Pt::new(
        center.x + dx + scale * (cos * relative.x - sin * relative.y),
        center.y + dy + scale * (sin * relative.x + cos * relative.y),
    )
}

fn transform_join(join: &mut JoinKind, angle: f64) {
    if let JoinKind::SmoothG1 { tangent_angle_rad } = join {
        *tangent_angle_rad = rotate_angle(*tangent_angle_rad, angle);
    }
}

fn transform_segment(segment: &mut Segment, center: Pt, dx: f64, dy: f64, scale: f64, angle: f64) {
    match segment {
        Segment::Line => {}
        Segment::CircularArc { radius_px, .. } => *radius_px *= scale,
        Segment::EllipticArc {
            rx_px,
            ry_px,
            x_axis_rotation_rad,
            ..
        } => {
            *rx_px *= scale;
            *ry_px *= scale;
            *x_axis_rotation_rad =
                rotate_angle(*x_axis_rotation_rad, angle).rem_euclid(std::f64::consts::PI);
        }
        Segment::Quad { ctrl } => {
            *ctrl = transform_point(*ctrl, center, dx, dy, scale, angle);
        }
        Segment::Cubic { ctrl1, ctrl2 } => {
            *ctrl1 = transform_point(*ctrl1, center, dx, dy, scale, angle);
            *ctrl2 = transform_point(*ctrl2, center, dx, dy, scale, angle);
        }
    }
}

fn paint(scene: &VectorScene, face: FaceId) -> Result<LinearRgb, String> {
    match scene.graph.faces[face.index()].paint {
        Paint::OpaqueSolid(color) => Ok(color),
        Paint::TransparentExterior => Err("optimizer paint face is transparent".into()),
    }
}

impl ContinuousProblem<'_> {
    fn materialize(&self, parameters: &[f64], include_paint: bool) -> Result<VectorScene, String> {
        if parameters.len() != self.parameter_kinds.len() {
            return Err("continuous parameter arity".into());
        }
        let mut scene = self.scene.clone();
        for group in &self.geometry_groups {
            let [dx_index, dy_index, scale_index, rotation_index] = group.parameter_indices;
            let dx = parameters[dx_index];
            let dy = parameters[dy_index];
            let scale = parameters[scale_index].exp();
            let angle = parameters[rotation_index];
            let mut transformed_vertices = BTreeSet::new();
            for &boundary_id in &group.boundaries {
                let boundary = &mut scene.graph.boundaries[boundary_id.index()];
                for vertex in [boundary.start_vertex, boundary.end_vertex] {
                    if transformed_vertices.insert(vertex) {
                        let position = scene.graph.vertices[vertex.index()].pos;
                        scene.graph.vertices[vertex.index()].pos =
                            transform_point(position, group.center, dx, dy, scale, angle);
                    }
                }
                if let Some(join) = &mut boundary.closure_join {
                    transform_join(join, angle);
                }
                for node in &mut boundary.curve.interior_nodes {
                    node.pos = transform_point(node.pos, group.center, dx, dy, scale, angle);
                    transform_join(&mut node.join, angle);
                }
                for segment in &mut boundary.curve.segments {
                    transform_segment(segment, group.center, dx, dy, scale, angle);
                }
            }
        }
        if include_paint {
            let foreground = Paint::OpaqueSolid(LinearRgb::new(
                parameters[self.foreground_paint_offset],
                parameters[self.foreground_paint_offset + 1],
                parameters[self.foreground_paint_offset + 2],
            ));
            for face in &self.layout.foreground {
                scene.graph.faces[face.index()].paint = foreground;
            }
            if let Some(offset) = self.background_paint_offset {
                let background = Paint::OpaqueSolid(LinearRgb::new(
                    parameters[offset],
                    parameters[offset + 1],
                    parameters[offset + 2],
                ));
                for face in &self.layout.background {
                    scene.graph.faces[face.index()].paint = background;
                }
            }
        }
        Ok(scene)
    }

    fn cache_key(parameters: &[f64], scope: ScoreScope, mesh_discriminator: u64) -> Vec<u64> {
        let mut key = parameters
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>();
        key.push(mesh_discriminator);
        key.push(u64::from(scope.global));
        key.push(u64::from(scope.halo_px));
        if let Some(roi) = scope.roi {
            key.extend([
                u64::from(roi.x0),
                u64::from(roi.y0),
                u64::from(roi.x1),
                u64::from(roi.y1),
            ]);
        } else {
            key.extend([u64::MAX; 4]);
        }
        key
    }

    fn presealed(&self, parameters: &[f64]) -> Result<vice_verify::PresealedScene, String> {
        let scene = self.materialize(parameters, true)?;
        vice_verify::preseal_scene(&scene, self.bindings, self.verification)
            .map_err(|error| error.to_string())
    }

    fn presealed_on_fixed_mesh(
        &self,
        parameters: &[f64],
        token: vice_opt::EvaluationToken,
    ) -> Result<vice_verify::PresealedScene, String> {
        let presealed = self.presealed(parameters)?;
        let signature = presealed.mesh_combinatorics_signature();
        let mut fixed = self.fixed_mesh_signatures.borrow_mut();
        match fixed.get(&token.fixed_mesh_id) {
            Some(parent_signature) if parent_signature != &signature => {
                Err("fixed-mesh comparison changed tessellation combinatorics".into())
            }
            Some(_) => Ok(presealed),
            None => {
                fixed.insert(token.fixed_mesh_id, signature);
                Ok(presealed)
            }
        }
    }

    fn serialized_score(
        &self,
        parameters: &[f64],
        scope: ScoreScope,
    ) -> Result<vice_opt::ScoreBreakdown, String> {
        let scene = self.materialize(parameters, true)?;
        let verified = vice_verify::quantize_and_verify(
            &scene,
            self.bindings,
            self.verification,
            self.quantization,
        )
        .map_err(|error| error.to_string())?;
        let scene = verified.scene();
        let plan =
            vice_svg::build_export_plan(scene, self.export_decimal_places, self.apron_width_px)
                .map_err(|error| error.to_string())?;
        let svg = vice_svg::materialize_svg(&plan, vice_svg::SvgProfile::SeamSafe)
            .map_err(|error| error.to_string())?;
        let witness =
            vice_svg::parse_and_render_independently(&svg).map_err(|error| error.to_string())?;
        vice_opt::score_serialized_full_resolution_scope(
            scene,
            self.observed,
            witness.premultiplied_rgba8(),
            witness.width_px(),
            witness.height_px(),
            self.likelihood,
            self.priors,
            scope,
        )
        .map_err(|error| error.to_string())
    }
}

impl TrustRegionProblem for ContinuousProblem<'_> {
    fn surrogate_bits(
        &self,
        parameters: &[f64],
        scope: ScoreScope,
        token: vice_opt::EvaluationToken,
    ) -> Result<f64, String> {
        let key = Self::cache_key(parameters, scope, token.fixed_mesh_id);
        if let Some(bits) = self.surrogate_cache.borrow().get(&key) {
            return Ok(*bits);
        }
        let bits =
            self.presealed_on_fixed_mesh(parameters, token)
                .map_or(INFEASIBLE_BITS, |presealed| {
                    vice_opt::score_full_resolution_scope(
                        presealed.scene(),
                        self.observed,
                        presealed.render(),
                        self.likelihood,
                        self.priors,
                        scope,
                    )
                    .map_or(INFEASIBLE_BITS, |score| score.total_bits)
                });
        self.surrogate_cache.borrow_mut().insert(key, bits);
        Ok(bits)
    }

    fn exact_bits(
        &self,
        parameters: &[f64],
        scope: ScoreScope,
        _token: vice_opt::EvaluationToken,
    ) -> Result<f64, String> {
        let key = Self::cache_key(parameters, scope, u64::MAX);
        if let Some(bits) = self.exact_cache.borrow().get(&key) {
            return Ok(*bits);
        }
        let bits = self
            .serialized_score(parameters, scope)
            .map_or(INFEASIBLE_BITS, |score| score.total_bits);
        self.exact_cache.borrow_mut().insert(key, bits);
        Ok(bits)
    }

    fn project(&self, parameters: &mut [f64], block: &BlockSpec) -> Result<(), String> {
        for &index in &block.parameter_indices {
            parameters[index] = match self.parameter_kinds[index] {
                ParameterKind::TranslationX { limit }
                | ParameterKind::TranslationY { limit }
                | ParameterKind::LogScale { limit }
                | ParameterKind::Rotation { limit } => parameters[index].clamp(-limit, limit),
                ParameterKind::Paint => parameters[index].clamp(0.0, 1.0),
            };
        }
        Ok(())
    }
}

fn score_scope(
    render: &PartitionRender,
    faces: &[FaceId],
    halo_px: u32,
) -> Result<ScoreScope, String> {
    if halo_px == 0 || faces.is_empty() {
        return Err("continuous ROI requires affected faces and a dependency halo".into());
    }
    let width = render.width_px as usize;
    let mut x0 = render.width_px;
    let mut y0 = render.height_px;
    let mut x1 = 0u32;
    let mut y1 = 0u32;
    let mut found = false;
    for face in faces {
        let coverage = render
            .face_coverage
            .get(face.index())
            .ok_or_else(|| "continuous ROI face is absent from fixed render".to_string())?;
        if coverage.len() != width * render.height_px as usize {
            return Err("continuous ROI dimensions disagree with fixed render".into());
        }
        for (index, value) in coverage.iter().enumerate() {
            if *value == 0.0 {
                continue;
            }
            found = true;
            let x = (index % width) as u32;
            let y = (index / width) as u32;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    if !found {
        return Err("continuous ROI has no affected certified pixels".into());
    }
    Ok(ScoreScope {
        roi: Some(vice_opt::Rect { x0, y0, x1, y1 }),
        halo_px,
        global: false,
    })
}

fn group_center(scene: &VectorScene, boundaries: &[BoundaryId]) -> Result<Pt, String> {
    let mut points = Vec::new();
    for &boundary_id in boundaries {
        let boundary = &scene.graph.boundaries[boundary_id.index()];
        points.push(scene.graph.vertices[boundary.start_vertex.index()].pos);
        points.extend(boundary.curve.interior_nodes.iter().map(|node| node.pos));
    }
    if points.is_empty() {
        return Err("geometry group has no shared nodes".into());
    }
    Ok(points
        .iter()
        .copied()
        .fold(Pt::ZERO, |sum, point| sum + point)
        * (1.0 / points.len() as f64))
}

fn geometry_faces(scene: &VectorScene, boundaries: &[BoundaryId]) -> Vec<FaceId> {
    let mut faces = BTreeSet::new();
    for &boundary in boundaries {
        let boundary = &scene.graph.boundaries[boundary.index()];
        for face in [boundary.left_face, boundary.right_face] {
            if face != scene.graph.exterior {
                faces.insert(face);
            }
        }
    }
    faces.into_iter().collect()
}

fn geometry_mutations(
    before: &VectorScene,
    after: &VectorScene,
) -> Result<Vec<SceneMutation>, String> {
    if before.canvas != after.canvas
        || before.formation != after.formation
        || before.graph.half_edges != after.graph.half_edges
        || before.graph.faces != after.graph.faces
        || before.graph.exterior != after.graph.exterior
    {
        return Err("continuous geometry step changed non-geometry structure".into());
    }
    let mut mutations = Vec::new();
    for (index, (before, after)) in before
        .graph
        .vertices
        .iter()
        .zip(&after.graph.vertices)
        .enumerate()
    {
        if before != after {
            mutations.push(SceneMutation::ReplaceVertexPosition {
                vertex: VertexId(index as u32),
                position: after.pos,
            });
        }
    }
    for (index, (before, after)) in before
        .graph
        .boundaries
        .iter()
        .zip(&after.graph.boundaries)
        .enumerate()
    {
        if before.left_face != after.left_face
            || before.right_face != after.right_face
            || before.start_vertex != after.start_vertex
            || before.end_vertex != after.end_vertex
        {
            return Err("continuous geometry step changed boundary incidence".into());
        }
        if before.curve != after.curve || before.closure_join != after.closure_join {
            mutations.push(SceneMutation::ReplaceBoundaryGeometry {
                boundary: BoundaryId(index as u32),
                curve: after.curve.clone(),
                closure_join: after.closure_join,
            });
        }
    }
    Ok(mutations)
}

fn apply_transition(
    parent: &VectorScene,
    kind: TransactionKind,
    mutations: Vec<SceneMutation>,
) -> Result<(VectorScene, TransactionApplication), String> {
    apply_compound_transaction_traced(
        parent,
        &CompoundTransaction {
            kind,
            expected_parent_digest: vice_ir::scene_digest_sha256(parent)
                .map_err(|error| error.to_string())?,
            mutations,
        },
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn optimize_continuous(
    candidate: SceneCandidate,
    observed: &vice_image::CanonicalImage,
    fixed_render: &PartitionRender,
    priors: PriorCodeLengths,
    config: &CoreConfig,
    preserve_scene_relations: bool,
) -> Result<
    (
        SceneCandidate,
        OptimizationResult,
        Vec<TransactionApplication>,
    ),
    String,
> {
    if !config
        .geometry_refinement_trigger_bits_per_block
        .is_finite()
        || config.geometry_refinement_trigger_bits_per_block <= 0.0
        || !config
            .small_geometry_refinement_trigger_bits_per_block
            .is_finite()
        || config.small_geometry_refinement_trigger_bits_per_block <= 0.0
    {
        return Err("geometry refinement triggers must be finite and positive".into());
    }
    let observed_boundaries = candidate
        .bindings
        .iter()
        .filter(|binding| binding.observed_chain_sha256().is_some())
        .map(vice_verify::BoundaryBinding::boundary)
        .collect::<Vec<_>>();
    if observed_boundaries.is_empty() {
        return Err("continuous optimizer has no observed geometry".into());
    }
    let boundary_groups = if preserve_scene_relations {
        vec![observed_boundaries.clone()]
    } else {
        observed_boundaries
            .iter()
            .copied()
            .map(|boundary| vec![boundary])
            .collect()
    };
    let minimum_tube = candidate
        .bindings
        .iter()
        .filter(|binding| binding.observed_chain_sha256().is_some())
        .map(vice_verify::BoundaryBinding::isotopy_tube_px)
        .fold(f64::INFINITY, f64::min);
    let max_translation_px = (minimum_tube * 0.25).clamp(0.05, 0.25);
    let mut initial = Vec::new();
    let mut parameter_kinds = Vec::new();
    let mut geometry_groups = Vec::new();
    let mut blocks = Vec::new();
    for (group_index, boundaries) in boundary_groups.into_iter().enumerate() {
        let center = group_center(&candidate.scene, &boundaries)?;
        let radius = boundaries
            .iter()
            .flat_map(|boundary| {
                let boundary = &candidate.scene.graph.boundaries[boundary.index()];
                std::iter::once(candidate.scene.graph.vertices[boundary.start_vertex.index()].pos)
                    .chain(boundary.curve.interior_nodes.iter().map(|node| node.pos))
            })
            .map(|point| point.dist(center))
            .fold(1.0f64, f64::max);
        let max_log_scale = (max_translation_px / radius).min(0.02);
        let max_rotation_rad = (max_translation_px / radius).min(0.02);
        let base = initial.len();
        initial.extend([0.0; 4]);
        parameter_kinds.extend([
            ParameterKind::TranslationX {
                limit: max_translation_px,
            },
            ParameterKind::TranslationY {
                limit: max_translation_px,
            },
            ParameterKind::LogScale {
                limit: max_log_scale,
            },
            ParameterKind::Rotation {
                limit: max_rotation_rad,
            },
        ]);
        let indices = [base, base + 1, base + 2, base + 3];
        let normalization = config.trust_region.initial_radius;
        blocks.push(BlockSpec {
            name: if preserve_scene_relations {
                "geometry_joint_relation_preserving".into()
            } else {
                format!("geometry_boundary_{group_index}_relation_preserving")
            },
            parameter_indices: indices.to_vec(),
            scales: vec![
                max_translation_px / normalization,
                max_translation_px / normalization,
                max_log_scale / normalization,
                max_rotation_rad / normalization,
            ],
            max_radius: normalization,
            scope: score_scope(
                fixed_render,
                &geometry_faces(&candidate.scene, &boundaries),
                DEPENDENCY_HALO_PX,
            )?,
        });
        geometry_groups.push(GeometryGroup {
            boundaries,
            center,
            parameter_indices: indices,
        });
    }
    let foreground_face = *candidate
        .paint_layout
        .foreground
        .first()
        .ok_or_else(|| "continuous optimizer has no foreground face".to_string())?;
    let foreground_paint_offset = initial.len();
    initial.extend_from_slice(&paint(&candidate.scene, foreground_face)?.components());
    parameter_kinds.extend([ParameterKind::Paint; 3]);
    blocks.push(BlockSpec {
        name: "foreground_paint".into(),
        parameter_indices: (foreground_paint_offset..foreground_paint_offset + 3).collect(),
        scales: vec![1.0; 3],
        max_radius: 4.0 / 255.0,
        scope: score_scope(
            fixed_render,
            &candidate.paint_layout.foreground,
            DEPENDENCY_HALO_PX,
        )?,
    });
    let background_paint_offset =
        if let Some(&background) = candidate.paint_layout.background.first() {
            let offset = initial.len();
            initial.extend_from_slice(&paint(&candidate.scene, background)?.components());
            parameter_kinds.extend([ParameterKind::Paint; 3]);
            blocks.push(BlockSpec {
                name: "background_paint".into(),
                parameter_indices: (offset..offset + 3).collect(),
                scales: vec![1.0; 3],
                max_radius: 4.0 / 255.0,
                scope: score_scope(
                    fixed_render,
                    &candidate.paint_layout.background,
                    DEPENDENCY_HALO_PX,
                )?,
            });
            Some(offset)
        } else {
            None
        };
    let problem = ContinuousProblem {
        scene: &candidate.scene,
        bindings: &candidate.bindings,
        observed,
        likelihood: config.likelihood,
        priors,
        verification: config.verification,
        quantization: config.quantization,
        geometry_groups,
        parameter_kinds,
        layout: candidate.paint_layout.clone(),
        foreground_paint_offset,
        background_paint_offset,
        export_decimal_places: config.export_decimal_places,
        apron_width_px: config.apron_width_px,
        surrogate_cache: RefCell::new(BTreeMap::new()),
        exact_cache: RefCell::new(BTreeMap::new()),
        fixed_mesh_signatures: RefCell::new(BTreeMap::new()),
    };
    let initial_score = problem
        .serialized_score(&initial, ScoreScope::FULL)
        .map_err(|error| {
            format!("initial continuous scene cannot be serialized and scored: {error}")
        })?;
    problem.exact_cache.borrow_mut().insert(
        ContinuousProblem::cache_key(&initial, ScoreScope::FULL, u64::MAX),
        initial_score.total_bits,
    );
    let predictive_bits_per_block = if initial_score.diagnostics.blocks == 0 {
        f64::INFINITY
    } else {
        initial_score.pixel_bits / initial_score.diagnostics.blocks as f64
    };
    let refinement_trigger = if candidate
        .scene
        .canvas
        .width_px
        .max(candidate.scene.canvas.height_px)
        < 512
    {
        config.small_geometry_refinement_trigger_bits_per_block
    } else {
        config.geometry_refinement_trigger_bits_per_block
    };
    let geometry_active = predictive_bits_per_block > refinement_trigger;
    if !geometry_active {
        blocks.retain(|block| !block.name.starts_with("geometry_"));
    }
    let mut trust_region = config.trust_region;
    if geometry_active {
        // One alternating geometry/paint pass is the bounded M7 rescue. The
        // calibrated trigger keeps this expensive path out of the p95 body.
        trust_region.max_rounds = 1;
    }
    let result = optimize_best_deterministic(&problem, vec![initial], &blocks, trust_region)
        .map_err(|error| error.to_string())?;
    let geometry_scene = problem.materialize(&result.parameters, false)?;
    let final_scene = problem.materialize(&result.parameters, true)?;
    let mut current = candidate.scene.clone();
    let mut transactions = Vec::new();
    let geometry = geometry_mutations(&current, &geometry_scene)?;
    if !geometry.is_empty() {
        let (child, transaction) =
            apply_transition(&current, TransactionKind::JointEscape, geometry)?;
        current = child;
        transactions.push(transaction);
    }
    let mut paints = candidate
        .paint_layout
        .foreground
        .iter()
        .map(|&face| SceneMutation::ReplaceFacePaint {
            face,
            paint: final_scene.graph.faces[face.index()].paint,
        })
        .collect::<Vec<_>>();
    paints.extend(candidate.paint_layout.background.iter().map(|&face| {
        SceneMutation::ReplaceFacePaint {
            face,
            paint: final_scene.graph.faces[face.index()].paint,
        }
    }));
    let (scene, paint_transaction) =
        apply_transition(&current, TransactionKind::PaintChange, paints)?;
    transactions.push(paint_transaction);
    if scene != final_scene {
        return Err("continuous transaction replay differs from optimized scene".into());
    }
    Ok((SceneCandidate { scene, ..candidate }, result, transactions))
}
