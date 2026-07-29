//! The §28 M6 candidate stage, measured on the CORPUS rather than on chains
//! whose shape I chose.
//!
//! `vice-fit`'s own tests fit families to chains drawn from a line, an arc and
//! a cubic, and check that each family reproduces its own shape and not
//! another's. That is the right test for CORRECTNESS and it is the wrong test
//! for POPULATION: every chain in it is one I wrote, at a curvature I picked,
//! with a corridor halfwidth I set to a constant. A stage that worked only on
//! such chains would pass all of them.
//!
//! So this walks real corpus scenes through the real M4 path — render the
//! degradation cell, decode, `analyze_full`, `observe_boundaries` — and runs
//! the candidate stage over whatever chains come out. The numbers it publishes
//! are the ones a later gate row would have to stand on: how many chains,
//! how many samples, how many candidates, which families were present, how
//! many refusals of each kind, and the largest ratio between §14.4's `d_n` and
//! the Euclidean deviation — the last being what the approximation `vice-fit`
//! integrated until C302 was worth, measured on the population rather than
//! bounded by an argument.
//!
//! **§27.1 is respected**: groups in the sealed-audit split are skipped, and
//! the count of skipped groups is reported. Scoring the sealed audit is what
//! opens it, and a measurement is a score.
//!
//! **This is still not a gate ROW.** No threshold is read from
//! `configs/GATES_V1.toml` and no clause is emitted into a scorecard. What it
//! now does assert is the §28 M6 gate's FIRST clause as a property — every
//! smooth node of every selected model, measured on the corpus by the same
//! instrument that reads 0.4949 rad on `vice-ir`'s canonical fixture — because
//! that clause is about the geometry rather than about a threshold, and a
//! property with no number to compare against is asserted or it is nothing.
//! The other three clauses are addressed in `tests/grammar_and_g1.rs`
//! (invariance, no BIC-only promotion) and in `oracle::design` (G00–G20, which
//! is NOT met and says why).

use serde::Serialize;
use vice_evidence::analysis::{analyze_full, ANALYSIS_CONFIG_V1};
use vice_evidence::boundary::{observe_boundaries, BOUNDARY_CONFIG_V1};
use vice_evidence::corridor::CORRIDOR_CONFIG_V1;
use vice_fit::{
    g1_readings, k_best_boundary_models, span_candidates, FitRefusal, NoFit, FIT_BUDGET_V1,
    K_DISCRETE_PATHS,
};
use vice_image::{CanonicalImage, IccAssumption};

use crate::gt::corpus::all_groups;
use crate::gt::degradation::{matrix_v1, render_cell};
use crate::gt::split::{Split, SPLIT_POLICY_V1};

