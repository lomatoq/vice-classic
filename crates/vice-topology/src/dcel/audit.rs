//! The audit: the part of §12 that is a COMPUTATION, and the measurement of
//! how much it can see.
//!
//! Six of §12's seven invariants have no failure mode in this representation
//! (see the [`super`] module table). This file is about the seventh —
//! "Euler/cubical signature preserved" — plus the properties of the ASSEMBLY
//! itself, which are the things a wrong constructor could get wrong.
//!
//! ## Two questions, and neither implies the other
//!
//! [`audit`] asks: **do the construction invariants hold for this value?** It
//! can fail on a corrupted value and it can fail on a systematically wrong
//! `assemble`. That is the check the §28 M5 clause "no dangling/invalid faces"
//! stands on.
//!
//! [`is_the_assembly_of_its_own_labelling`] asks: **is this value what
//! `assemble` would produce from the labelling it carries?** It is blind to a
//! systematically wrong `assemble` — a wrong constructor agrees with itself —
//! and it is the check that catches a value nobody assembled.
//!
//! They are published separately and their results are reported separately,
//! because a conjunct implied by its neighbour is a paraphrase rather than a
//! second witness (M45-N8, RT45-A6). Neither implies the other, and the
//! mutation walk below measures each one's share.
//!
//! ## The mutation walk, and why it is not a list of perturbations
//!
//! A control that tries three hand-picked corruptions closes three
//! corruptions. F-0048 Q1 asks whether the mechanism contains a literal
//! enumerating its subjects; a `vec![corrupt_a, corrupt_b, corrupt_c]` answers
//! yes, and the next finding is a fourth entry.
//!
//! [`Parts::perturbations`] instead DESTRUCTURES [`Parts`] exhaustively and
//! emits one perturbation per scalar slot of the actual data. Two consequences:
//!
//! - a field added to `Parts` without a site does not compile, because the
//!   pattern is exhaustive. The judge is the compiler, which is the form the
//!   project already accepted for `TopologyGateConfig::sites` (F-0048's list
//!   of good forms);
//! - the number of perturbations is a function of the DATA, so a bigger
//!   arrangement is a wider control automatically, and a control that went
//!   empty would be visible as a count of zero. That direction is asserted:
//!   `every_perturbation_of_every_slot_is_caught` fails on an empty walk as
//!   loudly as on an uncaught mutation, because an empty control is
//!   indistinguishable from a passing one from the outside (F-0039).

use serde::Serialize;
use vice_ir::ComplementaryConnectivity;

use super::lattice::{Arrangement, Lat};
use super::{Boundary, Dcel, Face, FaceId};
use crate::cubical::{signature, Labelling};

/// The derived half of a [`Dcel`].
///
/// Split out so that the audit has something to compare and the mutation walk
/// has something to walk. `pub(crate)`: outside this crate there is no way to
/// obtain, build or modify one, so the module's "only one constructor" claim
/// is not weakened by its existence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Parts {
    pub(crate) vertices: Vec<(u32, u32)>,
    pub(crate) boundaries: Vec<Boundary>,
    pub(crate) faces: Vec<Face>,
    pub(crate) face_of_padded_px: Vec<u32>,
    pub(crate) site: Vec<(u32, u32, u32)>,
}

