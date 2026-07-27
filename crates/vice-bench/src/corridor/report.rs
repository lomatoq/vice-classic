//! The M4 corridor artifact and its gate rows (spec §13.1, §28 M4).
//!
//! Three of the four §28 M4 gate clauses are computed here from this
//! report's own data — *"corridor coverage+sharpness on held-out
//! rasterizer"*, *"transparent exterior correct"* and *"semi-transparent
//! interiors rejected"*. The fourth, *"formation factorial updated"*, is a
//! property of the oracle artifact and lives there.
//!
//! Every row is a conjunction of the property AND a control that fails, for
//! the reason `oracle::report` gives at length: a coverage row is trivially
//! green on an empty sample set, a "transparent exterior correct" row is
//! trivially green if no arm ever produced an exterior, and a "rejected" row
//! is trivially green if everything is rejected. So the coverage row carries
//! a DISPLACEMENT control — the same samples scored against a truth moved
//! one pixel — the exterior row requires both exterior models to occur, and
//! the rejection row requires that the clean corpus is NOT rejected.

use serde::Serialize;

use super::{ArmKey, ArmRow, CorridorRun, RefusedArm, ScoredSample, SemiTransparentProbe};
use crate::gt::corpus::Platform;
use vice_evidence::corridor::COVERAGE_LEVELS;

/// v2 adds the over-opaque-layer population (REVIEW_M4 M4-N5). The shape of
/// an artifact changed, so its schema does: a reader that pinned v1 must not
/// silently accept a v2 document, even inside a milestone whose artifacts
/// nothing outside this tree consumes yet.
pub const CORRIDOR_REPORT_SCHEMA: &str = "vice-classic/m4-corridor-report/v2";

/// Provisional clean-AA targets of §13.1. NOT frozen gates: §13.1 states
/// them as provisional, `configs/GATES_V1.toml` `[boundary_accuracy]` is a
/// PLACEHOLDER owned by M7, and inventing a threshold here is what §27.7 and
/// F-0010 forbid. They are compared against and reported, and a miss is a
/// finding rather than a reason to widen anything.
pub const TARGET_COVERAGE_AT_95: f64 = 0.95;
pub const TARGET_MEDIAN_HALFWIDTH_PX: f64 = 0.35;
pub const TARGET_P95_HALFWIDTH_PX: f64 = 0.75;

/// The margin, in px, by which the control widens each sample's own error.
/// One pixel is far outside any corridor a clean AA edge can justify, so a
/// corridor that still covers the sample would mean it is measuring nothing.
pub const CONTROL_MARGIN_PX: f64 = 1.0;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoverageSummary {
    pub samples: u64,
    pub ds_px: f64,
    /// `(level, empirical ds-weighted coverage)` for each level of §13.1.
    pub coverage: Vec<(f64, f64)>,
    pub median_halfwidth_px: f64,
    pub p95_halfwidth_px: f64,
    pub median_distance_px: f64,
    pub p95_distance_px: f64,
    pub max_distance_px: f64,
    /// Mean signed displacement along the normal: positive means the
    /// extracted boundary sits on the foreground side of the truth.
    pub bias_px: f64,
    pub mean_corr_length_px: f64,
    pub capped_fraction_at_95: f64,
    /// The control: the ds-weighted fraction of samples whose corridor is
    /// still wider than their own distance PLUS [`CONTROL_MARGIN_PX`]. A
    /// corridor that survives that is not measuring anything.
    ///
    /// REVIEW_M4 M4-N6: this was called `coverage_under_displacement` and the
    /// gate row described it as the same samples scored against a truth
    /// displaced by one pixel, which is not what the line below computes. The
    /// reviewer did that recomputation and got 0.0003 along the sample
    /// normals, so the conclusion held and the description did not. What the
    /// quantity IS is a bound on SHARPNESS - no sample's corridor exceeds its
    /// own error by a pixel - and that is exactly the control worth having,
    /// because it is what catches a corridor inflated until the coverage
    /// clause passes. It is named for what it computes now.
    pub margin_survival: f64,
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    sorted[((sorted.len() as f64 * q) as usize).min(sorted.len() - 1)]
}

