use super::*;

pub(super) fn discrete_proposal_chain(
    chain: &BoundaryChain,
    cap: usize,
) -> (BoundaryChain, Vec<usize>) {
    let n = chain.samples.len();
    let cap = cap.max(crate::MIN_SUPPORT_SAMPLES).min(n);
    if cap == n {
        return (chain.clone(), (0..n).collect());
    }

    let mut selected = BTreeSet::new();
    if !chain.closed {
        selected.extend([0, n - 1]);
    }

    let mut corners = if chain.closed {
        crate::corner::cyclic_corner_proposals(&chain.samples)
    } else {
        crate::corner::corner_proposals(&chain.samples)
    };
    corners.retain(|proposal| proposal.saliency.is_finite() && proposal.saliency > 0.0);
    corners.sort_by(|left, right| {
        right
            .saliency
            .total_cmp(&left.saliency)
            .then_with(|| left.sample.cmp(&right.sample))
    });
    let minimum_corner_separation = (n / cap).max(1) / 2;
    for corner in corners {
        if selected.len() >= cap / 2 {
            break;
        }
        let separated = selected.iter().all(|present| {
            let direct = corner.sample.abs_diff(*present);
            let distance = if chain.closed {
                direct.min(n - direct)
            } else {
                direct
            };
            distance >= minimum_corner_separation
        });
        if separated {
            selected.insert(corner.sample);
        }
    }

    // Complete the spine by repeatedly splitting its largest remaining gap.
    // Starting from separated corner evidence prevents a uniform sample and a
    // corner sample from forming a spurious subpixel micro-span.
    while selected.len() < cap {
        let next = (0..n)
            .filter(|index| !selected.contains(index))
            .max_by_key(|index| {
                selected
                    .iter()
                    .map(|present| {
                        let direct = index.abs_diff(*present);
                        if chain.closed {
                            direct.min(n - direct)
                        } else {
                            direct
                        }
                    })
                    .min()
                    .unwrap_or(n)
            })
            .expect("cap is below sample count, so an index remains");
        selected.insert(next);
    }
    let indices = selected.into_iter().collect::<Vec<_>>();

    // Preserve the physical evidence mass in the proposal ranking. Each full
    // observation contributes to its nearest retained representative.
    let mut weights = vec![0.0; indices.len()];
    for (source, sample) in chain.samples.iter().enumerate() {
        let representative = indices
            .iter()
            .enumerate()
            .min_by_key(|(_, retained)| {
                let direct = source.abs_diff(**retained);
                if chain.closed {
                    direct.min(n - direct)
                } else {
                    direct
                }
            })
            .map(|(index, _)| index)
            .expect("proposal has at least the minimum support");
        weights[representative] += sample.weight_ds;
    }
    let samples = indices
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let mut sample = chain.samples[*source];
            sample.weight_ds = weights[index];
            sample
        })
        .collect::<Vec<_>>();
    (
        BoundaryChain {
            samples,
            vertices: indices.len() as u64,
            ..chain.clone()
        },
        indices,
    )
}

pub(super) fn rotate_unopened(chain: &BoundaryChain, cut: usize) -> BoundaryChain {
    if cut == 0 || !chain.closed {
        return chain.clone();
    }
    let n = chain.samples.len();
    BoundaryChain {
        samples: (0..n)
            .map(|index| chain.samples[(cut + index) % n])
            .collect(),
        ..chain.clone()
    }
}

/// Fit models while forcing the GT-equivalent family sequence and breakpoints.
///
/// This is the G20 injection point from §27.6. It deliberately does **not**
/// accept segments, parameters, joins, a code table or a selector:
///
/// - each forced span is fitted from the same observations as production;
/// - corner/smooth remains an automatic discrete choice subject to the same
///   jet compatibility and representability rules;
/// - parameters come from the same joint constrained solve;
/// - code and relations use the frozen production table.
///
/// Thus the arm removes discrete family/breakpoint search and nothing else.
pub fn fit_forced_boundary_models(
    chain: &BoundaryChain,
    families: &[SpanFamily],
    breakpoints: &[usize],
    canvas_dim_px: f64,
    k: usize,
) -> Result<ModelRun, ForcedFitRefusal> {
    fit_forced_boundary_models_impl(chain, families, breakpoints, canvas_dim_px, k, None)
}