/// What the audit found. Every variant names a quantity, because a violation
/// that only says "invalid" is read at the moment something is broken and is
/// then useless.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvariantViolation {
    #[error("the boundary successor is not a permutation of the directed segments: {detail}")]
    SuccessorNotAPermutation { detail: String },
    #[error(
        "Euler identity violated: V {vertices} - B {boundaries} + L {loops} = {lhs}, but the \
         1-skeleton has {skeleton_components} component(s) and a planar arrangement requires 2C = \
         {rhs}"
    )]
    EulerIdentity {
        vertices: i64,
        boundaries: i64,
        loops: i64,
        skeleton_components: i64,
        lhs: i64,
        rhs: i64,
    },
    #[error(
        "the arrangement says {dcel_components} component(s) and {dcel_holes} hole(s); the \
         independent union-find signature of the same labelling says {sig_components} and \
         {sig_holes}"
    )]
    DisagreesWithSignature {
        dcel_components: u32,
        dcel_holes: u32,
        sig_components: u32,
        sig_holes: u32,
    },
    #[error("boundary {boundary} appears in {uses} loop position(s); a shared boundary has two")]
    BoundaryNotUsedTwice { boundary: usize, uses: usize },
    #[error("boundary {boundary}: {detail}")]
    MalformedPath { boundary: usize, detail: String },
    #[error(
        "half-edge {half_edge} sits in face {sited_face} but its owners put it on face \
         {owned_face}"
    )]
    SiteDisagreesWithOwners {
        half_edge: u32,
        sited_face: u32,
        owned_face: u32,
    },
    #[error("face {face} has no loop, and every face of a non-empty arrangement is bounded")]
    FaceWithoutLoops { face: usize },
    #[error(
        "half-edge {half_edge} is sited at face {face}, loop {loop_index}, position {position}, \
         which is not a place in this structure"
    )]
    SiteOutOfRange {
        half_edge: u32,
        face: u32,
        loop_index: u32,
        position: u32,
    },
    #[error("the exterior face {0:?} is not a background face")]
    ExteriorIsNotBackground(FaceId),
    #[error(
        "the labelling has no interface at all, but the arrangement is not empty: {boundaries}          boundary chain(s), {loops} loop(s), {faces} face(s)"
    )]
    EmptyArrangementIsNotEmpty {
        boundaries: usize,
        loops: usize,
        faces: usize,
    },
}

/// What an audit measured, published beside the verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AuditReport {
    pub vertices: u32,
    pub boundaries: u32,
    pub segments: u32,
    pub loops: u32,
    pub faces: u32,
    pub foreground_faces: u32,
    pub holes: u32,
    pub skeleton_components: u32,
    pub directed_steps: u32,
}

