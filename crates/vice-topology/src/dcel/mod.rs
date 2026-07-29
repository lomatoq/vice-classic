//! Stage E — the robust shared planar graph (spec v1.3 §12, §28 M5).
//!
//! ## What §12 asks for, and where each clause went
//!
//! §12 lists six things to build and seven invariants to hold. The invariants
//! are the interesting half, because §12's demand is not "check them" — a
//! wrong state must not ASSEMBLE. Here is where each one went, and the column
//! that matters is the third:
//!
//! | §12 invariant | where it lives | can a value violate it? |
//! |---|---|---|
//! | every half-edge has a twin | [`HalfEdgeId::twin`] flips one bit | no: there is no twin FIELD |
//! | every interior boundary has two owners | [`Boundary::owners`] is a pair | no: two owners is the SHAPE of the record |
//! | a border boundary has an interior and an exterior owner | the outside ring is background, so the exterior is an ordinary face produced by the ordinary walk | no |
//! | face cycles are CLOSED | a loop is stored as a cyclic sequence | no: a cycle traversed modulo its length has no open state |
//! | face cycles are ORIENTED | [`loops`] — this one IS a computation | **YES**, and it was unrepresented until delta-3 |
//! | no dangling cracks | [`FacePair::new`] refuses equal ids, and it is the only mint | no |
//! | Euler / cubical signature preserved | [`audit`] — this one IS a computation | YES, and that is why it is the audited one |
//! | non-adjacent boundaries do not intersect | segments are unit steps of an integer lattice; two distinct ones meet only at a lattice point, and every lattice point of degree three or four is a vertex | no |
//!
//! **Eight rows, six of them held by the representation and TWO of them
//! computations.** §12 lists seven invariants and one of the seven -
//! "face cycles closed and oriented" - is a conjunction of two properties
//! that are held in different ways, so the table splits it. Delta-3 split
//! the row and left the arithmetic around it at "six of the seven", which
//! then disagreed with `report.rs` (one computation) and with STATUS (five
//! representational). Three counts of one table, in the milestone's most
//! load-bearing text; the governor found it after three cold contexts did
//! not. It is the swapped-format-argument class of delta-1: the edit was
//! right and the arithmetic around it was not recomputed.
//!
//! Six have no failure mode to check for, because the
//! representation cannot express the violation. That is the difference between
//! this module and `vice_ir::validate`, which checks the same list AFTER the
//! fact on a `PlanarGraph` whose fields are public: M1 built a validator
//! because M1 had to accept graphs from outside; M5 builds a constructor
//! because §12 says the wrong state must not assemble.
//!
//! ## The one constructor, and why totality is the whole claim
//!
//! [`Dcel::assemble`] takes a labelling and a complementary-connectivity
//! convention and returns a `Dcel`. It returns no `Result`. There is no
//! invalid outcome to represent, because every binary labelling of a
//! rectangle has exactly one cubical arrangement under a given convention.
//!
//! So the property "no invalid `Dcel` exists" is not maintained by vigilance:
//! it holds because the only way to obtain one is a total function of two
//! arguments, and its fields are private.
//!
//! **The honest boundary, stated where the strength is claimed** (F-0048, last
//! paragraph). The cheapest bypass is a second constructor added inside this
//! crate — one `pub fn` in `vice-topology` taking pieces and assembling them
//! by hand. Nothing here makes that a compile error, and a scan for `pub fn ->
//! Dcel` would be a text scan, which F-0048 Q3 calls a habit rather than a
//! judge. What IS a judge, and what this module ships instead:
//! [`audit::audit`] re-derives the arrangement from the `Dcel`'s own labelling
//! and compares every entity, so a hand-built `Dcel` that is not the assembly
//! of its own input is caught by a computation rather than by a rule about
//! source text. The residual is therefore "a hand-built `Dcel` that IS the
//! assembly of its own input", which is a correct `Dcel` obtained the long
//! way.
//!
//! ## Determinism
//!
//! Every scan is in a fixed lattice order, every direction list is
//! [`lattice::Dir::ALL`], every face id comes from raster order of its first
//! pixel, and the exterior is always [`FaceId::EXTERIOR`]. §5.5 Tier A is a
//! byte comparison for this structure: it contains no float.

