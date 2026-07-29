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
//! many refusals of each kind, and the largest normal departure — the last
//! being the size of the gap between the Euclidean deviation `vice-fit`
//! measures and §14.4's `d_n`, which is an approximation nobody should accept
//! on my description of it.
//!
//! **§27.1 is respected**: groups in the sealed-audit split are skipped, and
//! the count of skipped groups is reported. Scoring the sealed audit is what
//! opens it, and a measurement is a score.
//!
//! **This is not a gate.** No threshold is read, no clause is evaluated, no
//! row is emitted. §28 M6's gate is "exact G1 after joint solve; sample/cut/
//! transform invariance; oracle G00–G20; no BIC-only promotion", and none of
//! those four is evaluable while bullets 3–6 do not exist. What is here is a
//! population and its measurement, which is what a gate would later need and
//! is not itself one.

use serde::Serialize;
use vice_evidence::analysis::{analyze_full, ANALYSIS_CONFIG_V1};
use vice_evidence::boundary::{observe_boundaries, BOUNDARY_CONFIG_V1};
use vice_evidence::corridor::CORRIDOR_CONFIG_V1;
use vice_fit::{span_candidates, FitRefusal, FIT_BUDGET_V1};
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
    pub candidates: u64,
    /// Chains the candidate stage refused, by refusal name.
    pub refusals: Vec<(&'static str, u64)>,
    /// Family names that produced at least one candidate anywhere in the run,
    /// MEASURED. An absent family is an absent name rather than silence.
    pub families_present: Vec<String>,
    /// Largest `max_normal_departure_deg` over every chain of the run, and the
    /// deviation at which it occurred.
    pub max_normal_departure_deg: f64,
    pub departure_at_deviation_px: f64,
    /// Smallest headroom any chain left against the hard cap.
    pub min_budget_headroom: usize,
    /// Longest chain seen, in samples.
    pub longest_chain_samples: usize,
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

fn refusal_name(r: &FitRefusal) -> &'static str {
    match r {
        FitRefusal::ChainTooShort { .. } => "chain_too_short",
        FitRefusal::BudgetExceeded { .. } => "budget_exceeded",
        FitRefusal::NonPositiveHalfwidth { .. } => "non_positive_halfwidth",
        FitRefusal::NonFiniteSample { .. } => "non_finite_sample",
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
                    run.chains += 1;
                    run.longest_chain_samples = run.longest_chain_samples.max(chain.samples.len());
                    match span_candidates(chain, &FIT_BUDGET_V1) {
                        Ok(c) => {
                            run.chain_samples += c.chain_samples as u64;
                            run.supports += c.supports as u64;
                            run.candidates += c.candidates.len() as u64;
                            if c.max_normal_departure_deg > run.max_normal_departure_deg {
                                run.max_normal_departure_deg = c.max_normal_departure_deg;
                                run.departure_at_deviation_px = c.departure_at_deviation_px;
                            }
                            run.min_budget_headroom =
                                run.min_budget_headroom.min(c.budget_headroom);
                            for f in c.families_present {
                                families.insert(f.to_string());
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
    run.refusals.sort_unstable();
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

        println!("--- §28 M6 bullets 1 and 2, over the corpus, 1 cell per scene ---");
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
            "supports                    {} ({:.3} per sample)",
            run.supports,
            run.supports_per_sample()
        );
        println!("candidates                  {}", run.candidates);
        println!("families present            {:?}", run.families_present);
        println!("refusals                    {:?}", run.refusals);
        println!(
            "max normal departure        {:.3} deg, at a deviation of {:.5} px",
            run.max_normal_departure_deg, run.departure_at_deviation_px
        );
        println!("min budget headroom         {}", run.min_budget_headroom);

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
            run.supports_per_sample() <= vice_fit::SUPPORTS_PER_SAMPLE_BOUND as f64,
            "the corpus produced {:.3} supports per sample, over the declared bound",
            run.supports_per_sample()
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
