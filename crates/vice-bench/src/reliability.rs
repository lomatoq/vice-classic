//! Risk–coverage and the statistical court (spec §27.4, §1.5).
//!
//! Three things live here, and each one exists because §27.4 says a number
//! that is only asserted is not a gate.
//!
//! 1. **One-sided Clopper–Pearson upper bound** on the catastrophic risk
//!    among ACCEPTED outputs. Exact, not normal-approximate: the whole
//!    point is the regime of zero or few failures, where the normal
//!    approximation is worthless.
//! 2. **Source-group independence.** Several resolutions and degradations
//!    of one source SVG are correlated and are not independent Bernoulli
//!    trials. The trial unit is the SOURCE GROUP, and the provisional
//!    conservative rule of §27.4 is implemented literally: a group counts
//!    as catastrophic if a catastrophic defect occurred on at least one
//!    accepted mandatory variant.
//! 3. **The sample-size contract**, as a computable check rather than a
//!    sentence. §27.4 states that a one-sided 99% Clopper–Pearson bound
//!    below 1% with zero failures needs at least 459 independent accepted
//!    source groups; [`required_groups_for_zero_failures`] derives that
//!    number from the arithmetic instead of quoting it, and a test asserts
//!    the derivation reproduces 459 — so if the confidence level or the
//!    risk target ever moves, the required n moves with it.
//!
//! No confidence number is produced by M3: there is no vectorizer, so
//! every accepted count here is zero and the framework reports the DEFICIT
//! against the contract. That is the honest output at this milestone, and
//! it is a number rather than a promise.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::gt::raster::RasterProfile;

/// The independent unit of a reliability trial (spec §27.4). Frozen in
/// `configs/GATES_V1.toml` `reliability.unit_of_trial`; named here so the
/// gate file is checked against the implementation.
pub const UNIT_OF_TRIAL: &str = "source_group";

