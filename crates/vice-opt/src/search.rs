//! Deterministic diverse beam selection with explicit resource accounting.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use thiserror::Error;

use crate::posterior::ScoredHypothesis;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SearchBudget {
    pub max_candidates_considered: usize,
    pub max_memory_bytes: u64,
    pub max_elapsed_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct BeamConfig {
    pub width: usize,
    pub within_best_bits: f64,
    pub min_topology_classes: usize,
    pub min_formation_classes: usize,
    pub budget: SearchBudget,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BeamCandidate {
    pub score: ScoredHypothesis,
    pub canonical_scene_digest: String,
    pub estimated_memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BudgetLedger {
    pub elapsed_ms: u64,
    pub candidates_presented: u64,
    pub candidates_considered: u64,
    pub memory_bytes_considered: u64,
    pub pruned_by_candidate_budget: u64,
    pub pruned_by_memory_budget: u64,
    pub pruned_by_bit_margin: u64,
    pub pruned_by_beam_width: u64,
    pub delivery_equivalent_collapses: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BeamSelection {
    pub kept: Vec<BeamCandidate>,
    /// Scored candidates omitted only because a resource budget was reached.
    /// These must be passed to posterior mass accounting.
    pub budget_pruned: Vec<BeamCandidate>,
    pub dominated_pruned: Vec<BeamCandidate>,
    pub ledger: BudgetLedger,
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum BeamError {
    #[error("beam configuration is invalid")]
    InvalidConfig,
    #[error("search time budget exhausted before beam selection")]
    TimeBudgetExhausted,
    #[error("candidate score is non-finite or identity is empty")]
    InvalidCandidate,
}

fn ordering(a: &BeamCandidate, b: &BeamCandidate) -> std::cmp::Ordering {
    a.score
        .total_bits
        .total_cmp(&b.score.total_bits)
        .then_with(|| a.canonical_scene_digest.cmp(&b.canonical_scene_digest))
        .then_with(|| a.score.hypothesis_id.cmp(&b.score.hypothesis_id))
}

pub fn select_diverse_beam(
    mut candidates: Vec<BeamCandidate>,
    cfg: BeamConfig,
    elapsed_ms: u64,
) -> Result<BeamSelection, BeamError> {
    if cfg.width == 0
        || !cfg.within_best_bits.is_finite()
        || cfg.within_best_bits < 0.0
        || cfg.budget.max_candidates_considered == 0
        || cfg.budget.max_memory_bytes == 0
        || cfg.budget.max_elapsed_ms == 0
    {
        return Err(BeamError::InvalidConfig);
    }
    if elapsed_ms > cfg.budget.max_elapsed_ms {
        return Err(BeamError::TimeBudgetExhausted);
    }
    if candidates.iter().any(|c| {
        !c.score.total_bits.is_finite()
            || c.score.hypothesis_id.is_empty()
            || c.score.delivery_digest.is_empty()
            || c.canonical_scene_digest.is_empty()
    }) {
        return Err(BeamError::InvalidCandidate);
    }
    candidates.sort_by(ordering);
    let presented = candidates.len() as u64;

    // Collapse only hypotheses that materialize to the same delivery bytes.
    // Equal-scoring but distinct deliveries remain separate.
    let mut delivery_best: BTreeMap<String, BeamCandidate> = BTreeMap::new();
    let mut equivalent = Vec::new();
    for candidate in candidates {
        match delivery_best.get(&candidate.score.delivery_digest) {
            Some(_) => equivalent.push(candidate),
            None => {
                delivery_best.insert(candidate.score.delivery_digest.clone(), candidate);
            }
        }
    }
    let equivalent_count = equivalent.len() as u64;
    let mut unique: Vec<_> = delivery_best.into_values().collect();
    unique.sort_by(ordering);

    let mut considered = Vec::new();
    let mut budget_pruned = Vec::new();
    let mut memory = 0u64;
    let mut candidate_budget_pruned = 0u64;
    let mut memory_budget_pruned = 0u64;
    for candidate in unique {
        if considered.len() >= cfg.budget.max_candidates_considered {
            candidate_budget_pruned += 1;
            budget_pruned.push(candidate);
            continue;
        }
        let next = memory.saturating_add(candidate.estimated_memory_bytes);
        if next > cfg.budget.max_memory_bytes {
            memory_budget_pruned += 1;
            budget_pruned.push(candidate);
            continue;
        }
        memory = next;
        considered.push(candidate);
    }
    let best = considered
        .first()
        .map_or(f64::INFINITY, |c| c.score.total_bits);
    let mut eligible = Vec::new();
    let mut bit_margin_pruned = 0u64;
    let mut dominated_pruned = equivalent;
    for candidate in considered.iter().cloned() {
        if candidate.score.total_bits <= best + cfg.within_best_bits {
            eligible.push(candidate);
        } else {
            bit_margin_pruned += 1;
            dominated_pruned.push(candidate);
        }
    }
    let eligible_count = eligible.len();

    let mut selected = BTreeSet::new();
    let mut topology = BTreeSet::new();
    let mut formation = BTreeSet::new();
    for (i, c) in eligible.iter().enumerate() {
        if selected.len() >= cfg.width || topology.len() >= cfg.min_topology_classes {
            break;
        }
        if topology.insert(c.score.topology_class.clone()) {
            selected.insert(i);
        }
    }
    for (i, c) in eligible.iter().enumerate() {
        if selected.len() >= cfg.width || formation.len() >= cfg.min_formation_classes {
            break;
        }
        if formation.insert(c.score.formation_class.clone()) {
            selected.insert(i);
        }
    }
    for i in 0..eligible.len() {
        if selected.len() >= cfg.width {
            break;
        }
        selected.insert(i);
    }
    let selected_count = selected.len();
    let mut kept = Vec::new();
    for (i, candidate) in eligible.into_iter().enumerate() {
        if selected.contains(&i) {
            kept.push(candidate);
        } else {
            dominated_pruned.push(candidate);
        }
    }
    kept.sort_by(ordering);
    Ok(BeamSelection {
        kept,
        budget_pruned,
        dominated_pruned,
        ledger: BudgetLedger {
            elapsed_ms,
            candidates_presented: presented,
            candidates_considered: considered.len() as u64,
            memory_bytes_considered: memory,
            pruned_by_candidate_budget: candidate_budget_pruned,
            pruned_by_memory_budget: memory_budget_pruned,
            pruned_by_bit_margin: bit_margin_pruned,
            pruned_by_beam_width: eligible_count.saturating_sub(selected_count) as u64,
            delivery_equivalent_collapses: equivalent_count,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(id: &str, delivery: &str, topo: &str, form: &str, bits: f64) -> BeamCandidate {
        BeamCandidate {
            score: ScoredHypothesis {
                hypothesis_id: id.into(),
                delivery_digest: delivery.into(),
                topology_class: topo.into(),
                formation_class: form.into(),
                total_bits: bits,
            },
            canonical_scene_digest: format!("scene-{id}"),
            estimated_memory_bytes: 10,
        }
    }

    fn cfg() -> BeamConfig {
        BeamConfig {
            width: 3,
            within_best_bits: 3.0,
            min_topology_classes: 2,
            min_formation_classes: 2,
            budget: SearchBudget {
                max_candidates_considered: 10,
                max_memory_bytes: 100,
                max_elapsed_ms: 1000,
            },
        }
    }

    #[test]
    fn equal_scores_with_distinct_delivery_are_retained_for_diversity() {
        let got = select_diverse_beam(
            vec![
                c("a", "da", "t1", "f1", 1.0),
                c("b", "db", "t2", "f1", 1.0),
                c("c", "dc", "t1", "f2", 1.0),
            ],
            cfg(),
            5,
        )
        .unwrap();
        assert_eq!(got.kept.len(), 3);
    }

    #[test]
    fn same_delivery_collapses_but_budget_pruning_stays_visible() {
        let mut config = cfg();
        config.budget.max_candidates_considered = 1;
        let got = select_diverse_beam(
            vec![
                c("a", "same", "t1", "f1", 1.0),
                c("b", "same", "t2", "f2", 1.1),
                c("c", "other", "t2", "f2", 1.2),
            ],
            config,
            5,
        )
        .unwrap();
        assert_eq!(got.kept.len(), 1);
        assert_eq!(got.budget_pruned.len(), 1);
        assert_eq!(got.ledger.delivery_equivalent_collapses, 1);
    }
}
