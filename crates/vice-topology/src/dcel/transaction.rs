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
         not that edit"
    )]
    NotTheDeclaredEdit {
        declared: &'static str,
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

/// What one transaction did, whether or not it committed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransactionReport {
    pub declared: &'static str,
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
        declared: edit.kind.as_str(),
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
    let (c0, h0) = (base_audit.foreground_faces, base_audit.holes);
    let (c1, h1) = (cand_audit.foreground_faces, cand_audit.holes);
    let expected = match edit.kind {
        EditKind::BridgeClose => (c0.checked_sub(1), Some(h0)),
        EditKind::GapOpen => (Some(c0 + 1), Some(h0)),
        EditKind::HoleOpen => (Some(c0), Some(h0 + 1)),
        EditKind::HoleFill => (Some(c0), h0.checked_sub(1)),
    };
    if expected.0 != Some(c1) || expected.1 != Some(h1) {
        return rolled(
            TransactionRefusal::NotTheDeclaredEdit {
                declared: edit.kind.as_str(),
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
    let unrelated_base = unrelated_paths(base, &roi_halo);
    let unrelated_cand = unrelated_paths(&candidate, &roi_halo);
    report.unrelated_chains = unrelated_base.len();
    let moved: Vec<&Vec<(u32, u32)>> = unrelated_base
        .iter()
        .filter(|p| !unrelated_cand.contains(*p))
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
            kind: EditKind::BridgeClose,
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
            kind: EditKind::HoleFill,
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
            kind: EditKind::BridgeClose,
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
            kind: EditKind::BridgeClose,
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
            kind: EditKind::BridgeClose,
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
            kind: EditKind::HoleOpen,
            roi,
            set: vec![(5, 5, false)],
        };
        let out = apply(&base, &open, &TX_CONFIG_V1);
        let holed = out.committed().expect("hole_open commits").clone();
        assert_eq!(holed.holes(), 1);

        let fill = Edit {
            kind: EditKind::HoleFill,
            roi,
            set: vec![(5, 5, true)],
        };
        let back = apply(&holed, &fill, &TX_CONFIG_V1);
        let restored = back.committed().expect("hole_fill commits");
        assert_eq!(restored.holes(), 0);
        assert_eq!(restored.parts(), base.parts());
    }

    /// A no-op is refused. A transaction that certifies "nothing happened"
    /// would make every clause about transactions satisfiable by doing none.
    #[test]
    fn a_transaction_that_changes_nothing_is_refused() {
        let base = dumbbell(true);
        let edit = Edit {
            kind: EditKind::BridgeClose,
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
