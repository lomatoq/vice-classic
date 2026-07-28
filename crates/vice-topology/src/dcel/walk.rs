//! The mutation walk: the instrument that measures what [`super::audit`] can
//! SEE.
//!
//! In its own module for the §4.1 size rule, and the split is the honest one:
//! `audit.rs` asks whether an arrangement holds its invariants, this file asks
//! whether that question has any resolving power. Those are different jobs, and
//! the second is the one F-0035 requires before the first may be published as a
//! conjunct — "exhibit a world where it is false".
//!
//! ## Why it is not a list of perturbations
//!
//! A control that tries three hand-picked corruptions closes three
//! corruptions. F-0048 Q1 asks whether the mechanism contains a literal
//! enumerating its subjects; a `vec![corrupt_a, corrupt_b, corrupt_c]` answers
//! yes, and the next finding is a fourth entry.
//!
//! [`Parts::perturbations`] DESTRUCTURES [`Parts`] exhaustively instead and
//! emits one perturbation per scalar slot of the actual data:
//!
//! - a field added to `Parts` without a site does not compile, because the
//!   pattern is exhaustive. The judge is the compiler, which is the form the
//!   project already accepted for `TopologyGateConfig::sites`.
//!
//!   **The cheapest bypass, named where the strength is claimed** (F-0048's
//!   last paragraph, and REVIEW_M5_B N9): writing the new field as `extra: _`
//!   in the pattern. One line, clippy clean with `-D warnings`, and the field
//!   silently has no perturbation. What the compiler judges is that every field
//!   is MENTIONED, not that every field is EXERCISED. The red team measured it
//!   literally: a new field with a public reader, corrupted above 16 px, plus
//!   one line `face_area_px: _` — clippy clean, gate EXIT 0, four `[MET]`, and
//!   **the slot count did not move** (RT5-A11).
//!
//!   The "partial compensation" this comment used to claim — that `PartialEq`
//!   still compares a new field through assembly-equality — was **nil twice
//!   over**, and saying so was worse than saying nothing: the walk stopped
//!   calling assembly-equality (that WAS the RT5-A2 fix), and
//!   `audit_every_labelling` compares two copies carrying the same defect.
//!
//!   Closed since delta-2, and not by the pattern:
//!   `every_scalar_leaf_of_parts_has_exactly_one_perturbation` compares the
//!   number of sites against the number of scalar leaves of the SERIALIZED
//!   `Parts`. That count comes from the `Serialize` derive, i.e. from the struct
//!   definition, which `extra: _` does not touch — so the field moves one side
//!   of the equation and not the other, and the test fails;
//! - the number of perturbations is a function of the DATA, so a bigger
//!   arrangement is a wider control automatically, and a control that went
//!   empty is visible as a count of zero rather than as silence (F-0039).
//!
//! ## Two defects this walk had, found by the walk's own counter
//!
//! `no_ops` — perturbations that changed nothing — is published, and it is not
//! decoration. It found both of these, on the corpus, after the walk was
//! already green:
//!
//! - perturbing a boundary's owners by "move the left owner to the next face,
//!   skipping the right one" walks straight back to the original pair on a
//!   TWO-face arrangement. Three silent no-ops per probe;
//! - shifting an endpoint by `(id + 1) % vertices.len()` computes `(0 + 1) % 1`
//!   on a one-vertex arrangement, which is what a disk is. Two more.
//!
//! Both were slots the walk claimed to cover and did not, which is the same
//! class as everything else in this milestone: a mechanism that closes the
//! instance it was shown. The counter is what turned them from invisible into
//! a number.

use serde::Serialize;

use super::audit::{audit, Parts};
use super::{Dcel, FaceId, FacePair, HalfEdgeId, VertexId};

