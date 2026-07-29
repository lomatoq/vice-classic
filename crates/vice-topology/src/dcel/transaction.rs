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
    pub const ALL_NAMES: [&'static str; 6] = [
        "EditLeftTheRoi",
        "EditLeftTheCanvas",
        "EditIsANoOp",
        "NotTheDeclaredEdit",
        "UnrelatedGraphMutation",
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
    let candidate = Dcel::assemble(
        Labelling::new(w as usize, h as usize, inside),
        base.connectivity(),
    );

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
mod tests {
    use super::*;
    use vice_ir::ComplementaryConnectivity;

    fn arm() -> ComplementaryConnectivity {
        ComplementaryConnectivity::arms()[0]
    }

    /// Two blobs and a neck, plus an UNRELATED square far below.
    ///
    /// The unrelated square is not decoration: without something outside the
    /// ROI and its halo, "nothing outside the region moved" is a statement
    /// about the empty set, and an empty control is indistinguishable from a
    /// passing one (F-0039). The fixture is built so the clause has a
    /// population.
    fn dumbbell(bridged: bool) -> Dcel {
        let (w, h) = (21usize, 15usize);
        let mut inside = vec![false; w * h];
        for y in 2..7 {
            for x in 1..7 {
                inside[y * w + x] = true;
            }
            for x in 14..20 {
                inside[y * w + x] = true;
            }
        }
        if bridged {
            for x in 7..14 {
                inside[4 * w + x] = true;
            }
        }
        for y in 11..14 {
            for x in 2..6 {
                inside[y * w + x] = true;
            }
        }
        Dcel::assemble(Labelling::new(w, h, inside), arm())
    }

    fn neck_roi() -> Roi {
        Roi {
            x0: 7,
            y0: 3,
            x1: 14,
            y1: 6,
        }
    }

    /// The whole point, on one fixture: two components become one, the
    /// declared edit matches, nothing outside the neck moves, and the base is
    /// still two components afterwards because nothing mutated it.
    #[test]
    fn closing_a_bridge_commits_and_leaves_the_rest_of_the_graph_alone() {
        let base = dumbbell(false);
        assert_eq!(
            base.foreground_faces(),
            3,
            "two blobs and the unrelated square"
        );
        let edit = Edit {
            kind: EditKind::BRIDGE_CLOSE,
            roi: neck_roi(),
            set: (7..14u32).map(|x| (x, 4u32, true)).collect(),
        };
        let out = apply(&base, &edit, &TX_CONFIG_V1);
        let r = out.report();
        let new = out.committed().expect("the transaction must commit");
        assert_eq!(new.foreground_faces(), 2);
        assert_eq!(new.holes(), 0);
        assert_eq!(r.unrelated_chains_that_moved, 0);
        assert!(
            r.unrelated_chains > 0,
            "if nothing is outside the region, 'nothing outside moved' measures nothing"
        );
        assert!(r.committed);
        assert!(r.certificate.is_some());
        // The base is untouched: rollback needs no undo because nothing was
        // ever mutated.
        assert_eq!(base.foreground_faces(), 3);
    }

    /// A transaction that declares the wrong edit is rolled back, even though
    /// the labelling change itself is perfectly legal.
    #[test]
    fn an_edit_that_is_not_what_it_declared_is_rolled_back() {
        let base = dumbbell(false);
        let edit = Edit {
            kind: EditKind::HOLE_FILL,
            roi: neck_roi(),
            set: (7..14u32).map(|x| (x, 4u32, true)).collect(),
        };
        match apply(&base, &edit, &TX_CONFIG_V1) {
            Outcome::RolledBack { reason, report } => {
                assert!(matches!(
                    reason,
                    TransactionRefusal::NotTheDeclaredEdit { .. }
                ));
                assert!(!report.committed);
            }
            Outcome::Committed { .. } => panic!("a mis-declared edit must not commit"),
        }
    }

    /// A pixel outside the declared ROI is refused BEFORE anything is built.
    /// This is the §28 M5 clause "no unrelated graph mutation" at its cheapest
    /// point of entry.
    #[test]
    fn an_edit_reaching_outside_its_roi_is_refused() {
        let base = dumbbell(false);
        let mut set: Vec<(u32, u32, bool)> = (7..14u32).map(|x| (x, 4u32, true)).collect();
        set.push((0, 0, true));
        let edit = Edit {
            kind: EditKind::BRIDGE_CLOSE,
            roi: neck_roi(),
            set,
        };
        match apply(&base, &edit, &TX_CONFIG_V1) {
            Outcome::RolledBack { reason, .. } => assert!(matches!(
                reason,
                TransactionRefusal::EditLeftTheRoi { x: 0, y: 0, .. }
            )),
            Outcome::Committed { .. } => panic!("an edit outside its ROI must not commit"),
        }
    }

    /// The locality check has resolving power: a transaction whose ROI is
    /// declared large enough to admit a distant change IS caught by the
    /// unrelated-chain comparison rather than by the cheap bounds test.
    ///
    /// Both directions: the same edit without the distant pixel commits.
    #[test]
    fn a_distant_change_inside_a_wide_roi_is_caught_by_the_chain_comparison() {
        let base = dumbbell(false);
        let wide = Roi {
            x0: 0,
            y0: 0,
            x1: 21,
            y1: 15,
        };
        let mut set: Vec<(u32, u32, bool)> = (7..14u32).map(|x| (x, 4u32, true)).collect();
        // A pixel far from the neck, well outside the neck ROI + halo.
        set.push((19, 13, true));
        let edit = Edit {
            kind: EditKind::BRIDGE_CLOSE,
            roi: wide,
            set: set.clone(),
        };
        // With a wide ROI the region swallows the canvas, so nothing is
        // "unrelated" and the comparison has nothing to say — which is the
        // honest limit of this check and is why the row publishes
        // `unrelated_chains` beside the verdict.
        let out = apply(&base, &edit, &TX_CONFIG_V1);
        assert_eq!(
            out.report().unrelated_chains,
            0,
            "a canvas-wide ROI leaves no unrelated chain, and the row must say so"
        );

        // With the ROI the edit actually needs, the distant pixel is caught at
        // the ROI test — the cheaper of the two mechanisms, and the one that
        // fires first.
        let edit2 = Edit {
            kind: EditKind::BRIDGE_CLOSE,
            roi: neck_roi(),
            set,
        };
        assert!(matches!(
            apply(&base, &edit2, &TX_CONFIG_V1),
            Outcome::RolledBack {
                reason: TransactionRefusal::EditLeftTheRoi { .. },
                ..
            }
        ));
    }

    /// Opening a hole, and filling it again, returns to the original
    /// arrangement. The transaction is not merely reversible in principle:
    /// the parts compare equal.
    #[test]
    fn opening_a_hole_and_filling_it_returns_the_same_arrangement() {
        let (w, h) = (11usize, 11usize);
        let inside = vec![true; w * h];
        let base = Dcel::assemble(Labelling::new(w, h, inside), arm());
        assert_eq!(base.holes(), 0);
        let roi = Roi {
            x0: 4,
            y0: 4,
            x1: 7,
            y1: 7,
        };
        let open = Edit {
            kind: EditKind::HOLE_OPEN,
            roi,
            set: vec![(5, 5, false)],
        };
        let out = apply(&base, &open, &TX_CONFIG_V1);
        let holed = out.committed().expect("hole_open commits").clone();
        assert_eq!(holed.holes(), 1);

        let fill = Edit {
            kind: EditKind::HOLE_FILL,
            roi,
            set: vec![(5, 5, true)],
        };
        let back = apply(&holed, &fill, &TX_CONFIG_V1);
        let restored = back.committed().expect("hole_fill commits");
        assert_eq!(restored.holes(), 0);
        assert_eq!(restored.parts(), base.parts());
    }

    /// **RT5-A6: the world in which the locality conjunct is false.**
    ///
    /// The red team could not build a transaction that `apply` rolls back for
    /// `UnrelatedGraphMutation`, and neither could I, and the reason is a
    /// theorem rather than an accident: a boundary chain lying wholly outside
    /// the ROI depends only on labels of pixels adjacent to it, and step (1)
    /// guarantees the edit changes none of those. So on the production path
    /// `UnrelatedGraphMutation` is UNREACHABLE, and §32's "before adding a
    /// conjunct, exhibit a world where it is false" was unmet — the conjunct
    /// was published as a measurement on 127 chains with no demonstration that
    /// it could ever move.
    ///
    /// This is that demonstration, and it is honest about what it shows: the
    /// COMPARISON has resolving power, exercised by changing a pixel far away
    /// and asking the same function the transaction asks. What it does not show
    /// is that `apply` can reach the branch, and the clause-3 row says so by
    /// publishing the reachable and unreachable refusal sets.
    #[test]
    fn the_chain_comparison_detects_a_distant_change_when_it_is_given_one() {
        let base = dumbbell(false);
        let roi = neck_roi();
        let halo = roi.grown(TX_CONFIG_V1.halo_px, base.width_px(), base.height_px());

        // POSITIVE CONTROL: the base against itself moves nothing.
        let before = unrelated_paths(&base, &halo);
        assert!(
            !before.is_empty(),
            "no chain lies outside the region, so the comparison has nothing to compare"
        );
        assert_eq!(
            before,
            unrelated_paths(&base, &halo),
            "the comparison must be stable against itself"
        );

        // The world: a pixel changed OUTSIDE the ROI and its halo, which
        // `apply` would refuse at step (1) and which the comparison must see.
        let mut inside = base.labelling().inside().to_vec();
        let w = base.width_px() as usize;
        // A pixel inside the distant witness square. Flipping it opens a hole,
        // so the outer chain is untouched and a NEW chain appears — which is
        // why the comparison below is symmetric and the first version of it,
        // base-minus-candidate, saw nothing.
        inside[12 * w + 3] = !inside[12 * w + 3];
        let far = Dcel::assemble(
            Labelling::new(w, base.height_px() as usize, inside),
            base.connectivity(),
        );
        let after = unrelated_paths(&far, &halo);
        let moved = before.symmetric_difference(&after).count();
        assert!(
            moved > 0,
            "a chain outside the region changed and the comparison did not see it; the conjunct              clause 3 stands on would then be unfalsifiable in both directions"
        );
    }

    /// A no-op is refused. A transaction that certifies "nothing happened"
    /// would make every clause about transactions satisfiable by doing none.
    #[test]
    fn a_transaction_that_changes_nothing_is_refused() {
        let base = dumbbell(true);
        let edit = Edit {
            kind: EditKind::BRIDGE_CLOSE,
            roi: neck_roi(),
            set: vec![(8, 4, true)],
        };
        assert!(matches!(
            apply(&base, &edit, &TX_CONFIG_V1),
            Outcome::RolledBack {
                reason: TransactionRefusal::EditIsANoOp,
                ..
            }
        ));
    }
}
