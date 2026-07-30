//! Preregistration (spec §27.1 "bucket boundaries, catastrophic taxonomy
//! and pooling policy preregister до открытия sealed audit", §27.4).
//!
//! What preregistration is FOR: once audit results are visible, every
//! choice about how to slice, pool and classify them can be made to
//! flatter the result without anyone intending it. The only defence is to
//! fix those choices first and hash them, so a later reader can see that
//! the analysis plan predates the numbers.
//!
//! This document is therefore frozen by hash, and the hash is one of the
//! three the sealed-audit burn policy records when the audit is opened
//! (see `gt::split`): changing the analysis plan after opening the audit
//! burns the generation.

use serde::Serialize;

use crate::hashing::sha256_hex;

pub const PREREG_SCHEMA: &str = "vice-classic/gt-preregistration/v1";

/// A reporting bucket. Claims are made per bucket; a claim pooled across
/// buckets requires a preregistered family-wise correction or an honest
/// hierarchical model (§27.4), and post-hoc pooling is forbidden.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bucket {
    pub id: &'static str,
    /// Inclusive render size range.
    pub min_size_px: u32,
    pub max_size_px: u32,
    /// Identifiability classes that belong to this bucket.
    pub identifiability: &'static [&'static str],
    /// Minimum selective coverage, per SOURCE GROUP, below which the
    /// bucket fails regardless of its risk bound — the clause that stops a
    /// system passing by abstaining from almost everything (§1.5).
    pub min_coverage_per_source: f64,
    /// Separate mandatory-render coverage floor.
    pub min_coverage_per_render: f64,
    /// Provisional SLO target of §29 for this bucket, or `None` where the
    /// bucket has no boundary target at all.
    ///
    /// An `Option` rather than `f64::INFINITY` (REVIEW_M3 M3-N14):
    /// serde_json prints every non-finite f64 as `null`, so `+inf`, `-inf`
    /// and `NaN` all hashed identically and `check()` accepted all three -
    /// a hole in the very claim the frozen hash makes. "No target" is now a
    /// distinct value the serializer can see, and `check()` refuses a
    /// non-finite threshold outright.
    pub boundary_p95_px: Option<f64>,
}

/// One catastrophic-failure kind. The list is §27.4's minimum, verbatim in
/// meaning, with the measurable quantity named for each.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CatastrophicKind {
    pub id: &'static str,
    pub what: &'static str,
    pub measured_by: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PoolingPolicy {
    pub unit_of_trial: &'static str,
    pub group_rule: &'static str,
    pub cross_bucket: &'static str,
    pub excluded_from_sample: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Preregistration {
    pub schema: &'static str,
    pub version: &'static str,
    /// Confidence level of the one-sided Clopper-Pearson bound, frozen
    /// before any result is seen (§27.4).
    pub confidence: f64,
    /// Catastrophic-risk target the bound must fall below.
    pub risk_target: f64,
    pub buckets: Vec<Bucket>,
    pub catastrophic_taxonomy: Vec<CatastrophicKind>,
    pub pooling: PoolingPolicy,
    /// What this document deliberately does NOT fix, so a later reader can
    /// see the boundary of the commitment rather than infer it.
    pub not_preregistered: &'static [&'static str],
}