/// Run the construction invariants over one arrangement.
///
/// Each check below can fail. None is implied by another, and the reason is
/// stated at each: the permutation check is about the successor rule, the
/// Euler identity is about the loop extraction, the signature comparison is
/// about the face flood fill, and the site/owner checks are about the index.
pub fn audit(d: &Dcel) -> Result<AuditReport, InvariantViolation> {
    let arr = Arrangement::new(
        d.labelling().inside(),
        d.width_px(),
        d.height_px(),
        d.connectivity(),
    );

    // (0) The EMPTY arrangement, and why it is a case rather than a failure.
    //
    // A labelling with no interface — every pixel background — has a perfectly
    // good arrangement: one face, the exterior, and nothing else. It arrives
    // from the corpus, not from a fixture: `adv/sliver` is thinner than a pixel
    // and the §5.3 majority rule digitizes it to nothing, on 8 of 474 arms.
    //
    // The first version of this file refused it, because
    // `successor_is_a_permutation` refuses an empty step set — correctly, since
    // an empty relation is vacuously injective and vacuous success is
    // indistinguishable from success (F-0039). But that refusal is about the
    // PERMUTATION check having nothing to measure, and turning it into "this
    // arrangement is invalid" confuses an instrument's silence with a verdict,
    // which is meta-rule M-4.
    //
    // So the empty case is CHECKED rather than waved through: it must actually
    // be empty. The arm is then marked by `directed_steps == 0` and the §28 M5
    // report counts it separately, so a clause cannot be carried by arms that
    // contain nothing.
    if arr.steps().is_empty() {
        if !d.boundaries().is_empty() || d.loop_count() != 0 || d.faces().len() != 1 {
            return Err(InvariantViolation::EmptyArrangementIsNotEmpty {
                boundaries: d.boundaries().len(),
                loops: d.loop_count(),
                faces: d.faces().len(),
            });
        }
        if d.faces()[FaceId::EXTERIOR.index()].label {
            return Err(InvariantViolation::ExteriorIsNotBackground(
                FaceId::EXTERIOR,
            ));
        }
        return Ok(AuditReport {
            vertices: 0,
            boundaries: 0,
            segments: 0,
            loops: 0,
            faces: 1,
            foreground_faces: 0,
            holes: 0,
            skeleton_components: 0,
            directed_steps: 0,
        });
    }

    // (1) The successor rule. Everything downstream is orbits of this, so if
    // it is not a permutation the loops are not cycles.
    let steps = match arr.successor_is_a_permutation() {
        Ok(n) => n,
        Err(detail) => return Err(InvariantViolation::SuccessorNotAPermutation { detail }),
    };

    // (2) Paths are unit lattice steps inside the canvas, and each boundary
    // starts and ends at its declared vertices. A path is the one place the
    // structure carries coordinates, so it is the one place a coordinate can
    // be wrong.
    for (i, b) in d.boundaries().iter().enumerate() {
        if b.path.len() < 2 {
            return Err(InvariantViolation::MalformedPath {
                boundary: i,
                detail: format!(
                    "path has {} point(s); a boundary has at least two",
                    b.path.len()
                ),
            });
        }
        let verts = d.vertices();
        let (s, e) = (b.start.index(), b.end.index());
        if s >= verts.len() || e >= verts.len() {
            return Err(InvariantViolation::MalformedPath {
                boundary: i,
                detail: format!("endpoint vertex ids {s}/{e} out of range {}", verts.len()),
            });
        }
        if verts[s] != b.path[0] || verts[e] != b.path[b.path.len() - 1] {
            return Err(InvariantViolation::MalformedPath {
                boundary: i,
                detail: format!(
                    "endpoints {:?}/{:?} do not match the path ends {:?}/{:?}",
                    verts[s],
                    verts[e],
                    b.path[0],
                    b.path[b.path.len() - 1]
                ),
            });
        }
        for w in b.path.windows(2) {
            let (a, c) = (w[0], w[1]);
            let dx = i64::from(c.0) - i64::from(a.0);
            let dy = i64::from(c.1) - i64::from(a.1);
            if dx.abs() + dy.abs() != 1 {
                return Err(InvariantViolation::MalformedPath {
                    boundary: i,
                    detail: format!("{a:?} -> {c:?} is not a unit lattice step"),
                });
            }
            if !arr.in_lattice(Lat { x: c.0, y: c.1 }) {
                return Err(InvariantViolation::MalformedPath {
                    boundary: i,
                    detail: format!("{c:?} is outside the lattice"),
                });
            }
        }
    }

    // (3) Every boundary is used by exactly two half-edge positions: §12's
    // "every interior boundary has two owners" is unrepresentable, but "the
    // two owners actually walk it" is a property of the loop lists and can be
    // false.
    let mut uses = vec![0usize; d.boundaries().len() * 2];
    for (fi, f) in d.faces().iter().enumerate() {
        if f.loops.is_empty() {
            return Err(InvariantViolation::FaceWithoutLoops { face: fi });
        }
        for lp in &f.loops {
            for h in lp {
                if (h.0 as usize) >= uses.len() {
                    return Err(InvariantViolation::BoundaryNotUsedTwice {
                        boundary: h.boundary().index(),
                        uses: 0,
                    });
                }
                uses[h.0 as usize] += 1;
                let owned = d.face_of(*h).0;
                if owned != fi as u32 {
                    return Err(InvariantViolation::SiteDisagreesWithOwners {
                        half_edge: h.0,
                        sited_face: fi as u32,
                        owned_face: owned,
                    });
                }
            }
        }
    }
    for (i, b) in d.boundaries().iter().enumerate() {
        let n = uses[i * 2] + uses[i * 2 + 1];
        if n != 2 || uses[i * 2] != 1 {
            return Err(InvariantViolation::BoundaryNotUsedTwice {
                boundary: i,
                uses: n,
            });
        }
        let _ = b;
    }

    // (4) `next` must agree with the loop lists it is read from, on every
    // half-edge. The site index is derived data and derived data can drift.
    //
    // The range check comes first and is not decoration: `next` indexes the
    // loop lists through the site, and an audit that panicked on a structure
    // it was asked to judge would report a broken instrument as a crash. An
    // instrument says what it found (M-4).
    for h in d.half_edges() {
        let (f, l, p) = d.site_of(h);
        let ok = (f as usize) < d.faces().len()
            && (l as usize) < d.faces()[f as usize].loops.len()
            && (p as usize) < d.faces()[f as usize].loops[l as usize].len();
        if !ok {
            return Err(InvariantViolation::SiteOutOfRange {
                half_edge: h.0,
                face: f,
                loop_index: l,
                position: p,
            });
        }
    }
    for h in d.half_edges() {
        if d.face_of(d.next(h)) != d.face_of(h) {
            return Err(InvariantViolation::SiteDisagreesWithOwners {
                half_edge: h.0,
                sited_face: d.face_of(d.next(h)).0,
                owned_face: d.face_of(h).0,
            });
        }
    }

    // (5) The exterior is a background face. §5.3 wants it to be a real
    // `FaceId`; that it is index zero is arithmetic, that it is BACKGROUND is
    // a fact about the flood fill.
    if d.faces()[FaceId::EXTERIOR.index()].label {
        return Err(InvariantViolation::ExteriorIsNotBackground(
            FaceId::EXTERIOR,
        ));
    }

    // (6) Euler. V - B + L = 2C for a planar arrangement whose 1-skeleton has
    // C components: substitute F = L - C + 1 into V - B + F = 1 + C.
    //
    // This shares NOTHING with the flood fill: V, B and L come from the loop
    // extraction and C from a union-find over VERTICES, while the faces come
    // from a union-find over PIXELS. It is the intrinsic check, and it is the
    // one that would catch a loop extraction that lost a loop.
    let c = skeleton_components(d);
    let (v, bnd, l) = (
        d.vertices().len() as i64,
        d.boundaries().len() as i64,
        d.loop_count() as i64,
    );
    let (lhs, rhs) = (v - bnd + l, 2 * c);
    if lhs != rhs {
        return Err(InvariantViolation::EulerIdentity {
            vertices: v,
            boundaries: bnd,
            loops: l,
            skeleton_components: c,
            lhs,
            rhs,
        });
    }

    // (7) Agreement with the M4.5 instrument. Different mathematics for the
    // same quantity: union-find over pixels against faces-and-loops. The
    // shared link is named rather than left to be discovered — both read the
    // same `ComplementaryConnectivity` and the same labelling, so this
    // comparison is blind to a wrong CONVENTION and is not evidence about
    // §5.3. It is evidence about the flood fill and the face numbering.
    let sig = signature(d.labelling(), d.connectivity());
    let (dc, dh) = (d.foreground_faces() as u32, d.holes() as u32);
    if dc != sig.components || dh != sig.holes {
        return Err(InvariantViolation::DisagreesWithSignature {
            dcel_components: dc,
            dcel_holes: dh,
            sig_components: sig.components,
            sig_holes: sig.holes,
        });
    }

    Ok(AuditReport {
        vertices: v as u32,
        boundaries: bnd as u32,
        segments: d.segment_count() as u32,
        loops: l as u32,
        faces: d.faces().len() as u32,
        foreground_faces: dc,
        holes: dh,
        skeleton_components: c as u32,
        directed_steps: steps as u32,
    })
}

