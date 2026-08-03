//! vice-evidence — Flat2 palette, formation, mixture and boundary evidence
//! (M4).
//!
//! Scope (spec v1.3 §9, §10, §13, §16.2, §22, §28 M4):
//!
//! - [`interior`]: interior confidence — which pixels may train a palette
//!   and which are the edge the palette is trying to explain (§9.1);
//! - [`palette`]: the several Flat2 palette/exterior hypotheses of §9.2,
//!   including the full-bleed reading that does NOT assume the border is
//!   the background, and bounded colour intervals for shapes with no
//!   reliable interior core;
//! - [`formation`]: the minimal GLOBAL formation family of §10.1/§16.2 —
//!   blend space x coverage filter x 8-bit quantization x exterior — and
//!   what identifies each of its members;
//! - [`mixture`]: the premultiplied Flat2 mixture of §10/§22, the evidence
//!   §10 asks to be kept beside it, and the §1.6 detection of an interior
//!   fill with a true constant `0 < alpha < 1`;
//! - [`boundary`]: the subpixel boundary observations of §13 —
//!   `BoundarySample` with its physical `ds` weight, normal, calibrated
//!   halfwidth and correlation length — with a malformed chain refused
//!   rather than repaired and a critical 2x2 reported rather than decided
//!   in silence;
//! - [`corridor`]: the calibrated normal interval of §13.1, derived from
//!   the quantization cell, the local residual and the frozen clean-bucket
//!   noise scale;
//! - [`analysis`]: the stage as a whole — hypotheses in, one typed outcome
//!   out (`supported` / `ambiguous` / `unsupported`), with the §1.6
//!   exclusion of semi-transparent interiors as an OUTCOME rather than a
//!   comment;
//! - [`support`]: the mechanism that keeps evidence from becoming a second
//!   pixel likelihood (§10.2).
//!
//! Everything here is EVIDENCE in the sense §10.2 fixes: it generates
//! hypotheses, prunes impossible ones, sizes trust regions, orders
//! proposals and diagnoses uncertainty. It is not, and cannot become, a
//! second term added to the final observation likelihood — see [`support`]
//! for the mechanism rather than the promise.
//!
//! No confidence number is produced anywhere in this crate. §1.5 binds
//! confidence to a frozen calibration on a held-out split with a sample-size
//! contract the project does not yet satisfy, and M4 is not the milestone
//! that changes that.

#![forbid(unsafe_code)]

pub mod analysis;
pub mod boundary;
pub mod corridor;
pub mod formation;
pub mod formation_m9;
pub mod gradient;
pub mod interior;
pub mod line_art;
pub mod mixture;
pub mod multicolor;
pub mod palette;
pub mod support;

pub use analysis::{
    analyze, analyze_full, analyze_full_for_filters, AnalysisConfig, AnalysisOutput, Flat2Analysis,
    Flat2Outcome, UnsupportedReason, ANALYSIS_CONFIG_V1, ANALYSIS_SCHEMA, MAX_RESIDUAL_P95_CODES,
};
pub use boundary::{
    observe_boundaries, BoundaryChain, BoundaryConfig, BoundaryObservation, BoundaryRefusal,
    BoundarySample, ChainStatus, BOUNDARY_CONFIG_V1,
};
pub use corridor::{
    corridor_at, sample_confidence, AlphaSigma, Corridor, CorridorConfig, CLEAN_BUCKET_SIGMA_CODES,
    CORRIDOR_CONFIG_V1, COVERAGE_LEVELS,
};
pub use formation::{
    blend_space_is_identifiable, enumerate as enumerate_formations, filter_is_identifiable,
    filter_penalty, for_palette, formation_id, resolved_fraction, transition_width_px, FAMILY_SIZE,
    KERNEL_PROFILES_V1, MIN_RESOLVED_FRACTION,
};
pub use formation_m9::{
    codec_residual_for_format, enumerate_m9, estimate_global_kernel,
    estimate_global_kernel_from_alpha, formation_m9_id, ExtendedFormationHypothesis,
    GlobalKernelEstimate, KernelCandidate, KernelEstimationError, M9KernelProfile,
    M9_FORMATION_SCHEMA, M9_GAUSSIAN_SIGMAS_PX, M9_KERNEL_PROFILES_V1,
};
pub use gradient::{
    propose_gradients, GradientEvidenceRefusal, GradientEvidenceReport, GradientProposal,
    GRADIENT_EVIDENCE_SCHEMA,
};
pub use interior::{interior_confidence, InteriorConfidence, InteriorConfig, INTERIOR_CONFIG_V1};
pub use line_art::{
    propose_line_art_strokes, LineArtEvidenceReport, LineArtProposal, LineArtRefusal,
    LINE_ART_EVIDENCE_SCHEMA,
};
pub use mixture::{
    infer_mixture, Flat2Evidence, MixtureConfig, MixtureRefusal, SemiTransparentInterior,
    MIXTURE_CONFIG_V1,
};
pub use multicolor::{
    propose_multicolor, MulticolorConfig, MulticolorHypothesis, MulticolorProposals,
    MulticolorRefusal, PaletteScore, MULTICOLOR_CONFIG_V1, MULTICOLOR_SCHEMA,
};
pub use palette::{
    conditioning, oracle_override, propose_flat2, BackgroundHypothesis, ColorHypothesis,
    Flat2Hypothesis, Flat2Kind, Flat2Proposals, PaletteConfig, PaletteRefusal, PALETTE_CONFIG_V1,
};
pub use support::{
    DoubleCounted, ObservationSource, ObservationSupport, SupportError, SurrogateRole,
    SurrogateScore, NOT_A_LIKELIHOOD,
};
