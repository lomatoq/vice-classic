//! Factorial oracle harness (spec v1.3 §27.6, §28 M3.5).
//!
//! M3.5 builds the MEASUREMENT frame for causal attribution, before there is
//! an algorithm to attribute anything to (§32 rule 2). What it owes, in the
//! spec's own words: *"O0/G30 renderer ceiling; PF 2×2 where the reference
//! backend supports honest injection; intervention schemas / compatibility
//! keys; later arms `not_yet_applicable`"*, gated by *"no causal deltas
//! across incompatible runs; inverse-crime warning visible"*.
//!
//! The four modules divide the job so that each rule is a mechanism:
//!
//! | module | what it makes impossible |
//! |---|---|
//! | [`design`] | an arm that is absent without saying what is missing and who owns it |
//! | [`key`] | a delta assembled from arms of two different runs |
//! | [`crime`] | an aggregate that loses an inverse-crime warning |
//! | [`ceiling`] | a ceiling number whose two sides ran different formations |
//!
//! What M3.5 can honestly measure is ONE arm of the 2×2: `PF11`, GT
//! partition and GT formation, which is also `G30`. The harness has no
//! partitioner (M4.5) and no formation estimator (M4), so `PF00`, `PF01` and
//! `PF10` are typed refusals — and because every factorial effect needs all
//! four arms, so are all three effects. That is the design's own answer, not
//! a gap papered over: publishing `PF11 − PF10` under the name "partition
//! main effect" would be the order-dependent ladder §27.6 abolished.

pub mod ceiling;
pub mod crime;
pub mod design;
pub mod effects;
pub mod key;
pub mod paint;
pub mod report;

use serde::Serialize;

use ceiling::{measure_ceiling, ArmRefusal, CeilingArm, OracleBackend, RoundtrippedScene};
use design::FormationSource;

use crate::gt::corpus::all_groups;
use crate::gt::degradation::{matrix_v1, DegradationCell};
use crate::gt::raster::RasterProfile;
use crate::hashing::sha256_hex;

/// How much of the corpus one run covers. Recorded in the config hash, so a
/// cheap run can never be compared with a full one by accident — that is a
/// compatibility-key component doing its job (§27.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleScope {
    /// Every scene of the corpus.
    Full,
    /// Every tenth scene: the shape of the report in seconds, for tests.
    Test,
}

impl OracleScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            OracleScope::Full => "full",
            OracleScope::Test => "test",
        }
    }

    fn stride(&self) -> usize {
        match self {
            OracleScope::Full => 1,
            OracleScope::Test => 10,
        }
    }
}

/// The observation cells the oracle runs on, BY ID, so each one is a cell of
/// the frozen §27.2 matrix and each observation is literally a corpus render
/// rather than a lookalike produced here.
///
/// Chosen to make the inverse-crime axis measurable in both directions: two
/// cells whose engine is independent of everything of ours (`tiny-skia`,
/// `raqote`) and two whose engine is one of ours, so a clean arm and a
/// contaminated arm both exist on real data.
pub const ORACLE_CELL_IDS: &[&str] = &[
    "s32_pexact-clip_box_lin_none_dx0.00dy0.00_c1.00",
    "s32_ptiny-skia_box_lin_none_dx0.00dy0.00_c1.00",
    "s32_praqote_box_lin_none_dx0.00dy0.00_c1.00",
    "s32_pexact-clip_box_lin_none_dx0.33dy0.50_c1.00",
    // Added in M4, because PF10 estimates the formation and these are the
    // two cells where the estimate can be WRONG: a cell that blended in
    // encoded sRGB, and one whose paints are close enough that the mixture
    // is poorly conditioned. Without them the factorial's new arm would run
    // only where the answer is the default.
    "s32_pexact-clip_box_srgb_none_dx0.00dy0.00_c1.00",
    "s32_pexact-clip_box_lin_none_dx0.00dy0.00_c0.50",
];

