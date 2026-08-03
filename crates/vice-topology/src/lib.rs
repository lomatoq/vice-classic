//! vice-topology — event-driven topology envelope (spec v1.3 §11, §23,
//! §28 M4.5).
//!
//! Scope, and the boundary of it:
//!
//! - [`field`]: the five scalar fields of §11.1, per palette/formation
//!   hypothesis, with the conditional one refused BY NAME where its
//!   condition does not hold;
//! - [`cubical`]: the cubical complex, the complementary-connectivity
//!   signature of §5.3, and the three saddle readings of a critical 2x2;
//! - [`events`]: max-tree superlevel and min-tree sublevel critical events as
//!   BATCHES over equal values, component/hole birth and death, bridge and
//!   gap events, persistence plateaus, and the levels they generate — each
//!   level carrying WHERE it came from;
//! - [`hypothesis`]: one candidate with everything §11.3 lists, including two
//!   cost bounds each of which names the set it bounds;
//! - [`envelope`]: the three pruning tiers, with diversity quotas that a
//!   budget may not cut below and an approximation-risk record that says what
//!   was dropped;
//! - [`continuation`]: the two executable steps of §11.4 and five typed
//!   refusals naming the capability and the owner.
//!
//! ## What this crate does NOT do, on purpose
//!
//! It does not choose. §11.3 is "candidate envelope, not an early winner";
//! §32 rule 14 forbids a topology winner from a proxy; §36 makes "the
//! GT-equivalent topology falls out of the envelope through proxy or budget
//! pruning" a stop condition. Accordingly [`propose`] returns a SET, the
//! §28 M4.5 gate measures RECALL rather than accuracy of choice, and there is
//! no function here that returns one hypothesis. Selection arrives in M6/M7
//! with a posterior to select against.
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

pub mod continuation;
pub mod cubical;
pub mod dcel;
pub mod edit;
pub mod envelope;
pub mod events;
pub mod field;
pub mod hypothesis;
pub mod multidcel;
pub mod partition_script;
pub mod rag;
pub mod rag_transaction;

use std::collections::BTreeSet;

use serde::Serialize;
use vice_ir::ComplementaryConnectivity;

pub use continuation::{
    plan_continuations, ContinuationConfig, ContinuationPlan, ContinuationStep, TopologyEdit,
    CONTINUATION_CONFIG_V1,
};
pub use cubical::{
    critical_cells, residual_critical_cells, signature, threshold, Labelling, SaddleResolution,
    TopologySignature,
};
pub use dcel::{
    apply, audit, audit_every_labelling, measure_audit_resolving_power, AuditReport, Boundary,
    Dcel, Edit, ExhaustiveReport, Face, FaceId, FacePair, Fixture, HalfEdgeId, InvariantViolation,
    IsotopyRefusal, Outcome, ResolvingPower, Roi, TopologyCertificate, TransactionRefusal,
    TransactionReport, TxConfig, TX_CONFIG_V1,
};
pub use dcel::{
    face_map_agrees, face_map_from_boundaries, structural_fixtures, with_a_distant_witness,
    FaceMapDisagreement, STRUCTURAL_SIZES_PX,
};
pub use envelope::{
    prune, ApproximationRisk, Envelope, EnvelopeConfig, LostMassEstimate, PruningRecord,
    ENVELOPE_CONFIG_V1,
};
pub use events::{
    batch_critical_events, candidate_levels, CandidateLevel, CriticalEvent, EventKind, EventTrees,
    LevelConfig, LevelOrigin, Plateau, LEVEL_CONFIG_V1, LEVEL_QUANTA,
};
pub use field::{
    build_fields, forward_kernel, CoverageObservation, FieldConfig, FieldKind, FieldRefusal,
    FieldSet, FIELD_CONFIG_V1,
};
pub use hypothesis::{CostBounds, Provenance, TopologyHypothesis, NOT_A_LIKELIHOOD};
pub use multidcel::{
    MultiBoundary, MultiBoundaryId, MultiDcelError, MultiFace, MultiHalfEdgeId, MultiJunction,
    MulticolorDcel, MULTICOLOR_DCEL_SCHEMA,
};
pub use partition_script::{
    apply_partition_edit_script, PartitionEditScript, PartitionScriptEdit, PartitionScriptError,
    PartitionScriptOutcome, PartitionScriptStepLedger, PARTITION_EDIT_SCRIPT_SCHEMA,
};
pub use rag::{
    RagEdge, RagError, RagNode, RegionAdjacencyGraph, RegionId, RegionLabelling, RAG_SCHEMA,
};
pub use rag_transaction::{
    apply_rag_transaction, QuantizedPaint, RagEdit, RagTransactionError, RagTransactionLedger,
    RagTransactionOutcome, RegionScene,
};

