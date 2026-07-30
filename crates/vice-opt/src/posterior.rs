//! Posterior aggregation with explicit supported and unexplored search mass.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum BoundValue<T> {
    Certified(T),
    EmpiricallyCalibrated(T),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelIdentity {
    pub universe_sha256: String,
    pub pricing_sha256: String,
    pub backend_sha256: String,
    pub config_sha256: String,
}

impl ModelIdentity {
    pub fn new(
        universe_sha256: impl Into<String>,
        pricing_sha256: impl Into<String>,
        backend_sha256: impl Into<String>,
        config_sha256: impl Into<String>,
    ) -> Result<Self, PosteriorError> {
        let value = Self {
            universe_sha256: universe_sha256.into(),
            pricing_sha256: pricing_sha256.into(),
            backend_sha256: backend_sha256.into(),
            config_sha256: config_sha256.into(),
        };
        for (field, digest) in [
            ("universe", &value.universe_sha256),
            ("pricing", &value.pricing_sha256),
            ("backend", &value.backend_sha256),
            ("config", &value.config_sha256),
        ] {
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(PosteriorError::InvalidIdentity { field });
            }
        }
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScoredHypothesis {
    pub hypothesis_id: String,
    pub delivery_digest: String,
    pub topology_class: String,
    pub formation_class: String,
    pub total_bits: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct UnexploredMassBound {
    /// A proven lower bound on the bits of every unexplored hypothesis.
    pub best_possible_bits: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnexploredMassInput {
    /// The declared search universe was completely enumerated.
    Complete,
    /// A finite number of hypotheses remains, each with a proven score bound.
    Certified {
        hypotheses: u64,
        best_possible_bits: f64,
    },
    /// Frozen held-out calibration bounded the omitted relative mass. The
    /// number is in units where the best recorded hypothesis has mass one.
    EmpiricallyCalibrated { relative_mass_upper_bound: f64 },
    /// Search was truncated and neither R1 nor R2 supplies a bound.
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchMassInput {
    pub identity: ModelIdentity,
    pub explored_kept: Vec<ScoredHypothesis>,
    pub budget_pruned: Vec<ScoredHypothesis>,
    pub unexplored: UnexploredMassInput,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeliveryPosterior {
    pub delivery_digest: String,
    pub explored_mass: f64,
    pub retained_normalized_mass: f64,
    pub posterior_lower_bound: BoundValue<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchMassCertificate {
    pub identity: ModelIdentity,
    pub supported_hypotheses: Option<u64>,
    pub explored_hypotheses: u64,
    pub budget_pruned_hypotheses: u64,
    pub unexplored_hypotheses: Option<u64>,
    pub reference_bits: f64,
    pub explored_mass: f64,
    pub budget_pruned_mass: f64,
    pub retained_mass_lower_bound: BoundValue<f64>,
    pub unexplored_mass_upper_bound: BoundValue<f64>,
    pub denominator_mass_upper_bound: BoundValue<f64>,
    pub truncated: bool,
    pub delivery: Vec<DeliveryPosterior>,
}

impl SearchMassCertificate {
    pub fn best_delivery(&self) -> Option<&DeliveryPosterior> {
        self.delivery.first()
    }
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum PosteriorError {
    #[error("invalid {field} sha256 identity")]
    InvalidIdentity { field: &'static str },
    #[error("supported universe is empty or smaller than recorded hypotheses")]
    InvalidSupportedUniverse,
    #[error("hypothesis id is empty or duplicated")]
    DuplicateHypothesis,
    #[error("hypothesis score or unexplored bound is non-finite")]
    NonFiniteScore,
    #[error("no explored hypothesis can define a production result")]
    NoExploredHypothesis,
}

fn relative_mass(bits: f64, reference_bits: f64) -> f64 {
    let exponent = (reference_bits - bits).clamp(-1074.0, 1023.0);
    exponent.exp2()
}

pub fn posterior_with_search_mass(
    input: SearchMassInput,
) -> Result<SearchMassCertificate, PosteriorError> {
    if input.explored_kept.is_empty() {
        return Err(PosteriorError::NoExploredHypothesis);
    }
    let recorded = input.explored_kept.len() as u64 + input.budget_pruned.len() as u64;
    let mut ids = BTreeSet::new();
    for h in input.explored_kept.iter().chain(input.budget_pruned.iter()) {
        if h.hypothesis_id.is_empty()
            || !ids.insert(h.hypothesis_id.as_str())
            || !h.total_bits.is_finite()
        {
            return if h.total_bits.is_finite() {
                Err(PosteriorError::DuplicateHypothesis)
            } else {
                Err(PosteriorError::NonFiniteScore)
            };
        }
    }
    if matches!(
        input.unexplored,
        UnexploredMassInput::Certified {
            best_possible_bits,
            ..
        } if !best_possible_bits.is_finite()
    ) || matches!(
        input.unexplored,
        UnexploredMassInput::EmpiricallyCalibrated {
            relative_mass_upper_bound
        } if !relative_mass_upper_bound.is_finite() || relative_mass_upper_bound < 0.0
    ) {
        return Err(PosteriorError::NonFiniteScore);
    }

    let reference_bits = input
        .explored_kept
        .iter()
        .chain(input.budget_pruned.iter())
        .map(|h| h.total_bits)
        .fold(f64::INFINITY, f64::min);
    let mut by_delivery: BTreeMap<String, f64> = BTreeMap::new();
    let mut explored_mass = 0.0;
    for h in input.explored_kept.iter().chain(input.budget_pruned.iter()) {
        let mass = relative_mass(h.total_bits, reference_bits);
        explored_mass += mass;
        *by_delivery.entry(h.delivery_digest.clone()).or_default() += mass;
    }
    let budget_pruned_mass = input
        .budget_pruned
        .iter()
        .map(|h| relative_mass(h.total_bits, reference_bits))
        .sum::<f64>();
    let (
        supported_hypotheses,
        unexplored_hypotheses,
        unexplored_mass_upper_bound,
        denominator_mass_upper_bound,
        retained_mass_lower_bound,
        truncated,
    ) = match input.unexplored {
        UnexploredMassInput::Complete => (
            Some(recorded),
            Some(0),
            BoundValue::Certified(0.0),
            BoundValue::Certified(explored_mass),
            BoundValue::Certified(1.0),
            false,
        ),
        UnexploredMassInput::Certified {
            hypotheses,
            best_possible_bits,
        } => {
            let per_unexplored = relative_mass(best_possible_bits, reference_bits);
            let unexplored = (per_unexplored * hypotheses as f64).min(f64::MAX);
            let denominator = (explored_mass + unexplored).min(f64::MAX);
            (
                Some(recorded.saturating_add(hypotheses)),
                Some(hypotheses),
                BoundValue::Certified(unexplored),
                BoundValue::Certified(denominator),
                BoundValue::Certified(explored_mass / denominator),
                hypotheses > 0,
            )
        }
        UnexploredMassInput::EmpiricallyCalibrated {
            relative_mass_upper_bound,
        } => {
            let denominator = (explored_mass + relative_mass_upper_bound).min(f64::MAX);
            (
                None,
                None,
                BoundValue::EmpiricallyCalibrated(relative_mass_upper_bound),
                BoundValue::EmpiricallyCalibrated(denominator),
                BoundValue::EmpiricallyCalibrated(explored_mass / denominator),
                true,
            )
        }
        UnexploredMassInput::Unknown => (
            None,
            None,
            BoundValue::Unknown,
            BoundValue::Unknown,
            BoundValue::Unknown,
            true,
        ),
    };
    let mut delivery: Vec<_> = by_delivery
        .into_iter()
        .map(|(delivery_digest, mass)| DeliveryPosterior {
            delivery_digest,
            explored_mass: mass,
            retained_normalized_mass: mass / explored_mass,
            posterior_lower_bound: match denominator_mass_upper_bound {
                BoundValue::Certified(denominator) => BoundValue::Certified(mass / denominator),
                BoundValue::EmpiricallyCalibrated(denominator) => {
                    BoundValue::EmpiricallyCalibrated(mass / denominator)
                }
                BoundValue::Unknown => BoundValue::Unknown,
            },
        })
        .collect();
    delivery.sort_by(|a, b| {
        b.retained_normalized_mass
            .total_cmp(&a.retained_normalized_mass)
            .then_with(|| a.delivery_digest.cmp(&b.delivery_digest))
    });
    Ok(SearchMassCertificate {
        identity: input.identity,
        supported_hypotheses,
        explored_hypotheses: recorded,
        budget_pruned_hypotheses: input.budget_pruned.len() as u64,
        unexplored_hypotheses,
        reference_bits,
        explored_mass,
        budget_pruned_mass,
        retained_mass_lower_bound,
        unexplored_mass_upper_bound,
        denominator_mass_upper_bound,
        truncated,
        delivery,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> ModelIdentity {
        ModelIdentity::new(
            "0".repeat(64),
            "1".repeat(64),
            "2".repeat(64),
            "3".repeat(64),
        )
        .unwrap()
    }

    fn h(id: &str, delivery: &str, bits: f64) -> ScoredHypothesis {
        ScoredHypothesis {
            hypothesis_id: id.into(),
            delivery_digest: delivery.into(),
            topology_class: "t".into(),
            formation_class: "f".into(),
            total_bits: bits,
        }
    }

    #[test]
    fn delivery_equivalent_hypotheses_aggregate_before_confidence() {
        let got = posterior_with_search_mass(SearchMassInput {
            identity: identity(),
            explored_kept: vec![h("a", "same", 10.0)],
            budget_pruned: vec![h("b", "same", 11.0), h("c", "other", 12.0)],
            unexplored: UnexploredMassInput::Complete,
        })
        .unwrap();
        assert_eq!(got.delivery.len(), 2);
        assert_eq!(
            got.delivery[0].posterior_lower_bound,
            BoundValue::Certified(1.5 / 1.75)
        );
    }

    #[test]
    fn unexplored_mass_can_only_lower_confidence() {
        let complete = posterior_with_search_mass(SearchMassInput {
            identity: identity(),
            explored_kept: vec![h("a", "a", 10.0), h("b", "b", 12.0)],
            budget_pruned: vec![],
            unexplored: UnexploredMassInput::Complete,
        })
        .unwrap();
        let truncated = posterior_with_search_mass(SearchMassInput {
            identity: identity(),
            explored_kept: vec![h("a", "a", 10.0), h("b", "b", 12.0)],
            budget_pruned: vec![],
            unexplored: UnexploredMassInput::Certified {
                hypotheses: 18,
                best_possible_bits: 14.0,
            },
        })
        .unwrap();
        let BoundValue::Certified(complete) =
            complete.best_delivery().unwrap().posterior_lower_bound
        else {
            panic!("complete search is certified")
        };
        let BoundValue::Certified(truncated) =
            truncated.best_delivery().unwrap().posterior_lower_bound
        else {
            panic!("bounded search is certified")
        };
        assert!(truncated < complete);
    }

    #[test]
    fn absent_unexplored_bound_remains_explicitly_unknown() {
        let got = posterior_with_search_mass(SearchMassInput {
            identity: identity(),
            explored_kept: vec![h("a", "a", 10.0)],
            budget_pruned: vec![],
            unexplored: UnexploredMassInput::Unknown,
        })
        .unwrap();
        assert_eq!(got.unexplored_mass_upper_bound, BoundValue::Unknown);
        assert_eq!(
            got.best_delivery().unwrap().posterior_lower_bound,
            BoundValue::Unknown
        );
        assert_eq!(got.best_delivery().unwrap().retained_normalized_mass, 1.0);
    }

    #[test]
    fn empirical_search_mass_is_never_serialized_as_certified() {
        let got = posterior_with_search_mass(SearchMassInput {
            identity: identity(),
            explored_kept: vec![h("a", "a", 10.0)],
            budget_pruned: vec![],
            unexplored: UnexploredMassInput::EmpiricallyCalibrated {
                relative_mass_upper_bound: 0.25,
            },
        })
        .unwrap();
        assert_eq!(
            got.best_delivery().unwrap().posterior_lower_bound,
            BoundValue::EmpiricallyCalibrated(0.8)
        );
    }
}