/// One named slot of [`Parts`] and the edit that changes it.
///
/// A named type rather than a tuple in a signature: the walk is the mechanism
/// the audit's resolving power is measured with, and a mechanism whose type is
/// unpronounceable is a mechanism nobody re-reads.
pub(crate) type Perturbation = (&'static str, String, Box<dyn Fn(&mut Parts)>);

/// The scalar-slot families of [`Parts`], in a fixed order.
///
/// REDTEAM_M5 RT5-A10: `face_of_padded_px` is 149 488 of the 155 160 slots —
/// **96.34 %** — and for that family a catch is guaranteed by the SHAPE of the
/// check rather than by its strength, because the perturbation moves the map
/// while the rebuild reconstructs it from `boundaries`, which the perturbation
/// does not touch. "155 160 of 155 160" is true and reads as a coverage it is
/// not. The decomposition belongs in the report rather than in a reviewer's
/// reconstruction from the artifact, so every site declares its family.
pub const SLOT_FAMILIES: &[&str] = &[
    "vertices",
    "boundaries.owners",
    "boundaries.endpoints",
    "boundaries.path",
    "faces.label",
    "faces.loops",
    "face_of_padded_px",
    "site",
];

/// What one mutation walk found.
///
/// ## RT5-A2: two of the three published numbers used to be arithmetic
///
/// The previous version published `caught_by_audit`,
/// `caught_by_assembly_equality` and `caught_by_neither`, and the §28 M5 clause
/// stood on the last being zero. The red team proved it is a THEOREM: `broken`
/// carries the same labelling and convention as `d`, and `d` is always an
/// output of `assemble`, so
///
/// ```text
/// is_the_assembly_of_its_own_labelling(broken)
///   = (assemble(broken.labelling).parts() == broken.parts())
///   = (d.parts() == perturbed_parts)
///   = false, always
/// ```
///
/// so `caught_by_neither == 0` could not be otherwise and
/// `caught_by_assembly_equality == slots - no_ops` identically. What the clause
/// actually required of the audit was `caught_by_audit > 0` — **one slot** —
/// and the red team reduced `audit()` to range guards plus a single check,
/// deleting the whole seventh §12 invariant, and watched the gate stay green
/// with 530 tests passing.
///
/// F-0035 is written at the top of `dcel/report.rs` and the violation was three
/// files away: a conjunct that cannot be false measures the size of the input.
///
/// So both identities are GONE from this type. What is left is the number that
/// carries information and its complement, and the clause stands on the
/// complement being zero — a property the audit can fail, and does fail the
/// moment a check is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct ResolvingPower {
    pub slots: u64,
    /// Perturbations `audit` rejected. The only number here that depends on
    /// what `audit` does.
    pub caught_by_audit: u64,
    /// Perturbations `audit` ACCEPTED. Must be zero: a slot the audit cannot
    /// see is a place a defect can live, which is exactly what RT5-A1 did with
    /// `face_of_padded_px`.
    pub uncaught_by_audit: u64,
    /// Perturbations that changed nothing. Must be zero: a no-op is not a test
    /// of anything and must not be counted as a catch (F-0059).
    pub no_ops: u64,
    /// Slots and catches per family, in [`SLOT_FAMILIES`] order (RT5-A10).
    pub by_family: [FamilyCount; 8],
}

/// One family's share of the walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct FamilyCount {
    pub family: &'static str,
    pub slots: u64,
    pub caught_by_audit: u64,
    pub uncaught_by_audit: u64,
    pub no_ops: u64,
}

/// Perturb every derived slot of an arrangement, one at a time, and count what
/// `audit` catches.
///
/// `is_the_assembly_of_its_own_labelling` is NOT consulted here and that is the
/// point of RT5-A2: on a value built by perturbing an assembled one it is
/// constant-false, so including it made two of three published numbers
/// arithmetic. It remains a useful check for a value that arrived from
/// somewhere else, and `audit_every_labelling` still runs it there.
///
/// No broken `Dcel` escapes: the corrupted values live inside this function.
pub fn measure_audit_resolving_power(d: &Dcel) -> ResolvingPower {
    let mut out = ResolvingPower::default();
    for (i, fam) in SLOT_FAMILIES.iter().enumerate() {
        out.by_family[i].family = fam;
    }
    for (family, _name, f) in d.parts().perturbations() {
        let fi = SLOT_FAMILIES
            .iter()
            .position(|x| *x == family)
            .expect("every perturbation declares a registered family");
        let mut parts = d.parts().clone();
        f(&mut parts);
        out.slots += 1;
        out.by_family[fi].slots += 1;
        if parts == *d.parts() {
            out.no_ops += 1;
            out.by_family[fi].no_ops += 1;
            continue;
        }
        let broken = d.clone().with_parts(parts);
        if audit(&broken).is_err() {
            out.caught_by_audit += 1;
            out.by_family[fi].caught_by_audit += 1;
        } else {
            out.uncaught_by_audit += 1;
            out.by_family[fi].uncaught_by_audit += 1;
        }
    }
    out
}

