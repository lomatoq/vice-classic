//! Dual/primal continuation (spec §11.4): the part that is executable
//! without a DCEL, and a TYPED REFUSAL for the part that is not.
//!
//! §11.4 gives the compound operation:
//!
//! ```text
//! topology edit
//! -> rebuild affected DCEL
//! -> refit affected representation
//! -> refit paints
//! -> exact ROI posterior with halo
//! -> local certificates
//! -> accept into k-best envelope or rollback
//! ```
//!
//! Two of those seven are computable from what this milestone has, and five
//! are not. §27.6 forbids fake placeholder arms and §32 rule 7 forbids
//! placeholder APIs for later milestones, so the five are neither stubbed
//! nor silently dropped: each is a [`ContinuationStep::NotYetApplicable`]
//! naming the capability it needs and the milestone that owns it — the same
//! shape the M3.5 oracle harness uses for its unavailable arms, and the same
//! rule for choosing the owner (the LATEST of the missing milestones, i.e.
//! the one that actually gates).
//!
//! What IS computed, from data and not from a promise:
//!
//! - the **topology edit** itself. Two envelope members whose signatures
//!   differ by exactly one component or one hole are one edit apart, and
//!   which edit it is follows from the direction of the difference. The
//!   level between them and the persistence of the event that separates them
//!   are the numbers M5 will need to decide whether the edit is worth
//!   attempting;
//! - the **ROI with its halo**: the bounding box of the symmetric difference
//!   of the two labellings, grown by the halo. That is the region a local
//!   refit would have to touch, and it is a fact about the two labellings.
//!
//! An unexplained refusal must not be constructible: [`not_yet_applicable`]
//! panics on an empty capability list, pinned by a `#[should_panic]` test.
//! That is REVIEW_M3_5 M35-N5 applied to the constructor this milestone adds
//! rather than only to the one where the reviewer found it.

use serde::Serialize;

use crate::envelope::Envelope;
use crate::hypothesis::TopologyHypothesis;

/// What one edit does to the topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EditKind {
    /// One fewer component: two blobs became one. A bridge closed.
    BridgeClose,
    /// One more component: one blob became two. A gap opened.
    GapOpen,
    /// One more hole.
    HoleOpen,
    /// One fewer hole.
    HoleFill,
}