/// The regularized incomplete beta function `I_x(a, b)`, by continued
/// fraction (Lentz). Needed because the Clopper–Pearson bound is a Beta
/// quantile and pulling in a statistics crate for one function would add a
/// dependency whose numerics we would then have to review.
fn betacf(a: f64, b: f64, x: f64) -> f64 {
    const MAXIT: u32 = 300;
    const EPS: f64 = 3e-16;
    const FPMIN: f64 = 1e-300;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < FPMIN {
        d = FPMIN;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=MAXIT {
        let m_f = f64::from(m);
        let m2 = 2.0 * m_f;
        let aa = m_f * (b - m_f) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        h *= d * c;
        let aa = -(a + m_f) * (qab + m_f) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < FPMIN {
            d = FPMIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FPMIN {
            c = FPMIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < EPS {
            break;
        }
    }
    h
}

fn ln_gamma(x: f64) -> f64 {
    // Lanczos approximation, g = 7, n = 9. Accurate to ~1e-15 for x > 0,
    // which is far more than a Beta quantile at these magnitudes needs.
    #[allow(clippy::excessive_precision, clippy::inconsistent_digit_grouping)]
    const C: [f64; 9] = [
        0.99999999999980993,
        676.5203681218851,
        -1259.1392167224028,
        771.32342877765313,
        -176.61502916214059,
        12.507343278686905,
        -0.13857109526572012,
        9.9843695780195716e-6,
        1.5056327351493116e-7,
    ];
    if x < 0.5 {
        // Reflection, so the function is total on the domain we use.
        std::f64::consts::PI.ln() - (std::f64::consts::PI * x).sin().ln() - ln_gamma(1.0 - x)
    } else {
        let x = x - 1.0;
        let mut a = C[0];
        let t = x + 7.5;
        for (i, c) in C.iter().enumerate().skip(1) {
            a += c / (x + i as f64);
        }
        0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
    }
}

/// Regularized incomplete beta `I_x(a, b)`.
pub fn betai(a: f64, b: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let bt = (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (1.0 - x).ln()).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        bt * betacf(a, b, x) / a
    } else {
        1.0 - bt * betacf(b, a, 1.0 - x) / b
    }
}

/// One-sided upper Clopper–Pearson bound on a Bernoulli rate.
///
/// `P(X <= failures) = 1 - confidence` solved for p, i.e. the Beta
/// quantile `Beta(failures + 1, n - failures)` at `confidence`. Solved by
/// bisection on the exact CDF rather than by a closed form, because the
/// monotone bisection is easy to verify by eye and this is a gate.
pub fn clopper_pearson_upper(failures: u64, n: u64, confidence: f64) -> f64 {
    assert!(
        (0.0..1.0).contains(&confidence),
        "confidence must be in [0,1)"
    );
    if n == 0 {
        // No trials bound nothing. Returning 1.0 rather than 0.0 is the
        // whole discipline: an empty sample is maximal ignorance.
        return 1.0;
    }
    if failures >= n {
        return 1.0;
    }
    let (a, b) = ((failures + 1) as f64, (n - failures) as f64);
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        // I_mid(a, b) is the probability that the rate is BELOW mid.
        if betai(a, b, mid) < confidence {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Smallest `n` whose zero-failure Clopper–Pearson upper bound is below
/// `risk_target` at `confidence`.
///
/// Derived, not quoted: §27.4's "at least 459" is the answer this function
/// gives for (0.99, 0.01), and a test checks that. If either input ever
/// moves, the requirement moves with it instead of staying a remembered
/// number.
pub fn required_groups_for_zero_failures(confidence: f64, risk_target: f64) -> u64 {
    let mut n = 1u64;
    while n < 1_000_000 {
        if clopper_pearson_upper(0, n, confidence) < risk_target {
            return n;
        }
        n += 1;
    }
    u64::MAX
}

// ---------------------------------------------------------------------------
// Source-group independence
// ---------------------------------------------------------------------------

/// One scored render. M3 produces none of these — the vectorizer does not
/// exist — but the aggregation is defined and tested now, because §32
/// rule 2 puts the measurement before the algorithm.
///
/// The type carries the PROFILE rather than an `inverse_crime` flag
/// (REVIEW_M3 M3-N5): the flag used to be a `bool` the caller supplied, so
/// a `RenderOutcome` naming a `vice-render` cell with `inverse_crime:
/// false` was counted into the sample. Exclusion is a property of the
/// profile, so the profile is what the type carries and the flag is
/// derived.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderOutcome {
    pub group_id: String,
    pub cell_id: String,
    /// The rasterizer profile that produced the render.
    pub profile: RasterProfile,
    /// Did the system ACCEPT this input (as opposed to abstaining)?
    pub accepted: bool,
    /// Was an accepted output catastrophic (§27.4 taxonomy)?
    pub catastrophic: bool,
    /// Mandatory variants are the ones the group's verdict is computed
    /// from; optional extras may be reported but never dilute the count.
    pub mandatory: bool,
}

impl RenderOutcome {
    /// Renders from the inverse-crime profile are diagnostics and are
    /// excluded from the reliability sample entirely (§27.1). Derived, so
    /// no caller can assert otherwise.
    pub fn inverse_crime(&self) -> bool {
        self.profile.is_inverse_crime()
    }
}

/// The verdict for one independent trial unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupVerdict {
    /// At least one mandatory variant accepted, none catastrophic.
    AcceptedClean,
    /// At least one accepted mandatory variant was catastrophic.
    AcceptedCatastrophic,
    /// No mandatory variant was accepted: the group abstained. It counts
    /// against COVERAGE, never against risk.
    Abstained,
}

/// Aggregate renders into per-group verdicts (§27.4 provisional rule).
pub fn group_verdicts(outcomes: &[RenderOutcome]) -> BTreeMap<String, GroupVerdict> {
    let mut accepted: BTreeSet<&str> = BTreeSet::new();
    let mut catastrophic: BTreeSet<&str> = BTreeSet::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for o in outcomes {
        if o.inverse_crime() || !o.mandatory {
            continue;
        }
        seen.insert(&o.group_id);
        if o.accepted {
            accepted.insert(&o.group_id);
            if o.catastrophic {
                catastrophic.insert(&o.group_id);
            }
        }
    }
    seen.into_iter()
        .map(|g| {
            let v = if catastrophic.contains(g) {
                GroupVerdict::AcceptedCatastrophic
            } else if accepted.contains(g) {
                GroupVerdict::AcceptedClean
            } else {
                GroupVerdict::Abstained
            };
            (g.to_string(), v)
        })
        .collect()
}

/// A risk–coverage report for one bucket, in both units §27.4 requires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskCoverage {
    pub bucket: String,
    /// Independent trial units.
    pub groups_total: u64,
    pub groups_accepted: u64,
    pub groups_catastrophic: u64,
    /// Coverage per SOURCE, which is the unit the reliability claim uses.
    pub coverage_per_source: f64,
    /// Coverage per RENDER, published alongside because §27.4 asks for
    /// both and because the two differ exactly when abstention is
    /// concentrated in one kind of degradation.
    pub renders_total: u64,
    pub renders_accepted: u64,
    pub coverage_per_render: f64,
    /// One-sided upper bound on catastrophic risk among accepted groups.
    pub catastrophic_risk_upper: f64,
    pub confidence: f64,
    /// Groups needed for the target at zero failures.
    pub required_groups: u64,
    /// Positive while the sample is too small for the claim.
    pub group_deficit: u64,
    /// True only when the sample size, the bound AND the likelihood
    /// protocol all satisfy the contract. Nothing else may be read as "the
    /// target is met".
    pub contract_met: bool,
    /// Populated whenever the likelihood protocol refused (§17.1). While
    /// this is `Some`, `contract_met` is `false` by construction.
    pub likelihood_refusal: Option<String>,
}

/// Compute the risk–coverage row for one bucket.
///
/// `residual_model` is the declared likelihood model and whether it has
/// been calibrated. It is not optional decoration: §17.1 forbids a
/// production reliability claim under a diagnostic-only residual model, and
/// REVIEW_M3 M3-N6 measured that this function would previously return
/// `contract_met: true` on 459 fabricated groups without the guard ever
/// being consulted. The guard is now inside the computation, so a caller
/// cannot forget to ask - passing `None` means "no model declared", which
/// the guard refuses.
pub fn risk_coverage(
    bucket: &str,
    outcomes: &[RenderOutcome],
    confidence: f64,
    risk_target: f64,
    residual_model: Option<(crate::correlation::ResidualModel, bool)>,
) -> RiskCoverage {
    let verdicts = group_verdicts(outcomes);
    let groups_total = verdicts.len() as u64;
    let groups_accepted = verdicts
        .values()
        .filter(|v| **v != GroupVerdict::Abstained)
        .count() as u64;
    let groups_catastrophic = verdicts
        .values()
        .filter(|v| **v == GroupVerdict::AcceptedCatastrophic)
        .count() as u64;

    let scored: Vec<&RenderOutcome> = outcomes.iter().filter(|o| !o.inverse_crime()).collect();
    let renders_total = scored.len() as u64;
    let renders_accepted = scored.iter().filter(|o| o.accepted).count() as u64;

    let upper = clopper_pearson_upper(groups_catastrophic, groups_accepted, confidence);
    let required = required_groups_for_zero_failures(confidence, risk_target);
    let deficit = required.saturating_sub(groups_accepted);
    let likelihood_refusal = crate::correlation::guard_confidence_claim(
        residual_model.map(|(m, _)| m),
        residual_model.is_some_and(|(_, calibrated)| calibrated),
        IID_OVERCOUNT_MEASURED_ON_THIS_CORPUS,
    )
    .err()
    .map(|e| e.to_string());

    RiskCoverage {
        bucket: bucket.to_string(),
        groups_total,
        groups_accepted,
        groups_catastrophic,
        coverage_per_source: ratio(groups_accepted, groups_total),
        renders_total,
        renders_accepted,
        coverage_per_render: ratio(renders_accepted, renders_total),
        catastrophic_risk_upper: upper,
        confidence,
        required_groups: required,
        group_deficit: deficit,
        contract_met: deficit == 0 && upper < risk_target && likelihood_refusal.is_none(),
        likelihood_refusal,
    }
}

/// The measured iid over-count factor on this corpus, reported by the guard
/// when it refuses (`correlation::tests`, formation-mismatch residual).
pub const IID_OVERCOUNT_MEASURED_ON_THIS_CORPUS: f64 = 9.0;

fn ratio(num: u64, den: u64) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(group: &str, cell: &str, accepted: bool, catastrophic: bool) -> RenderOutcome {
        RenderOutcome {
            group_id: group.to_string(),
            cell_id: cell.to_string(),
            profile: RasterProfile::ExactClip,
            accepted,
            catastrophic,
            mandatory: true,
        }
    }

    /// Against values a reader can check by hand or against any textbook.
    #[test]
    fn clopper_pearson_matches_known_values() {
        // Reference values computed independently from the exact binomial
        // CDF (not from this implementation): the zero-failure case has the
        // closed form 1 - (1 - conf)^(1/n), and the one-failure case was
        // solved by bisection on sum_{i<=1} C(n,i) p^i (1-p)^(n-i).
        assert!((clopper_pearson_upper(0, 100, 0.95) - 0.029_513_049_6).abs() < 1e-9);
        assert!((clopper_pearson_upper(0, 300, 0.95) - 0.009_936_081_9).abs() < 1e-9);
        assert!((clopper_pearson_upper(1, 100, 0.95) - 0.046_559_811_5).abs() < 1e-9);
        // The zero-failure closed form must hold for every n we care about,
        // which checks the Beta path against algebra rather than against a
        // remembered constant.
        for n in [1u64, 7, 50, 459, 5000] {
            for conf in [0.90f64, 0.95, 0.99] {
                let closed = 1.0 - (1.0 - conf).powf(1.0 / n as f64);
                let got = clopper_pearson_upper(0, n, conf);
                assert!(
                    (got - closed).abs() < 1e-9,
                    "n={n} conf={conf}: {got} vs closed form {closed}"
                );
            }
        }
        // Monotone in every argument, which is what makes it a bound.
        assert!(clopper_pearson_upper(0, 200, 0.99) < clopper_pearson_upper(0, 100, 0.99));
        assert!(clopper_pearson_upper(1, 100, 0.99) > clopper_pearson_upper(0, 100, 0.99));
        assert!(clopper_pearson_upper(0, 100, 0.99) > clopper_pearson_upper(0, 100, 0.95));
    }

    /// An empty sample bounds nothing. Returning 0 here would let a system
    /// that answered nothing claim perfect reliability.
    #[test]
    fn an_empty_sample_gives_the_maximal_bound() {
        assert_eq!(clopper_pearson_upper(0, 0, 0.99), 1.0);
        assert_eq!(clopper_pearson_upper(5, 5, 0.99), 1.0);
    }

    /// §27.4's "at least 459" is DERIVED here, not quoted. If the
    /// confidence level or the risk target moves, so does the requirement.
    #[test]
    fn the_sample_size_contract_reproduces_the_spec_number() {
        let n = required_groups_for_zero_failures(0.99, 0.01);
        assert_eq!(n, 459, "spec §27.4 states 459 for 99% / <1%");
        assert!(clopper_pearson_upper(0, 459, 0.99) < 0.01);
        assert!(clopper_pearson_upper(0, 458, 0.99) >= 0.01);
        // And it really is a function of its inputs.
        assert!(required_groups_for_zero_failures(0.95, 0.01) < n);
        assert!(required_groups_for_zero_failures(0.99, 0.005) > n);
    }

    /// The rule of §27.4, literally: one catastrophic accepted variant
    /// condemns the whole group, and correlated renders cannot inflate n.
    #[test]
    fn one_catastrophic_variant_condemns_the_group_and_renders_do_not_inflate_n() {
        let outcomes = vec![
            r("g1", "s64", true, false),
            r("g1", "s128", true, true),
            r("g1", "s256", true, false),
            r("g2", "s64", true, false),
            r("g2", "s128", true, false),
            r("g3", "s64", false, false),
            r("g3", "s128", false, false),
        ];
        let v = group_verdicts(&outcomes);
        assert_eq!(v["g1"], GroupVerdict::AcceptedCatastrophic);
        assert_eq!(v["g2"], GroupVerdict::AcceptedClean);
        assert_eq!(v["g3"], GroupVerdict::Abstained);

        let rc = risk_coverage("test", &outcomes, 0.99, 0.01, None);
        assert_eq!(rc.groups_total, 3, "seven renders are three trials");
        assert_eq!(rc.groups_accepted, 2);
        assert_eq!(rc.groups_catastrophic, 1);
        // Coverage in both units, and they differ: per-source 2/3, per
        // render 5/7.
        assert!((rc.coverage_per_source - 2.0 / 3.0).abs() < 1e-12);
        assert!((rc.coverage_per_render - 5.0 / 7.0).abs() < 1e-12);
    }

    /// Adding phase shifts of one group must not change the trial count -
    /// the specific inflation §27.4 forbids.
    #[test]
    fn adding_correlated_renders_does_not_change_the_sample_size() {
        let base: Vec<RenderOutcome> = (0..3)
            .map(|i| r(&format!("g{i}"), "s64", true, false))
            .collect();
        let mut inflated = base.clone();
        for i in 0..3 {
            for k in 0..50 {
                inflated.push(r(&format!("g{i}"), &format!("phase{k}"), true, false));
            }
        }
        let a = risk_coverage("b", &base, 0.99, 0.01, None);
        let b = risk_coverage("b", &inflated, 0.99, 0.01, None);
        assert_eq!(a.groups_accepted, b.groups_accepted, "3 groups either way");
        assert_eq!(
            a.catastrophic_risk_upper, b.catastrophic_risk_upper,
            "153 renders must not buy a tighter bound than 3 groups"
        );
        assert!(b.coverage_per_render > 0.0);
    }

    /// Inverse-crime renders are excluded from the sample entirely.
    #[test]
    fn inverse_crime_renders_are_not_part_of_the_reliability_sample() {
        let mut outcomes = vec![r("g1", "s64", true, false)];
        outcomes.push(RenderOutcome {
            group_id: "g2".into(),
            cell_id: "s64_vice".into(),
            profile: RasterProfile::ViceRender,
            accepted: true,
            catastrophic: false,
            mandatory: true,
        });
        let rc = risk_coverage("b", &outcomes, 0.99, 0.01, None);
        assert_eq!(rc.groups_total, 1, "the inverse-crime group is not a trial");
        assert_eq!(rc.renders_total, 1);
        // The old hole: a caller claiming otherwise about a vice-render
        // cell. It is now unsayable - the flag is a function of the profile.
        assert!(outcomes[1].inverse_crime());
        assert!(!outcomes[0].inverse_crime());
    }

    /// The guard is INSIDE the computation, so a caller cannot forget it.
    ///
    /// REVIEW_M3 M3-N6 measured the previous hole: 459 fabricated accepted
    /// groups returned `contract_met: true` while `guard_confidence_claim`
    /// was never consulted. The statistical bound of §27.4 is still
    /// computed and reported - that is what §27.4 asks for - but "the
    /// contract is met" now requires the §17.1 protocol as well.
    #[test]
    fn the_contract_cannot_be_met_without_a_calibrated_likelihood_protocol() {
        use crate::correlation::ResidualModel;
        let outcomes: Vec<RenderOutcome> = (0..459)
            .map(|i| r(&format!("g{i}"), "s128", true, false))
            .collect();

        // Enough groups, zero failures - and still not met, because no
        // residual model is declared.
        let none = risk_coverage("b", &outcomes, 0.99, 0.01, None);
        assert_eq!(none.groups_accepted, 459);
        assert!(
            none.catastrophic_risk_upper < 0.01,
            "the BOUND is still computed"
        );
        assert_eq!(none.group_deficit, 0);
        assert!(
            !none.contract_met,
            "no declared model: the contract is not met"
        );
        assert!(none.likelihood_refusal.is_some());

        // A diagnostic-only model does not rescue it, even called calibrated.
        let iid = risk_coverage(
            "b",
            &outcomes,
            0.99,
            0.01,
            Some((ResidualModel::IidPixel, true)),
        );
        assert!(!iid.contract_met);
        assert!(iid.likelihood_refusal.unwrap().contains("iid_pixel"));

        // An admissible but uncalibrated model does not either.
        let uncal = risk_coverage(
            "b",
            &outcomes,
            0.99,
            0.01,
            Some((ResidualModel::Block, false)),
        );
        assert!(!uncal.contract_met);

        // Only the full set of conditions does - so the test is not
        // vacuously false for every input.
        let ok = risk_coverage(
            "b",
            &outcomes,
            0.99,
            0.01,
            Some((ResidualModel::Block, true)),
        );
        assert!(ok.contract_met);
        assert!(ok.likelihood_refusal.is_none());

        // And the sample-size clause still binds under a good model.
        let few: Vec<RenderOutcome> = (0..10)
            .map(|i| r(&format!("g{i}"), "s128", true, false))
            .collect();
        assert!(
            !risk_coverage("b", &few, 0.99, 0.01, Some((ResidualModel::Block, true))).contract_met
        );
    }

    /// The honest M3 output: zero accepted groups, a maximal bound, and a
    /// deficit that names exactly how far the corpus is from the contract.
    #[test]
    fn with_no_vectorizer_the_contract_reports_a_deficit_not_a_pass() {
        let outcomes: Vec<RenderOutcome> = (0..80)
            .map(|i| r(&format!("g{i}"), "s128", false, false))
            .collect();
        let rc = risk_coverage("flat2-clean-aa-128-512", &outcomes, 0.99, 0.01, None);
        assert_eq!(rc.groups_accepted, 0);
        assert_eq!(rc.catastrophic_risk_upper, 1.0);
        assert_eq!(rc.coverage_per_source, 0.0);
        assert_eq!(rc.required_groups, 459);
        assert_eq!(rc.group_deficit, 459);
        assert!(
            !rc.contract_met,
            "nothing accepted cannot meet the contract"
        );
    }

    /// And the contract is not satisfiable by abstention: accepting only a
    /// handful of easy groups leaves the deficit standing.
    #[test]
    fn the_contract_cannot_be_met_by_abstaining_almost_everywhere() {
        let mut outcomes: Vec<RenderOutcome> = (0..10)
            .map(|i| r(&format!("g{i}"), "s128", true, false))
            .collect();
        outcomes.extend((10..500).map(|i| r(&format!("g{i}"), "s128", false, false)));
        let rc = risk_coverage("b", &outcomes, 0.99, 0.01, None);
        assert_eq!(rc.groups_accepted, 10);
        assert!(!rc.contract_met);
        assert_eq!(rc.group_deficit, 449);
        assert!(rc.coverage_per_source < 0.03, "coverage is published too");
    }
}