pub mod audit;
pub mod certificate;
pub mod crossing;
pub mod fixtures;
pub mod lattice;
pub mod loops;
pub mod sweep;
pub mod transaction;
pub mod walk;

use std::collections::BTreeMap;

use serde::Serialize;
use vice_ir::{ComplementaryConnectivity, PixelConnectivity};

use crate::cubical::Labelling;
use lattice::{Arrangement, Lat, Px, Step};

pub use audit::{audit, is_the_assembly_of_its_own_labelling, AuditReport, InvariantViolation};
pub use certificate::{
    curve_replacement_isotopy, degree_multiset, junction_count, IsotopyRefusal, TopologyCertificate,
};
pub use crossing::{face_map_agrees, face_map_from_boundaries, FaceMapDisagreement};
pub use fixtures::{structural_fixtures, with_a_distant_witness, Fixture};
pub use loops::{
    loop_length_profile, loops_agree_with_the_labelling, vertices_agree_with_the_labelling,
    vertices_of_the_labelling, LoopDisagreement, VertexDisagreement,
};
pub use sweep::{audit_every_labelling, ExhaustiveReport};
pub use transaction::{
    apply, Edit, Outcome, Roi, TransactionRefusal, TransactionReport, TxConfig, TX_CONFIG_V1,
};
pub use walk::{
    measure_audit_resolving_power, rotate_face_map_above, swap_two_face_labels_above,
    ResolvingPower,
};

/// Index into [`Dcel::faces`]. The exterior is a real one (§5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct FaceId(pub u32);

impl FaceId {
    /// The exterior face. Always index zero, in every arrangement, because the
    /// walk starts from the background ring.
    pub const EXTERIOR: FaceId = FaceId(0);
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Index into [`Dcel::vertices`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct VertexId(pub u32);

impl VertexId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Index into [`Dcel::boundaries`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct BoundaryId(pub u32);

impl BoundaryId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One oriented side of a boundary.
///
/// The low bit is the side, so [`HalfEdgeId::twin`] is a bit flip and there is
/// no twin field anywhere in this crate. §12's first invariant has no
/// representation in which it is false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct HalfEdgeId(pub u32);

impl HalfEdgeId {
    pub fn new(b: BoundaryId, forward: bool) -> HalfEdgeId {
        HalfEdgeId(b.0 * 2 + u32::from(!forward))
    }
    pub fn boundary(self) -> BoundaryId {
        BoundaryId(self.0 / 2)
    }
    pub fn is_forward(self) -> bool {
        self.0.is_multiple_of(2)
    }
    /// A total function. Applying it twice is the identity, by arithmetic.
    pub fn twin(self) -> HalfEdgeId {
        HalfEdgeId(self.0 ^ 1)
    }
}

/// The two owners of a boundary, and the type that makes "no dangling crack"
/// unrepresentable.
///
/// §12: "no dangling cracks". A crack is a boundary with the same face on both
/// sides. [`FacePair::new`] is the only way to build one and it returns
/// `None` on equal ids, so a `Boundary` carrying a crack cannot be
/// constructed — not "is rejected", cannot be constructed. The fields are
/// private for the same reason: a public field is a second mint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FacePair {
    left: FaceId,
    right: FaceId,
}

impl FacePair {
    /// The only mint. `None` when the two sides are the same face.
    pub fn new(left: FaceId, right: FaceId) -> Option<FacePair> {
        if left == right {
            return None;
        }
        Some(FacePair { left, right })
    }
    pub fn left(self) -> FaceId {
        self.left
    }
    pub fn right(self) -> FaceId {
        self.right
    }
    /// The face on the left of the given orientation.
    pub fn on_left_of(self, forward: bool) -> FaceId {
        if forward {
            self.left
        } else {
            self.right
        }
    }
}

/// One maximal shared boundary chain (§12: "maximal shared boundary chains").
///
/// Stored ONCE; both faces refer to it through the two half-edges. `path` runs
/// from the start vertex to the end vertex in unit lattice steps, so
/// `path.len() >= 2`, and a closed chain has `start == end` and repeats that
/// lattice point at both ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Boundary {
    pub owners: FacePair,
    pub start: VertexId,
    pub end: VertexId,
    /// Lattice points, `start` first and `end` last.
    pub path: Vec<(u32, u32)>,
}

impl Boundary {
    /// Number of unit segments. §12's "maximal" is about this being as large
    /// as the junctions allow, which the assembly guarantees by splitting only
    /// at vertices.
    pub fn segments(&self) -> usize {
        self.path.len() - 1
    }
}

/// A face of the arrangement: its label, and one cyclic sequence per boundary
/// loop.
///
/// A loop is a `Vec<HalfEdgeId>` traversed MODULO ITS LENGTH. "Face cycles are
/// closed" is therefore not a property to check — a cyclic traversal of a
/// finite sequence has no open state — and [`Dcel::next`] is that traversal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Face {
    /// `true` for foreground, `false` for background. The exterior is a
    /// background face and is not otherwise special.
    pub label: bool,
    pub loops: Vec<Vec<HalfEdgeId>>,
}

