//! Three-way split and the sealed-audit burn policy (spec §27.1, §32
//! rule 27, §28 M3 gate "no test leakage; sealed-audit burn policy
//! active").
//!
//! Two independent mechanisms, and they answer different questions.
//!
//! **The split** answers "may this fixture be looked at while iterating?".
//! §27.1 requires splits to hold whole source files, shape families and
//! rasterizer profiles — not neighbouring renders of one SVG. So the unit
//! of assignment here is the SHAPE FAMILY, never the group and never the
//! render: two variants of `annulus` are far more similar to each other
//! than to anything else, and putting one in `development` and the other in
//! `sealed_audit` would leak the answer while looking like a clean split.
//! A separate held-out RASTERIZER PROFILE closes the second leak: a system
//! tuned against every AA model it will be judged on has been shown its
//! test.
//!
//! **The burn policy** answers "is this audit still untouched?". §27.1 says
//! any change to algorithm, config or gate AFTER audit results have been
//! seen burns the split. That is unenforceable as a promise, so it is
//! enforced as a record: opening the audit writes down the exact hashes of
//! the corpus, the preregistration and the gates at that moment, and every
//! later run re-checks them. A change after opening is a typed
//! [`BurnViolation`] naming what moved — not an honour system, and not
//! something a later reader has to reconstruct from commit dates.

use serde::{Deserialize, Serialize};

use super::{FixtureOrigin, GtSourceGroup};

/// The three-way split of §27.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Split {
    /// Free for algorithm iteration.
    Development,
    /// Frozen thresholds, noise scales, code tables, confidence policy.
    /// Looked at only to CALIBRATE, never to iterate on the algorithm.
    Calibration,
    /// Not opened until a release candidate. Opening it starts the burn
    /// clock (see [`AuditSeal`]).
    SealedAudit,
}

impl Split {
    pub fn as_str(&self) -> &'static str {
        match self {
            Split::Development => "development",
            Split::Calibration => "calibration",
            Split::SealedAudit => "sealed_audit",
        }
    }
}

/// The frozen split policy.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SplitPolicy {
    pub version: &'static str,
    /// Mixed into the family hash so the assignment is a stable function of
    /// (policy version, family name) and nothing else.
    pub salt: &'static str,
    /// Out of 100.
    pub development_pct: u32,
    pub calibration_pct: u32,
    pub sealed_audit_pct: u32,
    /// Rasterizer profiles that never appear in `development`. A system
    /// tuned on every AA model it will be judged on has seen its test.
    pub held_out_profiles: &'static [&'static str],
}

pub const SPLIT_POLICY_V1: SplitPolicy = SplitPolicy {
    version: "vice-classic/gt-split/v1",
    salt: "gt-split-v1",
    development_pct: 55,
    calibration_pct: 20,
    sealed_audit_pct: 25,
    // tiny-skia is held out: it is the AA model closest to what real
    // exporters produce, so it is the one worth keeping honest.
    held_out_profiles: &["tiny-skia"],
};

