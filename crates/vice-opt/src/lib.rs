//! M7 posterior and search engine.
//!
//! This crate owns four production decisions that must agree:
//! correlation-aware scoring of the full rendered observation, supported
//! search-mass accounting, atomic compound scene transactions, and exact
//! current-parent trust-region acceptance. Proposal evidence is deliberately
//! absent from [`PriorCodeLengths`]: pixels enter the final objective exactly
//! once, through [`score_full_resolution`].

#![forbid(unsafe_code)]

pub mod codec;
pub mod likelihood;
pub mod multiregion;
pub mod posterior;
pub mod search;
pub mod transaction;
pub mod trust_region;
pub mod universe;

pub use codec::{
    calibrated_codec_likelihood_config, measure_codec_residual, score_codec_residual,
    CodecLikelihoodConfig, CodecLikelihoodError, CodecLikelihoodReport,
    CodecResidualCalibrationStats, CLEAN_CODEC_LIKELIHOOD_CONFIG_V1, CODEC_LIKELIHOOD_SCHEMA,
    JPEG_CODEC_LIKELIHOOD_CONFIG_V1, WEBP_CODEC_LIKELIHOOD_CONFIG_V1,
};
pub use likelihood::{
    score_full_resolution, score_full_resolution_scope, score_full_resolution_scope_with_tensor,
    score_full_resolution_scope_with_workspace, score_serialized_full_resolution,
    score_serialized_full_resolution_scope, score_serialized_full_resolution_scope_with_tensor,
    score_serialized_full_resolution_scope_with_workspace, BlockLikelihoodConfig,
    LikelihoodDiagnostics, LikelihoodError, LikelihoodWorkspace, PredictionSource,
    PriorCodeLengths, ResidualModelId, ScoreBreakdown, ScoreOwnership,
};
pub use multiregion::{
    certify_exact_roi_transaction, fit_opaque_face_paints, fit_opaque_face_paints_weighted,
    run_exact_alternation, score_fixed_opaque_face_paints, AlternationCandidate, AlternationConfig,
    AlternationError, AlternationResult, AlternationTraceRow, ExactRoiCertificateError,
    ExactRoiTransactionCertificate, FacePaintFit, MultiregionPaintConfig, PaintFit, PaintFitError,
    M8_ROI_CERTIFICATE_SCHEMA, MULTIREGION_PAINT_CONFIG_V1,
};
pub use posterior::{
    finite_class_entropy_upper_bound, posterior_with_search_mass, BoundValue, ClassPosterior,
    DeliveryPosterior, ModelIdentity, PosteriorError, ScoredHypothesis, SearchMassCertificate,
    SearchMassInput, UnexploredMassBound, UnexploredMassInput,
};
pub use search::{
    select_diverse_beam, BeamCandidate, BeamConfig, BeamError, BeamSelection, BudgetLedger,
    SearchBudget,
};
pub use transaction::{
    apply_compound_transaction, apply_compound_transaction_traced, CompoundTransaction,
    SceneMutation, TransactionApplication, TransactionError, TransactionKind,
};
pub use trust_region::{
    optimize_best_deterministic, optimize_trust_region, BlockSpec, EvaluationToken,
    OptimizationBlockPlan, OptimizationResult, OptimizationTraceRow, Rect, ScoreScope,
    TrustRegionConfig, TrustRegionError, TrustRegionProblem,
};
pub use universe::{
    model_universe_hash, Admissibility, BoundStatus, SupportedModelUniverseV1,
    MODEL_UNIVERSE_SCHEMA,
};
