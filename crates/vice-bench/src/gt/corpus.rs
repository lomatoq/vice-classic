//! Corpus assembly and the manifest artifact (spec §27.1, §28 M3 gate
//! "reports reproduce from clean checkout").
//!
//! The corpus is regenerated from source on every build: nothing but the
//! six hand-authored SVG files is stored, and the manifest records the
//! sha256 of every render so a reproduction is a comparison rather than a
//! judgement call. That is the same contract `SMOKE_MANIFEST.toml` has for
//! the M0 corpus, applied to a corpus three orders of magnitude larger.

use std::collections::BTreeMap;

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::adversarial::all_adversarial_groups;
use super::authored::authored_groups;
use super::degradation::{matrix_v1, render_cell, DegradationCell};
use super::grammar::procedural_groups;
use super::split::{summarize, Split, SplitPolicy, SplitSummary, SPLIT_POLICY_V1};
use super::{GtSourceGroup, PartitionTruth, SalientFeature};
use crate::hashing::sha256_hex;

pub const CORPUS_SCHEMA: &str = "vice-classic/gt-corpus/v1";

/// Structural variants per procedural shape family in the committed
/// corpus. Part of the manifest, because changing it changes the corpus.
pub const PROCEDURAL_VARIANTS: usize = 4;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneEntry {
    pub id: String,
    pub scene_digest_sha256: String,
    pub partition_truth: PartitionTruth,
    pub authored_truth_construction: String,
    pub salient_features: Vec<SalientFeature>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupEntry {
    pub id: String,
    pub origin: &'static str,
    pub shape_family: String,
    pub provenance: String,
    pub split: &'static str,
    pub intentionally_ambiguous: bool,
    pub equivalence_class: Option<String>,
    pub scenes: Vec<SceneEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenderEntry {
    pub group_id: String,
    pub scene_id: String,
    pub cell_id: String,
    pub split: &'static str,
    pub identifiability: &'static str,
    /// True for renders from the inverse-crime profile: recorded, and
    /// excluded from the reliability sample (§27.1).
    pub inverse_crime: bool,
    pub width_px: u32,
    pub height_px: u32,
    pub sha256: String,
}

/// The platform a manifest's render digests belong to.
///
/// Corpus geometry is generated with `sin`/`cos`, colours with `powf` and
/// the gaussian PSF with `exp` — all libm, whose results Rust does NOT
/// guarantee bit-identical across platforms. ADR-0008 §8 recorded exactly
/// this ("Tier A only; frozen render digest artifacts must not contain
/// arcs") and this corpus is built out of trigonometry, so its digests are
/// a Tier A artifact (§5.5). Recording the platform is what makes the
/// limitation checkable instead of a footnote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Platform {
    pub os: String,
    pub arch: String,
}

impl Platform {
    pub fn current() -> Platform {
        Platform {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CorpusManifest {
    pub schema: &'static str,
    /// The Tier A platform whose libm produced the digests below.
    pub platform: Platform,
    pub procedural_variants_per_family: usize,
    pub split_policy_version: String,
    pub cells: Vec<String>,
    pub groups: Vec<GroupEntry>,
    pub renders: Vec<RenderEntry>,
    pub split_summary: Vec<SplitSummary>,
    pub identifiability_counts: BTreeMap<String, usize>,
    pub renders_by_profile: BTreeMap<String, usize>,
}

impl CorpusManifest {
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("manifest serializes")
    }

    /// The corpus hash: one of the three the burn policy records.
    pub fn hash(&self) -> String {
        sha256_hex(self.canonical_json().as_bytes())
    }

    /// Independent source groups, which is the unit the sample-size
    /// contract counts (§27.4).
    pub fn source_groups(&self) -> usize {
        self.groups.len()
    }

    pub fn groups_in(&self, split: Split) -> usize {
        self.groups
            .iter()
            .filter(|g| g.split == split.as_str())
            .count()
    }
}

/// Every source group of the corpus, in a deterministic order.
pub(crate) fn all_groups() -> Result<Vec<GtSourceGroup>, String> {
    let mut groups = procedural_groups(PROCEDURAL_VARIANTS);
    groups.extend(authored_groups().map_err(|e| e.to_string())?);
    groups.extend(all_adversarial_groups());
    groups.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(groups)
}

fn render_digest(rgba: &[u8], w: u32, h: u32) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"vice-classic/gt-render/v1");
    hasher.update(w.to_le_bytes());
    hasher.update(h.to_le_bytes());
    hasher.update(rgba);
    hex::encode(hasher.finalize())
}

/// Build the manifest, optionally over a subset of the degradation cells.
///
/// `cell_filter` exists because the full matrix over the whole corpus is
/// minutes of work: a fast path is needed for tests and CI, while the
/// committed manifest uses every cell. The filter is recorded in the cell
/// list, so a partial manifest can never be mistaken for the full one —
/// its hash differs and its `cells` field says why.
pub fn build_manifest(
    policy: &SplitPolicy,
    cell_filter: impl Fn(&DegradationCell) -> bool,
) -> Result<CorpusManifest, String> {
    let groups = all_groups()?;
    let cells: Vec<DegradationCell> = matrix_v1().into_iter().filter(|c| cell_filter(c)).collect();

    let mut group_entries = Vec::with_capacity(groups.len());
    let mut renders = Vec::new();
    let mut identifiability_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut renders_by_profile: BTreeMap<String, usize> = BTreeMap::new();

    for g in &groups {
        let split = policy.split_of_group(g);
        let members = g.equivalence_class.as_ref().map_or(1, |e| e.members.len());
        let mut scenes = Vec::with_capacity(g.scenes.len());
        for s in &g.scenes {
            scenes.push(SceneEntry {
                id: s.id().to_string(),
                scene_digest_sha256: vice_ir::scene_digest_sha256(s.scene().scene())
                    .map_err(|e| e.to_string())?,
                partition_truth: s.partition_truth().clone(),
                authored_truth_construction: s.authored_truth().construction.clone(),
                salient_features: s.salient_features().to_vec(),
            });
            for cell in &cells {
                if !policy.profile_allowed(split, cell.profile.as_str()) {
                    continue;
                }
                let r = render_cell(s, cell, members)?;
                *identifiability_counts
                    .entry(r.identifiability.as_str().to_string())
                    .or_default() += 1;
                *renders_by_profile
                    .entry(cell.profile.as_str().to_string())
                    .or_default() += 1;
                renders.push(RenderEntry {
                    group_id: g.id.clone(),
                    scene_id: s.id().to_string(),
                    cell_id: r.cell_id.clone(),
                    split: split.as_str(),
                    identifiability: r.identifiability.as_str(),
                    inverse_crime: r.inverse_crime,
                    width_px: r.width_px,
                    height_px: r.height_px,
                    sha256: render_digest(&r.rgba8, r.width_px, r.height_px),
                });
            }
        }
        group_entries.push(GroupEntry {
            id: g.id.clone(),
            origin: g.origin.as_str(),
            shape_family: g.shape_family.clone(),
            provenance: g.provenance.clone(),
            split: split.as_str(),
            intentionally_ambiguous: g.intentionally_ambiguous,
            equivalence_class: g.equivalence_class.as_ref().map(|e| e.id.clone()),
            scenes,
        });
    }

    Ok(CorpusManifest {
        schema: CORPUS_SCHEMA,
        platform: Platform::current(),
        procedural_variants_per_family: PROCEDURAL_VARIANTS,
        split_policy_version: policy.version.to_string(),
        cells: cells.iter().map(|c| c.id()).collect(),
        groups: group_entries,
        renders,
        split_summary: summarize(policy, &groups),
        identifiability_counts,
        renders_by_profile,
    })
}

/// The full committed manifest.
pub fn build_full_manifest() -> Result<CorpusManifest, String> {
    build_manifest(&SPLIT_POLICY_V1, |_| true)
}

/// A fast, deterministic subset for tests and CI: the cheap end of the
/// size axis, every profile, and the PSF excursions.
///
/// The supersampling SPINE cells are dropped rather than the profile
/// itself: the supersampler costs 576 inside-tests per pixel, and its box
/// coverage is already covered by the exact integrator, while the PSF
/// excursions are the only place it says something no other profile can.
pub fn fast_cell_filter(c: &DegradationCell) -> bool {
    c.size_px <= 32
        && !(c.profile == super::raster::RasterProfile::Supersample
            && c.psf == super::raster::Psf::Box)
}

/// The smallest honest subset: one size, every profile. Used by the unit
/// tests, which must stay in the seconds a `cargo test` budget allows -
/// building the full matrix over 60 groups is minutes of work, and a test
/// suite nobody runs is worse than a slower one.
pub fn test_cell_filter(c: &DegradationCell) -> bool {
    c.size_px == 16
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    /// One manifest, built once and shared: five tests each rebuilding it
    /// turned a 100-second check into a nine-minute one.
    fn test_manifest() -> &'static CorpusManifest {
        static M: OnceLock<CorpusManifest> = OnceLock::new();
        M.get_or_init(|| build_manifest(&SPLIT_POLICY_V1, test_cell_filter).unwrap())
    }

    #[test]
    fn the_fast_manifest_is_deterministic_and_covers_the_corpus() {
        let a = test_manifest();
        let b = build_manifest(&SPLIT_POLICY_V1, test_cell_filter).unwrap();
        assert_eq!(a.hash(), b.hash(), "the corpus must rebuild identically");

        println!(
            "groups {}, scenes {}, renders {}, cells {}",
            a.source_groups(),
            a.groups.iter().map(|g| g.scenes.len()).sum::<usize>(),
            a.renders.len(),
            a.cells.len()
        );
        assert!(a.source_groups() >= 55, "groups {}", a.source_groups());
        assert!(!a.renders.is_empty());

        // All three origins present, which is what "heterogeneous" means.
        for origin in ["procedural", "authored", "adversarial"] {
            assert!(
                a.groups.iter().any(|g| g.origin == origin),
                "no {origin} groups"
            );
        }
        // All three splits populated.
        for split in [Split::Development, Split::Calibration, Split::SealedAudit] {
            assert!(a.groups_in(split) > 0, "{} is empty", split.as_str());
        }
    }

    /// The leak test that matters for the gate: the held-out profile must
    /// not appear in a development render, anywhere in the manifest.
    #[test]
    fn no_development_render_uses_a_held_out_profile() {
        let m = test_manifest();
        for r in &m.renders {
            if r.split == "development" {
                for held in SPLIT_POLICY_V1.held_out_profiles {
                    assert!(
                        !r.cell_id.contains(held),
                        "development render {} uses held-out profile {held}",
                        r.cell_id
                    );
                }
            }
        }
        // And it DOES appear elsewhere, or the check above would be vacuous.
        assert!(
            m.renders
                .iter()
                .any(|r| r.split != "development" && r.cell_id.contains("tiny-skia")),
            "the held-out profile must still be measured outside development"
        );
    }

    /// The manifest records the Tier A platform its digests belong to, and
    /// the platform is part of the hash.
    ///
    /// Corpus geometry comes from `sin`/`cos` and colour from `powf`, so
    /// digests are reproducible on one platform and not guaranteed across
    /// platforms (ADR-0008 §8, §5.5 Tier A). This was found the hard way:
    /// CI on Linux failed to reproduce a manifest recorded on Windows while
    /// the SAME-platform determinism check passed, which is exactly the
    /// signature of a libm difference rather than of nondeterminism.
    #[test]
    fn the_manifest_records_the_platform_its_digests_belong_to() {
        let m = test_manifest();
        assert_eq!(m.platform, Platform::current());
        assert!(!m.platform.os.is_empty() && !m.platform.arch.is_empty());
        // And a manifest claiming another platform is a different manifest.
        let mut other = m.clone();
        other.platform.os = format!("{}-elsewhere", m.platform.os);
        assert_ne!(other.hash(), m.hash());
    }

    /// A partial manifest must be distinguishable from the full one, or a
    /// cheap run could be mistaken for the reproduction.
    #[test]
    fn a_partial_manifest_cannot_be_mistaken_for_the_full_one() {
        let full_ish = test_manifest();
        let fewer = build_manifest(&SPLIT_POLICY_V1, |c| {
            c.size_px == 16 && c.profile == crate::gt::raster::RasterProfile::ExactClip
        })
        .unwrap();
        assert_ne!(full_ish.hash(), fewer.hash());
        assert_ne!(full_ish.cells.len(), fewer.cells.len());
        assert!(fewer.cells.iter().all(|c| c.starts_with("s16_")));
    }

    /// Inverse-crime renders are present AND flagged: recorded so the gap
    /// can be measured, never silently mixed into the sample.
    #[test]
    fn inverse_crime_renders_are_present_and_flagged() {
        let m = test_manifest();
        let flagged = m.renders.iter().filter(|r| r.inverse_crime).count();
        assert!(flagged > 0, "the arm must exist so the gap is measurable");
        for r in m.renders.iter().filter(|r| r.inverse_crime) {
            assert!(r.cell_id.contains("vice-render"));
        }
        for r in m.renders.iter().filter(|r| !r.inverse_crime) {
            assert!(!r.cell_id.contains("vice-render"));
        }
    }

    /// Ambiguity groups and equivalence classes survive into the manifest,
    /// because a scorer reads the manifest and not the Rust types.
    #[test]
    fn ambiguity_metadata_reaches_the_manifest() {
        let m = test_manifest();
        let ambiguous: Vec<&GroupEntry> = m
            .groups
            .iter()
            .filter(|g| g.intentionally_ambiguous)
            .collect();
        assert!(
            ambiguous.len() >= 3,
            "the corpus must carry ambiguity pairs"
        );
        for g in ambiguous {
            assert_eq!(g.scenes.len(), 2);
            assert!(g.equivalence_class.is_some());
        }
        assert!(
            m.identifiability_counts
                .get("information_lost")
                .copied()
                .unwrap_or(0)
                > 0,
            "at small sizes some renders MUST be information-lost, or the label is decorative"
        );
        assert!(
            m.identifiability_counts
                .get("identifiable")
                .copied()
                .unwrap_or(0)
                > 0
        );
    }
}