pub const TOPOLOGY_SCHEMA: &str = "vice-classic/topology-envelope/v1";

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct TopologyConfig {
    pub schema: &'static str,
    pub field: FieldConfig,
    pub level: LevelConfig,
    pub envelope: EnvelopeConfig,
    pub continuation: ContinuationConfig,
}

pub const TOPOLOGY_CONFIG_V1: TopologyConfig = TopologyConfig {
    schema: TOPOLOGY_SCHEMA,
    field: FIELD_CONFIG_V1,
    level: LEVEL_CONFIG_V1,
    envelope: ENVELOPE_CONFIG_V1,
    continuation: CONTINUATION_CONFIG_V1,
};

/// What one proposal run saw, beside the envelope itself.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Proposal {
    pub envelope: Envelope,
    /// §11.1 fields that were not built, with the typed reason, per
    /// observation.
    pub field_refusals: Vec<FieldRefusalRecord>,
    pub fields_built: usize,
    pub events_seen: usize,
    /// Levels whose only justification is a fixed smoke probe.
    pub fixed_only_levels: usize,
    pub event_driven_levels: usize,
    /// Batches of equal-valued pixels, and the largest of them. §11.2's
    /// plateau rule is measurable rather than asserted because of these.
    pub tie_batches: u32,
    pub largest_batch_pixels: u32,
    pub saddle_alternatives_generated: usize,
    pub degenerate_levels_skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldRefusalRecord {
    pub palette_id: String,
    pub formation_id: String,
    pub field: FieldKind,
    pub refusal: FieldRefusal,
}

