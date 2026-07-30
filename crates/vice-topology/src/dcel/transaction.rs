//! Topological transactions (spec v1.3 §11.4, §12, §28 M5).
//!
//! §11.4 makes every topology edit a COMPOUND operation. M4.5 could execute
//! one and a half of its seven steps and refused the rest by name, each with
//! the capability it needed and the milestone that owns it
//! ([`crate::continuation`]). M5 owns three of those capabilities —
//! `labelling_mutation`, `shared_dcel` and `topology_certificate` — and this
//! module is where they are executed rather than described.
//!
//! ```text
//! topology edit                  <- EXECUTED here
//! rebuild affected DCEL          <- EXECUTED here
//! refit affected representation  <- M6, still refused by name
//! refit paints                   <- M6, still refused by name
//! exact ROI posterior with halo   <- M7; the REGION is computed here, the posterior is not
//! local certificates             <- EXECUTED here, for the topological half
//! accept or roll back            <- EXECUTED here against the CERTIFICATE,
//!                                   and never against a proxy score
//! ```
//!
//! ## What acceptance is allowed to depend on
//!
//! §32 rule 14 — "no topology winner from the M5 proxy" — and §11.3 — "a proxy
//! score may not irreversibly remove a topology without a certified bound" —
//! together fix what [`apply`] may decide on. It decides on the CERTIFICATE:
//! did the edit do what it declared, and did it leave the rest of the graph
//! alone. It never reads a cost, a bound or a score. There is no parameter
//! here through which one could arrive, which is why the §28 M5 clause "no
//! final-topology claim from proxy" is measured as a property of this
//! signature rather than promised in prose.
//!
//! ## Atomicity, and what it costs
//!
//! [`apply`] takes `&Dcel` and returns a new one or nothing. The base is
//! immutable, so "roll back" is "drop the candidate" and there is no partial
//! state for a failure to leave behind — the same reason `Dcel` has one
//! constructor. The price is real and is named here rather than in a footnote:
//! **the candidate is a full rebuild, not an incremental patch.** §11.4 says
//! "rebuild AFFECTED DCEL"; this rebuilds all of it and then PROVES that
//! nothing outside the region moved. For M5's purpose — a few transactions per
//! envelope — that is the cheaper engineering; for M7's optimizer inner loop it
//! will not be, and the incremental rebuild is recorded as an M7 obligation
//! with its price in `docs/STATUS_M5.md` rather than half-built here.

use serde::Serialize;

use super::audit::{audit, AuditReport, InvariantViolation};
use super::certificate::{topology_certificate, TopologyCertificate};
use super::lattice::{Arrangement, Dir, Lat, Step};
use super::Dcel;
use crate::continuation::EditKind;
use crate::cubical::Labelling;

/// A half-open pixel rectangle: `[x0, x1) x [y0, y1)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Roi {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl Roi {
    pub fn contains(&self, x: u32, y: u32) -> bool {
        (self.x0..self.x1).contains(&x) && (self.y0..self.y1).contains(&y)
    }
    pub fn area_px(&self) -> u64 {
        u64::from(self.x1.saturating_sub(self.x0)) * u64::from(self.y1.saturating_sub(self.y0))
    }
    /// Grown by `halo`, clamped to the canvas.
    pub fn grown(&self, halo: u32, w: u32, h: u32) -> Roi {
        Roi {
            x0: self.x0.saturating_sub(halo),
            y0: self.y0.saturating_sub(halo),
            x1: (self.x1 + halo).min(w),
            y1: (self.y1 + halo).min(h),
        }
    }
    /// Does a lattice point lie on or inside the rectangle's closure? A
    /// boundary runs along pixel CORNERS, so a pixel rectangle touches lattice
    /// points from `x0` to `x1` inclusive.
    pub fn touches_lattice(&self, x: u32, y: u32) -> bool {
        (self.x0..=self.x1).contains(&x) && (self.y0..=self.y1).contains(&y)
    }
}

/// One topology edit, declared before it is performed.
///
/// `kind` is a CLAIM about what the edit does to the signature, and [`apply`]
/// refuses the transaction when the claim and the result disagree. A
/// transaction that discovers its own kind afterwards would certify nothing.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Edit {
    pub kind: EditKind,
    pub roi: Roi,
    /// Pixels whose label the edit sets, as `(x, y, inside)`.
    pub set: Vec<(u32, u32, bool)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TxConfig {
    /// §11.4 wants the ROI taken WITH a halo, because a local edit changes
    /// coverage outside its own footprint through the formation kernel.
    pub halo_px: u32,
}

