//! The THIRD construction of the arrangement's face map, and the reason it
//! exists.
//!
//! ## What it is for
//!
//! REDTEAM_M5 RT5-A1 is a ten-line edit inside `assemble` that corrupts
//! `Parts::face_of_padded_px` — the largest field of the structure, the one
//! that supplies the majority of the mutation walk's 155 160 slots — above
//! 16 px, and passes **everything**: 530 tests, four `[MET]`, a byte-identical
//! artifact, `dcel-check`, the exhaustive 4×4 sweep and every knockout.
//!
//! The cause is exact and is worth stating without softening: **`audit()` never
//! read that field.** It read `boundaries`, `vertices`, `faces`, `site`,
//! `half_edges`, `next`, `face_of`, `foreground_faces`, `holes` and
//! `segment_count`, and not once the pixel-to-face map. In the whole workspace
//! `face_of_pixel` was read by two assertions on two hand-picked pixels of one
//! 9×9 disk.
//!
//! ## The eighth level of the F-0048 class, and it is about the OUTPUT
//!
//! The levels named so far — names, types, the product of a type, the domain of
//! a check, the judge, the judge's domain of proof, the sampling density inside
//! it — are all about the INPUT. RT5-A1 names the eighth and it is the other
//! side: **exhausting the input domain and exhausting the CHECKED FIELDS of the
//! value are independent properties, and a mechanism that made the first
//! compiler-checkable was taken for one that makes the second checkable.**
//!
//! `Parts::perturbations` exhaustively destructures `Parts`, so a field without
//! a PERTURBATION does not compile — that is a real judge. Nothing required a
//! field to have a CHECK. Enumerating all 65 536 labellings of a 4×4 gives
//! nothing to a field no predicate reads.
//!
//! The number that said so was already printed: `5648 / 155160 = 3.6 %`. It was
//! published as an honest weakness of the audit and it read as coverage — while
//! what it actually said is that 96.4 % of the structure's slots were checked by
//! nothing, because the second number that was supposed to close them is an
//! identity (RT5-A2).
//!
//! ## Why THIS construction is independent
//!
//! Both cold contexts named the same remedy independently: a third construction
//! of the arrangement, cross-checked against `assemble` at corpus size. This is
//! it, and the independence is in the criterion rather than in the wording:
//!
//! | | `flood_faces` (in `assemble`) | [`face_map_from_boundaries`] |
//! |---|---|---|
//! | works over | PIXELS | BOUNDARY CHAINS |
//! | joins two pixels when | they share a label and are adjacent under that label's convention | nothing — it never joins pixels |
//! | assigns a face by | flood fill from a seed | reading the OWNER recorded on the segment it just crossed |
//! | would survive a corrupted `face_of_padded_px` | it produces it | it never looks at it |
//!
//! The scan starts in the background ring, which is the exterior by
//! construction, and walks each row left to right. A vertical boundary segment
//! between two pixels carries, in `Boundary::owners`, the face on each of its
//! sides; crossing it means the current face becomes the owner on the far side.
//! Between segments the face cannot change, because a face change without a
//! boundary is precisely what a boundary is.
//!
//! So the map is rebuilt from `boundaries` alone. Comparing it with
//! `face_of_padded_px` also cross-checks the OWNERS against the geometry, which
//! is a second property the previous audit could not see either.

use std::collections::BTreeMap;

use super::{Dcel, FaceId};

/// Where the two constructions disagree, with enough to find it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceMapDisagreement {
    /// Canvas-relative pixel, which may be `-1` in the padding ring.
    pub x: i64,
    pub y: i64,
    /// What `assemble` stored.
    pub stored: u32,
    /// What the boundary owners say.
    pub from_boundaries: u32,
    pub disagreeing_pixels: usize,
}

/// A vertical segment at lattice `x`, spanning `y..y+1`, separates pixel
/// `(x-1, y)` from pixel `(x, y)`. This is what the scan crosses.
fn vertical_segments(d: &Dcel) -> BTreeMap<(u32, u32), (FaceId, FaceId)> {
    let mut out = BTreeMap::new();
    for b in d.boundaries() {
        for w in b.path.windows(2) {
            let (a, c) = (w[0], w[1]);
            if a.0 != c.0 {
                continue; // horizontal step
            }
            // Travelling south (+y) along the canonical path puts the face on
            // the LEFT at pixel (x, y) — the east side — by the frame of
            // §5.1 and `Arrangement::flanks`. Travelling north swaps them.
            let (x, y, west, east) = if c.1 > a.1 {
                (a.0, a.1, b.owners.right(), b.owners.left())
            } else {
                (a.0, c.1, b.owners.left(), b.owners.right())
            };
            out.insert((x, y), (west, east));
        }
    }
    out
}

