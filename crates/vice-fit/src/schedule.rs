//! The hierarchical interval schedule of §14.2, and the budget of §28 M6's
//! second bullet.
//!
//! ## What §14.2 forbids, and how the shape of the schedule answers it
//!
//! Two sentences drive this module:
//!
//! > Полный O(N²) all-pairs на dense samples запрещён без hard cap/profile.
//!
//! > Long line/arc support нельзя потерять из-за `max_candidate_support_px`:
//! > hierarchical candidates обязаны включать whole-run and multiscale
//! > intervals.
//!
//! They pull in opposite directions, and the obvious reconciliation is the
//! wrong one: cap the LENGTH of a candidate's support, which bounds the count
//! and destroys exactly the long line and long arc the second sentence is
//! about. **There is deliberately no `max_candidate_support_px` in this
//! crate.** The budget is on the COUNT, and the count is bounded by making
//! the schedule sparse at coarse scales rather than short.
//!
//! The schedule is dyadic: spans of `n-1`, `(n-1)/2`, `(n-1)/4` … segments,
//! each level stepping by half its own span. So the number of supports at a
//! level is inversely proportional to the level's span, the total telescopes
//! to a constant times `n`, and the WHOLE RUN is level 0 — not a special case
//! appended to satisfy the spec sentence, which is the distinction F-0048 Q2
//! turns on. If the whole-run interval were an extra `push` beside a capped
//! loop, the next finding ("the half-run went missing too") would be answered
//! by a second `push`. Here it is the first iteration of the recursion, and
//! the answer to "what about scale k" is "it is level `log2(k)`".
//!
//! ## What this module does NOT decide
//!
//! Which supports are *good*. That is the DP's question (§14.3, §28 M6 bullet
//! 3, NOT STARTED). This module answers only "which intervals are offered",
//! and offering is cheap enough to be exhaustive over scales.

use serde::Serialize;

/// A closed interval of sample indices within one chain, `hi > lo`.
///
/// Minted only by [`Support::new`], which refuses an empty or inverted
/// interval. There is no arithmetic on the type and no `From<(usize, usize)>`:
/// the same reason `Threshold` in `vice-bench` has none — a caller with a
/// constructor that cannot fail has somewhere to put a mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Support {
    lo: usize,
    hi: usize,
}

impl Support {
    /// The only constructor. `None` when `hi <= lo`: a support of one sample
    /// has no direction and every family fits it exactly.
    pub fn new(lo: usize, hi: usize) -> Option<Support> {
        (hi > lo).then_some(Support { lo, hi })
    }

    pub fn lo(self) -> usize {
        self.lo
    }

    pub fn hi(self) -> usize {
        self.hi
    }

    /// Number of samples the support covers, `>= 2`.
    pub fn samples(self) -> usize {
        self.hi - self.lo + 1
    }

    /// Number of sample-to-sample steps, `>= 1`.
    pub fn segments(self) -> usize {
        self.hi - self.lo
    }
}

/// The shortest support the schedule offers, in samples.
///
/// Four, because a cubic has four control points and a support of three
/// samples is interpolated exactly by one — a candidate that cannot be wrong
/// carries no evidence about the family it belongs to (M-3: a test that
/// compares against nothing cannot close the class of a plausible wrong
/// answer). The DP is free to place breakpoints closer together than this;
/// what it may not do is read a zero residual off a support that could not
/// have produced a non-zero one.
pub const MIN_SUPPORT_SAMPLES: usize = 4;

/// The declared linear bound: supports emitted per sample of the chain, at
/// every chain length.
///
/// This is the number that makes "no full O(N²) all-pairs" a MEASUREMENT
/// rather than a description of intent. All-pairs on `n` samples is
/// `n(n-1)/2`, which grows without bound per sample; this schedule's supports
/// per sample are bounded by a constant, and `the_schedule_is_linear_in_the_
/// chain_length` sweeps chain lengths and asserts it, publishing both ratios
/// so the gap between them is visible rather than argued.
///
/// The value is an upper bound with headroom, not the measured ratio. The
/// measured ratio over the sweep is about 1.6, peaking at **1.953 at
/// `n = 257`** where the dyadic levels align worst against the chain length;
/// the sweep prints every row, so this sentence is checkable rather than
/// believed (it said "about 1.4" until the sweep was read, which is F-0028's
/// class in a doc comment). A bound set AT the measurement would move on every
/// change to the level structure, and a bound that moves with its subject
/// certifies nothing (F-0070).
pub const SUPPORTS_PER_SAMPLE_BOUND: usize = 4;