pub const TX_CONFIG_V1: TxConfig = TxConfig { halo_px: 3 };

/// Measured work performed by the local boundary-arrangement rebuild.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IncrementalRebuildReport {
    pub algorithm: &'static str,
    pub changed_pixels: usize,
    /// Undirected unit-segment sites whose existence was recomputed.
    pub affected_segment_sites: usize,
    /// All possible undirected segment sites on this canvas. This denominator
    /// makes "incremental" an observable claim.
    pub complete_lattice_segment_sites: usize,
    pub base_boundary_segments: usize,
    pub candidate_boundary_segments: usize,
    /// Candidate segments copied from the base step set without re-reading
    /// their adjacent pixel labels.
    pub reused_boundary_segments: usize,
    /// Candidate segments reconstructed at affected sites.
    pub rebuilt_boundary_segments: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IncrementalRebuildError {
    #[error(
        "incremental rebuild shape mismatch: base is {base_w}x{base_h}, candidate is \
         {candidate_w}x{candidate_h}"
    )]
    ShapeMismatch {
        base_w: u32,
        base_h: u32,
        candidate_w: u32,
        candidate_h: u32,
    },
    #[error("incremental rebuild requires at least one changed pixel")]
    NoChangedPixels,
}

/// Why a transaction did not commit.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransactionRefusal {
    #[error("pixel ({x}, {y}) is outside the declared ROI {roi:?}")]
    EditLeftTheRoi { x: u32, y: u32, roi: Roi },
    #[error("pixel ({x}, {y}) is outside the canvas {w}x{h}")]
    EditLeftTheCanvas { x: u32, y: u32, w: u32, h: u32 },
    #[error("the edit changes nothing: a transaction that is a no-op certifies nothing")]
    EditIsANoOp,
    #[error(
        "declared {declared} but the signature went from ({c0}, {h0}) to ({c1}, {h1}), which is \
         {performed}"
    )]
    NotTheDeclaredEdit {
        declared: String,
        /// What the edit actually was. The old message said only what the
        /// declaration was NOT; naming the delta performed costs nothing and is
        /// what a reader of a rolled-back compound transaction needs.
        performed: String,
        c0: u32,
        h0: u32,
        c1: u32,
        h1: u32,
    },
    #[error(
        "unrelated graph mutation: {count} boundary chain(s) outside the ROI+halo differ between \
         the base and the candidate; first is {first}"
    )]
    UnrelatedGraphMutation { count: usize, first: String },
    #[error("incremental DCEL rebuild failed: {0}")]
    IncrementalRebuildFailed(#[from] IncrementalRebuildError),
    #[error("the candidate failed its own audit: {0}")]
    CandidateFailedAudit(#[from] InvariantViolation),
}

impl TransactionRefusal {
    /// The refusal's variant name.
    ///
    /// The judge is the compiler: this `match` is exhaustive, so a new refusal
    /// variant does not compile until it is named here. That is the difference
    /// between this and a caller re-deriving the name from the rendered
    /// message, which would be a text scan over a string this type is free to
    /// reword.
    pub fn name(&self) -> &'static str {
        match self {
            TransactionRefusal::EditLeftTheRoi { .. } => "EditLeftTheRoi",
            TransactionRefusal::EditLeftTheCanvas { .. } => "EditLeftTheCanvas",
            TransactionRefusal::EditIsANoOp => "EditIsANoOp",
            TransactionRefusal::NotTheDeclaredEdit { .. } => "NotTheDeclaredEdit",
            TransactionRefusal::UnrelatedGraphMutation { .. } => "UnrelatedGraphMutation",
            TransactionRefusal::IncrementalRebuildFailed(_) => "IncrementalRebuildFailed",
            TransactionRefusal::CandidateFailedAudit(_) => "CandidateFailedAudit",
        }
    }

    /// Every refusal this type can express.
    ///
    /// A literal (F-0048 Q1), and guarded in both directions by
    /// `every_refusal_variant_is_in_all_names`: one of each variant is
    /// constructed and its `name()` required to be present, and the length is
    /// required to match, so a variant added without a line here fails a test
    /// rather than silently shrinking a report's denominator.
    pub const ALL_NAMES: [&'static str; 7] = [
        "EditLeftTheRoi",
        "EditLeftTheCanvas",
        "EditIsANoOp",
        "NotTheDeclaredEdit",
        "UnrelatedGraphMutation",
        "IncrementalRebuildFailed",
        "CandidateFailedAudit",
    ];
}

