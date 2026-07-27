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

pub mod interior;
pub mod palette;
pub mod support;

pub use interior::{interior_confidence, InteriorConfidence, InteriorConfig, INTERIOR_CONFIG_V1};
pub use palette::{
    conditioning, oracle_override, propose_flat2, BackgroundHypothesis, ColorHypothesis,
    Flat2Hypothesis, Flat2Kind, Flat2Proposals, PaletteConfig, PaletteRefusal, PALETTE_CONFIG_V1,
};
pub use support::{
    DoubleCounted, ObservationSupport, SurrogateRole, SurrogateScore, NOT_A_LIKELIHOOD,
};