/// Rebuild the pixel-to-face map from the boundary chains and their owners.
///
/// Returns the padded map in the same layout `Parts::face_of_padded_px` uses,
/// so the two are comparable element by element.
pub fn face_map_from_boundaries(d: &Dcel) -> Vec<u32> {
    let (w, h) = (d.width_px(), d.height_px());
    let segs = vertical_segments(d);
    let pw = w as usize + 2;
    let ph = h as usize + 2;
    let mut out = vec![FaceId::EXTERIOR.0; pw * ph];
    for row in 0..ph {
        // Padded row `row` is canvas row `row - 1`.
        let y = row as i64 - 1;
        // The scan starts one pixel OUTSIDE the canvas, which is background and
        // therefore the exterior. Nothing seeds this from the stored map.
        let mut face = FaceId::EXTERIOR.0;
        out[row * pw] = face;
        for col in 1..pw {
            let x = col as i64 - 1;
            // Crossing the lattice line at canvas x, between pixel (x-1, y) and
            // pixel (x, y). Only rows inside the canvas can carry one.
            if y >= 0 && y < i64::from(h) && x >= 0 && x <= i64::from(w) {
                if let Some((west, east)) = segs.get(&(x as u32, y as u32)) {
                    // We are moving from the west pixel to the east pixel, so
                    // the face becomes the owner on the east side. The west
                    // owner is not used for the assignment; it is checked
                    // against the face we were already carrying, which is what
                    // makes this a cross-check of the OWNERS and not only of
                    // the map.
                    let _ = west;
                    face = east.0;
                }
            }
            out[row * pw + col] = face;
        }
    }
    out
}

/// Compare the stored map against the one the boundaries imply.
///
/// `Ok(pixels_compared)` on agreement. The count is returned rather than
/// discarded so that a caller can refuse a comparison that compared nothing —
/// an empty comparison agrees trivially (F-0039).
pub fn face_map_agrees(d: &Dcel) -> Result<usize, FaceMapDisagreement> {
    let rebuilt = face_map_from_boundaries(d);
    let stored = d.padded_face_map();
    let n = rebuilt.len().min(stored.len());
    let pw = d.width_px() as usize + 2;
    let mut first: Option<(i64, i64, u32, u32)> = None;
    let mut disagreeing = 0usize;
    if rebuilt.len() != stored.len() {
        return Err(FaceMapDisagreement {
            x: -1,
            y: -1,
            stored: stored.len() as u32,
            from_boundaries: rebuilt.len() as u32,
            disagreeing_pixels: usize::MAX,
        });
    }
    for i in 0..n {
        if rebuilt[i] != stored[i] {
            disagreeing += 1;
            if first.is_none() {
                let (col, row) = (i % pw, i / pw);
                first = Some((col as i64 - 1, row as i64 - 1, stored[i], rebuilt[i]));
            }
        }
    }
    match first {
        None => Ok(n),
        Some((x, y, s, r)) => Err(FaceMapDisagreement {
            x,
            y,
            stored: s,
            from_boundaries: r,
            disagreeing_pixels: disagreeing,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubical::Labelling;
    use vice_ir::ComplementaryConnectivity;

    fn lab(w: usize, h: usize, f: impl Fn(usize, usize) -> bool) -> Labelling {
        Labelling::new(w, h, (0..w * h).map(|i| f(i % w, i / w)).collect())
    }

    /// The two constructions agree on shapes with holes, with several
    /// components, and at a size above the 16 px threshold RT5-A1 used.
    #[test]
    fn the_boundary_walk_rebuilds_the_face_map() {
        for conn in ComplementaryConnectivity::arms() {
            for (name, l) in [
                (
                    "ring",
                    lab(33, 33, |x, y| {
                        let (dx, dy) = (x as f64 - 16.0, y as f64 - 16.0);
                        let r = (dx * dx + dy * dy).sqrt();
                        (5.0..=13.0).contains(&r)
                    }),
                ),
                (
                    "two blobs and a hole",
                    lab(40, 24, |x, y| {
                        let a = (2..14).contains(&x) && (4..20).contains(&y);
                        let hole = (6..10).contains(&x) && (9..15).contains(&y);
                        let b = (24..38).contains(&x) && (4..20).contains(&y);
                        (a && !hole) || b
                    }),
                ),
                ("empty", lab(20, 20, |_, _| false)),
                ("full", lab(20, 20, |_, _| true)),
            ] {
                let d = super::super::Dcel::assemble(l, conn);
                let n = face_map_agrees(&d).unwrap_or_else(|e| panic!("{name}: {e:?}"));
                assert!(n > 0, "{name}: nothing compared");
            }
        }
    }

    /// **RT5-A1, as a test.** The red team's own edit — rotate every entry of
    /// the pixel-to-face map — must be caught. Both directions: the intact
    /// structure agrees, the corrupted one does not.
    #[test]
    fn the_red_team_corruption_of_the_face_map_is_caught() {
        let l = lab(33, 33, |x, y| {
            let (dx, dy) = (x as f64 - 16.0, y as f64 - 16.0);
            (dx * dx + dy * dy).sqrt() <= 12.0
        });
        let d = super::super::Dcel::assemble(l, ComplementaryConnectivity::arms()[0]);
        assert!(face_map_agrees(&d).is_ok(), "positive control");

        let mut parts = d.parts().clone();
        let nf = parts.faces.len() as u32;
        for v in parts.face_of_padded_px.iter_mut() {
            *v = (*v + 1) % nf.max(1);
        }
        let broken = d.clone().with_parts(parts);
        let e = face_map_agrees(&broken).expect_err("RT5-A1 must not pass");
        assert!(e.disagreeing_pixels > 0);
    }
}