/// What the candidate stage did over the corpus.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FitRun {
    /// Scenes rendered and analysed.
    pub arms: u64,
    /// Groups skipped because they are in the sealed-audit split (§27.1).
    pub sealed_audit_groups_skipped: u64,
    /// Arms where the M4 path produced no evidence or no contour at all, so
    /// there was no chain to offer candidates on. Counted, because "zero
    /// candidates" and "nothing to fit" are different facts (F-0075).
    pub arms_without_a_boundary: u64,
    pub chains: u64,
    pub chain_samples: u64,
    pub supports: u64,
    /// The sum over chains of `vice_fit::anchored_schedule_bound`, which is the
    /// STRUCTURAL bound the schedule's size is a theorem against. Compared with
    /// `supports` rather than with a per-sample constant, because anchoring at
    /// corner proposals makes the schedule O(n log n) and inheriting the plain
    /// ladder's O(n) bound would be a bound that no longer bounds its subject.
    pub supports_bound: u64,
    /// Corner anchors the schedule hung its ladder on, over the run, and the
    /// largest §14.1 saliency seen. Published because the schedule's size
    /// depends on the first and the second says whether the corner instrument
    /// found any structure at all on this corpus.
    pub corner_anchors: u64,
    pub max_corner_saliency: f64,
    pub candidates: u64,
    /// Chains the candidate stage refused, by refusal name.
    pub refusals: Vec<(&'static str, u64)>,
    /// Family names that produced at least one candidate anywhere in the run,
    /// MEASURED. An absent family is an absent name rather than silence.
    pub families_present: Vec<String>,
    /// Per-family fit refusals measured over every offered support. The M6B-N7
    /// version kept this only inside each `ChainCandidates`, so a family could
    /// disappear below the coarse `families_present` floor without the corpus
    /// report saying why.
    pub family_no_fits: Vec<(String, &'static str, u64)>,
    /// Largest `d_n / d_euclidean` over every candidate of the run, with the
    /// Euclidean deviation at which it occurred — `None` when no candidate
    /// anywhere had a material sample (RT6-A5: a run that measured nothing
    /// must not print the lower boundary of the range). `1.0` would mean the
    /// pre-C302 lower bound was §14.4's integrand all along.
    pub worst_ratio: Option<vice_fit::RatioReading>,
    /// The population the ratio stands on, over the whole run.
    pub material_samples: u64,
    /// Candidates that FITTED and whose §14.4 cost then refused them, by
    /// reason. Overwhelmingly `normal_line_misses`: a candidate no sample of
    /// its own support can see along its own normal.
    pub cost_refusals: Vec<(&'static str, u64)>,
    /// Candidates offered before the cost stage refused any. `candidates` is
    /// what survived; the difference is what the correction removed.
    pub candidates_before_cost: u64,
    /// Smallest headroom any chain left against the hard cap.
    pub min_budget_headroom: usize,
    /// Longest chain seen, in samples.
    pub longest_chain_samples: usize,

    // --- §28 M6 bullets 3-6: the grammar, the joint solve and Stage H -------
    /// Chains for which at least one discrete path survived the joint solve.
    pub chains_with_a_model: u64,
    /// Chains the pipeline ran to completion on and every one of the k paths
    /// was refused by the solver.
    ///
    /// SEPARATE from `chains_refused_at_entry`, and the separation is
    /// REDTEAM_M6 §4's finding: `Err(_)` from `k_best_boundary_models` used to
    /// fall into this same counter, so an input-contract refusal and a solver
    /// that emptied its k-best were one number wearing this field's doc
    /// comment (RT5-A21's class — a refusal folded into a value that means
    /// something else).
    pub chains_with_no_model: u64,
    /// Chains the entry contract refused (`FitRefusal`), by name.
    pub chains_refused_at_entry: Vec<(&'static str, u64)>,
    /// Segments in the SELECTED model, summed over chains, and how many of the
    /// joins between them are smooth.
    pub selected_segments: u64,
    pub selected_smooth_joins: u64,
    /// Family names present in selected models, MEASURED.
    pub selected_families: Vec<String>,
    /// **The §28 M6 gate's first clause, over the corpus**: the largest G1
    /// spread at any smooth node of any SELECTED model, in radians, measured
    /// from the lowered control points by the same instrument that reads
    /// 0.4949 rad on `vice-ir`'s canonical fixture.
    pub worst_g1_spread_rad: f64,
    /// Smooth nodes the clause was evaluated over. A worst-of-nothing is zero
    /// and says nothing (F-0039), and the corpus test now asserts this EQUALS
    /// `selected_smooth_joins` — M6B-N4: `g1_readings` silently skips a node
    /// whose incident direction it cannot read, so without the equality the
    /// clause's population can shrink under its own instrument and 8 = 8
    /// would be a coincidence rather than a check.
    pub g1_nodes_measured: u64,
    /// Selected models whose `lower()` failed in the measurement. Used to be
    /// swallowed by an `if let Ok` (M6B-N4); asserted zero in the corpus test.
    pub g1_lowering_failures: u64,
    /// Worst `|d_n|` of any selected model after the solve, px.
    pub worst_selected_deviation_px: f64,
    /// Discrete paths the joint solve refused, by reason. §24's reason for
    /// k-best is that this is not zero.
    pub path_refusals: Vec<(String, u64)>,
    /// §15 relation hypotheses formed and accepted over the run.
    pub relations_considered: u64,
    pub relations_accepted: u64,
}

impl FitRun {
    /// Supports offered per sample, over the whole run. The quantity §14.2's
    /// "no full O(N^2) all-pairs" is about, on the real population rather than
    /// on the synthetic sweep in `vice_fit::schedule`.
    pub fn supports_per_sample(&self) -> f64 {
        if self.chain_samples == 0 {
            return 0.0;
        }
        self.supports as f64 / self.chain_samples as f64
    }
}

fn cost_refusal_name(r: &vice_fit::CostRefusal) -> &'static str {
    match r {
        vice_fit::CostRefusal::NormalLineMisses { .. } => "normal_line_misses",
        vice_fit::CostRefusal::NotFlattenable => "not_flattenable",
        vice_fit::CostRefusal::NonFinite { .. } => "non_finite",
    }
}

fn no_fit_name(r: &NoFit) -> &'static str {
    match r {
        NoFit::ArcIsDegenerate => "arc_is_degenerate",
        NoFit::CubicIsSingular => "cubic_is_singular",
        NoFit::NonFiniteFit => "non_finite_fit",
    }
}

fn refusal_name(r: &FitRefusal) -> &'static str {
    match r {
        FitRefusal::ChainTooShort { .. } => "chain_too_short",
        FitRefusal::BudgetExceeded { .. } => "budget_exceeded",
        FitRefusal::NonPositiveHalfwidth { .. } => "non_positive_halfwidth",
        FitRefusal::NonFiniteSample { .. } => "non_finite_sample",
        FitRefusal::NonUnitNormal { .. } => "non_unit_normal",
        FitRefusal::NonPositiveCorrLength { .. } => "non_positive_corr_length",
    }
}