/// What one transaction did, whether or not it committed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransactionReport {
    /// The declared edit's name. A `String` since M6 rather than a
    /// `&'static str`: the four unit steps have static names, a compound delta
    /// names itself from its own numbers, and both serialize to a JSON string,
    /// so `docs/gt/DCEL_M5.json`'s existing `declared` values are unmoved.
    pub declared: String,
    /// How many unit steps the declared edit is worth. `1` for the four named
    /// steps; anything else is a compound edit, and this is the field that
    /// makes "M5 measured only the one-step subclass" a published number rather
    /// than something a reader has to infer from the absence of rows.
    pub declared_steps: u64,
    pub roi: Roi,
    pub roi_with_halo: Roi,
    pub halo_px: u32,
    pub pixels_set: usize,
    pub pixels_changed: usize,
    pub committed: bool,
    /// Boundary chains of the base that lie wholly outside the ROI+halo.
    pub unrelated_chains: usize,
    /// How many of those were not found verbatim in the candidate. The §28 M5
    /// clause "no unrelated graph mutation" is this number being zero, and the
    /// number beside it is what makes zero mean something.
    pub unrelated_chains_that_moved: usize,
    pub incremental_rebuild: Option<IncrementalRebuildReport>,
    pub base: Option<AuditReport>,
    pub candidate: Option<AuditReport>,
    pub certificate: Option<TopologyCertificate>,
}

/// The result. `Committed` carries the new arrangement; `RolledBack` carries
/// nothing, because there is nothing to undo.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Committed {
        dcel: Box<Dcel>,
        report: Box<TransactionReport>,
    },
    RolledBack {
        reason: TransactionRefusal,
        report: Box<TransactionReport>,
    },
}

impl Outcome {
    pub fn report(&self) -> &TransactionReport {
        match self {
            Outcome::Committed { report, .. } | Outcome::RolledBack { report, .. } => report,
        }
    }
    pub fn committed(&self) -> Option<&Dcel> {
        match self {
            Outcome::Committed { dcel, .. } => Some(dcel),
            Outcome::RolledBack { .. } => None,
        }
    }
}

fn path_step(from: (u32, u32), to: (u32, u32)) -> Step {
    let dir = match (
        i64::from(to.0) - i64::from(from.0),
        i64::from(to.1) - i64::from(from.1),
    ) {
        (1, 0) => Dir::E,
        (-1, 0) => Dir::W,
        (0, 1) => Dir::S,
        (0, -1) => Dir::N,
        delta => panic!("stored DCEL path contains a non-unit step: {delta:?}"),
    };
    Step {
        from: Lat {
            x: from.0,
            y: from.1,
        },
        dir,
    }
}

fn boundary_steps(dcel: &Dcel) -> std::collections::BTreeSet<Step> {
    let mut steps = std::collections::BTreeSet::new();
    for boundary in dcel.boundaries() {
        for pair in boundary.path.windows(2) {
            let step = path_step(pair[0], pair[1]);
            steps.insert(step);
            steps.insert(step.twin());
        }
    }
    steps
}

fn pixel_perimeter_steps(x: u32, y: u32) -> [Step; 4] {
    [
        Step {
            from: Lat { x, y },
            dir: Dir::E,
        },
        Step {
            from: Lat { x, y },
            dir: Dir::S,
        },
        Step {
            from: Lat { x: x + 1, y },
            dir: Dir::S,
        },
        Step {
            from: Lat { x, y: y + 1 },
            dir: Dir::E,
        },
    ]
}

