use super::*;

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

    fn cache_key(parameters: &[f64], scope: ScoreScope) -> Vec<u64> {
        let mut key = parameters
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>();
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
}

impl TrustRegionProblem for PaintProblem<'_> {
    fn surrogate_bits(
        &self,
        parameters: &[f64],
        scope: ScoreScope,
        _token: vice_opt::EvaluationToken,
    ) -> Result<f64, String> {
        let scene = self.materialize(parameters)?;
        vice_opt::score_full_resolution_scope(
            &scene,
            self.observed,
            self.render,
            self.likelihood,
            self.priors,
            scope,
        )
        .map(|score| score.total_bits)
        .map_err(|error| error.to_string())
    }

    fn exact_bits(
        &self,
        parameters: &[f64],
        scope: ScoreScope,
        _token: vice_opt::EvaluationToken,
    ) -> Result<f64, String> {
        let key = Self::cache_key(parameters, scope);
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
        let bits = vice_opt::score_serialized_full_resolution_scope(
            &scene,
            self.observed,
            witness.premultiplied_rgba8(),
            witness.width_px(),
            witness.height_px(),
            self.likelihood,
            self.priors,
            scope,
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

fn paint_score_scope(
    render: &PartitionRender,
    faces: &[FaceId],
    halo_px: u32,
) -> Result<ScoreScope, String> {
    if halo_px == 0 || faces.is_empty() {
        return Err("paint ROI requires affected faces and a nonzero dependency halo".into());
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
            .ok_or_else(|| "paint ROI face is absent from the fixed render".to_string())?;
        if coverage.len() != width * render.height_px as usize {
            return Err("paint ROI coverage dimensions disagree with the render".into());
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
        return Err("paint ROI has no affected certified pixels".into());
    }
    Ok(ScoreScope {
        roi: Some(vice_opt::Rect { x0, y0, x1, y1 }),
        halo_px,
        global: false,
    })
}

pub(crate) fn optimize_paint(
    candidate: SceneCandidate,
    observed: &vice_image::CanonicalImage,
    fixed_render: &PartitionRender,
    priors: PriorCodeLengths,
    config: &CoreConfig,
) -> Result<(SceneCandidate, OptimizationResult, TransactionApplication), String> {
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
    // Box filtering reaches at most one pixel beyond an affected face. Two
    // pixels conservatively close filter, tessellation, and compositing
    // dependencies; the likelihood then keeps its global correlation-block
    // alignment. Every accepted local step is still serialized and checked
    // against the exact full scene by the trust-region schedule.
    const PAINT_DEPENDENCY_HALO_PX: u32 = 2;
    let mut blocks = vec![BlockSpec {
        name: "foreground_paint".into(),
        parameter_indices: vec![0, 1, 2],
        scales: vec![1.0; 3],
        max_radius: 4.0 / 255.0,
        scope: paint_score_scope(
            fixed_render,
            &candidate.paint_layout.foreground,
            PAINT_DEPENDENCY_HALO_PX,
        )?,
    }];
    if !candidate.paint_layout.background.is_empty() {
        blocks.push(BlockSpec {
            name: "background_paint".into(),
            parameter_indices: vec![3, 4, 5],
            scales: vec![1.0; 3],
            max_radius: 4.0 / 255.0,
            scope: paint_score_scope(
                fixed_render,
                &candidate.paint_layout.background,
                PAINT_DEPENDENCY_HALO_PX,
            )?,
        });
    }
    // The evidence solve is already the deterministic least-squares paint
    // initializer. Pixel-code nudges around it tripled serialized SVG courts
    // without adding a distinct basin for this convex paint-only block.
    let starts = vec![initial];
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
    let (scene, transaction) = apply_compound_transaction_traced(
        &candidate.scene,
        &CompoundTransaction {
            kind: TransactionKind::PaintChange,
            expected_parent_digest: vice_ir::scene_digest_sha256(&candidate.scene)
                .map_err(|error| error.to_string())?,
            mutations,
        },
    )
    .map_err(|error| error.to_string())?;
    Ok((SceneCandidate { scene, ..candidate }, result, transaction))
}