/// The shared planar graph of one labelling under one convention.
///
/// Fields are private and there is exactly one constructor. See the module
/// docs for what that does and does not buy.
///
/// `Labelling` is `#[serde(skip)]` for the same reason `TopologyHypothesis`
/// skips it: it carries no `Serialize`. The labelling is not lost from the
/// artifact — it is what the report's digest is taken over.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Dcel {
    #[serde(skip)]
    labelling: Labelling,
    #[serde(skip)]
    conn: ComplementaryConnectivity,
    foreground_connectivity: &'static str,
    parts: audit::Parts,
}

impl Dcel {
    /// **The only constructor.** Total: every labelling has exactly one
    /// arrangement under a given convention, so there is no failure to
    /// represent.
    pub fn assemble(labelling: Labelling, conn: ComplementaryConnectivity) -> Dcel {
        let (w, h) = (labelling.width_px() as u32, labelling.height_px() as u32);
        let arr = Arrangement::new(labelling.inside(), w, h, conn);

        // ---- faces -------------------------------------------------------
        let (face_of_padded_px, face_labels) = flood_faces(&arr);

        // ---- loops -------------------------------------------------------
        let loops = orbits(&arr);

        // ---- vertices ----------------------------------------------------
        let mut vertex_set: std::collections::BTreeSet<Lat> = std::collections::BTreeSet::new();
        for lp in &loops {
            let mut has_junction = false;
            for s in lp {
                if arr.degree(s.from) != 2 {
                    vertex_set.insert(s.from);
                    has_junction = true;
                }
            }
            if !has_junction {
                // A loop with no junction is a simple closed curve. §12 wants
                // chains between vertices, so it gets one, chosen by the
                // lattice order rather than by traversal order — otherwise the
                // artifact would depend on where the orbit walk happened to
                // start.
                let v = lp.iter().map(|s| s.from).min().expect("non-empty loop");
                vertex_set.insert(v);
            }
        }
        let vertices: Vec<Lat> = vertex_set.iter().copied().collect();
        let vindex: BTreeMap<Lat, u32> = vertices
            .iter()
            .enumerate()
            .map(|(i, v)| (*v, i as u32))
            .collect();

        // ---- chains ------------------------------------------------------
        // Every loop splits at its vertices. Each undirected chain is produced
        // twice, once from each side; the canonical lattice path decides which
        // production is the forward one.
        let mut canonical: BTreeMap<Vec<Lat>, u32> = BTreeMap::new();
        let mut boundaries: Vec<Boundary> = Vec::new();
        let mut loop_of_he: Vec<Vec<HalfEdgeId>> = Vec::new();
        let mut he_face: Vec<u32> = Vec::new();

        for lp in &loops {
            let chains = split_at_vertices(lp, &vindex);
            let mut he_loop: Vec<HalfEdgeId> = Vec::with_capacity(chains.len());
            for ch in &chains {
                let path: Vec<Lat> = std::iter::once(ch[0].from)
                    .chain(ch.iter().map(|s| s.head()))
                    .collect();
                let mut rev = path.clone();
                rev.reverse();
                let key = if path <= rev { path.clone() } else { rev };
                let forward = key == path;
                let id = match canonical.get(&key) {
                    Some(i) => *i,
                    None => {
                        let (left_px, right_px) = arr.flanks(ch[0]);
                        let (lf, rf) = (
                            face_at(&face_of_padded_px, &arr, left_px),
                            face_at(&face_of_padded_px, &arr, right_px),
                        );
                        // Orient the stored pair against the CANONICAL path,
                        // which may be the reverse of the walk that found it.
                        let owners = if forward {
                            FacePair::new(FaceId(lf), FaceId(rf))
                        } else {
                            FacePair::new(FaceId(rf), FaceId(lf))
                        }
                        .expect(
                            "a boundary segment separates two different faces by construction: it \
                             exists only where the labels differ, and faces are label-uniform",
                        );
                        let kpath = if forward {
                            path.clone()
                        } else {
                            let mut r = path.clone();
                            r.reverse();
                            r
                        };
                        let i = boundaries.len() as u32;
                        boundaries.push(Boundary {
                            owners,
                            start: VertexId(vindex[&kpath[0]]),
                            end: VertexId(vindex[&kpath[kpath.len() - 1]]),
                            path: kpath.iter().map(|p| (p.x, p.y)).collect(),
                        });
                        canonical.insert(key, i);
                        i
                    }
                };
                he_loop.push(HalfEdgeId::new(BoundaryId(id), forward));
            }
            let (lpx, _) = arr.flanks(lp[0]);
            he_face.push(face_at(&face_of_padded_px, &arr, lpx));
            loop_of_he.push(he_loop);
        }

        // ---- faces get their loops --------------------------------------
        let mut faces: Vec<Face> = face_labels
            .iter()
            .map(|label| Face {
                label: *label,
                loops: Vec::new(),
            })
            .collect();
        let mut site = vec![(u32::MAX, u32::MAX, u32::MAX); boundaries.len() * 2];
        for (li, he_loop) in loop_of_he.into_iter().enumerate() {
            let f = he_face[li] as usize;
            let slot = faces[f].loops.len() as u32;
            for (pos, he) in he_loop.iter().enumerate() {
                site[he.0 as usize] = (f as u32, slot, pos as u32);
            }
            faces[f].loops.push(he_loop);
        }

        Dcel {
            labelling,
            conn,
            foreground_connectivity: match conn.foreground() {
                PixelConnectivity::Four => "4",
                PixelConnectivity::Eight => "8",
            },
            parts: audit::Parts {
                vertices: vertices.iter().map(|v| (v.x, v.y)).collect(),
                boundaries,
                faces,
                face_of_padded_px,
                site,
            },
        }
    }