fn summarize<'a>(samples: impl Iterator<Item = &'a ScoredSample> + Clone) -> CoverageSummary {
    let mut ds_total = 0.0;
    let mut inside = vec![0.0f64; COVERAGE_LEVELS.len()];
    let mut bias = 0.0;
    let mut corr = 0.0;
    let mut capped = 0u64;
    let mut survives_margin = 0.0;
    let mut n = 0u64;
    let mut hw: Vec<f64> = Vec::new();
    let mut dist: Vec<f64> = Vec::new();
    for s in samples {
        n += 1;
        ds_total += s.weight_ds;
        for (li, acc) in inside.iter_mut().enumerate() {
            if s.distance_px <= s.halfwidth_px[li] {
                *acc += s.weight_ds;
            }
        }
        if s.distance_px + CONTROL_MARGIN_PX <= s.halfwidth_px[2] {
            survives_margin += s.weight_ds;
        }
        if s.capped[2] {
            capped += 1;
        }
        bias += s.bias_px * s.weight_ds;
        corr += s.corr_length_px * s.weight_ds;
        hw.push(s.halfwidth_px[2]);
        dist.push(s.distance_px);
    }
    hw.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    dist.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let norm = if ds_total > 0.0 { ds_total } else { 1.0 };
    CoverageSummary {
        samples: n,
        ds_px: ds_total,
        coverage: COVERAGE_LEVELS
            .iter()
            .zip(&inside)
            .map(|(l, i)| (*l, i / norm))
            .collect(),
        median_halfwidth_px: quantile(&hw, 0.5),
        p95_halfwidth_px: quantile(&hw, 0.95),
        median_distance_px: quantile(&dist, 0.5),
        p95_distance_px: quantile(&dist, 0.95),
        max_distance_px: dist.last().copied().unwrap_or(f64::NAN),
        bias_px: bias / norm,
        mean_corr_length_px: corr / norm,
        capped_fraction_at_95: capped as f64 / n.max(1) as f64,
        margin_survival: survives_margin / norm,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bucket {
    pub axis: &'static str,
    pub value: String,
    pub summary: CoverageSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutcomeCount {
    pub outcome: String,
    pub arms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FormationRecovery {
    pub supported_arms: u64,
    pub exterior_correct: u64,
    pub exterior_wrong: u64,
    pub blend_correct_where_identifiable: u64,
    pub blend_arms_identifiable: u64,
    /// Counted only where the filter is IDENTIFIABLE from the coverage
    /// field: on a shape thinner than the kernel every filter ties, and a
    /// tie-break is not a recovery.
    pub filter_correct_where_identifiable: u64,
    pub filter_arms_identifiable: u64,
    pub filter_arms_unidentifiable: u64,
    pub max_alpha_error: f64,
    pub alpha_error_arms: u64,
    pub exterior_models_seen: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SemiTransparentSummary {
    pub probes: u64,
    pub rejected: u64,
    pub delivered_as_two_colour: u64,
    /// Probes on an arm whose interior is RESOLVED, i.e. where scaling the
    /// alpha produces a plateau no opaque geometry can make. These are the
    /// ones §1.6 is decidable on, and the gate row is about them.
    pub probes_observable: u64,
    pub rejected_where_observable: u64,
    pub delivered_as_two_colour_where_observable: u64,
    /// Probes on an unresolved shape: recorded, and NOT counted as either a
    /// success or a failure, because a thinner opaque shape explains the
    /// same bytes.
    pub probes_unobservable: u64,
    pub by_alpha: Vec<(String, u64, u64)>,
    /// Clean corpus arms wrongly rejected as semi-transparent. The control
    /// in the other direction: a rejector that rejects everything is not a
    /// detector.
    pub clean_arms_rejected: u64,
    pub clean_arms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CorridorReport {
    pub schema: &'static str,
    pub milestone: &'static str,
    /// Tier A platform (§5.5, F-0020): every number below is a float derived
    /// from libm, so the artifact carries the platform it belongs to.
    pub platform: Platform,
    pub config: super::CorridorConfigRecord,
    pub config_hash: String,
    pub fixture_set_hash: String,
    pub scenes: u64,
    pub arms_measured: u64,
    pub arms_refused: u64,
    pub sealed_audit_groups_skipped: u64,
    pub outcomes: Vec<OutcomeCount>,
    pub overall: CoverageSummary,
    pub held_out: CoverageSummary,
    pub held_out_profiles: Vec<&'static str>,
    pub buckets: Vec<Bucket>,
    pub step_invariance: Vec<(f64, f64)>,
    pub formation_recovery: FormationRecovery,
    pub semi_transparent: SemiTransparentSummary,
    /// The OTHER §1.6 subclass — constant alpha over an opaque layer — named
    /// and counted rather than left implicit (REVIEW_M4 M4-N5). Not part of
    /// any gate row, and [`super::probes_1_6`] says why at length.
    pub over_opaque_layer: super::probes_1_6::OverOpaqueLayerSummary,
    pub targets: Vec<(String, f64, f64, bool)>,
    pub arms: Vec<ArmRow>,
    pub refused: Vec<RefusedArm>,
    pub probes: Vec<SemiTransparentProbe>,
    pub over_opaque_layer_probes: Vec<super::probes_1_6::OpaqueLayerProbe>,
    pub warnings: Vec<String>,
}

fn bucket_axes(k: &ArmKey) -> Vec<(&'static str, String)> {
    vec![
        ("profile", k.profile.to_string()),
        ("resolution_px", k.size_px.to_string()),
        ("psf", k.psf.clone()),
        ("blend_space", k.blend.to_string()),
        (
            "contrast",
            format!("{:.2}", f64::from(k.contrast_milli) / 1000.0),
        ),
        ("phase", k.phase.clone()),
        ("split", k.split.to_string()),
    ]
}

pub fn build(run: &CorridorRun) -> CorridorReport {
    let overall = summarize(run.samples.iter().map(|(_, s)| s));
    let held_out = summarize(
        run.samples
            .iter()
            .filter(|(k, _)| k.held_out)
            .map(|(_, s)| s),
    );

    let mut axis_values: Vec<(&'static str, String)> = Vec::new();
    for (k, _) in &run.samples {
        for a in bucket_axes(k) {
            if !axis_values.contains(&a) {
                axis_values.push(a);
            }
        }
    }
    axis_values.sort();
    let buckets: Vec<Bucket> = axis_values
        .into_iter()
        .map(|(axis, value)| Bucket {
            axis,
            value: value.clone(),
            summary: summarize(
                run.samples
                    .iter()
                    .filter(|(k, _)| {
                        bucket_axes(k)
                            .into_iter()
                            .any(|(a, v)| a == axis && v == value)
                    })
                    .map(|(_, s)| s),
            ),
        })
        .collect();

    let mut outcomes: Vec<OutcomeCount> = Vec::new();
    for a in &run.arms {
        match outcomes.iter_mut().find(|o| o.outcome == a.outcome) {
            Some(o) => o.arms += 1,
            None => outcomes.push(OutcomeCount {
                outcome: a.outcome.clone(),
                arms: 1,
            }),
        }
    }
    outcomes.sort_by(|a, b| a.outcome.cmp(&b.outcome));

    let supported: Vec<&ArmRow> = run
        .arms
        .iter()
        .filter(|a| a.exterior_recovered.is_some())
        .collect();
    let mut models: Vec<&'static str> = supported.iter().map(|a| a.exterior_truth).collect();
    models.sort_unstable();
    models.dedup();
    let formation_recovery = FormationRecovery {
        supported_arms: supported.len() as u64,
        exterior_correct: supported
            .iter()
            .filter(|a| a.exterior_recovered == Some(a.exterior_truth))
            .count() as u64,
        exterior_wrong: supported
            .iter()
            .filter(|a| a.exterior_recovered != Some(a.exterior_truth))
            .count() as u64,
        blend_arms_identifiable: supported
            .iter()
            .filter(|a| a.blend_identifiable == Some(true))
            .count() as u64,
        blend_correct_where_identifiable: supported
            .iter()
            .filter(|a| {
                a.blend_identifiable == Some(true) && a.blend_recovered == Some(a.blend_truth)
            })
            .count() as u64,
        filter_arms_identifiable: supported
            .iter()
            .filter(|a| a.filter_identifiable == Some(true))
            .count() as u64,
        filter_arms_unidentifiable: supported
            .iter()
            .filter(|a| a.filter_identifiable == Some(false))
            .count() as u64,
        filter_correct_where_identifiable: supported
            .iter()
            .filter(|a| {
                a.filter_identifiable == Some(true)
                    && a.filter_recovered.as_deref() == Some(a.filter_truth.as_str())
            })
            .count() as u64,
        max_alpha_error: supported
            .iter()
            .filter_map(|a| a.max_alpha_error)
            .fold(0.0, f64::max),
        alpha_error_arms: supported
            .iter()
            .filter(|a| a.max_alpha_error.is_some())
            .count() as u64,
        exterior_models_seen: models,
    };

    let mut by_alpha: Vec<(String, u64, u64)> = Vec::new();
    for p in &run.probes {
        let key = format!("{:.2}", p.alpha);
        match by_alpha.iter_mut().find(|(k, _, _)| *k == key) {
            Some(e) => {
                e.1 += 1;
                if p.rejected_as_semi_transparent {
                    e.2 += 1;
                }
            }
            None => by_alpha.push((key, 1, u64::from(p.rejected_as_semi_transparent))),
        }
    }
    by_alpha.sort();
    let obs: Vec<&SemiTransparentProbe> = run.probes.iter().filter(|p| p.observable).collect();
    let semi_transparent = SemiTransparentSummary {
        probes: run.probes.len() as u64,
        probes_observable: obs.len() as u64,
        probes_unobservable: (run.probes.len() - obs.len()) as u64,
        rejected_where_observable: obs
            .iter()
            .filter(|p| p.rejected_as_semi_transparent)
            .count() as u64,
        delivered_as_two_colour_where_observable: obs
            .iter()
            .filter(|p| p.outcome == "supported")
            .count() as u64,
        rejected: run
            .probes
            .iter()
            .filter(|p| p.rejected_as_semi_transparent)
            .count() as u64,
        delivered_as_two_colour: run
            .probes
            .iter()
            .filter(|p| p.outcome == "supported")
            .count() as u64,
        by_alpha,
        clean_arms_rejected: run
            .arms
            .iter()
            .filter(|a| a.outcome == "unsupported/semi_transparent_interior")
            .count() as u64,
        clean_arms: run.arms.len() as u64,
    };

    let over_opaque_layer = super::probes_1_6::summarize(&run.over_opaque_layer);

    let targets = vec![
        (
            "held_out coverage@95 >= target".to_string(),
            held_out
                .coverage
                .iter()
                .find(|(l, _)| (*l - 0.95).abs() < 1e-9)
                .map(|(_, c)| *c)
                .unwrap_or(f64::NAN),
            TARGET_COVERAGE_AT_95,
            held_out
                .coverage
                .iter()
                .any(|(l, c)| (*l - 0.95).abs() < 1e-9 && *c >= TARGET_COVERAGE_AT_95),
        ),
        (
            "held_out median halfwidth <= target".to_string(),
            held_out.median_halfwidth_px,
            TARGET_MEDIAN_HALFWIDTH_PX,
            held_out.median_halfwidth_px <= TARGET_MEDIAN_HALFWIDTH_PX,
        ),
        (
            "held_out p95 halfwidth <= target".to_string(),
            held_out.p95_halfwidth_px,
            TARGET_P95_HALFWIDTH_PX,
            held_out.p95_halfwidth_px <= TARGET_P95_HALFWIDTH_PX,
        ),
    ];

    let mut warnings = Vec::new();
    if run.sealed_audit_groups_skipped == 0 {
        warnings.push(
            "no sealed-audit group was skipped: either the corpus has none or the split policy \
             changed, and a calibration run that touches the audit BURNS it (spec 27.1)"
                .to_string(),
        );
    }
    if held_out.samples == 0 {
        warnings
            .push("no held-out-rasterizer sample: the gate row below would be vacuous".to_string());
    }
    for (name, got, want, ok) in &targets {
        if !ok {
            warnings.push(format!(
                "PROVISIONAL TARGET MISSED: {name} — measured {got:.4} against {want:.4}. Spec \
                 13.1: a wide corridor does not turn a failure into a success, so this is \
                 recorded as a finding, not corrected by widening"
            ));
        }
    }

    CorridorReport {
        schema: CORRIDOR_REPORT_SCHEMA,
        milestone: "M4",
        platform: Platform::current(),
        config: run.config.clone(),
        config_hash: run.config_hash.clone(),
        fixture_set_hash: run.fixture_set_hash.clone(),
        scenes: run.scenes,
        arms_measured: run.arms.len() as u64,
        arms_refused: run.refused.len() as u64,
        sealed_audit_groups_skipped: run.sealed_audit_groups_skipped,
        outcomes,
        overall,
        held_out,
        held_out_profiles: crate::gt::split::SPLIT_POLICY_V1.held_out_profiles.to_vec(),
        buckets,
        step_invariance: run.step_invariance.clone(),
        formation_recovery,
        semi_transparent,
        over_opaque_layer,
        targets,
        arms: run.arms.clone(),
        refused: run.refused.clone(),
        probes: run.probes.clone(),
        over_opaque_layer_probes: run.over_opaque_layer.clone(),
        warnings,
    }
}

impl CorridorReport {
    pub fn canonical_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("corridor report serializes")
    }

    fn coverage_at(&self, s: &CoverageSummary, level: f64) -> f64 {
        s.coverage
            .iter()
            .find(|(l, _)| (*l - level).abs() < 1e-9)
            .map(|(_, c)| *c)
            .unwrap_or(f64::NAN)
    }

    /// Three of the four §28 M4 clauses, as booleans over this report's own
    /// data. Each is a conjunction with a control that can fail.
    pub fn gate_table(&self) -> Vec<(&'static str, bool, String)> {
        // Clause: corridor coverage AND sharpness on the HELD-OUT
        // rasterizer. The control is the displacement: the same samples
        // against a truth moved one pixel must NOT be covered, or the
        // corridor is not measuring the boundary.
        let h = &self.held_out;
        let coverage_ok = self.targets.iter().all(|(_, _, _, ok)| *ok);
        let control_collapses = h.margin_survival < 0.05;
        let non_vacuous = h.samples > 100
            && self
                .arms
                .iter()
                .filter(|a| self.held_out_profiles.contains(&a.profile) && a.samples > 0)
                .map(|a| a.group_id.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                > 2;
        let corridor_row = coverage_ok && control_collapses && non_vacuous;

        // Clause: the transparent exterior is handled correctly. Both
        // exterior models must OCCUR, or "every arm recovered its exterior"
        // is a statement about one model.
        let f = &self.formation_recovery;
        let exterior_row = f.supported_arms > 0
            && f.exterior_wrong == 0
            && f.exterior_models_seen.len() == 2
            && f.alpha_error_arms > 0
            && f.max_alpha_error < 0.02;

        // Clause: semi-transparent interiors are rejected — and the clean
        // corpus is NOT, which is the half that stops "reject everything"
        // from passing.
        let s = &self.semi_transparent;
        let o = &self.over_opaque_layer;
        // §1.6 exactly: such an input is `unsupported` OR stays in a
        // competing model, and what it must NOT do is pass as an ordinary
        // two-colour coverage problem. So the condition is that NO
        // observable probe comes back `supported` — an `ambiguous` outcome
        // satisfies the clause and is counted as such rather than as a
        // failure. Plus three controls: the mechanism must fire, the corpus
        // must contain inputs where the question is undecidable (or
        // "observable" would be doing no work), and the CLEAN corpus must
        // not be rejected (or "reject everything" would pass).
        let semi_row = s.probes_observable > 0
            && s.probes_unobservable > 0
            && s.rejected_where_observable > 0
            && s.delivered_as_two_colour_where_observable == 0
            && s.clean_arms_rejected == 0
            && s.clean_arms > 0;

        vec![
            (
                "corridor coverage and sharpness on the held-out rasterizer",
                corridor_row,
                format!(
                    "held-out {} samples over {:.0} px of boundary: coverage@50/90/95/99 = \
                     {:.3}/{:.3}/{:.3}/{:.3}, median halfwidth {:.3} px, p95 {:.3} px, bias \
                     {:+.4} px; and the sharpness control - the ds-weighted fraction of samples \
                     whose corridor is still wider than their own distance plus {} px - stands \
                     at {:.4}",
                    h.samples,
                    h.ds_px,
                    self.coverage_at(h, 0.50),
                    self.coverage_at(h, 0.90),
                    self.coverage_at(h, 0.95),
                    self.coverage_at(h, 0.99),
                    h.median_halfwidth_px,
                    h.p95_halfwidth_px,
                    h.bias_px,
                    CONTROL_MARGIN_PX,
                    h.margin_survival
                ),
            ),
            (
                "transparent exterior correct",
                exterior_row,
                format!(
                    "{} supported arms, {} with the wrong exterior model; both models occur \
                     ({:?}); max |alpha - true coverage| = {:.5} over {} arms measured against \
                     the exact integrator",
                    f.supported_arms,
                    f.exterior_wrong,
                    f.exterior_models_seen,
                    f.max_alpha_error,
                    f.alpha_error_arms
                ),
            ),
            (
                "semi-transparent interiors rejected",
                semi_row,
                format!(
                    "SCOPE of this row: constant alpha over a TRANSPARENT exterior, on a shape \
                     whose interior is RESOLVED, which is {} of {} probes. There, {} were \
                     rejected under spec 1.6 and {} were delivered as an ordinary two-colour \
                     reading. The other {} probes sit on shapes with no resolved interior, where \
                     a thinner opaque shape explains the same bytes: {} of those were rejected \
                     anyway and {} were delivered as two-colour, and the harness claims neither \
                     as a result. {} of {} clean corpus arms were rejected for this reason. \
                     SEPARATELY, and outside this row, {} probes put the same ink at constant \
                     alpha over an OPAQUE layer, the subclass spec 1.6 names literally: {} came \
                     back as two-colour and {} as something else. That is neither a pass nor a \
                     failure: on the {} probes whose arm has ONE ink, the composite was compared \
                     with the two-colour scene of face beta*F+(1-beta)*B and {} are identical, \
                     {} are within one code and the largest difference is {} code(s), which is \
                     spec 1.5 information loss, measured — and {} of those were rejected under \
                     1.6, i.e. the detector does not fire on inputs that ARE two-colour. The \
                     remaining {} probes sit on multi-ink arms, where the equivalent authoring \
                     has more than two faces and the harness constructs none, so it claims \
                     nothing about them",
                    s.probes_observable,
                    s.probes,
                    s.rejected_where_observable,
                    s.delivered_as_two_colour_where_observable,
                    s.probes_unobservable,
                    s.rejected - s.rejected_where_observable,
                    s.delivered_as_two_colour - s.delivered_as_two_colour_where_observable,
                    s.clean_arms_rejected,
                    s.clean_arms,
                    o.probes,
                    o.delivered_as_two_colour,
                    o.other_outcomes,
                    o.single_ink_probes,
                    o.single_ink_byte_identical,
                    o.single_ink_within_one_code,
                    o.single_ink_max_byte_difference,
                    o.single_ink_rejected_as_semi_transparent,
                    o.multi_ink_probes
                ),
            ),
        ]
    }
}

/// The platform-INDEPENDENT projection: composition, arm identities,
/// outcomes and refusal reasons, and nothing that is a function of a float.
///
/// F-0022 in force: `fixture_set_hash` is a sha256 over SCENE DIGESTS, hence
/// a function of libm, and it does not survive. `config_hash` does — its
/// inputs are the schema, the scope, cell ids, level names and the analysis
/// config, which contain no float that libm produces. The analysis config
/// DOES contain literal floats, but they are constants of this source tree
/// rather than results of a computation, so they are identical on every
/// platform.
pub fn structural_projection(v: &serde_json::Value) -> serde_json::Value {
    let arms: Vec<serde_json::Value> = v["arms"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|a| {
            serde_json::json!({
                "scene_id": a["scene_id"],
                "group_id": a["group_id"],
                "cell_id": a["cell_id"],
                "split": a["split"],
                "profile": a["profile"],
                "outcome": a["outcome"],
                "exterior_truth": a["exterior_truth"],
                "exterior_recovered": a["exterior_recovered"],
                "blend_truth": a["blend_truth"],
                "blend_recovered": a["blend_recovered"],
                "filter_truth": a["filter_truth"],
                "filter_recovered": a["filter_recovered"],
            })
        })
        .collect();
    let probes: Vec<serde_json::Value> = v["probes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|p| {
            serde_json::json!({
                "scene_id": p["scene_id"],
                "cell_id": p["cell_id"],
                "alpha": p["alpha"],
                "outcome": p["outcome"],
                "rejected_as_semi_transparent": p["rejected_as_semi_transparent"],
            })
        })
        .collect();
    // The over-opaque-layer probes travel like their siblings above: which
    // arm was probed, at which alpha, whether its ink is single, and the
    // CATEGORICAL result. `max_byte_difference` stays behind.
    //
    // The line F-0022 draws is by ORIGIN, and it is worth stating where it
    // falls here rather than leaving it to look arbitrary. Everything in this
    // projection descends from corpus render bytes; what it excludes is
    // MEASURED QUANTITIES, because those differ by the last ulp between
    // platforms and a projection full of them would be the cross-platform
    // comparison ADR-0008 §8 forbids. These probes add one such quantity —
    // a byte difference computed through the sRGB transfer function, hence
    // through `powf` — and it is the one field left out.
    let layer: Vec<serde_json::Value> = v["over_opaque_layer_probes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|p| {
            serde_json::json!({
                "scene_id": p["scene_id"],
                "cell_id": p["cell_id"],
                "alpha": p["alpha"],
                "single_ink": p["single_ink"],
                "outcome": p["outcome"],
                "rejected_as_semi_transparent": p["rejected_as_semi_transparent"],
            })
        })
        .collect();
    serde_json::json!({
        "schema": v["schema"],
        "milestone": v["milestone"],
        "config": v["config"],
        "config_hash": v["config_hash"],
        "scenes": v["scenes"],
        "arms_measured": v["arms_measured"],
        "arms_refused": v["arms_refused"],
        "sealed_audit_groups_skipped": v["sealed_audit_groups_skipped"],
        "outcomes": v["outcomes"],
        "held_out_profiles": v["held_out_profiles"],
        "arms": arms,
        "probes": probes,
        "over_opaque_layer_probes": layer,
        "refused": v["refused"],
    })
}