pub(super) fn fit_forced_boundary_models_impl(
    chain: &BoundaryChain,
    families: &[SpanFamily],
    breakpoints: &[usize],
    canvas_dim_px: f64,
    k: usize,
    continuous_sample_cap: Option<usize>,
) -> Result<ModelRun, ForcedFitRefusal> {
    crate::validate_chain(chain).map_err(|refusal| ForcedFitRefusal::Input { refusal })?;
    crate::validate_canvas_dimension(canvas_dim_px)
        .map_err(|refusal| ForcedFitRefusal::Input { refusal })?;
    let closed = chain.closed;
    let base_chain = dedup_coincident(chain);
    let observed_samples = base_chain.samples.len();
    // The forced breakpoints are expressed from GT's canonical start. Open
    // that same physical loop exactly once; unlike G00 there is no cut search
    // to perform, but Stage H and the seam still need to know it is a loop.
    let opened_chain = if closed {
        rotate(&base_chain, 0)
    } else {
        base_chain
    };
    if families.len() != breakpoints.len() + 1 {
        return Err(ForcedFitRefusal::ShapeMismatch {
            families: families.len(),
            breakpoints: breakpoints.len(),
        });
    }
    let samples = &opened_chain.samples;
    let last = samples.len() - 1;
    let mut previous = 0usize;
    for &breakpoint in breakpoints {
        if breakpoint <= previous || breakpoint >= last {
            return Err(ForcedFitRefusal::BreakpointOutOfRange {
                breakpoint,
                previous,
                last_sample: last,
            });
        }
        previous = breakpoint;
    }

    let mut bounds = Vec::with_capacity(breakpoints.len() + 2);
    bounds.push(0usize);
    bounds.extend_from_slice(breakpoints);
    bounds.push(last);

    let mut candidates = Vec::with_capacity(families.len());
    for (span, (window, &family)) in bounds.windows(2).zip(families).enumerate() {
        let support =
            Support::new(window[0], window[1]).ok_or(ForcedFitRefusal::BreakpointOutOfRange {
                breakpoint: window[1],
                previous: window[0],
                last_sample: last,
            })?;
        let segment = crate::fit(samples, support, family).map_err(|refusal| {
            ForcedFitRefusal::FamilyNoFit {
                span,
                family,
                refusal,
            }
        })?;
        let cost = crate::proposal_cost(samples, support, &segment).map_err(|refusal| {
            ForcedFitRefusal::CostRefused {
                span,
                family,
                refusal,
            }
        })?;
        candidates.push(SpanCandidate {
            support,
            family,
            segment,
            cost,
        });
    }

    let table = &crate::GEOMETRY_CODE_TABLE_V1;
    let edges = build_edges(&candidates, samples, table, canvas_dim_px)
        .map_err(|refusal| ForcedFitRefusal::Input { refusal })?;
    if edges.len() != candidates.len() {
        let missing = candidates
            .iter()
            .enumerate()
            .find(|(candidate, _)| !edges.iter().any(|e| e.candidate == *candidate))
            .expect("edge count differs, so a candidate is absent");
        return Err(ForcedFitRefusal::EdgeMissing {
            span: missing.0,
            family: missing.1.family,
        });
    }
    let paths = k_best_paths_with_closure(&edges, samples, table, canvas_dim_px, k, closed)
        .map_err(|refusal| ForcedFitRefusal::Input { refusal })?;
    if paths.is_empty() {
        return Err(ForcedFitRefusal::NoPath);
    }

    let mut models = Vec::new();
    let mut refused = Vec::new();
    let mut not_representable = 0usize;
    let mut binding_isotopy_refusals = Vec::new();
    let mut continuous_solve_samples = 0usize;
    let mut mandatory_solve_samples = Vec::with_capacity(breakpoints.len() + 2);
    mandatory_solve_samples.push(0);
    mandatory_solve_samples.extend_from_slice(breakpoints);
    mandatory_solve_samples.push(last);
    for path in &paths {
        let closure_smooth = path.closure_smooth;
        if !path_is_representable(path, families) {
            not_representable += 1;
            continue;
        }
        let Some(init) = materialize_with_closure(path, &edges, &candidates, samples) else {
            not_representable += 1;
            continue;
        };
        let closure_is_represented = init.has_closed_tangent_alias();
        let solved = match continuous_sample_cap {
            Some(cap) => {
                joint_constrained_refit_bounded(&init, samples, cap, &mandatory_solve_samples)
            }
            None => joint_constrained_refit(&init, samples).map(|out| (out, samples.len())),
        };
        match solved {
            Ok((out, solve_samples)) => {
                continuous_solve_samples = continuous_solve_samples.max(solve_samples);
                let Ok(lowered) = out.chain.lower() else {
                    bump(&mut refused, "malformed");
                    continue;
                };
                let mut worst_g1 = g1_readings(&lowered, out.chain.start(), out.chain.end())
                    .iter()
                    .map(|r| r.spread_rad)
                    .fold(0.0f64, f64::max);
                if closure_smooth && closure_is_represented {
                    let Some(declared) = out.chain.nodes[0].tangent_rad else {
                        bump(&mut refused, "closure_tangent_missing");
                        continue;
                    };
                    let Some(spread) = closure_g1_spread_rad(
                        &lowered,
                        out.chain.start(),
                        out.chain.end(),
                        declared,
                    ) else {
                        bump(&mut refused, "closure_g1_unread");
                        continue;
                    };
                    worst_g1 = worst_g1.max(spread);
                }
                let mut code = path.code;
                code.residual_bits = crate::code::chain_residual_bits(&out.chain, samples, table);
                if !code.residual_bits.is_finite() {
                    bump(&mut refused, "non_finite_post_refit_code");
                    continue;
                }
                let free_geometry = SelectedBoundaryGeometry::TypedChain { chain: out.chain };
                let mut model = BoundaryModel {
                    stage_h_free_geometry: free_geometry.clone(),
                    stage_h_free_code: code,
                    geometry: free_geometry,
                    families: families.to_vec(),
                    breakpoints: path.breakpoints.clone(),
                    smooth: path.smooth.clone(),
                    closure_smooth,
                    code,
                    proposal_cost_px: path.proposal_cost_px,
                    worst_g1_spread_rad: worst_g1,
                    worst_normal_deviation_px: out.worst_normal_deviation_px,
                    worst_model_to_evidence_px: out.worst_model_to_evidence_px,
                    residual_before: out.residual_before,
                    residual_after: out.residual_after,
                    primitives: Default::default(),
                    primitive_kept: None,
                    relations: Default::default(),
                    relations_kept: 0,
                    relation_kept_indices: Vec::new(),
                };
                if continuous_sample_cap.is_some() {
                    let (displacement_px, allowed_px) =
                        observed_binding_isotopy(&model.stage_h_free_geometry, samples, closed)
                            .unwrap_or((f64::INFINITY, 0.0));
                    if displacement_px > allowed_px + BINDING_RELATION_RESCUE_MARGIN_PX_V1 {
                        binding_isotopy_refusals.push((displacement_px, allowed_px));
                        continue;
                    }
                }
                apply_stage_h(
                    &mut model,
                    samples,
                    table,
                    canvas_dim_px,
                    closed,
                    continuous_sample_cap,
                );
                if continuous_sample_cap.is_some() {
                    let (displacement_px, allowed_px) =
                        observed_binding_isotopy(&model.geometry, samples, closed)
                            .unwrap_or((f64::INFINITY, 0.0));
                    if displacement_px > allowed_px {
                        binding_isotopy_refusals.push((displacement_px, allowed_px));
                        continue;
                    }
                }
                if closure_smooth
                    && !closure_is_represented
                    && matches!(model.geometry, SelectedBoundaryGeometry::TypedChain { .. })
                {
                    bump(&mut refused, "smooth_closure_unrepresented");
                    continue;
                }
                models.push(model);
            }
            Err(why) => bump(&mut refused, refusal_name(&why)),
        }
    }
    if models.is_empty() {
        if let Some(&(displacement_px, allowed_px)) =
            binding_isotopy_refusals.iter().max_by(|left, right| {
                (left.0 / left.1.max(f64::MIN_POSITIVE))
                    .total_cmp(&(right.0 / right.1.max(f64::MIN_POSITIVE)))
            })
        {
            return Err(ForcedFitRefusal::BindingIsotopy {
                displacement_px,
                allowed_px,
            });
        }
        return Err(ForcedFitRefusal::NoAcceptedModel {
            paths: paths.len(),
            not_representable,
            refused,
        });
    }
    models.sort_by(compare_model_rank);
    refused.sort_unstable();
    let relations_considered = models.iter().map(|m| m.relations.len()).sum();
    let relations_accepted = models.iter().map(|m| m.relations_kept).sum();
    let primitives_considered = models.iter().map(|m| m.primitives.len()).sum();
    let primitives_accepted = models.iter().filter(|m| m.primitive_kept.is_some()).count();
    let material_cost_samples = candidates
        .iter()
        .map(|candidate| candidate.cost.material_samples)
        .sum();
    let worst_normal_to_euclidean_ratio = candidates
        .iter()
        .filter_map(|candidate| candidate.cost.worst_ratio)
        .max_by(|left, right| left.ratio.total_cmp(&right.ratio));
    Ok(ModelRun {
        relations_considered,
        relations_accepted,
        primitives_considered,
        primitives_accepted,
        models,
        observed_samples,
        discrete_search_samples: observed_samples,
        discrete_search_levels: vec![observed_samples],
        continuous_solve_samples,
        full_resolution_refit: continuous_sample_cap.is_none(),
        full_resolution_certified: true,
        selected_cut: 0,
        cuts_evaluated: 1,
        candidates: candidates.len(),
        edges: edges.len(),
        discrete_paths: paths.len(),
        refused,
        cost_refusals: Vec::new(),
        worst_normal_to_euclidean_ratio,
        material_cost_samples,
        not_representable,
        full_certification_refusals: 0,
        resource_pruned_proposals: 0,
        proposal_levels_skipped_after_certification: 0,
    })
}