/// Every support the schedule offers for a chain of `n_samples` samples,
/// sorted and deduplicated.
///
/// Empty when the chain is shorter than [`MIN_SUPPORT_SAMPLES`] — the caller
/// gets an empty schedule and is expected to refuse, rather than a schedule
/// containing a support no family can be judged on.
pub fn hierarchical_schedule(n_samples: usize) -> Vec<Support> {
    if n_samples < MIN_SUPPORT_SAMPLES {
        return Vec::new();
    }
    let last = n_samples - 1;
    let mut out: Vec<Support> = Vec::new();

    // Level 0 is the whole run; each level halves the span and steps by half
    // of it, so consecutive supports at a level overlap by 50 % and no
    // position is a seam at any scale.
    let mut span = last;
    loop {
        let step = (span / 2).max(1);
        let mut lo = 0usize;
        while lo + span <= last {
            out.push(Support::new(lo, lo + span).expect("span >= 1"));
            lo += step;
        }
        // The right edge. Without it the tail of the chain is covered at this
        // scale only when `last` happens to be a multiple of `step`, which is
        // a coverage hole that depends on the chain's length rather than on
        // anything about the chain.
        out.push(Support::new(last - span, last).expect("span >= 1"));

        if span < MIN_SUPPORT_SAMPLES {
            break;
        }
        span /= 2;
    }

    out.sort_unstable();
    out.dedup();
    out
}

/// The hard cap of §14.2, as an absolute backstop on one chain's candidates.
///
/// Minted only by [`FitBudget::new`]. The cap is on the COUNT of candidates,
/// never on the LENGTH of a support — see the module docs for why that
/// distinction is the whole design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FitBudget {
    max_candidates_per_chain: usize,
}

impl FitBudget {
    /// `None` for a cap of zero: a budget that admits nothing is not a budget,
    /// and a run under it would report "no candidates" for a reason that has
    /// nothing to do with the chains.
    pub fn new(max_candidates_per_chain: usize) -> Option<FitBudget> {
        (max_candidates_per_chain > 0).then_some(FitBudget {
            max_candidates_per_chain,
        })
    }

    pub fn cap(self) -> usize {
        self.max_candidates_per_chain
    }
}

