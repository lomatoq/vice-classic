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
//! ## How independent this construction actually is — measured, not described
//!
//! This section said "Why THIS construction is independent" and carried a table
//! whose last row read *"would survive a corrupted `face_of_padded_px` | it
//! never looks at it"*. Two cold contexts refuted it independently
//! (REDTEAM_M5 RT5-A9, REVIEW_M5_A D1-N1), and the refutation is one step up
//! the provenance graph rather than in this file at all:
//!
//! ```text
//! // in assemble(), where Boundary::owners is built:
//! let (left_px, right_px) = arr.flanks(ch[0]);
//! let (lf, rf) = (face_at(&face_of_padded_px, &arr, left_px),
//!                 face_at(&face_of_padded_px, &arr, right_px));
//! ```
//!
//! `owners` is a SAMPLE of `face_of_padded_px`, two pixels per chain. So the
//! rebuild is downstream of the field it certifies: the sentence is true of the
//! FUNCTION and false of the CHECK.
//!
//! **The residual class, stated exactly rather than gestured at.** Reviewer A
//! established it by publishing a refuted hypothesis first: moving the red
//! team's global rotation above the sampling point IS caught, because the scan
//! seeds from `FaceId::EXTERIOR` in the background ring rather than from the
//! stored map. That is the construction's one genuinely external bit. So the
//! set of corruptions this rebuild reproduces is exactly **every permutation of
//! face ids that fixes the exterior** — and E2b, which fixes the exterior and
//! swaps 1 with 2, passed 536 tests, four `[MET]` and a byte-identical artifact
//! with 529 of 1089 pixels sitting in a face whose label was not theirs.
//!
//! | | `flood_faces` (in `assemble`) | this rebuild |
//! |---|---|---|
//! | works over | PIXELS | BOUNDARY CHAINS |
//! | joins two pixels when | they share a label and are adjacent under that label's convention | never — it does not join pixels |
//! | assigns a face by | flood fill from a seed | reading the OWNER on the segment just crossed |
//! | reads the stored map | it produces it | no |
//! | **is derived from the stored map** | — | **YES, through `owners`** |
//! | corruptions it reproduces | — | **every permutation of face ids fixing the exterior** |
//!
//! ## The class rule, which is why this section is now a measurement
//!
//! **A cross-check is independent only up to the data it shares with what it
//! checks, and independence is a property of the PROVENANCE GRAPH rather than
//! of how different the algorithm looks.** The question is not "does B look
//! different" but "what is the largest corruption of A that B reproduces". Both
//! reviewers wrote that sentence independently; a third reviewer certified this
//! construction as independent by checking that the stored map is not read and
//! stopping there — which is the same error, committed while reviewing.
//!
//! ## What actually anchors the arrangement
//!
//! `audit`'s per-pixel check against the LABELLING, not this file. The labelling
//! is the input: it is not derived from the map, the owners or the faces, so it
//! is the one comparison in the audit whose two sides do not share a provenance.
//! This rebuild remains worth having — it caught RT5-A1, it is the only check
//! over the owners' geometry, and since delta-2 it also compares the west owner
//! against the face the scan carries — but it is a check on the copy, and the
//! anchor is what checks the value.

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
    face_map_and_owner_disagreements(d).0
}

/// A crossing whose WEST owner disagreed with the face the scan was carrying:
/// `(x, y, face carried, owner recorded)`.
pub type OwnerMismatch = (i64, i64, u32, u32);

/// The rebuilt map, the number of west-owner disagreements, and the first one.
pub type RebuiltMap = (Vec<u32>, usize, Option<OwnerMismatch>);