/// Is this value the assembly of the labelling it carries?
///
/// Blind to a systematically wrong `assemble` — a wrong constructor agrees
/// with itself — and that is exactly why it is reported separately from
/// [`audit`] rather than folded into it.
pub fn is_the_assembly_of_its_own_labelling(d: &Dcel) -> bool {
    let fresh = Dcel::assemble(d.labelling().clone(), d.connectivity());
    fresh.parts() == d.parts()
}

/// Components of the 1-skeleton (vertices joined by boundaries).
fn skeleton_components(d: &Dcel) -> i64 {
    let n = d.vertices().len();
    if n == 0 {
        return 0;
    }
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    for b in d.boundaries() {
        let (a, c) = (
            find(&mut parent, b.start.index()),
            find(&mut parent, b.end.index()),
        );
        if a != c {
            let (lo, hi) = if a < c { (a, c) } else { (c, a) };
            parent[hi] = lo;
        }
    }
    let mut roots = std::collections::BTreeSet::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        roots.insert(r);
    }
    roots.len() as i64
}

/// Assemble every labelling of a `w x h` grid under both convention arms and
/// audit each one.
///
/// This is the answer to F-0054 / F-9 in the only form that closes the class:
/// the witness set IS the input space, so there is no subclass left in which a
/// defect could be unreachable. `(w, h) = (4, 4)` is 65 536 labellings and
/// 131 072 arrangements.
pub fn audit_every_labelling(w: u32, h: u32) -> Result<ExhaustiveReport, String> {
    let n = (w * h) as usize;
    assert!(
        n <= 20,
        "the exhaustive sweep is 2^n; {n} bits is not a sweep"
    );
    let mut audited = 0u64;
    let mut empty = 0u64;
    let mut classes: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
    let mut critical = 0u64;
    for bits in 0u64..(1u64 << n) {
        let inside: Vec<bool> = (0..n).map(|i| bits & (1 << i) != 0).collect();
        let has_critical = !crate::cubical::critical_cells(&Labelling::new(
            w as usize,
            h as usize,
            inside.clone(),
        ))
        .is_empty();
        for conn in ComplementaryConnectivity::arms() {
            let l = Labelling::new(w as usize, h as usize, inside.clone());
            let is_empty = l.count_inside() == 0;
            if is_empty {
                // Covered rather than skipped, since C243: the corpus reaches
                // this state and the sweep that was supposed to leave no
                // subclass unreached was excluding one BY CONSTRUCTION.
                empty += 1;
            }
            let d = Dcel::assemble(l, conn);
            let r = audit(&d).map_err(|e| format!("bits={bits} conn={conn:?}: {e}"))?;
            if !is_the_assembly_of_its_own_labelling(&d) {
                return Err(format!("bits={bits} conn={conn:?}: not its own assembly"));
            }
            classes.insert((r.foreground_faces, r.holes));
            audited += 1;
        }
        if has_critical {
            critical += 1;
        }
    }
    Ok(ExhaustiveReport {
        width_px: w,
        height_px: h,
        arrangements_audited: audited,
        empty_arrangements_covered: empty,
        distinct_classes: classes.len() as u32,
        classes: classes.into_iter().collect(),
        labellings_with_a_critical_cell: critical,
    })
}