impl Parts {
    /// One perturbation per scalar slot of the actual data.
    ///
    /// The destructuring below is the mechanism: a field added to `Parts`
    /// without a site here does not compile.
    ///
    /// `pub(crate)`, not public: a way to corrupt the structure that crossed
    /// the crate boundary would be the second mint the module docs say does not
    /// exist. [`measure_audit_resolving_power`] is the public face, and it
    /// returns counts rather than broken values.
    pub(crate) fn perturbations(&self) -> Vec<Perturbation> {
        let Parts {
            vertices,
            boundaries,
            faces,
            face_of_padded_px,
            site,
        } = self;
        let mut out: Vec<Perturbation> = Vec::new();

        for i in 0..vertices.len() {
            for k in 0..2usize {
                out.push((
                    "vertices",
                    format!("vertices[{i}].{k}"),
                    Box::new(move |p: &mut Parts| {
                        let v = &mut p.vertices[i];
                        if k == 0 {
                            v.0 = v.0.wrapping_add(1);
                        } else {
                            v.1 = v.1.wrapping_add(1);
                        }
                    }),
                ));
            }
        }
        for (i, bnd) in boundaries.iter().enumerate() {
            out.push((
                "boundaries.endpoints",
                format!("boundaries[{i}].start"),
                Box::new(move |p: &mut Parts| {
                    // `.max(2)`, not `.max(1)`: an arrangement with ONE vertex
                    // — a single closed chain, which is what a disk is — sent
                    // `(0 + 1) % 1` straight back to 0 and perturbed nothing.
                    // Two silent no-ops per probed arrangement on the M5 run.
                    // With 2 the value always moves, to another vertex or out
                    // of range, and both are corruptions the audit must catch.
                    let n = p.vertices.len() as u32;
                    let b = &mut p.boundaries[i];
                    b.start = VertexId((b.start.0 + 1) % n.max(2));
                }),
            ));
            out.push((
                "boundaries.endpoints",
                format!("boundaries[{i}].end"),
                Box::new(move |p: &mut Parts| {
                    let n = p.vertices.len() as u32;
                    let b = &mut p.boundaries[i];
                    b.end = VertexId((b.end.0 + 1) % n.max(2));
                }),
            ));
            // TWO sites, one per scalar leaf of `FacePair`. A single swap
            // site was one perturbation for two leaves, so the site count
            // could not be compared against the serialized leaf count — and
            // that comparison is what makes `extra: _` fail (RT5-A11).
            out.push((
                "boundaries.owners",
                format!("boundaries[{i}].owners.left"),
                Box::new(move |p: &mut Parts| {
                    let n = p.faces.len() as u32;
                    let b = &mut p.boundaries[i];
                    let (l, r) = (b.owners.left(), b.owners.right());
                    // On a TWO-face arrangement there is no third id to move
                    // to: `(l + 1) % 2` lands on `r`, the skip lands back on
                    // `l`, and the site computes its own input. That was 26
                    // idle slots on the first full run of delta-2, found by the
                    // `no_ops` counter F-0059 exists for — in the site F-0059
                    // was written about, one milestone later.
                    //
                    // The fallback is an id that names no face at all, which is
                    // a real corruption (`face_of` then reports a face outside
                    // the structure) and is caught by the site/owner check.
                    let mut cand = (l.0 + 1) % n.max(2);
                    if cand == r.0 {
                        cand = (cand + 1) % n.max(2);
                    }
                    if cand == l.0 {
                        cand = n;
                    }
                    if let Some(fp) = FacePair::new(FaceId(cand), r) {
                        b.owners = fp;
                    }
                }),
            ));
            out.push((
                "boundaries.owners",
                format!("boundaries[{i}].owners.right"),
                Box::new(move |p: &mut Parts| {
                    // SWAP the two owners. The first version moved the left
                    // owner to `(l + 1) % faces.len()` and skipped `r`, which
                    // on a two-face arrangement walks straight back to `l` and
                    // changes nothing — three no-op perturbations on the M5
                    // corpus run, i.e. three slots the walk claimed to cover
                    // and did not. `no_ops` is published for exactly that
                    // reason and it is what found this.
                    let b = &mut p.boundaries[i];
                    let (l, r) = (b.owners.left(), b.owners.right());
                    if let Some(fp) = FacePair::new(r, l) {
                        b.owners = fp;
                    }
                }),
            ));
            for j in 0..bnd.path.len() {
                // BOTH coordinates. Only `pt.0` was perturbed until delta-2 —
                // about a tenth of the real scalar slots enumerated by nothing,
                // while `walk.rs`, `audit.rs` and ADR-0031 §3 all said "one
                // perturbation per scalar slot of the actual data".
                //
                // Delta-1 REPORTED this fixed. The edit never landed: the diff
                // of `2216959..d042bba` on this file is empty and no document
                // mentioned it (REVIEW_M5_A D1-N2). A finding named in a report
                // and absent from the tree is F-7 in miniature, and it was mine.
                for k in 0..2usize {
                    out.push((
                        "boundaries.path",
                        format!("boundaries[{i}].path[{j}].{k}"),
                        Box::new(move |p: &mut Parts| {
                            let pt = &mut p.boundaries[i].path[j];
                            if k == 0 {
                                pt.0 = pt.0.wrapping_add(1);
                            } else {
                                pt.1 = pt.1.wrapping_add(1);
                            }
                        }),
                    ));
                }
            }
        }
        for (i, face) in faces.iter().enumerate() {
            out.push((
                "faces.label",
                format!("faces[{i}].label"),
                Box::new(move |p: &mut Parts| {
                    p.faces[i].label = !p.faces[i].label;
                }),
            ));
            for j in 0..face.loops.len() {
                for k in 0..face.loops[j].len() {
                    out.push((
                        "faces.loops",
                        format!("faces[{i}].loops[{j}][{k}]"),
                        Box::new(move |p: &mut Parts| {
                            let h = &mut p.faces[i].loops[j][k];
                            *h = HalfEdgeId(h.0 ^ 1);
                        }),
                    ));
                }
            }
        }
        for i in 0..face_of_padded_px.len() {
            out.push((
                "face_of_padded_px",
                format!("face_of_padded_px[{i}]"),
                Box::new(move |p: &mut Parts| {
                    let n = p.faces.len() as u32;
                    p.face_of_padded_px[i] = (p.face_of_padded_px[i] + 1) % n.max(2);
                }),
            ));
        }
        for i in 0..site.len() {
            for k in 0..3usize {
                out.push((
                    "site",
                    format!("site[{i}].{k}"),
                    Box::new(move |p: &mut Parts| {
                        let s = &mut p.site[i];
                        match k {
                            0 => s.0 = s.0.wrapping_add(1),
                            1 => s.1 = s.1.wrapping_add(1),
                            _ => s.2 = s.2.wrapping_add(1),
                        }
                    }),
                ));
            }
        }
        out
    }
}