/// Run the candidate stage over the corpus.
///
/// `cells` selects how many degradation cells per scene to walk; the caller
/// passes a small number for the default test path and the whole matrix for a
/// full measurement. The value used is REPORTED by the caller rather than
/// assumed, because a run over one cell and a run over the matrix are
/// different populations wearing the same name.
pub fn measure(cells_per_scene: usize) -> Result<FitRun, String> {
    let groups = all_groups()?;
    let matrix = matrix_v1();
    let cells: Vec<_> = matrix.iter().take(cells_per_scene).collect();
    let mut run = FitRun {
        min_budget_headroom: usize::MAX,
        ..FitRun::default()
    };
    let mut families = std::collections::BTreeSet::new();
    let mut selected_families = std::collections::BTreeSet::new();

    for group in &groups {
        if SPLIT_POLICY_V1.split_of_group(group) == Split::SealedAudit {
            // §27.1: scoring the sealed audit is what OPENS it.
            run.sealed_audit_groups_skipped += 1;
            continue;
        }
        for scene in &group.scenes {
            for cell in &cells {
                let fixture = render_cell(scene, cell, group.scenes.len())?;
                let img = CanonicalImage::from_straight_srgb8(
                    fixture.width_px,
                    fixture.height_px,
                    fixture.rgba8.clone(),
                    true,
                    IccAssumption::NoProfileAssumedSrgb,
                )
                .map_err(|e| e.to_string())?;
                run.arms += 1;

                let Some(ev) = analyze_full(&img, &ANALYSIS_CONFIG_V1, None).chosen else {
                    run.arms_without_a_boundary += 1;
                    continue;
                };
                let Ok(obs) =
                    observe_boundaries(&ev, 0.95, &BOUNDARY_CONFIG_V1, &CORRIDOR_CONFIG_V1)
                else {
                    run.arms_without_a_boundary += 1;
                    continue;
                };
                if obs.chains.is_empty() {
                    run.arms_without_a_boundary += 1;
                    continue;
                }

                for chain in &obs.chains {
                    match k_best_boundary_models(
                        chain,
                        &FIT_BUDGET_V1,
                        f64::from(fixture.width_px.max(fixture.height_px)),
                        K_DISCRETE_PATHS,
                    ) {
                        Ok(mr) => {
                            run.relations_considered += mr.relations_considered as u64;
                            run.relations_accepted += mr.relations_accepted as u64;
                            for (name, n) in &mr.refused {
                                match run.path_refusals.iter_mut().find(|e| e.0 == *name) {
                                    Some(e) => e.1 += *n as u64,
                                    None => {
                                        run.path_refusals.push(((*name).to_string(), *n as u64))
                                    }
                                }
                            }
                            match mr.models.first() {
                                Some(m) => {
                                    run.chains_with_a_model += 1;
                                    run.selected_segments += m.families.len() as u64;
                                    run.selected_smooth_joins +=
                                        m.smooth.iter().filter(|s| **s).count() as u64;
                                    for f in &m.families {
                                        selected_families.insert(f.universe_name().to_string());
                                    }
                                    run.worst_selected_deviation_px = run
                                        .worst_selected_deviation_px
                                        .max(m.worst_normal_deviation_px);
                                    match m.chain.lower() {
                                        Ok(lowered) => {
                                            let r = g1_readings(
                                                &lowered,
                                                m.chain.start(),
                                                m.chain.end(),
                                            );
                                            run.g1_nodes_measured += r.len() as u64;
                                            for x in &r {
                                                run.worst_g1_spread_rad =
                                                    run.worst_g1_spread_rad.max(x.spread_rad);
                                            }
                                        }
                                        // Not swallowed (M6B-N4): a selected
                                        // model that cannot lower is a defect
                                        // of the measurement, not a skip.
                                        Err(_) => run.g1_lowering_failures += 1,
                                    }
                                }
                                None => run.chains_with_no_model += 1,
                            }
                        }
                        Err(why) => {
                            let name = refusal_name(&why);
                            match run.chains_refused_at_entry.iter_mut().find(|e| e.0 == name) {
                                Some(e) => e.1 += 1,
                                None => run.chains_refused_at_entry.push((name, 1)),
                            }
                        }
                    }
                    run.chains += 1;
                    run.longest_chain_samples = run.longest_chain_samples.max(chain.samples.len());
                    match span_candidates(chain, &FIT_BUDGET_V1) {
                        Ok(c) => {
                            run.chain_samples += c.chain_samples as u64;
                            run.supports += c.supports as u64;
                            run.supports_bound +=
                                vice_fit::anchored_schedule_bound(c.chain_samples, c.anchors.len())
                                    as u64;
                            run.corner_anchors += c.anchors.len() as u64;
                            run.max_corner_saliency = c
                                .corners
                                .iter()
                                .map(|k| k.saliency)
                                .fold(run.max_corner_saliency, f64::max);
                            run.candidates += c.candidates.len() as u64;
                            run.material_samples += c.material_samples as u64;
                            if let Some(r) = c.worst_ratio {
                                if run.worst_ratio.is_none_or(|w| r.ratio > w.ratio) {
                                    run.worst_ratio = Some(r);
                                }
                            }
                            run.candidates_before_cost += c.candidates.len() as u64;
                            for (_, why, n) in &c.no_costs {
                                run.candidates_before_cost += *n as u64;
                                let name = cost_refusal_name(why);
                                match run.cost_refusals.iter_mut().find(|e| e.0 == name) {
                                    Some(e) => e.1 += *n as u64,
                                    None => run.cost_refusals.push((name, *n as u64)),
                                }
                            }
                            run.min_budget_headroom =
                                run.min_budget_headroom.min(c.budget_headroom);
                            for f in c.families_present {
                                families.insert(f.to_string());
                            }
                            for (family, why, count) in c.no_fits {
                                let family = family.to_string();
                                let why = no_fit_name(&why);
                                match run
                                    .family_no_fits
                                    .iter_mut()
                                    .find(|e| e.0 == family && e.1 == why)
                                {
                                    Some(e) => e.2 += count as u64,
                                    None => run.family_no_fits.push((family, why, count as u64)),
                                }
                            }
                        }
                        Err(why) => {
                            let name = refusal_name(&why);
                            match run.refusals.iter_mut().find(|e| e.0 == name) {
                                Some(e) => e.1 += 1,
                                None => run.refusals.push((name, 1)),
                            }
                        }
                    }
                }
            }
        }
    }

    run.families_present = families.into_iter().collect();
    run.selected_families = selected_families.into_iter().collect();
    run.refusals.sort_unstable();
    run.cost_refusals.sort_unstable();
    run.path_refusals.sort_unstable();
    run.family_no_fits.sort_unstable();
    if run.min_budget_headroom == usize::MAX {
        run.min_budget_headroom = 0;
    }
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The corpus measurement.** Ignored by default because it renders and
    /// analyses every non-sealed corpus scene; run with `--ignored` in
    /// release, which is where this project's corpus-wide numbers already
    /// live.
    ///
    /// It asserts only what would make the printed numbers meaningless — an
    /// empty population, a family that vanished, a refusal that should never
    /// fire on well-formed evidence. The numbers themselves are PRINTED, not
    /// gated: §28 M6's gate is about the joint solve and the oracle
    /// decomposition, neither of which exists, and inventing a floor here
    /// would be a gate on the half of the milestone that happens to be built.
    #[test]
    #[ignore = "walks the corpus; renders and analyses every non-sealed scene"]
    fn the_candidate_stage_over_the_corpus() {
        let run = measure(1).expect("corpus run");

        println!("--- §28 M6 bullets 1-6, over the corpus, 1 cell per scene ---");
        println!("arms                        {}", run.arms);
        println!(
            "  without a boundary        {}",
            run.arms_without_a_boundary
        );
        println!(
            "sealed-audit groups skipped {}",
            run.sealed_audit_groups_skipped
        );
        println!("chains                      {}", run.chains);
        println!("chain samples               {}", run.chain_samples);
        println!("longest chain (samples)     {}", run.longest_chain_samples);
        println!(
            "supports                    {} ({:.3} per sample, structural bound {})",
            run.supports,
            run.supports_per_sample(),
            run.supports_bound
        );
        println!(
            "corner anchors              {} (max §14.1 saliency {:.4})",
            run.corner_anchors, run.max_corner_saliency
        );
        println!("candidates                  {}", run.candidates);
        println!("families present            {:?}", run.families_present);
        println!("family no-fits              {:?}", run.family_no_fits);
        println!("refusals                    {:?}", run.refusals);
        println!(
            "candidates before cost      {} (cost refusals {:?})",
            run.candidates_before_cost, run.cost_refusals
        );
        match run.worst_ratio {
            Some(r) => println!(
                "worst d_n / d_euclid        {:.4}, at a Euclidean deviation of {:.5} px, over \
                 {} material samples",
                r.ratio, r.at_deviation_px, run.material_samples
            ),
            None => println!(
                "worst d_n / d_euclid        NOT MEASURED: 0 material samples in the whole run"
            ),
        }
        println!("min budget headroom         {}", run.min_budget_headroom);
        println!("--- bullets 3-6: grammar, joint solve, Stage H ---");
        println!(
            "chains with a model         {} (solver emptied k-best: {}, refused at entry: {:?})",
            run.chains_with_a_model, run.chains_with_no_model, run.chains_refused_at_entry
        );
        println!(
            "selected segments           {} ({} smooth joins), families {:?}",
            run.selected_segments, run.selected_smooth_joins, run.selected_families
        );
        println!("path refusals               {:?}", run.path_refusals);
        println!(
            "relations                   {} considered, {} accepted",
            run.relations_considered, run.relations_accepted
        );
        println!(
            "worst selected |d_n|        {:.5} px",
            run.worst_selected_deviation_px
        );
        println!(
            "WORST G1 SPREAD             {:.3e} rad over {} smooth nodes ({} lowering failures)",
            run.worst_g1_spread_rad, run.g1_nodes_measured, run.g1_lowering_failures
        );

        assert!(
            run.arms > 0 && run.chains > 0,
            "the corpus produced no chains at all, so every number above is about nothing \
             (F-0039)"
        );
        assert!(
            run.sealed_audit_groups_skipped > 0,
            "no group was skipped as sealed audit: either the split policy stopped classifying \
             or this run is scoring the bucket §27.1 exists to keep closed"
        );
        assert_eq!(
            run.families_present.len(),
            vice_fit::FITTED_FAMILIES.len(),
            "families present on the corpus: {:?}. A family with a fitter that produces nothing \
             anywhere on the real population is either dead code or a fitter that never applies, \
             and both are findings",
            run.families_present
        );
        assert!(
            run.supports <= run.supports_bound,
            "the corpus produced {} supports against a structural bound of {}",
            run.supports,
            run.supports_bound
        );
        assert!(
            run.chains_with_a_model > 0,
            "no chain on the corpus produced a model that survived the joint solve; every number \
             about the grammar above is then about nothing (F-0039)"
        );
        assert!(
            run.g1_nodes_measured > 0,
            "no selected model on the corpus has a single smooth join, so the G1 clause was \
             evaluated over an empty population"
        );
        // M6B-N4: the clause's population equals the selection's smooth joins,
        // as a CHECK rather than a coincidence — `g1_readings` silently skips
        // nodes it cannot read, and a skip here would shrink the population
        // under the instrument with the worst-of still green.
        assert_eq!(
            run.g1_nodes_measured, run.selected_smooth_joins,
            "g1_readings measured {} nodes over {} selected smooth joins: the instrument skipped \
             part of the clause's own population",
            run.g1_nodes_measured, run.selected_smooth_joins
        );
        assert_eq!(
            run.g1_lowering_failures, 0,
            "{} selected models failed to lower during the G1 measurement",
            run.g1_lowering_failures
        );
        assert!(
            run.chains_refused_at_entry.is_empty(),
            "chains the entry contract refused on the real corpus: {:?} — the M4 evidence path \
             is producing malformed chains",
            run.chains_refused_at_entry
        );
        assert!(
            run.worst_g1_spread_rad < vice_fit::GATE_MAX_G1_SPREAD_RAD,
            "a model selected on the corpus has a G1 spread of {} rad",
            run.worst_g1_spread_rad
        );
        assert!(
            run.corner_anchors > 0,
            "the §14.1 corner instrument found no anchor anywhere on the corpus; then the              anchored schedule is the plain one wearing a different name"
        );
        // The refusals that would mean the M4 evidence is malformed rather
        // than that a chain is short.
        for (name, count) in &run.refusals {
            assert!(
                *name == "chain_too_short",
                "the corpus triggered refusal `{name}` {count} times; only `chain_too_short` is \
                 an expected property of real contours, and the others say something is wrong \
                 with the observation rather than with the chain"
            );
        }
    }
}
