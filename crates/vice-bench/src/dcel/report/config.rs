//! The population floors of the §28 M5 gate rows, and the type that loads
//! them from the frozen file.
//!
//! Split out of `report.rs` in M6 when that file reached §4.1's 800-line cap.

use crate::gates::GatesFile;
use crate::topology::gate::Threshold;

/// The six §28 M5 population floors, DECLARED here so that
/// `gates::tests::every_frozen_value_agrees_with_the_code_that_uses_it` has a
/// consumer to compare the frozen file against.
///
/// The rows do NOT read these — they read `DcelGateConfig`, which reads the
/// file. That is RT45-A10 and it is the whole point: a constant the row
/// compares against is a registration of the SPELLING, and `MIN / 20` keeps the
/// spelling while changing the value. These exist only so that the file and the
/// code must agree, so that:
///
/// - change one of these without the file, and the gates test fails;
/// - change the file without these, and it fails for the same reason;
/// - change both, and §27.7 refuses the commit.
pub const MIN_ARMS: u32 = 200;
pub const MIN_STRUCTURAL_ARMS: u32 = 20;
pub const MIN_CONVENTION_DEPENDENT_GROUPS: u32 = 5;
pub const MIN_TRANSACTIONS: u32 = 50;
pub const MIN_UNRELATED_CHAIN_POPULATION: u32 = 40;
pub const MIN_RESOLVING_POWER_PROBES: u32 = 10;
pub const MIN_SLOTS_PERTURBED: u32 = 40000;
pub const MIN_REGISTER_ARMS_WITH_A_LONG_LOOP: u32 = 6;

/// The §28 M5 COMPOUND-transaction floors (M6). Same role as the constants
/// above: the file and the code must agree, so changing one without the other
/// is red and changing both is refused by §27.7.
pub const MIN_COMPOUND_TRANSACTIONS: u32 = 100;
pub const MIN_DISTINCT_COMPOUND_DELTAS: u32 = 3;
pub const MIN_TRANSACTION_SHAPES: u32 = 3;

/// The population thresholds of the §28 M5 rows, LOADED from the frozen gate
/// file.
///
/// The same shape `TopologyGateConfig` uses and for the same reason (RT45-A10):
/// the row compares against this struct, this struct has exactly one source,
/// and `Threshold` has no arithmetic so `t / 20` is a type error rather than a
/// value that passes a text check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DcelGateConfig {
    pub min_arms: Threshold,
    pub min_structural_arms: Threshold,
    pub min_convention_dependent_groups: Threshold,
    pub min_transactions: Threshold,
    pub min_unrelated_chain_population: Threshold,
    pub min_resolving_power_probes: Threshold,
    /// Slots the mutation walk must actually perturb. `slots_perturbed > 0`
    /// stood here and was not a gate: a walk that visited one slot satisfied
    /// it. A floor read off a run is (RT5-A2's neighbourhood).
    pub min_slots_perturbed: Threshold,
    /// Register arms carrying a face loop of three or more half-edges: the
    /// population §12's ORIENTED clause stands on (RT5-A17, M5A-D3-N1).
    pub min_register_arms_with_a_long_loop: Threshold,
}

impl DcelGateConfig {
    /// The committed thresholds, for tests that need to evaluate a gate row.
    ///
    /// Goes through `load_for_a_gate_decision` exactly like the CLI does, so a
    /// test cannot evaluate a row against a file `HEAD` does not carry.
    pub fn for_tests_from_the_committed_file() -> Result<DcelGateConfig, String> {
        // Resolved from the crate manifest rather than from the working
        // directory: an integration test runs with the CRATE as its cwd, and
        // `GATE_PATHS[0]` is workspace-relative.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(crate::gates::GATE_PATHS[0]);
        let g = GatesFile::load_for_a_gate_decision(&path)
            .map_err(|e| format!("load {}: {e}", path.display()))?;
        DcelGateConfig::from_gates(&g)
    }

    pub fn from_gates(g: &GatesFile) -> Result<DcelGateConfig, String> {
        let t = |key: &str| Threshold::from_gates(g, "dcel", key);
        Ok(DcelGateConfig {
            min_arms: t("gate_min_arms")?,
            min_structural_arms: t("gate_min_structural_arms")?,
            min_convention_dependent_groups: t("gate_min_convention_dependent_groups")?,
            min_transactions: t("gate_min_transactions")?,
            min_unrelated_chain_population: t("gate_min_unrelated_chain_population")?,
            min_resolving_power_probes: t("gate_min_resolving_power_probes")?,
            min_slots_perturbed: t("gate_min_slots_perturbed")?,
            min_register_arms_with_a_long_loop: t("gate_min_register_arms_with_a_long_loop")?,
        })
    }
}