/// What an exhaustive sweep saw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExhaustiveReport {
    pub width_px: u32,
    pub height_px: u32,
    pub arrangements_audited: u64,
    /// Labellings with no interface at all. Audited like every other one since
    /// C243 — the corpus reaches this state (`adv/sliver`) and a sweep that
    /// excluded it was leaving a subclass unreached by construction, which is
    /// the very class F-0054 / F-9 names.
    pub empty_arrangements_covered: u64,
    pub distinct_classes: u32,
    pub classes: Vec<(u32, u32)>,
    pub labellings_with_a_critical_cell: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn every_perturbation_of_every_slot_is_caught() {
        let d = ring();
        assert!(audit(&d).is_ok(), "positive control: the real one passes");
        assert!(is_the_assembly_of_its_own_labelling(&d));

        let ps = d.parts().perturbations();
        assert!(
            ps.len() > 200,
            "the walk found only {} slots; it is not covering the structure",
            ps.len()
        );

        let (mut by_audit, mut by_assembly) = (0usize, 0usize);
        for (name, f) in &ps {
            let mut parts = d.parts().clone();
            f(&mut parts);
            if parts == *d.parts() {
                // A no-op perturbation is not a test of anything, and it must
                // not be counted as a pass. There are none, and this asserts
                // it rather than assuming it.
                panic!("perturbation {name} changed nothing");
            }
            let broken = d.clone().with_parts(parts);
            let a = audit(&broken).is_err();
            let b = !is_the_assembly_of_its_own_labelling(&broken);
            assert!(a || b, "perturbation {name} was caught by neither check");
            if a {
                by_audit += 1;
            }
            if b {
                by_assembly += 1;
            }
        }
        // Published rather than asserted at a threshold: the split is the
        // honest measurement of how much each check sees, and pinning it would
        // freeze a number nothing chose.
        println!(
            "slots {} | caught by audit {} | caught by assembly-equality {}",
            ps.len(),
            by_audit,
            by_assembly
        );
        assert!(by_audit > 0, "the audit caught nothing at all");
        assert_eq!(by_assembly, ps.len(), "assembly equality is total");
    }

    /// The audit is green on every arrangement of every labelling of a 4x3
    /// grid, under both conventions, and the sweep SEES more than one
    /// topological class — an exhaustive run over a space with one answer in
    /// it would prove nothing.
    #[test]
    fn the_audit_is_green_over_a_whole_small_input_space() {
        let r = audit_every_labelling(4, 3).expect("exhaustive audit");
        assert_eq!(r.arrangements_audited, 2 * (1 << 12));
        assert_eq!(r.empty_arrangements_covered, 2);
        assert!(r.distinct_classes >= 6, "classes seen: {:?}", r.classes);
        assert!(
            r.classes.contains(&(1, 1)),
            "the ring class must be in a 4x3 sweep: {:?}",
            r.classes
        );
        assert!(
            r.labellings_with_a_critical_cell > 0,
            "a sweep with no critical 2x2 never exercises the convention branch"
        );
    }
}
