//! Posterior aggregation with explicit supported and unexplored search mass.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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

#[derive(Debug, Clone, PartialEq)]
pub struct SearchMassInput {
    pub identity: ModelIdentity,
    pub supported_hypotheses: u64,
    pub explored_kept: Vec<ScoredHypothesis>,
    pub budget_pruned: Vec<ScoredHypothesis>,
    /// `None` is permitted, but is maximally conservative: every unexplored
    /// item is treated as if it could tie the current best.
    pub unexplored_bound: Option<UnexploredMassBound>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeliveryPosterior {
    pub delivery_digest: String,
    pub explored_mass: f64,
    pub posterior_lower_bound: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchMassCertificate {
    pub identity: ModelIdentity,
    pub supported_hypotheses: u64,
    pub explored_hypotheses: u64,
    pub budget_pruned_hypotheses: u64,
    pub unexplored_hypotheses: u64,
    pub reference_bits: f64,
    pub explored_mass: f64,
    pub budget_pruned_mass: f64,
    pub unexplored_mass_upper_bound: f64,
    pub denominator_mass_upper_bound: f64,
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
    if input.supported_hypotheses == 0 || input.supported_hypotheses < recorded {
        return Err(PosteriorError::InvalidSupportedUniverse);
    }
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
    if input
        .unexplored_bound
        .is_some_and(|b| !b.best_possible_bits.is_finite())
    {
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
    let unexplored_hypotheses = input.supported_hypotheses - recorded;
    let per_unexplored = input
        .unexplored_bound
        .map_or(1.0, |b| relative_mass(b.best_possible_bits, reference_bits));
    let unexplored_mass_upper_bound = (per_unexplored * unexplored_hypotheses as f64).min(f64::MAX);
    let denominator_mass_upper_bound = explored_mass
        .clamp(0.0, f64::MAX)
        .mul_add(1.0, unexplored_mass_upper_bound)
        .min(f64::MAX);
    let mut delivery: Vec<_> = by_delivery
        .into_iter()
        .map(|(delivery_digest, mass)| DeliveryPosterior {
            delivery_digest,
            explored_mass: mass,
            posterior_lower_bound: if denominator_mass_upper_bound.is_finite()
                && denominator_mass_upper_bound > 0.0
            {
                mass / denominator_mass_upper_bound
            } else {
                0.0
            },
        })
        .collect();
    delivery.sort_by(|a, b| {
        b.posterior_lower_bound
            .total_cmp(&a.posterior_lower_bound)
            .then_with(|| a.delivery_digest.cmp(&b.delivery_digest))
    });
    Ok(SearchMassCertificate {
        identity: input.identity,
        supported_hypotheses: input.supported_hypotheses,
        explored_hypotheses: recorded,
        budget_pruned_hypotheses: input.budget_pruned.len() as u64,
        unexplored_hypotheses,
        reference_bits,
        explored_mass,
        budget_pruned_mass,
        unexplored_mass_upper_bound,
        denominator_mass_upper_bound,
        truncated: !input.budget_pruned.is_empty() || unexplored_hypotheses > 0,
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
            supported_hypotheses: 3,
            explored_kept: vec![h("a", "same", 10.0)],
            budget_pruned: vec![h("b", "same", 11.0), h("c", "other", 12.0)],
            unexplored_bound: None,
        })
        .unwrap();
        assert_eq!(got.delivery.len(), 2);
        assert!((got.delivery[0].posterior_lower_bound - (1.5 / 1.75)).abs() < 1e-12);
    }

    #[test]
    fn unexplored_mass_can_only_lower_confidence() {
        let complete = posterior_with_search_mass(SearchMassInput {
            identity: identity(),
            supported_hypotheses: 2,
            explored_kept: vec![h("a", "a", 10.0), h("b", "b", 12.0)],
            budget_pruned: vec![],
            unexplored_bound: None,
        })
        .unwrap();
        let truncated = posterior_with_search_mass(SearchMassInput {
            identity: identity(),
            supported_hypotheses: 20,
            explored_kept: vec![h("a", "a", 10.0), h("b", "b", 12.0)],
            budget_pruned: vec![],
            unexplored_bound: Some(UnexploredMassBound {
                best_possible_bits: 14.0,
            }),
        })
        .unwrap();
        assert!(
            truncated.best_delivery().unwrap().posterior_lower_bound
                < complete.best_delivery().unwrap().posterior_lower_bound
        );
    }

    #[test]
    fn absent_unexplored_bound_is_maximally_conservative() {
        let got = posterior_with_search_mass(SearchMassInput {
            identity: identity(),
            supported_hypotheses: 4,
            explored_kept: vec![h("a", "a", 10.0)],
            budget_pruned: vec![],
            unexplored_bound: None,
        })
        .unwrap();
        assert_eq!(got.unexplored_mass_upper_bound, 3.0);
        assert_eq!(got.best_delivery().unwrap().posterior_lower_bound, 0.25);
    }
}