/// Rebuild a changed binary arrangement by updating only boundary-step sites
/// adjacent to changed pixels, then canonicalizing the resulting complete
/// step set into a DCEL.
///
/// This function deliberately does not call [`Dcel::assemble`]. The full
/// constructor is retained as an independent oracle for the differential
/// harness.
pub fn rebuild_incremental(
    base: &Dcel,
    labelling: Labelling,
) -> Result<(Dcel, IncrementalRebuildReport), IncrementalRebuildError> {
    let (base_w, base_h) = (base.width_px(), base.height_px());
    let (candidate_w, candidate_h) = (labelling.width_px() as u32, labelling.height_px() as u32);
    if (base_w, base_h) != (candidate_w, candidate_h) {
        return Err(IncrementalRebuildError::ShapeMismatch {
            base_w,
            base_h,
            candidate_w,
            candidate_h,
        });
    }
    let changed_pixels: Vec<(u32, u32)> = base
        .labelling()
        .inside()
        .iter()
        .zip(labelling.inside())
        .enumerate()
        .filter_map(|(index, (before, after))| {
            (before != after).then_some((index as u32 % candidate_w, index as u32 / candidate_w))
        })
        .collect();
    if changed_pixels.is_empty() {
        return Err(IncrementalRebuildError::NoChangedPixels);
    }

    let base_steps = boundary_steps(base);
    let mut affected_steps = std::collections::BTreeSet::new();
    for &(x, y) in &changed_pixels {
        for step in pixel_perimeter_steps(x, y) {
            affected_steps.insert(step);
            affected_steps.insert(step.twin());
        }
    }
    let retained_steps: std::collections::BTreeSet<Step> =
        base_steps.difference(&affected_steps).copied().collect();
    let reused_boundary_segments = retained_steps.len() / 2;
    let (candidate_steps, rebuilt_boundary_segments) = {
        let arrangement = Arrangement::new(
            labelling.inside(),
            candidate_w,
            candidate_h,
            base.connectivity(),
        );
        let mut steps = retained_steps;
        for &step in &affected_steps {
            if arrangement.exists(step) {
                steps.insert(step);
            }
        }
        let rebuilt = steps.intersection(&affected_steps).count() / 2;
        (steps, rebuilt)
    };
    let candidate_boundary_segments = candidate_steps.len() / 2;
    let mut ordered_steps: Vec<Step> = candidate_steps.into_iter().collect();
    ordered_steps.sort_by_key(|step| (step.from.y, step.from.x, step.dir.index()));
    let candidate = Dcel::assemble_from_steps(labelling, base.connectivity(), &ordered_steps);
    let complete_lattice_segment_sites =
        base_w as usize * (base_h as usize + 1) + (base_w as usize + 1) * base_h as usize;
    let report = IncrementalRebuildReport {
        algorithm: "local_boundary_step_delta_v1",
        changed_pixels: changed_pixels.len(),
        affected_segment_sites: affected_steps.len() / 2,
        complete_lattice_segment_sites,
        base_boundary_segments: base_steps.len() / 2,
        candidate_boundary_segments,
        reused_boundary_segments,
        rebuilt_boundary_segments,
    };
    Ok((candidate, report))
}

