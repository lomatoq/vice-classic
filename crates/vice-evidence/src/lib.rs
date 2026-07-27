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

pub mod formation;
pub mod interior;
pub mod mixture;
pub mod palette;
pub mod support;

pub use formation::{
    blend_space_is_identifiable, enumerate as enumerate_formations, filter_penalty, for_palette,
    formation_id, transition_width_px, FAMILY_SIZE, KERNEL_PROFILES_V1,
};
pub use interior::{interior_confidence, InteriorConfidence, InteriorConfig, INTERIOR_CONFIG_V1};
pub use mixture::{
    infer_mixture, Flat2Evidence, MixtureConfig, MixtureRefusal, SemiTransparentInterior,
    MIXTURE_CONFIG_V1,
};
pub use palette::{
    conditioning, oracle_override, propose_flat2, BackgroundHypothesis, ColorHypothesis,
    Flat2Hypothesis, Flat2Kind, Flat2Proposals, PaletteConfig, PaletteRefusal, PALETTE_CONFIG_V1,
};
pub use support::{
    DoubleCounted, ObservationSupport, SurrogateRole, SurrogateScore, NOT_A_LIKELIHOOD,
};