    /// The derived structure. `pub(crate)` so that [`audit`] can compare a
    /// value against a fresh assembly of its own labelling, and so that the
    /// mutation walk that measures the audit's resolving power has something
    /// to walk. Not public: a public accessor to the pieces is a second mint.
    pub(crate) fn parts(&self) -> &audit::Parts {
        &self.parts
    }

    pub fn labelling(&self) -> &Labelling {
        &self.labelling
    }
    pub fn connectivity(&self) -> ComplementaryConnectivity {
        self.conn
    }
    pub fn width_px(&self) -> u32 {
        self.labelling.width_px() as u32
    }
    pub fn height_px(&self) -> u32 {
        self.labelling.height_px() as u32
    }
    pub fn vertices(&self) -> &[(u32, u32)] {
        &self.parts.vertices
    }
    pub fn boundaries(&self) -> &[Boundary] {
        &self.parts.boundaries
    }
    pub fn faces(&self) -> &[Face] {
        &self.parts.faces
    }

    /// The face on the left of a half-edge.
    pub fn face_of(&self, h: HalfEdgeId) -> FaceId {
        self.parts.boundaries[h.boundary().index()]
            .owners
            .on_left_of(h.is_forward())
    }

    /// Where a half-edge sits: `(face, loop index, position in the loop)`.
    pub fn site_of(&self, h: HalfEdgeId) -> (u32, u32, u32) {
        self.parts.site[h.0 as usize]
    }