/// Apply one topological transaction.
///
/// Note what is NOT a parameter: no cost, no bound, no score, no budget. §32
/// rule 14 and §11.3 forbid a proxy from deciding a topology, and the cheapest
/// way to keep that promise is for the deciding function to have no way to
/// receive one.
pub fn apply(base: &Dcel, edit: &Edit, cfg: &TxConfig) -> Outcome {
    let (w, h) = (base.width_px(), base.height_px());
    let roi_halo = edit.roi.grown(cfg.halo_px, w, h);
    let mut report = TransactionReport {
        declared: edit.kind.name(),
        declared_steps: edit.kind.steps(),
        roi: edit.roi,
        roi_with_halo: roi_halo,
        halo_px: cfg.halo_px,
        pixels_set: edit.set.len(),
        pixels_changed: 0,
        committed: false,
        unrelated_chains: 0,
        unrelated_chains_that_moved: 0,
        incremental_rebuild: None,
        base: None,
        candidate: None,
        certificate: None,
    };

    let rolled = |reason: TransactionRefusal, report: TransactionReport| Outcome::RolledBack {
        reason,
        report: Box::new(report),
    };

    // (1) The edit stays where it said it would.
    let mut inside: Vec<bool> = base.labelling().inside().to_vec();
    let mut changed = 0usize;
    for (x, y, v) in &edit.set {
        if *x >= w || *y >= h {
            return rolled(
                TransactionRefusal::EditLeftTheCanvas { x: *x, y: *y, w, h },
                report,
            );
        }
        if !edit.roi.contains(*x, *y) {
            return rolled(
                TransactionRefusal::EditLeftTheRoi {
                    x: *x,
                    y: *y,
                    roi: edit.roi,
                },
                report,
            );
        }
        let i = *y as usize * w as usize + *x as usize;
        if inside[i] != *v {
            changed += 1;
            inside[i] = *v;
        }
    }
    report.pixels_changed = changed;
    if changed == 0 {
        return rolled(TransactionRefusal::EditIsANoOp, report);
    }

    // (2) Build the candidate. Assembly is total, so this cannot fail; the
    // audit below is not a conjunct that cannot be false — the mutation walk
    // in `audit.rs` exhibits the world where it fails, which is what §32's
    // "before adding a conjunct, exhibit a world where it is false" asks for.
    let (candidate, rebuild) =
        match rebuild_incremental(base, Labelling::new(w as usize, h as usize, inside)) {
            Ok(result) => result,
            Err(error) => return rolled(TransactionRefusal::from(error), report),
        };
    report.incremental_rebuild = Some(rebuild);

    let base_audit = match audit(base) {
        Ok(a) => a,
        Err(e) => return rolled(TransactionRefusal::CandidateFailedAudit(e), report),
    };
    let cand_audit = match audit(&candidate) {
        Ok(a) => a,
        Err(e) => return rolled(TransactionRefusal::CandidateFailedAudit(e), report),
    };
    report.base = Some(base_audit);
    report.candidate = Some(cand_audit);

    // (3) The declared edit is the edit performed.
    //
    // COMPOUND-CAPABLE since M6. This was four `match` arms over a four-variant
    // enum, and an edit whose signature delta was not one of those four could
    // not be declared at all — so `vice-bench`'s harness dropped 310 of 480
    // arms before reaching here, which is exactly the subclass §28 M5 names
    // ("local COMPOUND topology transactions", limitations 37 and 44). The
    // check is now arithmetic over Z^2: the declared delta is ADDED to the
    // base signature and the sum is required to equal the candidate's. Every
    // multi-step edit is expressible, and the four unit steps evaluate exactly
    // as they did before.
    //
    // Signed arithmetic on purpose. The old form used `checked_sub` on `u32`
    // and mapped underflow to `None`, i.e. an unrepresentable declaration and a
    // wrong one produced the same refusal; here they are the same thing, and
    // there is no path that can underflow.
    let (c0, h0) = (base_audit.foreground_faces, base_audit.holes);
    let (c1, h1) = (cand_audit.foreground_faces, cand_audit.holes);
    let performed = EditKind::between((c0, h0), (c1, h1));
    if performed != edit.kind {
        return rolled(
            TransactionRefusal::NotTheDeclaredEdit {
                declared: edit.kind.name(),
                performed: performed.name(),
                c0,
                h0,
                c1,
                h1,
            },
            report,
        );
    }

    // (4) No unrelated graph mutation. Chains are compared by their LATTICE
    // PATHS rather than by id, because an edit that merges two faces
    // legitimately renumbers faces, and comparing ids would report the edit
    // itself as collateral damage.
    // SYMMETRIC. The first version compared base-minus-candidate only, so a
    // chain that APPEARED outside the region was invisible: flipping one pixel
    // inside a distant square opens a hole there, leaves the outer chain
    // untouched, and adds an inner one. The test that gives this conjunct its
    // falsifying world found that on its first run (RT5-A6's neighbourhood).
    let unrelated_base = unrelated_paths(base, &roi_halo);
    let unrelated_cand = unrelated_paths(&candidate, &roi_halo);
    report.unrelated_chains = unrelated_base.len();
    let moved: Vec<&Vec<(u32, u32)>> = unrelated_base
        .symmetric_difference(&unrelated_cand)
        .collect();
    report.unrelated_chains_that_moved = moved.len();
    if let Some(first) = moved.first() {
        let n = moved.len();
        return rolled(
            TransactionRefusal::UnrelatedGraphMutation {
                count: n,
                first: format!("{:?}..{:?}", first[0], first[first.len() - 1]),
            },
            report,
        );
    }

    // (5) The certificate, and acceptance against it.
    let cert = topology_certificate(base, &candidate, edit.kind, &base_audit, &cand_audit);
    report.certificate = Some(cert.clone());
    report.committed = true;
    Outcome::Committed {
        dcel: Box::new(candidate),
        report: Box::new(report),
    }
}

/// Lattice paths of the boundary chains that lie WHOLLY outside the region.
fn unrelated_paths(d: &Dcel, roi: &Roi) -> std::collections::BTreeSet<Vec<(u32, u32)>> {
    d.boundaries()
        .iter()
        .filter(|b| !b.path.iter().any(|p| roi.touches_lattice(p.0, p.1)))
        .map(|b| b.path.clone())
        .collect()
}

#[cfg(test)]
#[path = "transaction/tests.rs"]
mod tests;