impl Dcel {
    /// Replace the derived half. `pub(crate)` and used only by
    /// [`measure_audit_resolving_power`], which does not let the result out.
    pub(crate) fn with_parts(mut self, p: Parts) -> Dcel {
        self.parts = p;
        self
    }
}

/// **REDTEAM_M5 RT5-A1, as a callable control.**
///
/// Rotates every entry of the pixel-to-face map when the arrangement is at
/// least `threshold` px wide — the red team's own ten-line edit, which before
/// delta-1 passed 530 tests, four `[MET]` clauses, a byte-identical artifact,
/// `dcel-check`, the exhaustive 4x4 sweep and every knockout.
///
/// It lives in the tree rather than in a deletable clone because that is the
/// red team's tenth obligation and F-7: a check on a mechanism's PRESENCE sees
/// a phrase, and an attack that is not executable is not a control. The §28 M5
/// harness calls it for the clause-4 knockout.
///
/// `pub` on purpose and narrow by construction: it returns a `Dcel` that is
/// deliberately wrong, and the only thing in the workspace that calls it is a
/// knockout whose entire job is to require a clause to go red.
pub fn rotate_face_map_above(d: &Dcel, threshold_px: u32) -> Dcel {
    if d.width_px() < threshold_px {
        return d.clone();
    }
    let mut parts = d.parts().clone();
    let nf = parts.faces.len() as u32;
    for v in parts.face_of_padded_px.iter_mut() {
        *v = (*v + 1) % nf.max(1);
    }
    d.clone().with_parts(parts)
}