/// The rebuilt map, the number of crossings whose WEST owner disagreed with the
/// face the scan was carrying, and the first such crossing.
pub fn face_map_and_owner_disagreements(d: &Dcel) -> RebuiltMap {
    let (w, h) = (d.width_px(), d.height_px());
    let segs = vertical_segments(d);
    let pw = w as usize + 2;
    let ph = h as usize + 2;
    let mut out = vec![FaceId::EXTERIOR.0; pw * ph];
    let mut mismatched_west = 0usize;
    let mut first_west: Option<OwnerMismatch> = None;
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
                    // The west owner IS compared against the face the scan is
                    // already carrying. Until delta-2 this line read
                    // `let _ = west;` under a comment saying it was checked
                    // (REDTEAM_M5 MINOR, REVIEW_M5_B N12). A declared mechanism
                    // that does not exist is worse than an absent one, because
                    // the claim is what a reader budgets against.
                    //
                    // With it the walk is self-standing over the OWNERS: a
                    // chain whose owners disagree with the run of faces around
                    // it is a disagreement here, rather than something the
                    // audit's site check has to catch downstream.
                    if west.0 != face {
                        mismatched_west += 1;
                        if first_west.is_none() {
                            first_west = Some((x, y, face, west.0));
                        }
                    }
                    face = east.0;
                }
            }
            out[row * pw + col] = face;
        }
    }
    (out, mismatched_west, first_west)
}

/// Compare the stored map against the one the boundaries imply.
///
/// `Ok(pixels_compared)` on agreement. The count is returned rather than
/// discarded so that a caller can refuse a comparison that compared nothing —
/// an empty comparison agrees trivially (F-0039).
pub fn face_map_agrees(d: &Dcel) -> Result<usize, FaceMapDisagreement> {
    let (rebuilt, mismatched_west, first_west) = face_map_and_owner_disagreements(d);
    if let Some((x, y, carried, west)) = first_west {
        return Err(FaceMapDisagreement {
            x,
            y,
            stored: carried,
            from_boundaries: west,
            disagreeing_pixels: mismatched_west,
        });
    }
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

    /// **RT5-A9: the corruption delta-1 did not catch.**
    ///
    /// A permutation of face ids that FIXES the exterior and swaps 1 with 2.
    /// `face_map_agrees` says TRUE on it — and that is asserted here rather
    /// than hidden, because it is the honest boundary of this construction:
    /// `Boundary::owners` is sampled from `face_of_padded_px` inside
    /// `assemble`, so a relabelling that permutes both consistently is
    /// reproduced by the rebuild. Independence is a property of the PROVENANCE
    /// GRAPH, not of how different the algorithm looks.
    ///
    /// What catches it is the labelling anchor in `audit`, and this test pins
    /// both halves so neither can be removed believing the other covers it.
    #[test]
    fn a_relabelling_that_keeps_every_count_defeats_the_rebuild_and_not_the_anchor() {
        let l = lab(33, 33, |x, y| {
            let (dx, dy) = (x as f64 - 16.0, y as f64 - 16.0);
            let r = (dx * dx + dy * dy).sqrt();
            (5.0..=13.0).contains(&r)
        });
        let d = super::super::Dcel::assemble(l, ComplementaryConnectivity::arms()[0]);
        assert!(d.faces().len() >= 3, "the fixture needs two interior faces");
        assert!(face_map_agrees(&d).is_ok(), "positive control");
        assert!(super::super::audit(&d).is_ok(), "positive control");

        let broken = super::super::swap_two_face_labels_above(&d, 16);
        assert_ne!(
            broken.parts(),
            d.parts(),
            "the corruption must change something"
        );
        // Every delta-1 check REPRODUCES it. This is the measured limit of the
        // third construction, asserted rather than described: `owners` are
        // sampled out of `face_of_padded_px` inside `assemble`, so a rebuild
        // from `owners` cannot see a relabelling, and the signature comparison
        // cannot either — one foreground label traded for one background label
        // leaves both counts where they were.
        assert!(
            face_map_agrees(&broken).is_ok(),
            "the rebuild is downstream of the owners, which are sampled from the map"
        );
        let sig = crate::cubical::signature(broken.labelling(), broken.connectivity());
        assert_eq!(
            (broken.foreground_faces() as u32, broken.holes() as u32),
            (sig.components, sig.holes),
            "the signature comparison survives the swap too"
        );
        // The anchor catches it, because the labelling is not derived from
        // anything the corruption touched.
        let e = super::super::audit(&broken).expect_err("the anchor must catch RT5-A9");
        assert!(
            matches!(
                e,
                super::super::InvariantViolation::FaceMapContradictsTheLabelling { .. }
            ),
            "{e}"
        );
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
