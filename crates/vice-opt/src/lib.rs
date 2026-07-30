//! M7 posterior and search engine.
//!
//! This crate owns four production decisions that must agree:
//! correlation-aware scoring of the full rendered observation, supported
//! search-mass accounting, atomic compound scene transactions, and exact
//! current-parent trust-region acceptance. Proposal evidence is deliberately
//! absent from [`PriorCodeLengths`]: pixels enter the final objective exactly
//! once, through [`score_full_resolution`].

#![forbid(unsafe_code)]

pub mod likelihood;
pub mod posterior;
pub mod search;
pub mod transaction;
pub mod trust_region;
pub mod universe;

pub use likelihood::{
    score_full_resolution, score_serialized_full_resolution, BlockLikelihoodConfig,
    LikelihoodDiagnostics, LikelihoodError, PredictionSource, PriorCodeLengths, ResidualModelId,
    ScoreBreakdown, ScoreOwnership,
};
pub use posterior::{
    posterior_with_search_mass, BoundValue, DeliveryPosterior, ModelIdentity, PosteriorError,
    ScoredHypothesis, SearchMassCertificate, SearchMassInput, UnexploredMassBound,
    UnexploredMassInput,
};
pub use search::{
    select_diverse_beam, BeamCandidate, BeamConfig, BeamError, BeamSelection, BudgetLedger,
    SearchBudget,
};
pub use transaction::{
    apply_compound_transaction, CompoundTransaction, SceneMutation, TransactionError,
    TransactionKind,
};
pub use trust_region::{
    optimize_best_deterministic, optimize_trust_region, BlockSpec, EvaluationToken,
    OptimizationResult, OptimizationTraceRow, Rect, ScoreScope, TrustRegionConfig,
    TrustRegionError, TrustRegionProblem,
};
pub use universe::{
    model_universe_hash, Admissibility, BoundStatus, SupportedModelUniverseV1,
    MODEL_UNIVERSE_SCHEMA,
};