fn family_hash(policy: &SplitPolicy, family: &str) -> u64 {
    // FNV-1a over salt + family. Deterministic across platforms and
    // processes; no HashMap iteration, no randomness.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in policy
        .salt
        .as_bytes()
        .iter()
        .chain(b"/")
        .chain(family.as_bytes())
    {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

impl SplitPolicy {
    /// Which split a SHAPE FAMILY belongs to.
    ///
    /// Deliberately a function of the family name alone: adding a group to
    /// a family, or adding an unrelated family, cannot move an existing
    /// family across the split line. An assignment that reshuffled when the
    /// corpus grew would silently move audit fixtures into development.
    pub fn split_of_family(&self, family: &str) -> Split {
        let bucket = (family_hash(self, family) % 100) as u32;
        if bucket < self.development_pct {
            Split::Development
        } else if bucket < self.development_pct + self.calibration_pct {
            Split::Calibration
        } else {
            Split::SealedAudit
        }
    }

    pub fn split_of_group(&self, group: &GtSourceGroup) -> Split {
        self.split_of_family(&group.shape_family)
    }

    /// May a render of this profile appear in this split?
    pub fn profile_allowed(&self, split: Split, profile: &str) -> bool {
        split != Split::Development || !self.held_out_profiles.contains(&profile)
    }
}

// ---------------------------------------------------------------------------
// Sealed-audit burn policy
// ---------------------------------------------------------------------------

pub const AUDIT_SEAL_SCHEMA: &str = "vice-classic/gt-audit-seal/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SealStatus {
    /// Never opened. The only status M3 may record.
    Sealed,
    /// Opened at the hashes recorded below. Any later change to those
    /// hashes burns this generation.
    Opened,
    /// Burned: results were seen and then something changed. A new
    /// generation with untouched fixtures is required (§27.1).
    Burned,
}

/// The audit generation record. Written as an artifact so the state
/// survives outside anyone's memory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditSeal {
    pub schema: String,
    /// Incremented whenever a burned generation is replaced.
    pub generation: u32,
    pub status: SealStatus,
    pub policy_version: String,
    /// Hashes AT THE MOMENT OF OPENING. Empty while sealed.
    #[serde(default)]
    pub corpus_hash: String,
    #[serde(default)]
    pub prereg_hash: String,
    #[serde(default)]
    pub gates_hash: String,
    #[serde(default)]
    pub opened_note: String,
    #[serde(default)]
    pub burn_reason: String,
}

/// What the burn check found.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BurnViolation {
    #[error(
        "sealed-audit generation {generation} was opened at {what} {recorded}, but the current \
         value is {actual}: results were seen and then this changed, so the audit split is \
         BURNED and a new untouched generation is required (spec §27.1)"
    )]
    ChangedAfterOpening {
        generation: u32,
        what: &'static str,
        recorded: String,
        actual: String,
    },
    #[error("sealed-audit generation {generation} is already burned: {reason}")]
    AlreadyBurned { generation: u32, reason: String },
    #[error("sealed-audit record has schema {got:?}, expected {want:?}")]
    WrongSchema { got: String, want: String },
    #[error(
        "the sealed audit is still SEALED: scoring it is what opens it, and opening must be a \
         deliberate, recorded act"
    )]
    StillSealed,
    #[error("sealed-audit record is opened but records no {what} hash")]
    IncompleteRecord { what: &'static str },
}

impl AuditSeal {
    /// A fresh, never-opened generation.
    pub fn sealed(generation: u32) -> AuditSeal {
        AuditSeal {
            schema: AUDIT_SEAL_SCHEMA.to_string(),
            generation,
            status: SealStatus::Sealed,
            policy_version: SPLIT_POLICY_V1.version.to_string(),
            corpus_hash: String::new(),
            prereg_hash: String::new(),
            gates_hash: String::new(),
            opened_note: String::new(),
            burn_reason: String::new(),
        }
    }

    /// Record the deliberate act of opening the audit split.
    pub fn open(
        &self,
        corpus_hash: &str,
        prereg_hash: &str,
        gates_hash: &str,
        note: &str,
    ) -> AuditSeal {
        AuditSeal {
            status: SealStatus::Opened,
            corpus_hash: corpus_hash.to_string(),
            prereg_hash: prereg_hash.to_string(),
            gates_hash: gates_hash.to_string(),
            opened_note: note.to_string(),
            ..self.clone()
        }
    }