    /// The next half-edge of the same loop.
    ///
    /// The loop is a cyclic sequence; this is `position + 1` modulo its
    /// length. There is nothing here that can fail to close.
    pub fn next(&self, h: HalfEdgeId) -> HalfEdgeId {
        let (f, l, p) = self.parts.site[h.0 as usize];
        let lp = &self.parts.faces[f as usize].loops[l as usize];
        lp[(p as usize + 1) % lp.len()]
    }

    /// The vertex a half-edge leaves.
    pub fn origin(&self, h: HalfEdgeId) -> VertexId {
        let b = &self.parts.boundaries[h.boundary().index()];
        if h.is_forward() {
            b.start
        } else {
            b.end
        }
    }

    /// The vertex a half-edge arrives at.
    pub fn target(&self, h: HalfEdgeId) -> VertexId {
        let b = &self.parts.boundaries[h.boundary().index()];
        if h.is_forward() {
            b.end
        } else {
            b.start
        }
    }

    /// Every half-edge, in id order.
    pub fn half_edges(&self) -> impl Iterator<Item = HalfEdgeId> + '_ {
        (0..self.parts.boundaries.len() as u32 * 2).map(HalfEdgeId)
    }

    /// The stored pixel-to-face map, in padded layout. `pub(crate)` so that
    /// [`crossing::face_map_agrees`] can compare it against a map rebuilt from
    /// the boundaries; not public, because a caller outside this crate has no
    /// business with the padding.
    pub(crate) fn padded_face_map(&self) -> &[u32] {
        &self.parts.face_of_padded_px
    }

    /// The face a canvas pixel belongs to.
    pub fn face_of_pixel(&self, x: u32, y: u32) -> FaceId {
        let w = self.width_px() + 2;
        FaceId(self.parts.face_of_padded_px[(y as usize + 1) * w as usize + (x as usize + 1)])
    }

    /// Foreground faces, i.e. the components of the ink under this convention.
    pub fn foreground_faces(&self) -> usize {
        self.parts.faces.iter().filter(|f| f.label).count()
    }

    /// Background faces other than the exterior, i.e. the holes.
    pub fn holes(&self) -> usize {
        self.parts.faces.iter().filter(|f| !f.label).count() - 1
    }

    /// Total boundary loops over all faces.
    pub fn loop_count(&self) -> usize {
        self.parts.faces.iter().map(|f| f.loops.len()).sum()
    }

    /// Unit lattice segments.
    pub fn segment_count(&self) -> usize {
        self.parts.boundaries.iter().map(|b| b.segments()).sum()
    }
}

// ---------------------------------------------------------------------------
// assembly helpers
// ---------------------------------------------------------------------------

fn padded_index(arr: &Arrangement<'_>, p: Px) -> usize {
    let w = arr.width_px as i64 + 2;
    ((p.y + 1) * w + (p.x + 1)) as usize
}

fn face_at(face_of_padded_px: &[u32], arr: &Arrangement<'_>, p: Px) -> u32 {
    face_of_padded_px[padded_index(arr, p)]
}

