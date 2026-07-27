//! vice-topology — event-driven topology envelope (spec v1.3 §11, §23,
//! §28 M4.5).
//!
//! This commit carries the GENERATOR half of §11: the scalar fields, the
//! cubical complex with its complementary-connectivity signature, and the
//! critical events of the max and min trees as BATCHES over equal values.
//! The candidate envelope and its pruning tiers are the next commit; §33
//! splits M4.5 the same way, and the split is not cosmetic — the generator
//! is testable on its own and the envelope is not testable without it.
//!
//! - [`field`]: the five scalar fields of §11.1, per palette/formation
//!   hypothesis, with the conditional one refused BY NAME where its
//!   condition does not hold;
//! - [`cubical`]: the cubical complex, the complementary-connectivity
//!   signature of §5.3, and the three saddle readings of a critical 2x2;
//! - [`events`]: max-tree superlevel and min-tree sublevel critical events as
//!   batches over equal values, component/hole birth and death, bridge and
//!   gap events, persistence plateaus, and the levels they generate — each
//!   level carrying WHERE it came from.
//!
//! ## What this crate does NOT do, on purpose
//!
//! It does not choose. §11.3 is "candidate envelope, not an early winner";
//! §32 rule 14 forbids a topology winner from a proxy; §36 makes "the
//! GT-equivalent topology falls out of the envelope through proxy or budget
//! pruning" a stop condition. Nothing here returns one hypothesis, and the
//! §28 M4.5 gate measures RECALL rather than accuracy of choice.
//!
//! It also does not certify an embedding. Every signature here is
//! combinatorial — a statement about a digital labelling under one
//! convention. REVIEW_M1 M1-N5 is why that is worth saying: a scene there
//! satisfied every combinatorial invariant and still had no planar embedding.
//! The certificate comes with the DCEL (M5).
//!
//! ## Determinism
//!
//! Every operator has a fixed iteration count, every union-find has a
//! canonical survivor, every edge list is sorted before it is consumed and
//! every equal-valued set of pixels is one batch. §11.2 requires the last of
//! those in so many words; the rest are what make the artifact of §5.5 Tier A
//! a comparison rather than a judgement call.

#![forbid(unsafe_code)]

pub mod cubical;
pub mod events;
pub mod field;

pub use cubical::{
    critical_cells, residual_critical_cells, signature, threshold, Labelling, SaddleResolution,
    TopologySignature,
};
pub use events::{
    batch_critical_events, candidate_levels, CandidateLevel, CriticalEvent, EventKind, EventTrees,
    LevelConfig, LevelOrigin, Plateau, LEVEL_CONFIG_V1, LEVEL_QUANTA,
};
pub use field::{
    build_fields, forward_kernel, CoverageObservation, FieldConfig, FieldKind, FieldRefusal,
    FieldSet, FIELD_CONFIG_V1,
};

pub const TOPOLOGY_SCHEMA: &str = "vice-classic/topology-envelope/v1";
