//! The gate rows of §28 M3.5 and the factorial clause of §28 M4.
//!
//! Split out of the report module at the seam the §4.1 size rule asks for,
//! and it is the seam that matters anyway: the report BUILDS the artifact,
//! this file JUDGES it, and the judgement reads only fields the artifact
//! carries.

use super::{derived_warnings, OracleReport};
use crate::gt::raster::RasterProfile;
use crate::oracle::ceiling::CEILING_METRICS;
use crate::oracle::crime::InverseCrime;
use crate::oracle::design::{ArmOutcome, FormationSource};

impl OracleReport {
    /// The §28 M3.5 clauses plus the §28 M4 factorial clause, as booleans
    /// over this report's own data.
    pub fn gate_table(&self) -> Vec<(&'static str, bool, String)> {
        // Clause 1. Every published effect is either a delta whose operands
        // carry the factorial's own key fingerprint, or a typed refusal that
        // names what is missing — AND the machinery demonstrably refuses a
        // mismatch on every key component, demonstrably produces effects
        // when the arms ARE commensurable, and demonstrably DERIVES the key
        // from the measurements (condition B1).
        let effects_well_formed = self.factorial.iter().all(|f| {
            f.effects().iter().all(|o| match o {
                ArmOutcome::Measured(d) => {
                    d.key_fingerprint() == f.key_fingerprint && d.terms().len() == 4
                }
                ArmOutcome::NotYetApplicable(r) => {
                    !r.reason.is_empty() && !r.owner_milestone.is_empty()
                }
            })
        }) && self.geometry_deltas.iter().all(|g| match &g.outcome {
            ArmOutcome::Measured(d) => !d.terms().is_empty(),
            ArmOutcome::NotYetApplicable(r) => !r.reason.is_empty(),
        });
        // `!is_empty()` is not decoration. Without it a report containing NO
        // factorial at all satisfies clause 1 by having nothing to check —
        // `all()` over an empty set is true — which is meta-rule M-2 in the
        // one place this file exists to defend against it.
        let effect_count_intact = !self.factorial.is_empty()
            && self.factorial.len() == self.config.backends.len() * CEILING_METRICS.len()
            && self.factorial.iter().all(|f| f.effects().len() == 3);
        let st = &self.mechanism_selftest;
        let mechanism_live = st.all_key_components_refuse_a_mismatch
            && st.key_is_derived_from_the_measurements
            && st.effects_produced == 3
            && (st.partition_main_effect - st.sequential_pf10_minus_pf00).abs() > 1e-9;

        // Clause 2. The warning exists, is attached to the arms that earn
        // it, survives every aggregation, and is not constant.
        //
        // Both vacuum gaps REVIEW_M3_5 M35-N4 found are closed here:
        // `fold_all(∅) = Clean` let an aggregate with NO arms pass, so the
        // arm count is compared with the number of arms that actually match;
        // and `!warnings.is_empty()` accepted any sentence at all, so the
        // warnings are compared with the set DERIVED from the arms.
        let recomputed_ok = self.ceiling.iter().all(|agg| {
            let mine: Vec<&crate::oracle::ceiling::CeilingArm> = self
                .ceiling_arms
                .iter()
                .filter(|a| {
                    a.arm == agg.arm && a.backend_id == agg.backend_id && a.cell_id == agg.cell_id
                })
                .collect();
            !mine.is_empty()
                && mine.len() as u64 == agg.arms
                && InverseCrime::fold_all(mine.iter().map(|a| &a.inverse_crime))
                    == agg.inverse_crime
        });
        // Re-derived from the two profile NAMES the artifact records, by the
        // same function that produced them.
        let derived_ok = self.ceiling_arms.iter().all(|a| {
            match (
                RasterProfile::from_id(a.backend_rasterizer),
                RasterProfile::from_id(a.observation_profile),
            ) {
                (Some(b), Some(o)) => InverseCrime::of(b, o) == a.inverse_crime,
                _ => false,
            }
        });
        let expected = derived_warnings(
            &self.ceiling_arms,
            &self.ceiling,
            !self.factorial.is_empty(),
        );
        let warnings_are_the_derived_set = self.warnings == expected;
        let warning_visible = self.inverse_crime_arms > 0
            && self.clean_arms > 0
            && !self.warnings.is_empty()
            && warnings_are_the_derived_set
            && recomputed_ok
            && derived_ok
            && st.contaminated_arm_contaminates_the_aggregate
            && st.all_clean_arms_leave_the_aggregate_clean;

        // §28 M4: "formation factorial updated". PF10 — GT partition with an
        // ESTIMATED formation — is measured on real data, PF00/PF01 remain
        // typed refusals owned by M4.5, and all three effects still refuse
        // because a contrast over half a factorial is the order-dependent
        // difference §27.6 abolished.
        let estimated: Vec<&crate::oracle::ceiling::CeilingArm> = self
            .ceiling_arms
            .iter()
            .filter(|a| a.formation_source == FormationSource::Estimated)
            .collect();
        let ground_truth = self
            .ceiling_arms
            .iter()
            .filter(|a| a.formation_source == FormationSource::GroundTruth)
            .count();
        let pf10_measured = self
            .pf_arms
            .iter()
            .find(|a| a.arm == "PF10")
            .is_some_and(|a| a.outcome.measured().is_some());
        let pf01_refused = self
            .pf_arms
            .iter()
            .filter(|a| a.arm == "PF00" || a.arm == "PF01")
            .all(|a| {
                a.outcome.refusal().is_some_and(|r| {
                    r.owner_milestone == "M4.5" && r.missing == vec!["auto_partition"]
                })
            });
        let effects_still_refuse = self
            .factorial
            .iter()
            .all(|f| f.effects().iter().all(|o| o.refusal().is_some()));
        let factorial_row = pf10_measured
            && pf01_refused
            && effects_still_refuse
            && !estimated.is_empty()
            && ground_truth > 0
            && self.factorial_common_fixtures > 0
            && self
                .factorial
                .iter()
                .all(|f| f.present_arms == vec!["PF10".to_string(), "PF11".to_string()]);

        vec![
            (
                "no causal deltas across incompatible runs",
                effects_well_formed && effect_count_intact && mechanism_live,
                format!(
                    "{} factorial instances x 3 effects, each a commensurable contrast or a typed \
                     refusal; the selftest shows all {} key components refuse a mismatch, that an \
                     arm cannot be DERIVED from measurements of two runs ({}), and that four \
                     commensurable arms DO yield 3 effects ({} vs the sequential {})",
                    self.factorial.len(),
                    st.key_components.len(),
                    st.derivation_refusal,
                    st.partition_main_effect,
                    st.sequential_pf10_minus_pf00
                ),
            ),
            (
                "inverse-crime warning visible",
                warning_visible,
                format!(
                    "{} contaminated arms and {} clean ones; {} warning line(s), equal to the set \
                     derived from the arms; every aggregate covers a NON-EMPTY set of arms whose \
                     count it reports, its status equals the fold of exactly those arms, and \
                     every arm status equals the one derived from its two profiles",
                    self.inverse_crime_arms,
                    self.clean_arms,
                    self.warnings.len()
                ),
            ),
            (
                "formation factorial updated (spec 28 M4)",
                factorial_row,
                format!(
                    "PF10 (GT partition + ESTIMATED formation) measured on {} arms against {} ground-truth ones; the estimator recovered the cell own formation on {}; {} arms refused rather than filled with the truth; the factorial runs over the {} (scene, cell) pairs BOTH sources produced and drops {}; PF00/PF01 stay typed refusals owned by M4.5 (auto partition), so all three effects still refuse - half a factorial is the sequential difference 27.6 abolished",
                    estimated.len(),
                    ground_truth,
                    estimated.iter().filter(|a| a.formation_matches_gt).count(),
                    self.arms_refused,
                    self.factorial_common_fixtures,
                    self.factorial_dropped_fixtures
                ),
            ),
        ]
    }
}