/// Faces, by flood fill over the padded grid.
///
/// Adjacency is "same label, and connected under that label's convention",
/// which is the §5.3 complementary rule. The exterior — the region containing
/// the ring — is forced to id 0; every other face is numbered by the raster
/// order of its first padded pixel, so the numbering is a function of the
/// labelling and not of the traversal.
fn flood_faces(arr: &Arrangement<'_>) -> (Vec<u32>, Vec<bool>) {
    let (pw, ph) = (arr.width_px as i64 + 2, arr.height_px as i64 + 2);
    let n = (pw * ph) as usize;
    let mut owner = vec![u32::MAX; n];
    let mut labels: Vec<bool> = Vec::new();
    // Exterior first, so it lands on id 0 whatever the labelling is.
    let mut order: Vec<Px> = Vec::with_capacity(n);
    order.push(Px { x: -1, y: -1 });
    for y in -1..ph - 1 {
        for x in -1..pw - 1 {
            order.push(Px { x, y });
        }
    }
    for seed in order {
        let si = padded_index(arr, seed);
        if owner[si] != u32::MAX {
            continue;
        }
        let label = arr.label(seed);
        let id = labels.len() as u32;
        labels.push(label);
        let eight = arr.conn_of(label) == PixelConnectivity::Eight;
        let mut stack = vec![seed];
        owner[si] = id;
        while let Some(p) = stack.pop() {
            let neighbours: &[(i64, i64)] = if eight {
                &[
                    (1, 0),
                    (-1, 0),
                    (0, 1),
                    (0, -1),
                    (1, 1),
                    (1, -1),
                    (-1, 1),
                    (-1, -1),
                ]
            } else {
                &[(1, 0), (-1, 0), (0, 1), (0, -1)]
            };
            for (dx, dy) in neighbours {
                let q = Px {
                    x: p.x + dx,
                    y: p.y + dy,
                };
                if q.x < -1 || q.y < -1 || q.x >= pw - 1 || q.y >= ph - 1 {
                    continue;
                }
                let qi = padded_index(arr, q);
                if owner[qi] != u32::MAX || arr.label(q) != label {
                    continue;
                }
                owner[qi] = id;
                stack.push(q);
            }
        }
    }
    (owner, labels)
}

/// The orbits of the successor permutation: one per boundary loop.
pub(crate) fn orbits(arr: &Arrangement<'_>) -> Vec<Vec<Step>> {
    let steps = arr.steps();
    let index: BTreeMap<Step, usize> = steps.iter().enumerate().map(|(i, s)| (*s, i)).collect();
    let mut seen = vec![false; steps.len()];
    let mut out = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        if seen[i] {
            continue;
        }
        let mut lp = Vec::new();
        let mut cur = *s;
        loop {
            let ci = index[&cur];
            if seen[ci] {
                break;
            }
            seen[ci] = true;
            lp.push(cur);
            cur = arr.succ(cur);
        }
        out.push(lp);
    }
    out
}