    /// The check that makes the policy real.
    ///
    /// Called on every run that touches the audit split, and by CI on every
    /// commit. It cannot be satisfied by intent: it compares the recorded
    /// hashes with the current ones.
    pub fn check(
        &self,
        corpus_hash: &str,
        prereg_hash: &str,
        gates_hash: &str,
    ) -> Result<(), BurnViolation> {
        if self.schema != AUDIT_SEAL_SCHEMA {
            return Err(BurnViolation::WrongSchema {
                got: self.schema.clone(),
                want: AUDIT_SEAL_SCHEMA.to_string(),
            });
        }
        match self.status {
            SealStatus::Burned => Err(BurnViolation::AlreadyBurned {
                generation: self.generation,
                reason: self.burn_reason.clone(),
            }),
            SealStatus::Sealed => Err(BurnViolation::StillSealed),
            SealStatus::Opened => {
                for (what, recorded, actual) in [
                    ("corpus", &self.corpus_hash, corpus_hash),
                    ("preregistration", &self.prereg_hash, prereg_hash),
                    ("gates", &self.gates_hash, gates_hash),
                ] {
                    if recorded.is_empty() {
                        return Err(BurnViolation::IncompleteRecord { what });
                    }
                    if recorded != actual {
                        return Err(BurnViolation::ChangedAfterOpening {
                            generation: self.generation,
                            what,
                            recorded: recorded.clone(),
                            actual: actual.to_string(),
                        });
                    }
                }
                Ok(())
            }
        }
    }

    /// Burn this generation, naming the reason.
    pub fn burn(&self, reason: &str) -> AuditSeal {
        AuditSeal {
            status: SealStatus::Burned,
            burn_reason: reason.to_string(),
            ..self.clone()
        }
    }

    /// The successor generation: sealed again, with untouched fixtures.
    pub fn next_generation(&self) -> AuditSeal {
        AuditSeal::sealed(self.generation + 1)
    }
}

/// Summary of how a corpus lands in the splits, for the manifest and the
/// report.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SplitSummary {
    pub split: &'static str,
    pub families: usize,
    pub groups: usize,
    pub scenes: usize,
    pub by_origin: Vec<(&'static str, usize)>,
}