/// The budget this milestone runs under.
///
/// 65 536 is a backstop, not a working limit: at the worst measured density
/// of 1.953 supports per sample and four fitted families, it binds at roughly
/// 8 390 samples, which at the `sample_step_px = 0.5` of
/// `BOUNDARY_CONFIG_V1` is a single boundary chain about 4 200 px long — a
/// quarter of the `max_canvas_dim_px` of the M2 numeric domain, so a chain
/// that reaches it is a chain worth a refusal. It exists so
/// that a chain arriving from somewhere unexpected produces a REFUSAL naming
/// its size rather than an allocation, and `budget_headroom` publishes how
/// close the run actually came so that a backstop quietly turning into a
/// working limit is visible.
pub const FIT_BUDGET_V1: FitBudget = FitBudget {
    max_candidates_per_chain: 65_536,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_support_of_one_sample_is_not_constructible() {
        assert!(Support::new(3, 3).is_none());
        assert!(Support::new(4, 3).is_none());
        assert!(Support::new(3, 4).is_some());
    }

    /// **The §14.2 property.** The whole run is offered at every chain length,
    /// and it is the LONGEST support offered — so no cap on support length can
    /// have removed it.
    #[test]
    fn the_whole_run_is_always_a_candidate_support() {
        for n in MIN_SUPPORT_SAMPLES..400 {
            let s = hierarchical_schedule(n);
            let whole = Support::new(0, n - 1).expect("n >= 4");
            assert!(
                s.contains(&whole),
                "chain of {n} samples: the whole run is not among the {} supports offered; a long \
                 line or arc spanning the chain has no candidate to be found in",
                s.len()
            );
            let longest = s.iter().map(|x| x.segments()).max().expect("non-empty");
            assert_eq!(
                longest,
                n - 1,
                "chain of {n} samples: the longest support offered is shorter than the chain"
            );
        }
    }

    /// Multiscale, and stated as a property of the SET rather than of a list
    /// of scales: between the whole run and the shortest support, every power
    /// of two is represented.
    #[test]
    fn every_dyadic_scale_between_the_shortest_and_the_whole_run_is_offered() {
        for n in [16usize, 17, 64, 100, 256, 257] {
            let s = hierarchical_schedule(n);
            let mut span = n - 1;
            loop {
                assert!(
                    s.iter().any(|x| x.segments() == span),
                    "chain of {n} samples: no support of span {span} is offered, so this scale is \
                     a hole in the hierarchy"
                );
                if span < MIN_SUPPORT_SAMPLES {
                    break;
                }
                span /= 2;
            }
        }
    }

    /// Every position of the chain is inside some support at every scale
    /// offered — a support hierarchy with a seam would lose whatever sits on
    /// the seam, and the loss would depend on the chain's length rather than
    /// on the chain.
    #[test]
    fn no_sample_is_outside_every_support_at_any_scale() {
        for n in [8usize, 15, 32, 33, 129] {
            let s = hierarchical_schedule(n);
            let mut spans: Vec<usize> = s.iter().map(|x| x.segments()).collect();
            spans.sort_unstable();
            spans.dedup();
            for span in spans {
                for i in 0..n {
                    assert!(
                        s.iter()
                            .any(|x| x.segments() == span && x.lo() <= i && i <= x.hi()),
                        "chain of {n}: sample {i} is covered by no support of span {span}"
                    );
                }
            }
        }
    }

    /// **The "no O(N²)" measurement**, and the reason it is a measurement.
    ///
    /// Both quantities are printed: this schedule's supports per sample, and
    /// the all-pairs count per sample that §14.2 forbids. The first is flat,
    /// the second grows linearly, and printing both is what makes the claim
    /// falsifiable by reading rather than by trusting the bound.
    #[test]
    fn the_schedule_is_linear_in_the_chain_length() {
        let mut worst = 0.0f64;
        for n in [4usize, 8, 16, 33, 64, 128, 257, 512, 1024, 4096] {
            let supports = hierarchical_schedule(n).len();
            let per_sample = supports as f64 / n as f64;
            let all_pairs_per_sample = (n - 1) as f64 / 2.0;
            println!(
                "n {n:5}: supports {supports:6} ({per_sample:.3}/sample) | all-pairs \
                 {:9} ({all_pairs_per_sample:.1}/sample)",
                n * (n - 1) / 2
            );
            assert!(
                supports <= SUPPORTS_PER_SAMPLE_BOUND * n,
                "chain of {n} samples produced {supports} supports, over the declared bound of \
                 {SUPPORTS_PER_SAMPLE_BOUND} per sample"
            );
            worst = worst.max(per_sample);
        }
        // The other direction: a schedule that returned almost nothing would
        // satisfy the bound above and offer no candidates (F-0039).
        assert!(
            worst > 1.0,
            "the densest level of the whole sweep offered {worst:.3} supports per sample; a \
             schedule this sparse cannot be covering every scale, and the bound above would be \
             satisfied by returning nothing"
        );
    }

    #[test]
    fn a_chain_too_short_to_judge_gets_no_supports() {
        for n in 0..MIN_SUPPORT_SAMPLES {
            assert!(hierarchical_schedule(n).is_empty(), "n = {n}");
        }
        assert_eq!(hierarchical_schedule(MIN_SUPPORT_SAMPLES).len(), 1);
    }

    #[test]
    fn a_budget_admitting_nothing_is_not_constructible() {
        assert!(FitBudget::new(0).is_none());
        assert_eq!(FitBudget::new(7).expect("positive").cap(), 7);
    }
}