impl Preregistration {
    pub fn v1() -> Preregistration {
        Preregistration {
            schema: PREREG_SCHEMA,
            version: "v1",
            confidence: 0.99,
            risk_target: 0.01,
            buckets: vec![
                Bucket {
                    id: "flat2-clean-aa-identifiable-128-512",
                    min_size_px: 128,
                    max_size_px: 512,
                    identifiability: &["identifiable"],
                    min_coverage_per_source: 0.80,
                    min_coverage_per_render: 0.80,
                    boundary_p95_px: Some(0.35),
                },
                Bucket {
                    id: "flat2-clean-aa-identifiable-64",
                    min_size_px: 64,
                    max_size_px: 64,
                    identifiability: &["identifiable"],
                    min_coverage_per_source: 0.60,
                    min_coverage_per_render: 0.60,
                    boundary_p95_px: Some(0.50),
                },
                Bucket {
                    id: "flat2-small-16-32",
                    min_size_px: 16,
                    max_size_px: 32,
                    identifiability: &["identifiable"],
                    // No coverage floor is preregistered below 64 px: §1.5
                    // fixes targets for 64 and above only, and inventing one
                    // here would be preregistering a number nobody has
                    // justified.
                    min_coverage_per_source: 0.0,
                    min_coverage_per_render: 0.0,
                    boundary_p95_px: Some(1.0),
                },
                Bucket {
                    id: "equivalent-family",
                    min_size_px: 16,
                    max_size_px: 512,
                    identifiability: &["equivalent_family"],
                    min_coverage_per_source: 0.0,
                    min_coverage_per_render: 0.0,
                    boundary_p95_px: Some(0.50),
                },
                Bucket {
                    id: "information-lost",
                    min_size_px: 16,
                    max_size_px: 512,
                    identifiability: &["information_lost"],
                    // Scored on correct ABSTENTION (§29), so a coverage
                    // floor here would reward exactly the wrong behaviour.
                    min_coverage_per_source: 0.0,
                    min_coverage_per_render: 0.0,
                    // Scored on correct ABSTENTION, so there is no boundary
                    // target - stated as absence, not as an infinity the
                    // serializer would flatten to null.
                    boundary_p95_px: None,
                },
            ],
            catastrophic_taxonomy: vec![
                CatastrophicKind {
                    id: "wrong_visible_topology",
                    what: "wrong visible component / hole / fusion / split outside the declared \
                           equivalence class",
                    measured_by: "component, hole and face counts of the delivered partition \
                                  against the fixture's measured PartitionTruth",
                },
                CatastrophicKind {
                    id: "exposed_seam",
                    what: "exposed gap, seam or apron that changes the readable shape",
                    measured_by: "per-pixel alpha of the serialized render against the \
                                  partition-renderer reference",
                },
                CatastrophicKind {
                    id: "accepted_self_intersection",
                    what: "accepted self-intersection or boundary crossing",
                    measured_by: "certified curve intersection predicates on the delivered scene",
                },
                CatastrophicKind {
                    id: "gross_boundary_outlier",
                    what: "boundary error above the frozen p99/max gate",
                    measured_by: "signed distance from delivered boundary to GT boundary, p99 \
                                  and max",
                },
                CatastrophicKind {
                    id: "broken_g1",
                    what: "broken smooth G1 producing a visible kink",
                    measured_by: "tangent discontinuity at nodes declared smooth, against the \
                                  frozen kink threshold",
                },
                CatastrophicKind {
                    id: "wrong_paint_or_missing_face",
                    what: "wrong colour or omitted face above the salient-detail gate",
                    measured_by: "per-face paint distance and face correspondence against the \
                                  fixture's salient features",
                },
                CatastrophicKind {
                    id: "serialized_mismatch",
                    what: "serialized SVG that does not match the judged scene",
                    measured_by: "digest identity between the judged scene and the exported \
                                  bytes, plus a render court on the exported file",
                },
            ],
            pooling: PoolingPolicy {
                unit_of_trial: "source group (a shape family variant); NEVER a render",
                group_rule: "a group is catastrophic if a catastrophic defect occurred on at \
                             least one ACCEPTED mandatory variant; a group with no accepted \
                             mandatory variant is an abstention and counts against coverage only",
                cross_bucket: "claims are per bucket; a combined claim requires a family-wise \
                               correction or a hierarchical model chosen BEFORE the audit is \
                               opened; post-hoc pooling is forbidden",
                excluded_from_sample: &[
                    "renders produced by the inverse-crime profile (vice-render)",
                    "renders whose degradation cell is not realizable by its profile",
                    "optional (non-mandatory) variants",
                ],
            },
            not_preregistered: &[
                "the confidence policy itself (thresholds are calibrated on the calibration \
                 split, which is what that split is for)",
                "any coverage floor below 64 px",
                "the human-court analysis plan (§27.4 human court; there is no court to run \
                 until a system produces output)",
            ],
        }
    }

    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("preregistration serializes")
    }

    pub fn hash(&self) -> String {
        sha256_hex(self.canonical_json().as_bytes())
    }

    /// Structural checks that make the document usable as a commitment.
    pub fn check(&self) -> Result<(), Vec<String>> {
        let mut bad = Vec::new();
        if self.buckets.is_empty() {
            bad.push("no buckets: nothing can be claimed per bucket".to_string());
        }
        if self.catastrophic_taxonomy.len() < 7 {
            bad.push(format!(
                "catastrophic taxonomy has {} kinds; §27.4 lists 7 as the MINIMUM",
                self.catastrophic_taxonomy.len()
            ));
        }
        for k in &self.catastrophic_taxonomy {
            if k.measured_by.trim().is_empty() {
                bad.push(format!(
                    "{}: a kind without a measurement is not a kind",
                    k.id
                ));
            }
        }
        if !(0.0..1.0).contains(&self.confidence) || self.confidence < 0.5 {
            bad.push("confidence must be a level in [0.5, 1)".to_string());
        }
        if !(0.0..1.0).contains(&self.risk_target) || self.risk_target <= 0.0 {
            bad.push("risk target must be in (0, 1)".to_string());
        }
        let mut ids: Vec<&str> = self.buckets.iter().map(|b| b.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        if ids.len() != before {
            bad.push("duplicate bucket ids".to_string());
        }
        for b in &self.buckets {
            if b.min_size_px > b.max_size_px {
                bad.push(format!("{}: inverted size range", b.id));
            }
            if b.identifiability.is_empty() {
                bad.push(format!("{}: no identifiability class", b.id));
            }
            if !(0.0..=1.0).contains(&b.min_coverage_per_source) {
                bad.push(format!("{}: coverage floor outside [0,1]", b.id));
            }
            if !(0.0..=1.0).contains(&b.min_coverage_per_render) {
                bad.push(format!("{}: render coverage floor outside [0,1]", b.id));
            }
            // A non-finite threshold is invisible to the hash (it prints as
            // `null`), so it may not be a threshold at all.
            if let Some(v) = b.boundary_p95_px {
                if !v.is_finite() || v <= 0.0 {
                    bad.push(format!(
                        "{}: boundary target must be finite and positive, or absent",
                        b.id
                    ));
                }
            }
        }
        // An identifiable bucket at or above 64 px without a coverage floor
        // would let total abstention pass, which §1.5 forbids explicitly.
        for b in &self.buckets {
            if b.identifiability == ["identifiable"]
                && b.min_size_px >= 64
                && (b.min_coverage_per_source <= 0.0 || b.min_coverage_per_render <= 0.0)
            {
                bad.push(format!(
                    "{}: an identifiable bucket at >= 64 px must preregister a coverage floor",
                    b.id
                ));
            }
        }
        if bad.is_empty() {
            Ok(())
        } else {
            Err(bad)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_is_structurally_sound() {
        Preregistration::v1().check().expect("V1 must be usable");
    }

    #[test]
    fn the_structural_check_is_not_vacuous() {
        let base = Preregistration::v1();

        let mut p = base.clone();
        p.buckets.clear();
        assert!(p.check().is_err(), "no buckets");

        let mut p = base.clone();
        p.catastrophic_taxonomy.truncate(3);
        assert!(p.check().is_err(), "taxonomy below the §27.4 minimum");

        let mut p = base.clone();
        p.catastrophic_taxonomy[0].measured_by = "";
        assert!(p.check().is_err(), "a kind with no measurement");

        let mut p = base.clone();
        p.confidence = 0.2;
        assert!(p.check().is_err(), "implausible confidence");

        let mut p = base.clone();
        p.buckets[0].min_coverage_per_source = 0.0;
        assert!(
            p.check().is_err(),
            "an identifiable bucket at >= 64 px without a coverage floor would let total \
             abstention pass"
        );

        let mut p = base.clone();
        p.buckets[0].min_coverage_per_render = 0.0;
        assert!(
            p.check().is_err(),
            "an identifiable bucket at >= 64 px needs a render coverage floor"
        );

        let mut p = base.clone();
        let dup = p.buckets[0].clone();
        p.buckets.push(dup);
        assert!(p.check().is_err(), "duplicate bucket ids");

        // A non-finite threshold is refused rather than silently hashed as
        // `null` (REVIEW_M3 M3-N14).
        for bad_value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 0.0, -1.0] {
            let mut p = base.clone();
            p.buckets[0].boundary_p95_px = Some(bad_value);
            assert!(
                p.check().is_err(),
                "boundary target {bad_value} must be refused"
            );
        }
    }

    /// The hash must SEE every threshold. It did not: `+inf`, `-inf` and
    /// `NaN` all serialize as `null` and produced one hash, while `check()`
    /// accepted all three (REVIEW_M3 M3-N14).
    #[test]
    fn the_hash_distinguishes_every_representable_threshold() {
        let base = Preregistration::v1();
        let h = base.hash();
        let mut seen = std::collections::BTreeSet::from([h.clone()]);
        // 0.35 is bucket 0 own value; a sweep must not include it.
        for v in [0.1f64, 0.36, 1e308, 1e-308] {
            let mut p = Preregistration::v1();
            p.buckets[0].boundary_p95_px = Some(v);
            assert!(seen.insert(p.hash()), "value {v} does not move the hash");
        }
        // Absence is its own value, distinct from any number.
        let mut p = Preregistration::v1();
        p.buckets[0].boundary_p95_px = None;
        assert!(seen.insert(p.hash()), "absence must be distinguishable");
        // `null` still appears - but now it means exactly one thing, and
        // the count proves it: one per bucket that declares no target. The
        // hole was non-finite NUMBERS collapsing onto that same token; a
        // deliberate absence sharing it is not a hole, because `check()`
        // refuses every non-finite value, so nothing else can print `null`.
        let nulls = base.canonical_json().matches(":null").count();
        let absent = base
            .buckets
            .iter()
            .filter(|b| b.boundary_p95_px.is_none())
            .count();
        assert_eq!(
            nulls, absent,
            "every `null` in the canonical document must be a declared absence"
        );
        assert!(
            absent >= 1,
            "the corpus has a bucket with no boundary target"
        );
    }

    /// The taxonomy must cover §27.4's minimum list, kind by kind, so a
    /// later edit cannot quietly drop one.
    #[test]
    fn the_taxonomy_covers_the_spec_minimum() {
        let p = Preregistration::v1();
        let ids: Vec<&str> = p.catastrophic_taxonomy.iter().map(|k| k.id).collect();
        for want in [
            "wrong_visible_topology",
            "exposed_seam",
            "accepted_self_intersection",
            "gross_boundary_outlier",
            "broken_g1",
            "wrong_paint_or_missing_face",
            "serialized_mismatch",
        ] {
            assert!(ids.contains(&want), "§27.4 requires {want}");
        }
    }

    /// The hash is the commitment. Freezing it makes "the plan predates the
    /// numbers" checkable rather than assertable.
    #[test]
    fn the_preregistration_hash_is_frozen() {
        let h = Preregistration::v1().hash();
        assert_eq!(
            h, FROZEN_V1_HASH,
            "the analysis plan changed; after the sealed audit is opened this BURNS the audit \
             generation (spec §27.1)"
        );
        let mut other = Preregistration::v1();
        other.risk_target = 0.02;
        assert_ne!(other.hash(), h, "a weaker target must change the hash");
    }

    const FROZEN_V1_HASH: &str = "67223533f3a4ab4d9e413d4b8e32b944523c0fb8111cfdc5201bf17fabce062b";
}
