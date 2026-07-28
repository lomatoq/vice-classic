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
//!   project already accepted for `TopologyGateConfig::sites`;
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

use super::audit::{audit, is_the_assembly_of_its_own_labelling, Parts};
use super::{Dcel, FacePair, HalfEdgeId, VertexId};

/// One named slot of [`Parts`] and the edit that changes it.
///
/// A named type rather than a tuple in a signature: the walk is the mechanism
/// the audit's resolving power is measured with, and a mechanism whose type is
/// unpronounceable is a mechanism nobody re-reads.
pub(crate) type Perturbation = (String, Box<dyn Fn(&mut Parts)>);

/// What one mutation walk found.
///
/// Published by the §28 M5 gate rather than kept in a test, because "the audit
/// is green" says nothing about what the audit can SEE. This is the world in
/// which it is red, counted (F-0035: exhibit the world where the conjunct is
/// false before publishing the conjunct).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct ResolvingPower {
    pub slots: u64,
    pub caught_by_audit: u64,
    pub caught_by_assembly_equality: u64,
    /// Must be zero. A slot no check sees is a place a defect can live.
    pub caught_by_neither: u64,
    /// Perturbations that changed nothing. Must be zero: a no-op is not a test
    /// of anything and must not be counted as a catch.
    pub no_ops: u64,
}

/// Perturb every derived slot of an arrangement, one at a time, and count what
/// each check catches.
///
/// No broken `Dcel` escapes: the corrupted values live inside this function.
pub fn measure_audit_resolving_power(d: &Dcel) -> ResolvingPower {
    let mut out = ResolvingPower::default();
    for (_name, f) in d.parts().perturbations() {
        let mut parts = d.parts().clone();
        f(&mut parts);
        out.slots += 1;
        if parts == *d.parts() {
            out.no_ops += 1;
            continue;
        }
        let broken = d.clone().with_parts(parts);
        let a = audit(&broken).is_err();
        let b = !is_the_assembly_of_its_own_labelling(&broken);
        if a {
            out.caught_by_audit += 1;
        }
        if b {
            out.caught_by_assembly_equality += 1;
        }
        if !a && !b {
            out.caught_by_neither += 1;
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
                format!("boundaries[{i}].end"),
                Box::new(move |p: &mut Parts| {
                    let n = p.vertices.len() as u32;
                    let b = &mut p.boundaries[i];
                    b.end = VertexId((b.end.0 + 1) % n.max(2));
                }),
            ));
            out.push((
                format!("boundaries[{i}].owners"),
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
                out.push((
                    format!("boundaries[{i}].path[{j}]"),
                    Box::new(move |p: &mut Parts| {
                        let pt = &mut p.boundaries[i].path[j];
                        pt.0 = pt.0.wrapping_add(1);
                    }),
                ));
            }
        }
        for (i, face) in faces.iter().enumerate() {
            out.push((
                format!("faces[{i}].label"),
                Box::new(move |p: &mut Parts| {
                    p.faces[i].label = !p.faces[i].label;
                }),
            ));
            for j in 0..face.loops.len() {
                for k in 0..face.loops[j].len() {
                    out.push((
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