/// The reference backends. `ViceRender` is present ON PURPOSE and always
/// flagged: §16.3 says the internal renderer is not tested against itself,
/// and the way to keep that visible is to run the pairing and warn, not to
/// omit it and be silent.
pub const ORACLE_BACKENDS: &[RasterProfile] =
    &[RasterProfile::ExactClip, RasterProfile::ViceRender];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OracleConfig {
    pub schema: &'static str,
    pub scope: &'static str,
    pub serialization: &'static str,
    pub backends: Vec<String>,
    pub cells: Vec<String>,
    pub metrics: Vec<&'static str>,
    pub intervention_schema_version: &'static str,
}

impl OracleConfig {
    pub fn v1(scope: OracleScope) -> OracleConfig {
        OracleConfig {
            schema: key::ORACLE_CONFIG_SCHEMA,
            scope: scope.as_str(),
            serialization: ceiling::Serialization::CanonicalRoundtrip.id(),
            backends: ORACLE_BACKENDS
                .iter()
                .map(|p| OracleBackend::new(*p).id())
                .collect(),
            cells: ORACLE_CELL_IDS.iter().map(|s| (*s).to_string()).collect(),
            metrics: ceiling::CEILING_METRICS.to_vec(),
            intervention_schema_version: design::INTERVENTION_SCHEMA_VERSION,
        }
    }

    /// `config_hash` of the compatibility key: everything that can move a
    /// number, and nothing that cannot.
    pub fn hash(&self) -> String {
        sha256_hex(
            serde_json::to_string(self)
                .expect("oracle config serializes")
                .as_bytes(),
        )
    }
}

/// One arm that produced no number, with the reason as data.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefusedArm {
    pub scene_id: String,
    pub cell_id: String,
    pub backend_id: String,
    pub reason: String,
}

/// Everything one run measured.
#[derive(Debug, Clone)]
pub struct OracleRun {
    pub config: OracleConfig,
    pub config_hash: String,
    pub scenes: u64,
    pub arms: Vec<CeilingArm>,
    pub refused: Vec<RefusedArm>,
    /// Scene digests of the fixtures used, hashed: ties this run to the
    /// exact corpus it measured without copying the manifest into it.
    pub fixture_set_hash: String,
}

/// Resolve the declared cell ids against the frozen matrix.
fn oracle_cells() -> Result<Vec<DegradationCell>, String> {
    let matrix = matrix_v1();
    ORACLE_CELL_IDS
        .iter()
        .map(|want| {
            matrix
                .iter()
                .find(|c| c.id() == *want)
                .copied()
                .ok_or_else(|| {
                    format!("oracle cell {want} is not a cell of the frozen 27.2 matrix")
                })
        })
        .collect()
}

