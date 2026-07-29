//! Current-parent trust-region optimization with exact transactional checks.

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Rect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ScoreScope {
    pub roi: Option<Rect>,
    pub halo_px: u32,
    pub global: bool,
}

impl ScoreScope {
    pub const FULL: Self = Self {
        roi: None,
        halo_px: 0,
        global: true,
    };
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockSpec {
    pub name: String,
    pub parameter_indices: Vec<usize>,
    pub scales: Vec<f64>,
    pub max_radius: f64,
    pub scope: ScoreScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvaluationToken {
    /// Changes only after an accepted state transition.
    pub cache_epoch: u64,
    /// Parent and every child in one exact comparison receive the same id.
    pub comparison_id: u64,
    /// Fixed tessellation/mesh identity for this exact comparison.
    pub fixed_mesh_id: u64,
}

pub trait TrustRegionProblem {
    fn exact_bits(
        &self,
        parameters: &[f64],
        scope: ScoreScope,
        token: EvaluationToken,
    ) -> Result<f64, String>;

    /// Project a proposal onto hard constraints. The implementation may change
    /// only parameters in the supplied block.
    fn project(&self, parameters: &mut [f64], block: &BlockSpec) -> Result<(), String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TrustRegionConfig {
    pub initial_radius: f64,
    pub minimum_radius: f64,
    pub expansion: f64,
    pub contraction: f64,
    pub finite_difference_step: f64,
    pub min_bits_improvement: f64,
    pub max_rounds: usize,
    pub max_backtracks: usize,
    pub full_check_every_accepted_blocks: usize,
}

impl TrustRegionConfig {
    fn validate(self) -> Result<(), TrustRegionError> {
        let finite_positive = [
            self.initial_radius,
            self.minimum_radius,
            self.expansion,
            self.contraction,
            self.finite_difference_step,
        ]
        .into_iter()
        .all(|v| v.is_finite() && v > 0.0);
        if !finite_positive
            || self.minimum_radius > self.initial_radius
            || self.expansion <= 1.0
            || self.contraction >= 1.0
            || !self.min_bits_improvement.is_finite()
            || self.min_bits_improvement < 0.0
            || self.max_rounds == 0
            || self.max_backtracks == 0
            || self.full_check_every_accepted_blocks == 0
        {
            return Err(TrustRegionError::InvalidConfig);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OptimizationTraceRow {
    pub round: usize,
    pub block: String,
    pub cache_epoch: u64,
    pub comparison_id: u64,
    pub parent_bits: f64,
    pub child_bits: f64,
    pub radius: f64,
    pub accepted: bool,
    pub full_check: bool,
    pub rolled_back_to_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OptimizationResult {
    pub parameters: Vec<f64>,
    pub full_bits: f64,
    pub accepted_blocks: usize,
    pub trace: Vec<OptimizationTraceRow>,
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum TrustRegionError {
    #[error("trust-region configuration is invalid")]
    InvalidConfig,
    #[error("parameter block is malformed")]
    InvalidBlock,
    #[error("problem evaluation failed: {0}")]
    Evaluation(String),
    #[error("problem evaluation returned a non-finite score")]
    NonFiniteScore,
}

fn evaluate<P: TrustRegionProblem>(
    problem: &P,
    parameters: &[f64],
    scope: ScoreScope,
    token: EvaluationToken,
) -> Result<f64, TrustRegionError> {
    let bits = problem
        .exact_bits(parameters, scope, token)
        .map_err(TrustRegionError::Evaluation)?;
    if bits.is_finite() {
        Ok(bits)
    } else {
        Err(TrustRegionError::NonFiniteScore)
    }
}

fn validate_blocks(blocks: &[BlockSpec], parameter_count: usize) -> Result<(), TrustRegionError> {
    if blocks.is_empty()
        || blocks.iter().any(|b| {
            b.name.is_empty()
                || b.parameter_indices.is_empty()
                || b.parameter_indices.len() != b.scales.len()
                || !b.max_radius.is_finite()
                || b.max_radius <= 0.0
                || b.scales.iter().any(|s| !s.is_finite() || *s <= 0.0)
                || b.parameter_indices.iter().any(|i| *i >= parameter_count)
                || if b.scope.global {
                    b.scope.roi.is_some()
                } else {
                    b.scope.halo_px == 0 || b.scope.roi.is_none_or(|r| r.x0 >= r.x1 || r.y0 >= r.y1)
                }
        })
    {
        Err(TrustRegionError::InvalidBlock)
    } else {
        Ok(())
    }
}

fn project_checked<P: TrustRegionProblem>(
    problem: &P,
    parameters: &mut [f64],
    block: &BlockSpec,
) -> Result<(), TrustRegionError> {
    let before = parameters.to_vec();
    problem
        .project(parameters, block)
        .map_err(TrustRegionError::Evaluation)?;
    if parameters.iter().any(|v| !v.is_finite()) {
        return Err(TrustRegionError::Evaluation(
            "projection produced a non-finite parameter".into(),
        ));
    }
    for (index, (old, new)) in before.iter().zip(parameters.iter()).enumerate() {
        if old != new && !block.parameter_indices.contains(&index) {
            return Err(TrustRegionError::Evaluation(
                "projection changed a parameter outside the declared block".into(),
            ));
        }
    }
    Ok(())
}

pub fn optimize_trust_region<P: TrustRegionProblem>(
    problem: &P,
    initial_parameters: Vec<f64>,
    blocks: &[BlockSpec],
    cfg: TrustRegionConfig,
) -> Result<OptimizationResult, TrustRegionError> {
    cfg.validate()?;
    validate_blocks(blocks, initial_parameters.len())?;
    if initial_parameters.iter().any(|v| !v.is_finite()) {
        return Err(TrustRegionError::InvalidBlock);
    }
    let mut parameters = initial_parameters;
    let mut cache_epoch = 0u64;
    let mut comparison_id = 0u64;
    let mut radius = cfg.initial_radius;
    let mut trace = Vec::new();
    let mut accepted_blocks = 0usize;
    let mut verified_parameters = parameters.clone();
    let mut verified_bits = evaluate(
        problem,
        &parameters,
        ScoreScope::FULL,
        EvaluationToken {
            cache_epoch,
            comparison_id,
            fixed_mesh_id: comparison_id,
        },
    )?;
    comparison_id += 1;

    for round in 0..cfg.max_rounds {
        let mut accepted_this_round = false;
        for block in blocks {
            let block_radius = radius.min(block.max_radius);
            let gradient_token = EvaluationToken {
                cache_epoch,
                comparison_id,
                fixed_mesh_id: comparison_id,
            };
            comparison_id += 1;
            let parent_for_gradient = evaluate(problem, &parameters, block.scope, gradient_token)?;
            let mut gradient = vec![0.0; block.parameter_indices.len()];
            for (slot, (&index, &scale)) in block
                .parameter_indices
                .iter()
                .zip(&block.scales)
                .enumerate()
            {
                let mut perturbed = parameters.clone();
                perturbed[index] += cfg.finite_difference_step * scale;
                project_checked(problem, &mut perturbed, block)?;
                let child = evaluate(problem, &perturbed, block.scope, gradient_token)?;
                let delta = perturbed[index] - parameters[index];
                gradient[slot] = if delta == 0.0 {
                    0.0
                } else {
                    (child - parent_for_gradient) / delta
                };
            }
            let scaled_norm = gradient
                .iter()
                .zip(&block.scales)
                .map(|(g, s)| (g * s).powi(2))
                .sum::<f64>()
                .sqrt();
            if scaled_norm == 0.0 {
                continue;
            }
            let mut accepted = false;
            for backtrack in 0..cfg.max_backtracks {
                let step_radius = block_radius * cfg.contraction.powi(backtrack as i32);
                if step_radius < cfg.minimum_radius {
                    break;
                }
                let token = EvaluationToken {
                    cache_epoch,
                    comparison_id,
                    fixed_mesh_id: comparison_id,
                };
                comparison_id += 1;
                // Current parent is recomputed for every block and exact
                // comparison, after all earlier accepted blocks.
                let parent_bits = evaluate(problem, &parameters, block.scope, token)?;
                let mut child = parameters.clone();
                for ((&index, &scale), &g) in block
                    .parameter_indices
                    .iter()
                    .zip(&block.scales)
                    .zip(&gradient)
                {
                    child[index] -= step_radius * g * scale * scale / scaled_norm;
                }
                project_checked(problem, &mut child, block)?;
                let child_bits = evaluate(problem, &child, block.scope, token)?;
                let improved = child_bits + cfg.min_bits_improvement < parent_bits;
                trace.push(OptimizationTraceRow {
                    round,
                    block: block.name.clone(),
                    cache_epoch,
                    comparison_id: token.comparison_id,
                    parent_bits,
                    child_bits,
                    radius: step_radius,
                    accepted: improved,
                    full_check: false,
                    rolled_back_to_verified: false,
                });
                if improved {
                    parameters = child;
                    cache_epoch += 1;
                    accepted_blocks += 1;
                    accepted_this_round = true;
                    accepted = true;
                    radius = (radius * cfg.expansion).min(block.max_radius);
                    break;
                }
            }
            if !accepted {
                radius = (radius * cfg.contraction).max(cfg.minimum_radius);
            }

            if accepted && accepted_blocks.is_multiple_of(cfg.full_check_every_accepted_blocks) {
                let token = EvaluationToken {
                    cache_epoch,
                    comparison_id,
                    fixed_mesh_id: comparison_id,
                };
                comparison_id += 1;
                let full_bits = evaluate(problem, &parameters, ScoreScope::FULL, token)?;
                let rollback = full_bits > verified_bits + cfg.min_bits_improvement;
                trace.push(OptimizationTraceRow {
                    round,
                    block: block.name.clone(),
                    cache_epoch,
                    comparison_id: token.comparison_id,
                    parent_bits: verified_bits,
                    child_bits: full_bits,
                    radius,
                    accepted: !rollback,
                    full_check: true,
                    rolled_back_to_verified: rollback,
                });
                if rollback {
                    parameters.clone_from(&verified_parameters);
                    cache_epoch += 1;
                    radius = (radius * cfg.contraction).max(cfg.minimum_radius);
                } else {
                    verified_parameters.clone_from(&parameters);
                    verified_bits = full_bits;
                }
            }
        }
        if !accepted_this_round || radius <= cfg.minimum_radius {
            break;
        }
    }
    let final_token = EvaluationToken {
        cache_epoch,
        comparison_id,
        fixed_mesh_id: comparison_id,
    };
    let full_bits = evaluate(problem, &parameters, ScoreScope::FULL, final_token)?;
    Ok(OptimizationResult {
        parameters,
        full_bits,
        accepted_blocks,
        trace,
    })
}

/// Run multiple caller-provided deterministic initializations and return the
/// exact best result. Equal scores break on lexicographic parameter bits.
pub fn optimize_best_deterministic<P: TrustRegionProblem>(
    problem: &P,
    starts: Vec<Vec<f64>>,
    blocks: &[BlockSpec],
    cfg: TrustRegionConfig,
) -> Result<OptimizationResult, TrustRegionError> {
    let mut results = Vec::with_capacity(starts.len());
    for start in starts {
        results.push(optimize_trust_region(problem, start, blocks, cfg)?);
    }
    results
        .into_iter()
        .min_by(|a, b| {
            a.full_bits.total_cmp(&b.full_bits).then_with(|| {
                a.parameters
                    .iter()
                    .map(|v| v.to_bits())
                    .cmp(b.parameters.iter().map(|v| v.to_bits()))
            })
        })
        .ok_or(TrustRegionError::InvalidConfig)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Quadratic {
        calls: RefCell<Vec<(Vec<f64>, ScoreScope, EvaluationToken)>>,
    }

    impl TrustRegionProblem for Quadratic {
        fn exact_bits(
            &self,
            p: &[f64],
            scope: ScoreScope,
            token: EvaluationToken,
        ) -> Result<f64, String> {
            self.calls.borrow_mut().push((p.to_vec(), scope, token));
            Ok((p[0] - 1.0).powi(2) + (p[1] - 2.0).powi(2))
        }

        fn project(&self, p: &mut [f64], block: &BlockSpec) -> Result<(), String> {
            for &i in &block.parameter_indices {
                p[i] = p[i].clamp(-10.0, 10.0);
            }
            Ok(())
        }
    }

    fn config() -> TrustRegionConfig {
        TrustRegionConfig {
            initial_radius: 1.0,
            minimum_radius: 1e-4,
            expansion: 1.2,
            contraction: 0.5,
            finite_difference_step: 1e-5,
            min_bits_improvement: 1e-12,
            max_rounds: 6,
            max_backtracks: 8,
            full_check_every_accepted_blocks: 1,
        }
    }

    #[test]
    fn each_later_block_reads_the_current_parent() {
        let problem = Quadratic {
            calls: RefCell::new(Vec::new()),
        };
        let blocks = vec![
            BlockSpec {
                name: "x".into(),
                parameter_indices: vec![0],
                scales: vec![1.0],
                max_radius: 1.0,
                scope: ScoreScope::FULL,
            },
            BlockSpec {
                name: "y".into(),
                parameter_indices: vec![1],
                scales: vec![1.0],
                max_radius: 1.0,
                scope: ScoreScope::FULL,
            },
        ];
        let got = optimize_trust_region(&problem, vec![4.0, 5.0], &blocks, config()).unwrap();
        let accepted: Vec<_> = got
            .trace
            .iter()
            .filter(|r| r.accepted && !r.full_check)
            .collect();
        assert!(accepted.len() >= 2);
        assert!(accepted[1].parent_bits < accepted[0].parent_bits);
        assert!(got.full_bits < 18.0);
    }

    #[test]
    fn exact_parent_and_child_share_one_cache_token() {
        let problem = Quadratic {
            calls: RefCell::new(Vec::new()),
        };
        let block = BlockSpec {
            name: "both".into(),
            parameter_indices: vec![0, 1],
            scales: vec![1.0, 1.0],
            max_radius: 1.0,
            scope: ScoreScope {
                roi: Some(Rect {
                    x0: 1,
                    y0: 1,
                    x1: 4,
                    y1: 4,
                }),
                halo_px: 2,
                global: false,
            },
        };
        let got = optimize_trust_region(&problem, vec![4.0, 5.0], &[block], config()).unwrap();
        assert!(got.trace.iter().any(|row| !row.full_check && row.accepted));
        let calls = problem.calls.borrow();
        for row in got.trace.iter().filter(|row| !row.full_check) {
            let matching: Vec<_> = calls
                .iter()
                .filter(|(_, _, token)| token.comparison_id == row.comparison_id)
                .collect();
            assert_eq!(matching.len(), 2);
            assert_eq!(matching[0].2.cache_epoch, matching[1].2.cache_epoch);
        }
    }
}