impl EditKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EditKind::BridgeClose => "bridge_close",
            EditKind::GapOpen => "gap_open",
            EditKind::HoleOpen => "hole_open",
            EditKind::HoleFill => "hole_fill",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Bbox {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl Bbox {
    pub fn area_px(&self) -> u64 {
        u64::from(self.x1.saturating_sub(self.x0)) * u64::from(self.y1.saturating_sub(self.y0))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopologyEdit {
    pub kind: EditKind,
    pub from_digest: String,
    pub to_digest: String,
    pub from_components: u32,
    pub from_holes: u32,
    pub to_components: u32,
    pub to_holes: u32,
    /// Level halfway between the two candidates' own levels: where the edit
    /// happens if the two came from the same field.
    pub level_between: f64,
    /// The larger of the two candidates' justifying persistences: how much
    /// evidence stands behind the distinction this edit crosses.
    pub persistence: f64,
    /// Pixels whose label differs between the two candidates.
    pub changed_px: u64,
    /// The region a local refit would have to touch, with the halo already
    /// added.
    pub roi_with_halo: Bbox,
    pub halo_px: u32,
}

/// One step of the §11.4 compound operation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ContinuationStep {
    Executed {
        what: &'static str,
        detail: String,
    },
    /// The step has two halves and only one of them exists yet. Written as
    /// its own variant rather than as an `Executed` with a caveat in the
    /// prose, because "the ROI is computed, the posterior over it is not" is
    /// exactly the kind of half-truth a reader turns into a whole one.
    PartiallyExecuted {
        what: &'static str,
        executed: String,
        missing: Vec<&'static str>,
        owner_milestone: &'static str,
        reason: &'static str,
    },
    NotYetApplicable {
        what: &'static str,
        missing: Vec<&'static str>,
        owner_milestone: &'static str,
        reason: &'static str,
    },
}

impl ContinuationStep {
    pub fn is_executed(&self) -> bool {
        matches!(self, ContinuationStep::Executed { .. })
    }
    pub fn is_partial(&self) -> bool {
        matches!(self, ContinuationStep::PartiallyExecuted { .. })
    }
    /// Every capability this step is still waiting on.
    pub fn missing(&self) -> &[&'static str] {
        match self {
            ContinuationStep::Executed { .. } => &[],
            ContinuationStep::PartiallyExecuted { missing, .. }
            | ContinuationStep::NotYetApplicable { missing, .. } => missing,
        }
    }
    pub fn owner(&self) -> Option<&'static str> {
        match self {
            ContinuationStep::Executed { .. } => None,
            ContinuationStep::PartiallyExecuted {
                owner_milestone, ..
            }
            | ContinuationStep::NotYetApplicable {
                owner_milestone, ..
            } => Some(*owner_milestone),
        }
    }
}

/// The only way to build a refusal, and it refuses to build an unexplained
/// one.
///
/// # Panics
///
/// When `missing` is empty. A `not_yet_applicable` that does not say what is
/// missing is exactly the "fake placeholder arm" §27.6 forbids, and the
/// cheapest way to make it unconstructible is to make constructing it a bug.
pub fn not_yet_applicable(
    what: &'static str,
    missing: &[&'static str],
    owner_milestone: &'static str,
    reason: &'static str,
) -> ContinuationStep {
    assert!(
        !missing.is_empty(),
        "a not_yet_applicable step must name what is missing: an unexplained refusal is a \
         placeholder with better manners (spec 27.6)"
    );
    ContinuationStep::NotYetApplicable {
        what,
        missing: missing.to_vec(),
        owner_milestone,
        reason,
    }
}

/// The other constructor of a refusal, for a step that half-exists.
///
/// # Panics
///
/// When `missing` is empty — the same rule as [`not_yet_applicable`], applied
/// to BOTH constructors rather than to the one where it was first needed.
/// REVIEW_M3_5 M35-N5 was exactly a project that guarded one of two
/// constructors and called the class closed.
pub fn partially_executed(
    what: &'static str,
    executed: String,
    missing: &[&'static str],
    owner_milestone: &'static str,
    reason: &'static str,
) -> ContinuationStep {
    assert!(
        !missing.is_empty(),
        "a partially executed step must name what is missing: an unexplained refusal is a \
         placeholder with better manners (spec 27.6)"
    );
    ContinuationStep::PartiallyExecuted {
        what,
        executed,
        missing: missing.to_vec(),
        owner_milestone,
        reason,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ContinuationPlan {
    pub edit: TopologyEdit,
    pub steps: Vec<ContinuationStep>,
}

impl ContinuationPlan {
    pub fn executed_steps(&self) -> usize {
        self.steps.iter().filter(|s| s.is_executed()).count()
    }
    pub fn partial_steps(&self) -> usize {
        self.steps.iter().filter(|s| s.is_partial()).count()
    }
    pub fn refused_steps(&self) -> usize {
        self.steps.len() - self.executed_steps() - self.partial_steps()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ContinuationConfig {
    /// Halo added around the changed region, in px. §11.4 wants the ROI
    /// posterior taken WITH a halo because a local edit changes coverage
    /// outside its own footprint through the formation kernel.
    pub halo_px: u32,
    /// Plans emitted per envelope, ordered by persistence.
    pub max_plans: usize,
}

pub const CONTINUATION_CONFIG_V1: ContinuationConfig = ContinuationConfig {
    halo_px: 3,
    max_plans: 8,
};

fn edit_kind(a: &TopologyHypothesis, b: &TopologyHypothesis) -> Option<EditKind> {
    let (dc, dh) = (
        b.signature.components as i64 - a.signature.components as i64,
        b.signature.holes as i64 - a.signature.holes as i64,
    );
    match (dc, dh) {
        (-1, 0) => Some(EditKind::BridgeClose),
        (1, 0) => Some(EditKind::GapOpen),
        (0, 1) => Some(EditKind::HoleOpen),
        (0, -1) => Some(EditKind::HoleFill),
        _ => None,
    }
}

fn difference(a: &TopologyHypothesis, b: &TopologyHypothesis, halo: u32) -> Option<(u64, Bbox)> {
    let (w, h) = (a.labelling.width_px(), a.labelling.height_px());
    if w != b.labelling.width_px() || h != b.labelling.height_px() {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
    let mut n = 0u64;
    for (i, (p, q)) in a
        .labelling
        .inside()
        .iter()
        .zip(b.labelling.inside())
        .enumerate()
    {
        if p == q {
            continue;
        }
        n += 1;
        let (x, y) = (i % w, i / w);
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + 1);
        y1 = y1.max(y + 1);
    }
    if n == 0 {
        return None;
    }
    let halo = halo as usize;
    Some((
        n,
        Bbox {
            x0: x0.saturating_sub(halo) as u32,
            y0: y0.saturating_sub(halo) as u32,
            x1: (x1 + halo).min(w) as u32,
            y1: (y1 + halo).min(h) as u32,
        },
    ))
}

/// The seven steps of §11.4, two computed and five refused by name.
fn steps_for(edit: &TopologyEdit) -> Vec<ContinuationStep> {
    vec![
        ContinuationStep::Executed {
            what: "topology edit",
            detail: format!(
                "{} takes ({}, {}) to ({}, {}) across {} px, at level {:.4} with persistence \
                 {:.4}",
                edit.kind.as_str(),
                edit.from_components,
                edit.from_holes,
                edit.to_components,
                edit.to_holes,
                edit.changed_px,
                edit.level_between,
                edit.persistence
            ),
        },
        not_yet_applicable(
            "rebuild affected DCEL",
            &["shared_dcel"],
            "M5",
            "spec 12 builds the DCEL/cubical arrangement; M4.5 has labellings and signatures, \
             which are not a half-edge structure and cannot be made into one by naming a type",
        ),
        not_yet_applicable(
            "refit affected representation",
            &["typed_grammar", "k_best_dp"],
            "M6",
            "spec 14 fits typed spans; there is no fitter in the tree and a topology edit has \
             nothing to refit without one",
        ),
        not_yet_applicable(
            "refit paints",
            &["shared_dcel", "face_paint_refit"],
            "M6",
            "paints are per FACE, and faces are a DCEL notion; the Flat2 palette of M4 is per \
             IMAGE",
        ),
        partially_executed(
            "exact ROI posterior with halo",
            format!(
                "the REGION is computed: the changed area grown by {} px is [{}, {}) x [{}, {}), \
                 {} px",
                edit.halo_px,
                edit.roi_with_halo.x0,
                edit.roi_with_halo.x1,
                edit.roi_with_halo.y0,
                edit.roi_with_halo.y1,
                edit.roi_with_halo.area_px()
            ),
            &["exact_observation_likelihood", "roi_posterior"],
            "M7",
            "spec 17 defines the posterior and spec 18.3 the transactional acceptance that uses \
             it; M4.5 has surrogate cost bounds only, and spec 10.2 forbids treating one as the \
             other",
        ),
        not_yet_applicable(
            "local certificates",
            &["topology_certificate", "isotopy_certificate"],
            "M5",
            "spec 12 and spec 28 M5 own the certificates; a signature comparison is combinatorial \
             and is not a certificate about an embedding (REVIEW_M1 M1-N5)",
        ),
        not_yet_applicable(
            "accept into the k-best envelope or roll back",
            &["transactional_acceptance", "exact_observation_likelihood"],
            "M7",
            "acceptance needs a posterior to accept AGAINST; spec 11.3 and spec 32 rule 14 forbid \
             a proxy from removing a topology without a certified bound, so M4.5 accepts nothing \
             and rejects nothing",
        ),
    ]
}

/// Plans for the edits an envelope actually contains.
///
/// Pairs are ordered by persistence, so the plans a reader sees first are the
/// distinctions the image argues about most.
pub fn plan_continuations(env: &Envelope, cfg: &ContinuationConfig) -> Vec<ContinuationPlan> {
    let mut plans = Vec::new();
    let hs = &env.hypotheses;
    for i in 0..hs.len() {
        for j in 0..hs.len() {
            if i == j {
                continue;
            }
            let Some(kind) = edit_kind(&hs[i], &hs[j]) else {
                continue;
            };
            let Some((changed, roi)) = difference(&hs[i], &hs[j], cfg.halo_px) else {
                continue;
            };
            plans.push(ContinuationPlan {
                edit: TopologyEdit {
                    kind,
                    from_digest: hs[i].signature.digest.clone(),
                    to_digest: hs[j].signature.digest.clone(),
                    from_components: hs[i].signature.components,
                    from_holes: hs[i].signature.holes,
                    to_components: hs[j].signature.components,
                    to_holes: hs[j].signature.holes,
                    level_between: 0.5 * (hs[i].provenance.level + hs[j].provenance.level),
                    persistence: hs[i]
                        .ambiguity
                        .event_persistence
                        .max(hs[j].ambiguity.event_persistence),
                    changed_px: changed,
                    roi_with_halo: roi,
                    halo_px: cfg.halo_px,
                },
                steps: Vec::new(),
            });
        }
    }
    plans.sort_by(|a, b| {
        b.edit
            .persistence
            .total_cmp(&a.edit.persistence)
            .then(a.edit.from_digest.cmp(&b.edit.from_digest))
            .then(a.edit.to_digest.cmp(&b.edit.to_digest))
    });
    plans.truncate(cfg.max_plans);
    for p in &mut plans {
        p.steps = steps_for(&p.edit);
    }
    plans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubical::{Labelling, SaddleResolution};
    use crate::envelope::{prune, ENVELOPE_CONFIG_V1};
    use crate::events::LevelOrigin;
    use crate::field::FieldKind;
    use crate::hypothesis::{from_events, CandidateContext};
    use vice_ir::ComplementaryConnectivity;

    fn arm() -> ComplementaryConnectivity {
        ComplementaryConnectivity::arms()[0]
    }

    fn cand(
        inside: Vec<bool>,
        w: usize,
        h: usize,
        alpha: &[f64],
        level: f64,
    ) -> TopologyHypothesis {
        from_events(
            Labelling::new(w, h, inside),
            &CandidateContext {
                conn: arm(),
                alpha,
                level,
                level_origins: &[LevelOrigin::PersistencePlateau],
                persistence: 0.25,
                kernel: None,
                palette_id: "pal",
                formation_id: "form",
                field: FieldKind::RawCoverage,
                saddle: SaddleResolution::Thresholded,
            },
        )
    }

    /// A bridged and a split reading of the same dumbbell are ONE edit
    /// apart, the edit is named, and the ROI is the neck plus the halo — not
    /// the whole canvas.
    #[test]
    fn a_bridge_and_a_gap_reading_produce_one_named_edit_with_a_local_roi() {
        let (w, h) = (13usize, 5usize);
        let mut bridged = vec![false; w * h];
        for y in 1..4 {
            for x in 0..5 {
                bridged[y * w + x] = true;
            }
            for x in 8..13 {
                bridged[y * w + x] = true;
            }
        }
        let mut split = bridged.clone();
        for x in 5..8 {
            bridged[2 * w + x] = true;
        }
        // `split` keeps the two blobs apart.
        for x in 5..8 {
            split[2 * w + x] = false;
        }
        let alpha: Vec<f64> = bridged.iter().map(|b| if *b { 1.0 } else { 0.2 }).collect();
        let a = cand(bridged, w, h, &alpha, 0.4);
        let b = cand(split, w, h, &alpha, 0.6);
        assert_eq!(a.signature.components, 1);
        assert_eq!(b.signature.components, 2);
        let env = prune(vec![a, b], &ENVELOPE_CONFIG_V1);
        assert_eq!(env.hypotheses.len(), 2, "the envelope keeps BOTH readings");
        let plans = plan_continuations(&env, &CONTINUATION_CONFIG_V1);
        assert!(!plans.is_empty());
        let kinds: Vec<&str> = plans.iter().map(|p| p.edit.kind.as_str()).collect();
        assert!(
            kinds.contains(&"gap_open") || kinds.contains(&"bridge_close"),
            "{kinds:?}"
        );
        let p = &plans[0];
        assert_eq!(p.edit.changed_px, 3, "only the neck changes");
        assert!(
            p.edit.roi_with_halo.area_px() < (w * h) as u64,
            "the ROI must be LOCAL: {:?} against the whole {}x{} canvas",
            p.edit.roi_with_halo,
            w,
            h
        );
    }

    /// Two of the seven steps are executed and five are refused by name,
    /// each with a capability and an owner. The counts are asserted so that
    /// "executable part" cannot quietly become zero.
    #[test]
    fn five_of_the_seven_steps_are_typed_refusals_with_owners() {
        let edit = TopologyEdit {
            kind: EditKind::BridgeClose,
            from_digest: "a".into(),
            to_digest: "b".into(),
            from_components: 2,
            from_holes: 0,
            to_components: 1,
            to_holes: 0,
            level_between: 0.5,
            persistence: 0.3,
            changed_px: 4,
            roi_with_halo: Bbox {
                x0: 0,
                y0: 0,
                x1: 4,
                y1: 4,
            },
            halo_px: 3,
        };
        let steps = steps_for(&edit);
        assert_eq!(
            steps.len(),
            7,
            "the compound operation of 11.4 has seven steps"
        );
        assert_eq!(
            steps.iter().filter(|s| s.is_executed()).count(),
            1,
            "the topology edit itself is derived from data"
        );
        assert_eq!(
            steps.iter().filter(|s| s.is_partial()).count(),
            1,
            "the ROI half of the exact-ROI step is computed and the posterior half is not"
        );
        assert_eq!(
            steps
                .iter()
                .filter(|s| matches!(s, ContinuationStep::NotYetApplicable { .. }))
                .count(),
            5
        );
        for s in &steps {
            if s.is_executed() {
                continue;
            }
            assert!(!s.missing().is_empty());
            let owner = s.owner().expect("a refusal has an owner");
            assert!(["M5", "M6", "M7"].contains(&owner));
        }
        let owners: Vec<&str> = steps.iter().filter_map(|s| s.owner()).collect();
        assert!(owners.contains(&"M5") && owners.contains(&"M6") && owners.contains(&"M7"));
    }

    /// An unexplained refusal is not constructible.
    #[test]
    #[should_panic(expected = "must name what is missing")]
    fn a_refusal_with_no_missing_capability_is_a_bug() {
        let _ = not_yet_applicable("something", &[], "M5", "because");
    }

    /// And the SECOND constructor is guarded too. REVIEW_M3_5 M35-N5 was
    /// exactly a project that guarded one of two and called the class closed.
    #[test]
    #[should_panic(expected = "must name what is missing")]
    fn a_partial_step_with_no_missing_capability_is_a_bug() {
        let _ = partially_executed("something", "half".into(), &[], "M5", "because");
    }

    /// Candidates that differ by more than one topological step are not one
    /// edit apart, and no plan is invented for them.
    #[test]
    fn a_two_step_difference_is_not_one_edit() {
        let (w, h) = (9usize, 9usize);
        let alpha = vec![0.5f64; w * h];
        let one = cand(
            (0..w * h).map(|i| (1..4).contains(&(i % w))).collect(),
            w,
            h,
            &alpha,
            0.5,
        );
        let three = cand(
            (0..w * h)
                .map(|i| {
                    let x = i % w;
                    x == 1 || x == 4 || x == 7
                })
                .collect(),
            w,
            h,
            &alpha,
            0.5,
        );
        assert_eq!(one.signature.components, 1);
        assert_eq!(three.signature.components, 3);
        assert!(edit_kind(&one, &three).is_none());
    }
}