/// **REVIEW_M5_A D1-N1 / REDTEAM_M5 RT5-A9, as a callable control.**
///
/// The corruption delta-1 did NOT catch: a relabelling of faces that leaves
/// every structural relation internally consistent and attaches the wrong
/// LABEL to a face's pixels.
///
/// Reviewer A found the class by publishing a refuted hypothesis first — moving
/// the red team's global rotation above the sampling point IS caught, because a
/// global rotation moves the exterior off id 0, and that single bit was the
/// audit's only external anchor. A corruption respecting that bit was caught by
/// nothing: 536 tests green, four `[MET]`, byte-identical artifact, `audit()`
/// returning `None` and `face_map_agrees` returning true.
///
/// This is the smallest faithful form of it. Swapping the labels of one
/// foreground face and one non-exterior background face keeps EVERY check
/// delta-1 had:
///
/// - `face_map_agrees` rebuilds from `owners`, which are untouched — and which
///   `assemble` samples out of `face_of_padded_px` in the first place, so the
///   rebuild is downstream of what it certifies;
/// - the site and owner checks are untouched;
/// - the Euler identity is untouched;
/// - even the SIGNATURE comparison survives, because swapping one foreground
///   label for one background label leaves both counts exactly where they were.
///
/// What it breaks is the only thing nothing was comparing: the pixels now sit
/// in a face whose label contradicts the labelling they were built from.
pub fn swap_two_face_labels_above(d: &Dcel, threshold_px: u32) -> Dcel {
    if d.width_px() < threshold_px {
        return d.clone();
    }
    let fg = d.parts().faces.iter().position(|f| f.label);
    let bg = d
        .parts()
        .faces
        .iter()
        .enumerate()
        .position(|(i, f)| i != 0 && !f.label);
    let (Some(fg), Some(bg)) = (fg, bg) else {
        return d.clone();
    };
    let mut parts = d.parts().clone();
    parts.faces[fg].label = false;
    parts.faces[bg].label = true;
    d.clone().with_parts(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubical::Labelling;
    use crate::dcel::audit::is_the_assembly_of_its_own_labelling;
    use crate::dcel::Dcel;
    use vice_ir::ComplementaryConnectivity;

    fn ring() -> Dcel {
        let (w, h) = (13usize, 13usize);
        let inside: Vec<bool> = (0..w * h)
            .map(|i| {
                let (dx, dy) = ((i % w) as f64 - 6.0, (i / w) as f64 - 6.0);
                let r2 = dx * dx + dy * dy;
                (4.0..=30.0).contains(&r2)
            })
            .collect();
        Dcel::assemble(
            Labelling::new(w, h, inside),
            ComplementaryConnectivity::arms()[0],
        )
    }

    /// Every scalar slot of the structure, perturbed one at a time, is caught.
    ///
    /// Both directions are asserted: the walk must be non-empty (an empty
    /// control is indistinguishable from a passing one), and the UNperturbed
    /// value must pass both checks (a checker that always fails would satisfy
    /// the first half and measure nothing).
    #[test]
    fn every_perturbation_of_every_slot_is_caught_by_the_audit_alone() {
        let d = ring();
        assert!(audit(&d).is_ok(), "positive control: the real one passes");
        assert!(is_the_assembly_of_its_own_labelling(&d));

        let ps = d.parts().perturbations();
        assert!(
            ps.len() > 200,
            "the walk found only {} slots; it is not covering the structure",
            ps.len()
        );

        let mut by_audit = 0usize;
        for (_family, name, f) in &ps {
            let mut parts = d.parts().clone();
            f(&mut parts);
            if parts == *d.parts() {
                // A no-op perturbation is not a test of anything, and it must
                // not be counted as a pass. There are none, and this asserts
                // it rather than assuming it.
                panic!("perturbation {name} changed nothing");
            }
            let broken = d.clone().with_parts(parts);
            assert!(
                audit(&broken).is_err(),
                "perturbation {name} was accepted by the audit. RT5-A1 is what an unchecked slot                  costs: a field no predicate reads passes 530 tests, four MET clauses and a                  byte-identical artifact"
            );
            by_audit += 1;
        }
        println!("slots {} | caught by audit {}", ps.len(), by_audit);
        assert_eq!(
            by_audit,
            ps.len(),
            "the audit must catch EVERY real perturbation. `assembly equality is total` used to              stand here and it was a theorem (RT5-A2), so it certified nothing"
        );
    }

    /// **RT5-A11: a field must be EXERCISED, not merely mentioned.**
    ///
    /// The exhaustive destructuring in `Parts::perturbations` is judged by the
    /// compiler — but what the compiler judges is that every field is NAMED.
    /// The red team measured the bypass literally: a new field with a public
    /// reader, corrupted above 16 px, plus one line `face_area_px: _` in the
    /// pattern — clippy clean, gate EXIT 0, four MET, **and the slot count did
    /// not move**. The compensation delta-1 claimed through `PartialEq` is nil
    /// twice over: the walk no longer calls assembly-equality (that WAS the
    /// RT5-A2 fix), and `audit_every_labelling` compares two copies carrying
    /// the same defect.
    ///
    /// So the site count is compared against a count derived from somewhere
    /// the walk does not control: the number of scalar leaves of the SERIALIZED
    /// `Parts`. That number comes from the `Serialize` derive, i.e. from the
    /// struct definition. A field added as `extra: _` moves the leaf count and
    /// not the site count, and this fails.
    ///
    /// Both directions: the count must also be non-trivial, because an empty
    /// structure would satisfy `0 == 0`.
    #[test]
    fn every_scalar_leaf_of_parts_has_exactly_one_perturbation() {
        fn leaves(v: &serde_json::Value) -> u64 {
            match v {
                serde_json::Value::Array(a) => a.iter().map(leaves).sum(),
                serde_json::Value::Object(o) => o.values().map(leaves).sum(),
                serde_json::Value::Null => 0,
                _ => 1,
            }
        }
        for d in [
            ring(),
            Dcel::assemble(
                Labelling::new(9, 9, (0..81).map(|i| (i / 9) % 3 == 1).collect()),
                ComplementaryConnectivity::arms()[1],
            ),
        ] {
            let value = serde_json::to_value(d.parts()).expect("Parts serializes");
            let n = leaves(&value);
            assert!(n > 100, "only {n} scalar leaves; the walk covers nothing");
            assert_eq!(
                d.parts().perturbations().len() as u64,
                n,
                "one perturbation per SCALAR LEAF of the serialized structure. The leaf count \
                 comes from the Serialize derive, so a field written `extra: _` in the walk's \
                 pattern moves this side and not the other (RT5-A11)"
            );
        }
    }

    /// **RT5-A14: what each check uniquely carries, measured here rather than
    /// quoted.**
    ///
    /// The mutation walk cannot see the labelling anchor at all, and that is a
    /// property of the INSTRUMENT rather than a defect in the anchor: the walk
    /// is made of perturbations of a CORRECT `Parts`, and the anchor's whole
    /// domain is defects inside `assemble`, which are not perturbations of
    /// anything (F-0066). Switching the anchor off moves no slot count and no
    /// artifact byte.
    ///
    /// So the two checks are measured against each other directly, over the
    /// same perturbations, and the numbers the clause-4 row cites come from
    /// here. What this establishes is that they are NOT redundant: each catches
    /// slots the other does not, so citing both is not double-counting one.
    #[test]
    fn the_anchor_and_the_rebuild_carry_disjoint_loads() {
        let d = ring();
        let (mut only_rebuild, mut only_anchor, mut both, mut neither) = (0, 0, 0, 0);
        for (_family, _name, f) in d.parts().perturbations() {
            let mut parts = d.parts().clone();
            f(&mut parts);
            if parts == *d.parts() {
                continue;
            }
            let broken = d.clone().with_parts(parts);
            let rebuild = crate::dcel::crossing::face_map_agrees(&broken).is_err();
            let anchor = anchor_disagrees(&broken);
            match (rebuild, anchor) {
                (true, false) => only_rebuild += 1,
                (false, true) => only_anchor += 1,
                (true, true) => both += 1,
                (false, false) => neither += 1,
            }
        }
        println!(
            "rebuild-only {only_rebuild} | anchor-only {only_anchor} | both {both} | neither \
             {neither}"
        );
        assert!(
            only_rebuild > 0,
            "the rebuild carries nothing the anchor does not, so clause 4 would be citing one \
             check twice"
        );
        // Whatever neither sees must still be caught by the rest of the audit,
        // which `every_perturbation_of_every_slot_is_caught_by_the_audit_alone`
        // asserts over the same population.
    }

    /// **RT5-A12 = M5B-N14: the leaf judge keys on the SERIALIZATION.**
    ///
    /// `every_scalar_leaf_of_parts_has_exactly_one_perturbation` compares the
    /// site count against the scalar leaves of `serde_json::to_value(parts)`.
    /// Two contexts found the same bypass by executing it: `#[serde(skip)]` on
    /// the new field plus `extra: _` in the walk's pattern — **two lines**,
    /// clippy `-D warnings` clean, every test green, the field invisible to the
    /// walk, to the leaf count and to the artifact.
    ///
    /// The judge's key is the `Serialize` derive, which is an ATTRIBUTE ON THE
    /// SAME SURFACE the attacker edits, in the same commit. B states the rule:
    /// "a count the attacker does not control" is verified by ENUMERATING the
    /// ways to control it, and a derive is a function of attributes on the
    /// thing being checked rather than a constant of nature.
    ///
    /// So the ways to control it are enumerated, over the source of the struct
    /// itself. Both directions: the scan must FIND the struct (an empty scan is
    /// vacuously compliant, F-0039), and it must reject any serde attribute
    /// inside it.
    ///
    /// **The honest residual**, at the cheapest known price: this is a text
    /// scan, which F-0048 Q3 calls a habit rather than a judge. Renaming the
    /// struct, moving it to another file, or writing the attribute in a form
    /// this scan does not match all defeat it. What it does close is the
    /// two-line bypass that was measured, and the leaf count keeps its own
    /// independent job. A judge that does not share a surface with `Parts` at
    /// all would need reflection Rust does not have; the price of the nearest
    /// thing — a proc-macro deriving both the perturbation sites and the
    /// count from one definition — is a new crate and is recorded in
    /// `docs/STATUS_M5.md`.
    #[test]
    fn no_field_of_parts_can_hide_from_the_serializer() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/dcel/audit.rs"),
        )
        .expect("audit.rs");
        let start = src
            .find("pub struct Parts {")
            .expect("the struct this walk is about must be findable");
        let end = start
            + src[start..]
                .find("\n}\n")
                .expect("the struct must be closed");
        let body = &src[start..end];
        assert!(
            body.lines().count() > 5,
            "the scan found a {}-line body; it is not reading the struct",
            body.lines().count()
        );
        for (i, line) in body.lines().enumerate() {
            let t = line.trim();
            assert!(
                !t.starts_with("#[serde(") && !t.contains("serde(skip"),
                "line {i} of `Parts` carries a serde attribute: {t:?}. The leaf-count judge is \
                 keyed on the serialization, so an attribute here moves the judge's own ruler \
                 (RT5-A12, M5B-N14)"
            );
        }
        // And the positive control: the derive the judge depends on is present.
        let head = &src[start.saturating_sub(200)..start];
        assert!(
            head.contains("Serialize"),
            "`Parts` no longer derives Serialize, so the leaf count measures nothing"
        );
    }

    /// Every perturbation declares a family, and every family is exercised.
    ///
    /// RT5-A10: 96.34 % of the slots are one family, and for that family a
    /// catch is guaranteed by the shape of the check. The decomposition is
    /// published, so a family that silently went empty is visible as a zero
    /// rather than absorbed into a total.
    #[test]
    fn every_slot_family_is_non_empty_and_declared() {
        let d = ring();
        let r = measure_audit_resolving_power(&d);
        let total: u64 = r.by_family.iter().map(|f| f.slots).sum();
        assert_eq!(total, r.slots, "the families must partition the walk");
        for f in &r.by_family {
            assert!(
                SLOT_FAMILIES.contains(&f.family),
                "unregistered family {:?}",
                f.family
            );
            assert!(f.slots > 0, "family {:?} contributed no slot", f.family);
            assert_eq!(
                f.uncaught_by_audit, 0,
                "family {:?} has an uncaught slot",
                f.family
            );
        }
        println!(
            "families: {:?}",
            r.by_family
                .iter()
                .map(|f| (f.family, f.slots))
                .collect::<Vec<_>>()
        );
    }

    /// The anchor, as a predicate, for the measurement above.
    fn anchor_disagrees(d: &Dcel) -> bool {
        let (w, h) = (d.width_px(), d.height_px());
        for y in 0..h {
            for x in 0..w {
                let want = d.labelling().inside()[y as usize * w as usize + x as usize];
                match d.faces().get(d.face_of_pixel(x, y).index()) {
                    Some(f) if f.label == want => {}
                    _ => return true,
                }
            }
        }
        false
    }
}
