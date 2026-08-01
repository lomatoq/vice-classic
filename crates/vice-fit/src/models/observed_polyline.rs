use super::*;

/// Hard structural cap for the evidence-owned polyline member of the Stage-G
/// grammar. It is a bounded last member of the same line-family universe, not
/// an unpriced polygon fallback.
pub const MAX_OBSERVED_POLYLINE_SEGMENTS_V1: usize = 128;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "observed_polyline_refusal", rename_all = "snake_case")]
pub enum ObservedPolylineRefusal {
    Input {
        refusal: crate::FitRefusal,
    },
    TooComplex {
        segments: usize,
        cap: usize,
    },
    Lowering {
        refusal: RefitRefusal,
    },
    Corridor {
        direction: &'static str,
        deviation_px: f64,
        allowed_px: f64,
    },
    BindingIsotopy {
        displacement_px: f64,
        allowed_px: f64,
    },
    SelfIntersection,
}

/// Construct the bounded, explicitly priced line-chain that follows the
/// physical Stage-F observation itself. Compact typed grammar models remain
/// preferred by code length. This member exists so failure of every compact
/// joint solve does not incorrectly mean that an otherwise certified observed
/// contour is outside the supported line-family universe.
pub fn observed_polyline_rescue_model(
    chain: &BoundaryChain,
    canvas_dim_px: f64,
) -> Result<BoundaryModel, ObservedPolylineRefusal> {
    crate::validate_canvas_dimension(canvas_dim_px)
        .map_err(|refusal| ObservedPolylineRefusal::Input { refusal })?;
    let chain = dedup_coincident(chain);
    crate::validate_samples(&chain.samples)
        .map_err(|refusal| ObservedPolylineRefusal::Input { refusal })?;

    let mut ordered = if chain.closed {
        let root = canonical_cuts(&chain).first().copied().unwrap_or(0);
        (0..chain.samples.len())
            .map(|offset| chain.samples[(root + offset) % chain.samples.len()])
            .collect::<Vec<_>>()
    } else {
        chain.samples.clone()
    };
    if chain.closed {
        ordered.push(ordered[0]);
    }

    let binding_tube_px = chain
        .samples
        .iter()
        .map(|sample| sample.halfwidth)
        .fold(0.0f64, f64::max)
        + 0.5
            * chain
                .samples
                .iter()
                .map(|sample| sample.weight_ds)
                .fold(0.0f64, f64::max);
    let mut kept = BTreeSet::from([0usize, ordered.len() - 1]);
    let mut pending = vec![(0usize, ordered.len() - 1)];
    while let Some((lo, hi)) = pending.pop() {
        if hi <= lo + 1 {
            continue;
        }
        let a = ordered[lo].p;
        let b = ordered[hi].p;
        let worst = (lo + 1..hi)
            .map(|index| {
                let sample = ordered[index];
                let deviation = point_segment_distance(sample.p, a, b);
                let allowance = (FEASIBLE_HALFWIDTHS * sample.halfwidth)
                    .min(binding_tube_px)
                    .max(f64::MIN_POSITIVE);
                (index, deviation / allowance)
            })
            .max_by(|left, right| left.1.total_cmp(&right.1));
        if let Some((split, _)) = worst.filter(|(_, ratio)| *ratio > 0.5) {
            kept.insert(split);
            pending.push((split, hi));
            pending.push((lo, split));
        }
    }
    // A corridor-safe chord can still cut across a narrow neck. Restore
    // observed vertices on every exact non-adjacent crossing until the
    // simplified chain is topology-safe. This is monotone refinement toward
    // the audited observation, and the structural cap remains authoritative.
    loop {
        let indices = kept.iter().copied().collect::<Vec<_>>();
        let segment_count = indices.len().saturating_sub(1);
        let mut crossing = None;
        'pairs: for left in 0..segment_count {
            for right in left + 1..segment_count {
                let adjacent =
                    right == left + 1 || (chain.closed && left == 0 && right + 1 == segment_count);
                if adjacent {
                    continue;
                }
                let (a0, a1) = (indices[left], indices[left + 1]);
                let (b0, b1) = (indices[right], indices[right + 1]);
                if vice_geom::predicates::closed_segments_intersect(
                    ordered[a0].p,
                    ordered[a1].p,
                    ordered[b0].p,
                    ordered[b1].p,
                ) {
                    crossing = Some((a0, a1, b0, b1));
                    break 'pairs;
                }
            }
        }
        let Some((a0, a1, b0, b1)) = crossing else {
            break;
        };
        let mut refined = false;
        for (lo, hi) in [(a0, a1), (b0, b1)] {
            if hi > lo + 1 {
                refined |= kept.insert((lo + hi) / 2);
            }
        }
        if !refined {
            return Err(ObservedPolylineRefusal::SelfIntersection);
        }
        if kept.len().saturating_sub(1) > MAX_OBSERVED_POLYLINE_SEGMENTS_V1 {
            break;
        }
    }
    let kept = kept.into_iter().collect::<Vec<_>>();
    let segments = kept.len().saturating_sub(1);
    if segments == 0 || segments > MAX_OBSERVED_POLYLINE_SEGMENTS_V1 {
        return Err(ObservedPolylineRefusal::TooComplex {
            segments,
            cap: MAX_OBSERVED_POLYLINE_SEGMENTS_V1,
        });
    }
    let refit = RefitChain {
        nodes: kept
            .iter()
            .map(|&index| RefitNode {
                pos: ordered[index].p,
                tangent_rad: None,
            })
            .collect(),
        segments: vec![RefitSegment::Line; segments],
    };
    refit
        .lower()
        .map_err(|refusal| ObservedPolylineRefusal::Lowering { refusal })?;
    let poly = crate::solve::flatten_chain(&refit)
        .map_err(|refusal| ObservedPolylineRefusal::Lowering { refusal })?;
    let forward = crate::solve::evidence_to_model_corridor(&poly, &chain.samples);
    if !forward.feasible() {
        return Err(ObservedPolylineRefusal::Corridor {
            direction: "evidence_to_model",
            deviation_px: forward.deviation_px,
            allowed_px: forward.allowed_px,
        });
    }
    let reverse = crate::solve::model_to_evidence_corridor(&poly, &chain.samples);
    if !reverse.feasible() {
        return Err(ObservedPolylineRefusal::Corridor {
            direction: "model_to_evidence",
            deviation_px: reverse.deviation_px,
            allowed_px: reverse.allowed_px,
        });
    }

    let geometry = SelectedBoundaryGeometry::TypedChain {
        chain: refit.clone(),
    };
    let (displacement_px, allowed_px) =
        observed_binding_isotopy(&geometry, &chain.samples, chain.closed)
            .unwrap_or((f64::INFINITY, 0.0));
    if displacement_px > allowed_px {
        return Err(ObservedPolylineRefusal::BindingIsotopy {
            displacement_px,
            allowed_px,
        });
    }

    let unique_nodes = refit.nodes.len().saturating_sub(usize::from(chain.closed));
    let residual_bits =
        crate::code::chain_residual_bits(&refit, &chain.samples, &crate::GEOMETRY_CODE_TABLE_V1);
    let code = crate::code::observed_polyline_code(
        unique_nodes,
        segments,
        chain.samples.len(),
        canvas_dim_px,
        residual_bits,
    );
    Ok(BoundaryModel {
        stage_h_free_geometry: geometry.clone(),
        stage_h_free_code: code,
        geometry,
        families: vec![SpanFamily::Line; segments],
        breakpoints: kept
            .iter()
            .copied()
            .skip(1)
            .take(segments.saturating_sub(1))
            .collect(),
        smooth: vec![false; segments.saturating_sub(1)],
        closure_smooth: false,
        code,
        proposal_cost_px: forward.deviation_px,
        worst_g1_spread_rad: 0.0,
        worst_normal_deviation_px: forward.deviation_px,
        worst_model_to_evidence_px: reverse.deviation_px,
        residual_before: code.residual_bits,
        residual_after: code.residual_bits,
        primitives: Arc::new(Vec::new()),
        primitive_kept: None,
        relations: Arc::new(Vec::new()),
        relations_kept: 0,
        relation_kept_indices: Vec::new(),
    })
}
