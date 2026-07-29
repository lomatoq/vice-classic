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

use super::lattice::{Arrangement, Lat};
use super::{Boundary, Dcel, Face, FaceId};
use crate::cubical::signature;

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
        "boundary {boundary} passes through vertex {vertex:?} at interior position {position} of          {length}; §12 asks for MAXIMAL shared boundary chains, and a chain that contains a vertex          is two chains"
    )]
    ChainIsNotMaximal {
        boundary: usize,
        vertex: (u32, u32),
        position: usize,
        length: usize,
    },
    #[error("the face loops disagree with the loops of the labelling: {0}")]
    LoopsDisagreeWithTheLabelling(String),
    #[error(
        "pixel ({x}, {y}) is labelled {label} but sits in face {face}, whose label is {face_label};          {disagreeing} of {total} pixels disagree with the labelling they were built from"
    )]
    FaceMapContradictsTheLabelling {
        x: u32,
        y: u32,
        label: bool,
        face: u32,
        face_label: bool,
        disagreeing: usize,
        total: usize,
    },
    #[error(
        "the pixel-to-face map disagrees with the boundary owners on {disagreeing_pixels} pixel(s);          at ({x}, {y}) the map says face {stored} and the chain crossed says {from_boundaries}"
    )]
    FaceMapDisagreesWithTheBoundaries {
        x: i64,
        y: i64,
        stored: u32,
        from_boundaries: u32,
        disagreeing_pixels: usize,
    },
    #[error(
        "half-edge {half_edge} is sited at face {face}, loop {loop_index}, position {position},          but the half-edge AT that position is {found}"
    )]
    SiteIsNotWhereTheHalfEdgeIs {
        half_edge: u32,
        face: u32,
        loop_index: u32,
        position: u32,
        found: u32,
    },
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
    /// Which branch of the judge produced this verdict.
    ///
    /// REVIEW_M5_B N15: the probe's branch set was a hand-written
    /// two-element dichotomy computed by the CALLER (`count_inside() == 0`),
    /// so a new early return in this function cost one line there and the
    /// probe would never learn of it. The judge names its own branches now,
    /// and the harness buckets by whatever it reports — a third branch
    /// creates a third bucket without anyone remembering to add one.
    ///
    /// N17: the label carries its own RETURN SITE, via `line!()`. A hand
    /// label alone let a new branch reuse an existing name and hide from the
    /// probe for zero lines; two returns cannot share a line number, so the
    /// name is unique whether or not its author wanted it to be.
    pub branch: &'static str,
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
    // (A) THE ANCHOR. Every pixel sits in a face whose label is the pixel's own
    // label.
    //
    // REDTEAM_M5 RT5-A9 / REVIEW_M5_A D1-N1, and this check is the remedy both
    // named. Delta-1 added `crossing::face_map_agrees`, which rebuilds the map
    // from `Boundary::owners` — and `owners` is computed in `assemble` by
    // SAMPLING `face_of_padded_px`, two pixels per chain. So the "third
    // independent construction" sat DOWNSTREAM of the field it certified: the
    // audit tied the map to the owners and the owners to the map, and neither
    // to the LABELLING. The loop was closed with exactly one bit of external
    // anchoring — that the exterior is face 0.
    //
    // Reviewer A established the boundary by publishing a REFUTED hypothesis:
    // moving the red team's rotation above the sampling point IS caught,
    // because a global rotation moves the exterior off id 0. A permutation that
    // RESPECTS that single anchor — fix the exterior, swap 1 and 2 — was caught
    // by nothing, with 529 of 1089 pixels sitting in a face whose label
    // contradicts the labelling, 536 tests green and the artifact byte-identical.
    //
    // The truth was already in the structure and nothing read it. `labelling`
    // is the INPUT: it is not derived from the map, from the owners or from the
    // faces, so this is the one comparison in the audit whose two sides do not
    // share a provenance. It is placed BEFORE the empty-arrangement branch
    // because a judge's early return is a declared exclusion from its domain
    // (REVIEW_M5_B N11), and the empty case has pixels too — all background,
    // all of which must sit in the exterior.
    {
        let (w, h) = (d.width_px(), d.height_px());
        let mut disagreeing = 0usize;
        let mut first: Option<(u32, u32, bool, u32, bool)> = None;
        for y in 0..h {
            for x in 0..w {
                let want = d.labelling().inside()[y as usize * w as usize + x as usize];
                let f = d.face_of_pixel(x, y);
                let got = match d.faces().get(f.index()) {
                    Some(face) => face.label,
                    None => {
                        return Err(InvariantViolation::FaceMapContradictsTheLabelling {
                            x,
                            y,
                            label: want,
                            face: f.0,
                            face_label: !want,
                            disagreeing: 1,
                            total: (w as usize) * (h as usize),
                        })
                    }
                };
                if got != want {
                    disagreeing += 1;
                    if first.is_none() {
                        first = Some((x, y, want, f.0, got));
                    }
                }
            }
        }
        if let Some((x, y, label, face, face_label)) = first {
            return Err(InvariantViolation::FaceMapContradictsTheLabelling {
                x,
                y,
                label,
                face,
                face_label,
                disagreeing,
                total: (w as usize) * (h as usize),
            });
        }
    }

    // (A2) MAXIMALITY. §12 asks for "maximal shared boundary chains", and
    // nothing bound the word.
    //
    // REVIEW_M5_A D3-N2: splitting one chain at an interior degree-two point
    // leaves `audit()` returning None, `loops_agree` true, `face_map_agrees`
    // true, and V and B growing together so Euler is preserved. Reviewer A
    // rated it MINOR because clause 3 catches it on the corpus by comparing
    // lattice PATHS, and found it by publishing two refuted assumptions of
    // their own.
    //
    // It is one comparison, so it is closed here rather than carried: a chain
    // is maximal exactly when none of its INTERIOR points is a vertex. The
    // vertex set is built from lattice degree, which is a function of the
    // labelling, so this does not share a provenance with the chain splitting
    // it judges.
    {
        let verts: std::collections::BTreeSet<(u32, u32)> = d.vertices().iter().copied().collect();
        for (i, b) in d.boundaries().iter().enumerate() {
            let n = b.path.len();
            for (k, p) in b.path.iter().enumerate().take(n.saturating_sub(1)).skip(1) {
                if verts.contains(p) {
                    return Err(InvariantViolation::ChainIsNotMaximal {
                        boundary: i,
                        vertex: *p,
                        position: k,
                        length: n,
                    });
                }
            }
        }
    }

    // (B) THE ORIENTED HALF of §12's "face cycles closed and oriented".
    //
    // RT5-A13 / M5A-D2-N1, found independently by two contexts: `target(h) ==
    // origin(next(h))` was checked nowhere and `Dcel::target`/`origin` were
    // called by nothing in the workspace. Swapping two half-edges inside one
    // loop violates the property on 35 768 of 131 072 4x4 arrangements, and
    // `audit()` returned `Err` zero times.
    //
    // The loops are compared against loops RE-DERIVED FROM THE LABELLING rather
    // than against `target`/`origin`, which would have read `boundaries` and
    // `site` — outputs of the same `assemble`, i.e. RT5-A9's shape a third
    // time. See `loops` for the residual: this shares the ALGORITHM with
    // `assemble` and not the DATA.
    //
    // Placed before the empty branch for the reason N11 gave: an early return
    // in a judge is a declared exclusion from its domain.
    if let Err(e) = crate::dcel::loops::loops_agree_with_the_labelling(d) {
        return Err(InvariantViolation::LoopsDisagreeWithTheLabelling(
            e.to_string(),
        ));
    }

    if arr.steps().is_empty() {
        if !d.boundaries().is_empty() || d.loop_count() != 0 || d.faces().len() != 1 {
            return Err(InvariantViolation::EmptyArrangementIsNotEmpty {
                boundaries: d.boundaries().len(),
                loops: d.loop_count(),
                faces: d.faces().len(),
            });
        }
        if d.faces().first().is_none_or(|f| f.label) {
            return Err(InvariantViolation::ExteriorIsNotBackground(
                FaceId::EXTERIOR,
            ));
        }
        // REVIEW_M5_B N11: an early return in a judge is a DECLARED EXCLUSION
        // from its domain, and this one skipped the face-map comparison
        // entirely. A corruption of the map confined to the empty subclass
        // passed the full gate with four `[MET]`, and clause 4's green rested
        // on arm ORDER — the eight sliver arms sit at positions 87..96 and the
        // stride of 17 hits 86 and 103, missing by one position.
        //
        // The comparison is well defined and non-empty here: no boundaries
        // means the rebuild is all-exterior, which is exactly what the stored
        // map must be, and on a 20x20 that is 484 comparisons rather than
        // zero. So the branch executes it instead of naming it as skipped.
        //
        // This is the third empty subclass of the milestone (F-0058, RT5-A4,
        // this one), which is why the fix is the branch and not the instance.
        if let Err(e) = crate::dcel::crossing::face_map_agrees(d) {
            return Err(InvariantViolation::FaceMapDisagreesWithTheBoundaries {
                x: e.x,
                y: e.y,
                stored: e.stored,
                from_boundaries: e.from_boundaries,
                disagreeing_pixels: e.disagreeing_pixels,
            });
        }
        return Ok(AuditReport {
            branch: concat!("empty@", line!()),
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
    // The site is the INVERSE of the loop lists, so it is checked as one: the
    // half-edge at the sited position must be the half-edge itself. The
    // previous check asked only that `next` stayed in the same face, which a
    // position shifted WITHIN a loop satisfies — so a whole family of `site`
    // perturbations was seen by nothing (RT5-A1's class, on a smaller field).
    for h in d.half_edges() {
        let (f, l, p) = d.site_of(h);
        let found = d.faces()[f as usize].loops[l as usize][p as usize];
        if found != h {
            return Err(InvariantViolation::SiteIsNotWhereTheHalfEdgeIs {
                half_edge: h.0,
                face: f,
                loop_index: l,
                position: p,
                found: found.0,
            });
        }
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
    // Indexed only after checking there is something to index. An instrument
    // that panics on the structure it was asked to judge reports a broken
    // instrument as a crash (meta-rule M-4), and `with_parts` can hand this
    // function a `Parts` with no faces at all.
    match d.faces().first() {
        None => {
            return Err(InvariantViolation::EmptyArrangementIsNotEmpty {
                boundaries: d.boundaries().len(),
                loops: d.loop_count(),
                faces: 0,
            })
        }
        Some(f) if f.label => {
            return Err(InvariantViolation::ExteriorIsNotBackground(
                FaceId::EXTERIOR,
            ))
        }
        Some(_) => {}
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

    // (7a) THE THIRD CONSTRUCTION. The pixel-to-face map is rebuilt from the
    // boundary chains and their owners — a walk that never joins two pixels and
    // never looks at the stored map — and compared element by element.
    //
    // This is REDTEAM_M5 RT5-A1, and the finding is worth restating where the
    // fix lives: `face_of_padded_px` is the largest field of the structure and
    // NO predicate read it. A ten-line edit rotating every entry above 16 px
    // passed 530 tests, four MET clauses, a byte-identical artifact, the
    // exhaustive 4x4 sweep and every knockout. Exhausting the input domain and
    // exhausting the CHECKED FIELDS of the value are independent properties.
    match crate::dcel::crossing::face_map_agrees(d) {
        Ok(_) => {}
        Err(e) => {
            return Err(InvariantViolation::FaceMapDisagreesWithTheBoundaries {
                x: e.x,
                y: e.y,
                stored: e.stored,
                from_boundaries: e.from_boundaries,
                disagreeing_pixels: e.disagreeing_pixels,
            })
        }
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
        branch: concat!("arrangement@", line!()),
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

// The tests for this file's instrument live where the instrument they measure
// lives: the mutation walk's own tests moved to `walk.rs` in delta-3 when
// `audit.rs` crossed the §4.1 size rule, and the anchor's controls are the
// gate-level knockouts in `vice-bench` plus `crossing::tests` and
// `loops::tests`, each beside the check it is about.