/// Run the harness.
pub fn run(scope: OracleScope) -> Result<OracleRun, String> {
    let config = OracleConfig::v1(scope);
    let config_hash = config.hash();
    let cells = oracle_cells()?;
    let groups = all_groups()?;

    let mut arms = Vec::new();
    let mut refused = Vec::new();
    let mut digests = Vec::new();
    let mut scenes = 0u64;

    for group in groups.iter().step_by(scope.stride()) {
        let members = group
            .equivalence_class
            .as_ref()
            .map_or(1, |e| e.members.len());
        for scene in &group.scenes {
            let round = match RoundtrippedScene::of(scene) {
                Ok(r) => r,
                Err(e) => {
                    refused.push(RefusedArm {
                        scene_id: scene.id().to_string(),
                        cell_id: "*".to_string(),
                        backend_id: "*".to_string(),
                        reason: e.to_string(),
                    });
                    continue;
                }
            };
            scenes += 1;
            digests.push(round.scene_digest.clone());
            for cell in &cells {
                for backend in ORACLE_BACKENDS.iter().map(|p| OracleBackend::new(*p)) {
                    // Both formation sources of the 2x2's formation factor.
                    // The partition factor is GT on both: auto partition is
                    // M4.5's, and PF00/PF01 stay typed refusals.
                    for source in [FormationSource::GroundTruth, FormationSource::Estimated] {
                        match measure_ceiling(
                            scene,
                            &round,
                            cell,
                            backend,
                            source,
                            members,
                            &config_hash,
                        ) {
                            Ok(a) => arms.push(a),
                            Err(e) => refused.push(RefusedArm {
                                scene_id: scene.id().to_string(),
                                cell_id: cell.id(),
                                backend_id: backend.id(),
                                reason: match &e {
                                    ArmRefusal::RenderFailed { .. } => e.to_string(),
                                    other => other.to_string(),
                                },
                            }),
                        }
                    }
                }
            }
        }
    }

    digests.sort();
    Ok(OracleRun {
        config,
        config_hash,
        scenes,
        arms,
        refused,
        fixture_set_hash: sha256_hex(digests.join("\u{1f}").as_bytes()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    pub(crate) fn test_run() -> &'static OracleRun {
        static R: OnceLock<OracleRun> = OnceLock::new();
        R.get_or_init(|| run(OracleScope::Test).expect("the test-scope oracle run must succeed"))
    }

    /// Every declared observation cell is a cell of the frozen §27.2
    /// matrix, so the oracle judges the corpus rather than a private set of
    /// images that resemble it.
    #[test]
    fn the_oracle_cells_are_cells_of_the_frozen_matrix() {
        let cells = oracle_cells().expect("all oracle cells must exist in matrix_v1");
        assert_eq!(cells.len(), ORACLE_CELL_IDS.len());
        for c in &cells {
            assert_eq!(c.resize, crate::gt::degradation::ResizeChain::None);
        }
        let profiles: std::collections::BTreeSet<&str> =
            cells.iter().map(|c| c.profile.as_str()).collect();
        assert!(
            profiles.contains("tiny-skia") && profiles.contains("raqote"),
            "at least one observation must come from an engine independent of all of ours, \
             or every arm would be inverse crime and the clean control would not exist"
        );
    }

    /// The run produces arms, and the config hash moves with the scope: a
    /// test-scope arm and a full-scope arm are not commensurable, which is
    /// the compatibility key doing its job rather than a comment asking.
    #[test]
    fn a_run_produces_arms_and_the_scope_is_part_of_the_key() {
        let r = test_run();
        assert!(r.scenes >= 4, "scenes {}", r.scenes);
        assert_eq!(
            r.arms.len() as u64 + r.refused.len() as u64,
            r.scenes * (ORACLE_CELL_IDS.len() * ORACLE_BACKENDS.len() * 2) as u64,
            "every (scene, cell, backend, formation source) tuple must be measured or refused"
        );
        // BOTH arms of the formation factor exist on real data.
        assert!(r.arms.iter().any(|a| a.arm == "PF11"));
        assert!(
            r.arms.iter().any(|a| a.arm == "PF10"),
            "M4 must produce the estimated-formation arm on the corpus"
        );
        assert_ne!(
            OracleConfig::v1(OracleScope::Test).hash(),
            OracleConfig::v1(OracleScope::Full).hash()
        );
    }

    /// The serialization component of the ceiling is measured, not assumed:
    /// the canonical round trip returns the same digest for every fixture,
    /// so what the ceiling reports is the renderer's.
    #[test]
    fn the_canonical_round_trip_contributes_exactly_zero() {
        let r = test_run();
        assert!(r
            .arms
            .iter()
            .all(|a| a.metrics.serialization_digest_identical));
    }

    /// Determinism: the same scope twice gives the same numbers.
    #[test]
    fn the_run_is_deterministic() {
        let a = run(OracleScope::Test).unwrap();
        let b = run(OracleScope::Test).unwrap();
        assert_eq!(a.config_hash, b.config_hash);
        assert_eq!(a.fixture_set_hash, b.fixture_set_hash);
        assert_eq!(a.arms, b.arms);
    }
}