/// §23 `propose_topology_envelope`, over one or more palette/formation
/// hypotheses.
///
/// The loop is the pseudocode's: fields, then complementary-connectivity
/// arms, then the levels the events generate, then the saddle readings. Two
/// departures from a literal transcription, both of them measurable rather
/// than stylistic:
///
/// - saddle alternatives are generated only where a critical 2x2 EXISTS.
///   Where there is none the three readings are the same labelling, and
///   emitting them would inflate the candidate count with exact duplicates
///   the envelope would then report as pruning work;
/// - the signature is computed before the cost bounds, and a labelling whose
///   digest has already been seen merges its provenance into the survivor
///   instead of being fully built and then discarded. The envelope's
///   duplicate tier still exists and still counts, because a caller may
///   propose candidates this function did not.
pub fn propose(observations: &[CoverageObservation<'_>], cfg: &TopologyConfig) -> Proposal {
    let mut candidates: Vec<TopologyHypothesis> = Vec::new();
    let mut by_digest: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    let mut refusals = Vec::new();
    let mut fields_built = 0usize;
    let mut events_seen = 0usize;
    let mut fixed_only_levels = 0usize;
    let mut event_driven_levels = 0usize;
    let mut tie_batches = 0u32;
    let mut largest_batch = 0u32;
    let mut saddle_alternatives = 0usize;
    let mut degenerate_skipped = 0usize;

    for obs in observations {
        let (w, h) = (obs.width_px, obs.height_px);
        let set = build_fields(obs, &cfg.field);
        for (kind, refusal) in set.refused {
            refusals.push(FieldRefusalRecord {
                palette_id: obs.palette_id.clone(),
                formation_id: obs.formation_id.clone(),
                field: kind,
                refusal,
            });
        }
        let kernel = forward_kernel(obs.filter);
        fields_built += set.fields.len();
        for fh in &set.fields {
            for conn in ComplementaryConnectivity::arms() {
                let trees = batch_critical_events(&fh.values, w, h, conn);
                events_seen += trees.events.len();
                tie_batches = tie_batches.max(trees.tie_batches);
                largest_batch = largest_batch.max(trees.largest_batch_pixels);
                for level in candidate_levels(&trees, &cfg.level) {
                    if level.is_event_driven() {
                        event_driven_levels += 1;
                    } else {
                        fixed_only_levels += 1;
                    }
                    let origins: Vec<LevelOrigin> = level.origins.iter().copied().collect();
                    let base =
                        threshold(&fh.values, w, h, level.value, SaddleResolution::Thresholded);
                    let has_saddle = !critical_cells(&base).is_empty();
                    let readings: &[SaddleResolution] = if has_saddle {
                        SaddleResolution::ALL
                    } else {
                        &[SaddleResolution::Thresholded]
                    };
                    if has_saddle {
                        saddle_alternatives += readings.len() - 1;
                    }
                    for saddle in readings {
                        let labelling = if *saddle == SaddleResolution::Thresholded {
                            base.clone()
                        } else {
                            threshold(&fh.values, w, h, level.value, *saddle)
                        };
                        // §23 `minimum_evidence_feasible`: a labelling with
                        // no interface describes no boundary. Counted, not
                        // dropped in silence, and the envelope's invalid
                        // tier still exists for candidates a caller
                        // proposes.
                        let inside = labelling.count_inside();
                        if inside == 0 || inside == w * h {
                            degenerate_skipped += 1;
                            continue;
                        }
                        let sig = signature(&labelling, conn);
                        if let Some(i) = by_digest.get(&sig.digest).copied() {
                            merge_provenance(&mut candidates[i], &origins);
                            continue;
                        }
                        let h_new = hypothesis::from_events(
                            labelling,
                            &hypothesis::CandidateContext {
                                conn,
                                alpha: obs.alpha,
                                level: level.value,
                                level_origins: &origins,
                                persistence: level.persistence,
                                kernel: kernel.as_ref(),
                                palette_id: &obs.palette_id,
                                formation_id: &obs.formation_id,
                                field: fh.kind,
                                saddle: *saddle,
                            },
                        );
                        by_digest.insert(sig.digest, candidates.len());
                        candidates.push(h_new);
                    }
                }
            }
        }
    }

    Proposal {
        envelope: prune(candidates, &cfg.envelope),
        field_refusals: refusals,
        fields_built,
        events_seen,
        fixed_only_levels,
        event_driven_levels,
        tie_batches,
        largest_batch_pixels: largest_batch,
        saddle_alternatives_generated: saddle_alternatives,
        degenerate_levels_skipped: degenerate_skipped,
    }
}

/// A labelling reached a second way keeps BOTH ways on the survivor.
///
/// It cannot be reached under the OTHER connectivity arm, and that is by
/// construction rather than by accident: the signature digest includes the
/// foreground convention, so the same pixels under 4- and under 8-connectivity
/// are two different candidates. M45-N7 found the flag that claimed otherwise
/// set on zero candidates over the whole corpus; it is gone, and what the
/// clause-1 relaxation actually needs — that the envelope carries BOTH arms —
/// is counted in [`Envelope::candidates_by_arm`] instead.
///
/// Without this the "no magic-threshold-only" measurement would undercount:
/// a candidate first reached through the fixed probe and then through an
/// event would still be marked as fixed-only, and the ablation would report
/// a loss that does not exist.
fn merge_provenance(h: &mut TopologyHypothesis, origins: &[LevelOrigin]) {
    let event_driven = origins.iter().any(|o| o.is_event_driven());
    if event_driven && h.ambiguity.level_from_fixed_probe_only {
        h.ambiguity.level_from_fixed_probe_only = false;
        h.provenance.level_from_fixed_probe_only = false;
    }
    let mut names: BTreeSet<&'static str> = h.provenance.level_origins.iter().copied().collect();
    for o in origins {
        names.insert(o.as_str());
    }
    h.provenance.level_origins = names.into_iter().collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_ir::PixelFilter;

    fn obs<'a>(alpha: &'a [f64], w: usize, h: usize) -> CoverageObservation<'a> {
        CoverageObservation {
            palette_id: "pal".into(),
            formation_id: "form".into(),
            filter: PixelFilter::Box,
            filter_identifiable: true,
            alpha,
            width_px: w,
            height_px: h,
        }
    }

    /// A blurred ring: the envelope must CONTAIN the one-component,
    /// one-hole reading. This is the recall question the §28 M4.5 gate asks,
    /// on one fixture, in one crate.
    #[test]
    fn the_envelope_contains_the_ring_reading_of_a_ring() {
        let (w, h) = (48usize, 48usize);
        let alpha: Vec<f64> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
                let d = ((x - 24.0).powi(2) + (y - 24.0).powi(2)).sqrt();
                let outer = (16.5 - d).clamp(0.0, 1.0);
                let inner = (d - 7.5).clamp(0.0, 1.0);
                outer.min(inner)
            })
            .collect();
        let p = propose(&[obs(&alpha, w, h)], &TOPOLOGY_CONFIG_V1);
        assert!(
            p.envelope.contains_signature(1, 1),
            "the ring reading is missing; classes present: {:?}",
            p.envelope.signature_classes()
        );
        assert!(p.envelope.hypotheses.len() > 1, "an envelope, not a winner");
        assert!(p.events_seen > 0);
    }

    /// An ambiguous dumbbell keeps BOTH readings: bridged and split. §11.3
    /// and the §28 M4.5 clause about ambiguous fixtures are the same
    /// sentence, and this is it on one fixture.
    #[test]
    fn an_ambiguous_neck_keeps_both_the_bridged_and_the_split_reading() {
        let (w, h) = (33usize, 15usize);
        let mut alpha = vec![0.0f64; w * h];
        for y in 4..11 {
            for x in 2..13 {
                alpha[y * w + x] = 1.0;
            }
            for x in 20..31 {
                alpha[y * w + x] = 1.0;
            }
        }
        // A neck at exactly the level where the answer is a genuine tie.
        for x in 13..20 {
            for y in 6..9 {
                alpha[y * w + x] = 0.5;
            }
        }
        let p = propose(&[obs(&alpha, w, h)], &TOPOLOGY_CONFIG_V1);
        let classes = p.envelope.signature_classes();
        assert!(
            p.envelope.contains_signature(1, 0),
            "the bridged reading is missing: {classes:?}"
        );
        assert!(
            p.envelope.contains_signature(2, 0),
            "the split reading is missing: {classes:?}"
        );
    }

    /// Removing the fixed smoke probes does not remove the answer.
    ///
    /// This is the mechanism behind "no magic-threshold-only architecture":
    /// the candidate that carries the correct reading is reachable without
    /// any fixed level, and the number of candidates that are fixed-only is
    /// published.
    #[test]
    fn the_answer_survives_the_removal_of_every_fixed_probe() {
        let (w, h) = (40usize, 40usize);
        let alpha: Vec<f64> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
                let d = ((x - 20.0).powi(2) + (y - 20.0).powi(2)).sqrt();
                (14.5 - d).clamp(0.0, 1.0)
            })
            .collect();
        let p = propose(&[obs(&alpha, w, h)], &TOPOLOGY_CONFIG_V1);
        assert!(p.envelope.contains_signature(1, 0));
        let without_fixed = p
            .envelope
            .hypotheses
            .iter()
            .filter(|c| !c.ambiguity.level_from_fixed_probe_only)
            .any(|c| c.signature.components == 1 && c.signature.holes == 0);
        assert!(
            without_fixed,
            "the disk reading exists only through a fixed probe, which is the architecture the \
             gate forbids"
        );
        assert!(
            p.event_driven_levels > p.fixed_only_levels,
            "event-driven levels {} must outnumber fixed-only {}",
            p.event_driven_levels,
            p.fixed_only_levels
        );
    }

    /// Same bytes, same envelope, twice.
    #[test]
    fn the_proposal_is_deterministic() {
        let (w, h) = (32usize, 32usize);
        let alpha: Vec<f64> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f64, (i / w) as f64);
                ((x * 0.19).sin() * (y * 0.23).cos() * 0.5 + 0.5).clamp(0.0, 1.0)
            })
            .collect();
        let a = propose(&[obs(&alpha, w, h)], &TOPOLOGY_CONFIG_V1);
        let b = propose(&[obs(&alpha, w, h)], &TOPOLOGY_CONFIG_V1);
        assert_eq!(a, b);
    }

    /// Nothing in this crate returns one hypothesis, and the envelope of a
    /// genuinely ambiguous image is not of size one.
    #[test]
    fn the_stage_returns_an_envelope_and_never_a_winner() {
        let (w, h) = (24usize, 24usize);
        let alpha: Vec<f64> = (0..w * h)
            .map(|i| {
                let x = (i % w) as f64;
                (x / 24.0).clamp(0.0, 1.0)
            })
            .collect();
        let p = propose(&[obs(&alpha, w, h)], &TOPOLOGY_CONFIG_V1);
        assert!(
            p.envelope.hypotheses.len() > 1,
            "a linear ramp has no single right level, and the stage must not pick one"
        );
        // And the pruning record accounts for every proposal.
        let r = &p.envelope.pruning;
        assert_eq!(
            r.proposed,
            r.invalid_removed
                + r.duplicates_removed
                + r.dominated_removed
                + r.budget_removed
                + p.envelope.hypotheses.len(),
            "every proposed candidate is either kept or removed by a NAMED tier"
        );
    }
}