pub fn summarize(policy: &SplitPolicy, groups: &[GtSourceGroup]) -> Vec<SplitSummary> {
    let mut out = Vec::new();
    for split in [Split::Development, Split::Calibration, Split::SealedAudit] {
        let mine: Vec<&GtSourceGroup> = groups
            .iter()
            .filter(|g| policy.split_of_group(g) == split)
            .collect();
        let mut families: Vec<&str> = mine.iter().map(|g| g.shape_family.as_str()).collect();
        families.sort_unstable();
        families.dedup();
        let by_origin = [
            FixtureOrigin::Procedural,
            FixtureOrigin::Authored,
            FixtureOrigin::Adversarial,
        ]
        .iter()
        .map(|o| (o.as_str(), mine.iter().filter(|g| g.origin == *o).count()))
        .collect();
        out.push(SplitSummary {
            split: split.as_str(),
            families: families.len(),
            groups: mine.len(),
            scenes: mine.iter().map(|g| g.scenes.len()).sum(),
            by_origin,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gt::adversarial::all_adversarial_groups;
    use crate::gt::authored::authored_groups;
    use crate::gt::grammar::procedural_groups;
    use std::collections::{BTreeMap, BTreeSet};

    fn corpus() -> Vec<GtSourceGroup> {
        let mut g = procedural_groups(4);
        g.extend(authored_groups().unwrap());
        g.extend(all_adversarial_groups());
        g
    }

    /// The leak the split exists to prevent: two variants of one family
    /// landing on opposite sides of the line.
    #[test]
    fn a_shape_family_is_never_split_across_two_splits() {
        let groups = corpus();
        let mut seen: BTreeMap<&str, Split> = BTreeMap::new();
        for g in &groups {
            let s = SPLIT_POLICY_V1.split_of_group(g);
            if let Some(prev) = seen.insert(&g.shape_family, s) {
                assert_eq!(
                    prev, s,
                    "family {} lands in two splits: neighbouring variants would leak",
                    g.shape_family
                );
            }
        }
        assert!(seen.len() >= 20, "families: {}", seen.len());
    }

    /// Assignment must not reshuffle when the corpus grows, or adding a
    /// fixture would quietly move audit families into development.
    #[test]
    fn assignment_is_stable_under_corpus_growth() {
        let small = procedural_groups(1);
        let large = corpus();
        for g in &small {
            let a = SPLIT_POLICY_V1.split_of_group(g);
            let b = large
                .iter()
                .find(|x| x.shape_family == g.shape_family)
                .map(|x| SPLIT_POLICY_V1.split_of_group(x))
                .unwrap();
            assert_eq!(a, b, "family {} moved when the corpus grew", g.shape_family);
        }
    }

    /// All three splits must be non-empty and none may swallow the corpus:
    /// a "split" that puts everything in development is not a split.
    #[test]
    fn all_three_splits_are_populated_and_balanced() {
        let groups = corpus();
        let summary = summarize(&SPLIT_POLICY_V1, &groups);
        let total: usize = summary.iter().map(|s| s.groups).sum();
        assert_eq!(total, groups.len());
        for s in &summary {
            println!(
                "{}: {} families, {} groups, {} scenes",
                s.split, s.families, s.groups, s.scenes
            );
            assert!(
                s.families >= 3,
                "{} has only {} families",
                s.split,
                s.families
            );
            assert!(s.groups >= 3, "{} has only {} groups", s.split, s.groups);
        }
    }

    #[test]
    fn the_held_out_profile_never_appears_in_development() {
        assert!(!SPLIT_POLICY_V1.profile_allowed(Split::Development, "tiny-skia"));
        assert!(SPLIT_POLICY_V1.profile_allowed(Split::Calibration, "tiny-skia"));
        assert!(SPLIT_POLICY_V1.profile_allowed(Split::SealedAudit, "tiny-skia"));
        assert!(SPLIT_POLICY_V1.profile_allowed(Split::Development, "exact-clip"));
    }

    /// The burn policy is enforced by comparison, not by promise.
    #[test]
    fn opening_the_audit_records_hashes_and_any_later_change_burns_it() {
        let sealed = AuditSeal::sealed(1);

        // While sealed, scoring the audit is refused outright.
        assert_eq!(sealed.check("c", "p", "g"), Err(BurnViolation::StillSealed));

        let opened = sealed.open("c", "p", "g", "release candidate 1");
        assert_eq!(opened.status, SealStatus::Opened);
        assert!(opened.check("c", "p", "g").is_ok(), "unchanged is fine");

        // Each of the three is checked, not just the first (M-1: class, not
        // enumeration).
        for (corpus, prereg, gates, what) in [
            ("c2", "p", "g", "corpus"),
            ("c", "p2", "g", "preregistration"),
            ("c", "p", "g2", "gates"),
        ] {
            match opened.check(corpus, prereg, gates) {
                Err(BurnViolation::ChangedAfterOpening { what: w, .. }) => assert_eq!(w, what),
                other => panic!("expected a burn for {what}, got {other:?}"),
            }
        }

        let burned = opened.burn("thresholds were retuned after seeing audit results");
        assert!(matches!(
            burned.check("c", "p", "g"),
            Err(BurnViolation::AlreadyBurned { .. })
        ));
        assert_eq!(burned.next_generation().generation, 2);
        assert_eq!(burned.next_generation().status, SealStatus::Sealed);
    }

    /// A record that claims to be open but has no hashes cannot be used to
    /// pass the check: an empty field is not a match.
    #[test]
    fn an_incomplete_open_record_is_refused() {
        let mut bogus = AuditSeal::sealed(1);
        bogus.status = SealStatus::Opened;
        assert!(matches!(
            bogus.check("", "", ""),
            Err(BurnViolation::IncompleteRecord { .. })
        ));
    }

    #[test]
    fn the_seal_artifact_round_trips_through_json() {
        let s = AuditSeal::sealed(3).open("aa", "bb", "cc", "note");
        let text = serde_json::to_string(&s).unwrap();
        let back: AuditSeal = serde_json::from_str(&text).unwrap();
        assert_eq!(s, back);
        let ids: BTreeSet<&str> = ["sealed", "opened", "burned"].into_iter().collect();
        assert!(ids.contains("sealed"));
    }
}