/// Internal injection point for the no-BIC knockout. Production callers cannot
/// replace the frozen table with a feature-local one (M6B-N5).
pub(super) fn k_best_boundary_models_with_table(
    chain: &BoundaryChain,
    budget: &FitBudget,
    table: &GeometryCodeTable,
    canvas_dim_px: f64,
    k: usize,
    stage_h: bool,
) -> Result<ModelRun, FitRefusal> {
    // M6B-N3: an empty CLOSED chain reached `rotate`, which indexes
    // `samples[cut]`, and PANICKED — while every other degenerate input is
    // refused by name. The refusal has to precede `canonical_cuts`/`rotate`,
    // which is F-0088's rule: an input is refused where it is malformed, not
    // where its consequences surface. `< MIN_SUPPORT_SAMPLES` rather than
    // `is_empty`, because `span_candidates` would refuse those lengths anyway
    // and a rotate on a two-sample closed chain has nothing to select over.
    if chain.samples.len() < crate::MIN_SUPPORT_SAMPLES {
        return Err(FitRefusal::ChainTooShort {
            samples: chain.samples.len(),
            minimum: crate::MIN_SUPPORT_SAMPLES,
        });
    }
    let chain = &dedup_coincident(chain);
    let cuts = if chain.closed {
        canonical_cuts(chain)
    } else {
        vec![0usize]
    };
    let mut best: Option<ModelRun> = None;
    let mut best_cut = 0usize;
    let mut candidates_across_cuts = 0usize;
    let mut cuts_evaluated = 0usize;
    let mut cost_refusals_across_cuts = Vec::<(&'static str, &'static str, usize)>::new();
    let mut material_cost_samples_across_cuts = 0usize;
    let mut worst_ratio_across_cuts: Option<crate::RatioReading> = None;
    for cut in cuts {
        let rotated = rotate(chain, cut);
        let run = models_for_open_chain(&rotated, budget, table, canvas_dim_px, k, stage_h)?;
        cuts_evaluated += 1;
        candidates_across_cuts = candidates_across_cuts.saturating_add(run.candidates);
        for &(family, reason, count) in &run.cost_refusals {
            match cost_refusals_across_cuts
                .iter_mut()
                .find(|entry| entry.0 == family && entry.1 == reason)
            {
                Some(entry) => entry.2 = entry.2.saturating_add(count),
                None => cost_refusals_across_cuts.push((family, reason, count)),
            }
        }
        material_cost_samples_across_cuts =
            material_cost_samples_across_cuts.saturating_add(run.material_cost_samples);
        if run.worst_normal_to_euclidean_ratio.is_some_and(|reading| {
            worst_ratio_across_cuts.is_none_or(|current| reading.ratio > current.ratio)
        }) {
            worst_ratio_across_cuts = run.worst_normal_to_euclidean_ratio;
        }
        if candidates_across_cuts > budget.cap() {
            return Err(FitRefusal::CutSearchBudgetExceeded {
                cuts_evaluated,
                candidates: candidates_across_cuts,
                cap: budget.cap(),
            });
        }
        let better = match &best {
            None => true,
            Some(b) => match (b.models.first(), run.models.first()) {
                (_, None) => false,
                (None, Some(_)) => true,
                (Some(x), Some(y)) => compare_model_rank(y, x).is_lt(),
            },
        };
        if better {
            best = Some(run);
            best_cut = cut;
        }
    }
    let mut best = best.ok_or(FitRefusal::ChainTooShort {
        samples: chain.samples.len(),
        minimum: crate::MIN_SUPPORT_SAMPLES,
    })?;
    best.cuts_evaluated = cuts_evaluated;
    best.candidates = candidates_across_cuts;
    best.selected_cut = best_cut;
    best.observed_samples = chain.samples.len();
    best.discrete_search_samples = chain.samples.len();
    best.discrete_search_levels = vec![chain.samples.len()];
    best.cost_refusals = cost_refusals_across_cuts;
    best.material_cost_samples = material_cost_samples_across_cuts;
    best.worst_normal_to_euclidean_ratio = worst_ratio_across_cuts;
    Ok(best)
}