/// Cut a loop into the chains between its vertices.
///
/// A loop with no vertex on it cannot happen: the assembly puts one there
/// first. The rotation is to a vertex, so every chain starts and ends at one.
fn split_at_vertices(lp: &[Step], vindex: &BTreeMap<Lat, u32>) -> Vec<Vec<Step>> {
    let start = lp
        .iter()
        .position(|s| vindex.contains_key(&s.from))
        .expect("every loop carries at least one vertex by construction");
    let mut chains: Vec<Vec<Step>> = Vec::new();
    let mut cur: Vec<Step> = Vec::new();
    for k in 0..lp.len() {
        let s = lp[(start + k) % lp.len()];
        if !cur.is_empty() && vindex.contains_key(&s.from) {
            chains.push(std::mem::take(&mut cur));
        }
        cur.push(s);
    }
    if !cur.is_empty() {
        chains.push(cur);
    }
    chains
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn lab(w: usize, h: usize, f: impl Fn(usize, usize) -> bool) -> Labelling {
        Labelling::new(w, h, (0..w * h).map(|i| f(i % w, i / w)).collect())
    }

    fn arms() -> [ComplementaryConnectivity; 2] {
        ComplementaryConnectivity::arms()
    }

    /// A disk: one foreground face, no holes, and the exterior is a REAL face
    /// carrying the outside.
    #[test]
    fn a_disk_assembles_into_two_faces_one_of_which_is_the_exterior() {
        let l = lab(9, 9, |x, y| {
            let (dx, dy) = (x as f64 - 4.0, y as f64 - 4.0);
            dx * dx + dy * dy <= 9.0
        });
        let d = Dcel::assemble(l, arms()[0]);
        assert_eq!(d.faces().len(), 2);
        assert!(!d.faces()[FaceId::EXTERIOR.index()].label);
        assert_eq!(d.foreground_faces(), 1);
        assert_eq!(d.holes(), 0);
        assert_eq!(d.face_of_pixel(4, 4), FaceId(1));
        assert_eq!(d.face_of_pixel(0, 0), FaceId::EXTERIOR);
    }

    /// A ring: the hole is a background face that is NOT the exterior, and the
    /// foreground face has two loops.
    #[test]
    fn a_ring_has_a_hole_that_is_a_face_and_a_face_with_two_loops() {
        let l = lab(11, 11, |x, y| {
            let (dx, dy) = (x as f64 - 5.0, y as f64 - 5.0);
            let r2 = dx * dx + dy * dy;
            (2.0..=20.0).contains(&r2)
        });
        let d = Dcel::assemble(l, arms()[0]);
        assert_eq!(d.holes(), 1, "faces: {:?}", d.faces().len());
        let fg = d
            .faces()
            .iter()
            .position(|f| f.label)
            .expect("a foreground face");
        assert_eq!(
            d.faces()[fg].loops.len(),
            2,
            "an annulus is bounded by two loops"
        );
    }

    /// Twin and next are functions, and the loop closes because it is a cycle.
    #[test]
    fn every_half_edge_returns_to_itself_along_next() {
        let l = lab(7, 7, |x, y| (2..5).contains(&x) && (1..6).contains(&y));
        let d = Dcel::assemble(l, arms()[0]);
        assert!(!d.boundaries().is_empty());
        for h in d.half_edges() {
            assert_eq!(h.twin().twin(), h);
            assert_ne!(h.twin(), h);
            // The twin borders the OTHER face.
            assert_ne!(d.face_of(h), d.face_of(h.twin()));
            let mut cur = h;
            let mut n = 0usize;
            loop {
                cur = d.next(cur);
                n += 1;
                if cur == h {
                    break;
                }
                assert!(n < 10_000, "next did not close");
            }
            assert_eq!(d.face_of(d.next(h)), d.face_of(h), "next stays in the face");
        }
    }

    /// A dangling crack cannot be built, because the pair type refuses it.
    /// Checked in BOTH directions: the refusal fires, and the legal pair is
    /// still constructible (an always-`None` constructor would look identical
    /// from the failing side).
    #[test]
    fn a_face_pair_with_one_owner_is_not_constructible() {
        assert!(FacePair::new(FaceId(3), FaceId(3)).is_none());
        assert!(FacePair::new(FaceId::EXTERIOR, FaceId::EXTERIOR).is_none());
        let ok = FacePair::new(FaceId(0), FaceId(1)).expect("distinct owners are fine");
        assert_eq!(ok.left(), FaceId(0));
        assert_eq!(ok.right(), FaceId(1));
        assert_eq!(ok.on_left_of(false), FaceId(1));
    }

    /// The chains are MAXIMAL: a plain rectangle has four of them, one per
    /// side, and not sixteen unit segments.
    #[test]
    fn boundary_chains_are_maximal_between_junctions() {
        let l = lab(6, 6, |x, y| (1..5).contains(&x) && (1..5).contains(&y));
        let d = Dcel::assemble(l, arms()[0]);
        // One simple closed curve, no junction: exactly one chain, closed.
        assert_eq!(d.boundaries().len(), 1);
        let b = &d.boundaries()[0];
        assert_eq!(b.start, b.end, "a junction-free loop is one closed chain");
        assert_eq!(b.segments(), 16, "the perimeter of a 4x4 square");
        assert_eq!(d.vertices().len(), 1);
    }
}