/// The models for ONE canonical cut, without re-cutting.
///
/// Exposed because the cut-invariance test needs to measure what a SINGLE cut
/// selects: calling the whole pipeline on a rotated chain re-cuts it and
/// measures the pipeline again, which is the property under test rather than
/// the thing that makes it non-trivial.
pub fn models_at_cut(
    chain: &BoundaryChain,
    cut: usize,
    budget: &FitBudget,
    canvas_dim_px: f64,
    k: usize,
) -> Result<ModelRun, FitRefusal> {
    crate::validate_chain(chain)?;
    crate::validate_canvas_dimension(canvas_dim_px)?;
    let chain = dedup_coincident(chain);
    if cut >= chain.samples.len() {
        return Err(FitRefusal::CutOutOfRange {
            cut,
            samples: chain.samples.len(),
        });
    }
    let rotated = rotate(&chain, cut);
    models_for_open_chain(
        &rotated,
        budget,
        &crate::GEOMETRY_CODE_TABLE_V1,
        canvas_dim_px,
        k,
        true,
    )
}

fn models_for_open_chain(
    chain: &BoundaryChain,
    budget: &FitBudget,
    table: &GeometryCodeTable,
    canvas_dim_px: f64,
    k: usize,
    stage_h: bool,
) -> Result<ModelRun, FitRefusal> {
    let cands = span_candidates(chain, budget)?;
    let samples = &chain.samples;
    let edges = build_edges(&cands.candidates, samples, table, canvas_dim_px)?;
    let paths = k_best_paths_with_closure(&edges, samples, table, canvas_dim_px, k, chain.closed)?;

    let mut models = Vec::new();
    let mut refused: Vec<(&'static str, usize)> = Vec::new();
    let mut not_representable = 0usize;
    let mut continuous_solve_samples = 0usize;

    for path in &paths {
        let closure_smooth = path.closure_smooth;
        let families: Vec<SpanFamily> = path
            .candidates
            .iter()
            .map(|c| cands.candidates[*c].family)
            .collect();
        if !path_is_representable(path, &families) {
            not_representable += 1;
            continue;
        }
        let Some(init) = materialize_with_closure(path, &edges, &cands.candidates, samples) else {
            not_representable += 1;
            continue;
        };
        let closure_is_represented = init.has_closed_tangent_alias();
        let solved = if stage_h {
            joint_constrained_refit(&init, samples).map(|out| (out, samples.len()))
        } else {
            let mut mandatory = Vec::with_capacity(path.breakpoints.len() + 2);
            mandatory.push(0);
            mandatory.extend_from_slice(&path.breakpoints);
            mandatory.push(samples.len() - 1);
            joint_constrained_refit_bounded(
                &init,
                samples,
                PROPOSAL_CONTINUOUS_REFIT_SAMPLE_CAP_V1,
                &mandatory,
            )
        };
        match solved {
            Ok((out, solve_samples)) => {
                continuous_solve_samples = continuous_solve_samples.max(solve_samples);
                let Ok(lowered) = out.chain.lower() else {
                    bump(&mut refused, "malformed");
                    continue;
                };
                let mut worst_g1 = g1_readings(&lowered, out.chain.start(), out.chain.end())
                    .iter()
                    .map(|r| r.spread_rad)
                    .fold(0.0f64, f64::max);
                if closure_smooth && closure_is_represented {
                    let Some(declared) = out.chain.nodes[0].tangent_rad else {
                        bump(&mut refused, "closure_tangent_missing");
                        continue;
                    };
                    let Some(spread) = closure_g1_spread_rad(
                        &lowered,
                        out.chain.start(),
                        out.chain.end(),
                        declared,
                    ) else {
                        bump(&mut refused, "closure_g1_unread");
                        continue;
                    };
                    worst_g1 = worst_g1.max(spread);
                }
                let mut code = path.code;
                code.residual_bits = crate::code::chain_residual_bits(&out.chain, samples, table);
                if !code.residual_bits.is_finite() {
                    bump(&mut refused, "non_finite_post_refit_code");
                    continue;
                }
                let free_geometry = SelectedBoundaryGeometry::TypedChain { chain: out.chain };
                let mut m = BoundaryModel {
                    stage_h_free_geometry: free_geometry.clone(),
                    stage_h_free_code: code,
                    geometry: free_geometry,
                    families,
                    breakpoints: path.breakpoints.clone(),
                    smooth: path.smooth.clone(),
                    closure_smooth,
                    code,
                    proposal_cost_px: path.proposal_cost_px,
                    worst_g1_spread_rad: worst_g1,
                    worst_normal_deviation_px: out.worst_normal_deviation_px,
                    worst_model_to_evidence_px: out.worst_model_to_evidence_px,
                    residual_before: out.residual_before,
                    residual_after: out.residual_after,
                    primitives: Default::default(),
                    primitive_kept: None,
                    relations: Default::default(),
                    relations_kept: 0,
                    relation_kept_indices: Vec::new(),
                };
                if stage_h {
                    apply_stage_h(&mut m, samples, table, canvas_dim_px, chain.closed, None);
                }
                if closure_smooth
                    && !closure_is_represented
                    && matches!(m.geometry, SelectedBoundaryGeometry::TypedChain { .. })
                {
                    bump(&mut refused, "smooth_closure_unrepresented");
                    continue;
                }
                models.push(m);
            }
            Err(why) => bump(&mut refused, refusal_name(&why)),
        }
    }
    // §24's `rank_by_proposal_integral_and_code_length`: code length first
    // because it is the physical-bit score §14.5 names as the selector, the
    // §14.4 integral as the tie-break, because §14.4 says it ORDERS candidates
    // and does not select among final models.
    models.sort_by(compare_model_rank);
    refused.sort_unstable();

    let relations_considered = models.iter().map(|m| m.relations.len()).sum();
    let relations_accepted = models.iter().map(|m| m.relations_kept).sum();
    let primitives_considered = models.iter().map(|m| m.primitives.len()).sum();
    let primitives_accepted = models.iter().filter(|m| m.primitive_kept.is_some()).count();
    let cost_refusals = cands
        .no_costs
        .iter()
        .map(|(family, refusal, count)| (*family, refusal.name(), *count))
        .collect();
    Ok(ModelRun {
        relations_considered,
        relations_accepted,
        primitives_considered,
        primitives_accepted,
        models,
        observed_samples: chain.samples.len(),
        discrete_search_samples: chain.samples.len(),
        discrete_search_levels: vec![chain.samples.len()],
        continuous_solve_samples,
        full_resolution_refit: continuous_solve_samples >= chain.samples.len(),
        full_resolution_certified: true,
        selected_cut: 0,
        cuts_evaluated: 1,
        candidates: cands.candidates.len(),
        edges: edges.len(),
        discrete_paths: paths.len(),
        refused,
        cost_refusals,
        worst_normal_to_euclidean_ratio: cands.worst_ratio,
        material_cost_samples: cands.material_samples,
        not_representable,
        full_certification_refusals: 0,
        resource_pruned_proposals: 0,
        proposal_levels_skipped_after_certification: 0,
    })
}

/// §24's declared ordering, in one function the knockout test can exercise.
///
/// The physical-bit code is the final chain selector. The §14.4 proposal
/// integral is the deterministic order within an exact code tie; it is not a
/// second likelihood. Keeping both arguments here makes deleting the proposal
/// leg mechanically red instead of leaving `rank_by_proposal_integral...`
/// guarded only by prose (RT6-A4).
pub(super) fn compare_rank_values(
    a_code: f64,
    a_proposal: f64,
    b_code: f64,
    b_proposal: f64,
) -> Ordering {
    a_code
        .total_cmp(&b_code)
        .then(a_proposal.total_cmp(&b_proposal))
}

pub(super) fn compare_model_rank(a: &BoundaryModel, b: &BoundaryModel) -> Ordering {
    compare_rank_values(
        a.code.total_bits(),
        a.proposal_cost_px,
        b.code.total_bits(),
        b.proposal_cost_px,
    )
}

fn bump(acc: &mut Vec<(&'static str, usize)>, name: &'static str) {
    match acc.iter_mut().find(|e| e.0 == name) {
        Some(e) => e.1 += 1,
        None => acc.push((name, 1)),
    }
}
