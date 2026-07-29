//! The ORIENTED half of §12's "face cycles closed and oriented".
//!
//! ## What was missing, and why nothing saw it
//!
//! §12 asks for face cycles that are **closed AND oriented**. The `Dcel` table
//! claimed both from one argument — "a loop is a `Vec<HalfEdgeId>` traversed
//! modulo its length, so it has no open state". That argument establishes
//! CLOSED and says nothing about ORIENTED: a cyclic sequence of the right
//! half-edges in the wrong ORDER is still a cycle.
//!
//! `target(h) == origin(next(h))` — the property that makes a loop a walk
//! rather than a bag — was checked nowhere, and `Dcel::target` and
//! `Dcel::origin` were called by nothing in the whole workspace. REDTEAM_M5
//! RT5-A13 and REVIEW_M5_A D2-N1 found it independently: swapping two
//! half-edges inside one loop violates the property on **35 768 of 131 072**
//! 4×4 arrangements, and `audit()` returned `Err` **zero** times, with the
//! exhaustive sweep green, the gate at EXIT 0 and the artifact byte-identical.
//!
//! Neither of the two anchors could see it. The labelling anchor compares
//! pixels to face labels and a reordering moves no pixel; `crossing` reads only
//! `boundaries` and a reordering moves no boundary. And the exhaustive sweep
//! exhausts the INPUT domain, which says nothing about a property no predicate
//! evaluates — the eighth level of the F-0048 class, a third time, now on
//! `next`.
//!
//! ## Where the protection against a wrong `succ` rule actually comes from
//!
//! The residual below said the sweep and the Euler identity cover the `succ`
//! rule. Two cold contexts ran experiments that looked contradictory — the red
//! team's `succ` defect was invisible to the sweep, reviewer A's was caught by
//! it — and reading both shows one mechanism and one limit:
//!
//! - the PREDICATE that fires in both reports is the same one, and it is
//!   neither this file nor the Euler identity: it is the owner/site check in
//!   [`super::audit`], which notices that a half-edge's owners no longer agree
//!   with the loop it sits in;
//! - the SWEEP is that predicate's carrier, and its reach ends at `w = 4`. The
//!   red team's defect fires only at `w >= 16`, so the sweep never reached it;
//!   reviewer A's is unconditional, so the sweep caught it on labelling 18
//!   under foreground-4. That difference is F-8 — a judge whose domain of
//!   proof is smaller than its domain of use — and it is why the structural
//!   register runs to 512 px.
//!
//! So the protection exists and is not what the first version of this comment
//! named, and its reach is bounded in a way that comment did not state.
//!
//! ## Q4 first, because this is the third time
//!
//! The obvious check is `target(h) == origin(next(h))` over every half-edge.
//! It works, and it shares a provenance with what it checks: `target`/`origin`
//! read `boundaries[].start/end` and `next` reads `site` and `faces[].loops`,
//! all of them outputs of the same `assemble`. That is precisely the shape of
//! RT5-A9, and writing it here would be the third instance of one class.
//!
//! So the loops are re-derived from the **labelling** instead. [`orbits`] over
//! an [`Arrangement`] built from `labelling` and `conn` is the same computation
//! `assemble` uses, run again from the input rather than from the output, and
//! the stored loops are expanded back into lattice steps and compared with it
//! as cyclic sequences. The comparison's far side is anchored to the one input
//! in the system that is not derived from anything (F-0065).
//!
//! **The honest residual**, stated where the strength is claimed: this shares
//! the ALGORITHM with `assemble` — a wrong `succ` rule would produce the same
//! wrong loops on both sides. What it does not share is the DATA, so any
//! corruption of the stored loops is caught, which is the class RT5-A13 lives
//! in. The `succ` rule itself is what the exhaustive sweep and the Euler
//! identity are for, and those do not share an algorithm with it.

use std::collections::BTreeMap;

use super::lattice::{Arrangement, Dir, Lat, Step};
use super::{orbits, Dcel};

/// Where the stored loops and the loops of the labelling differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopDisagreement {
    pub stored_loops: usize,
    pub derived_loops: usize,
    /// A cycle present on one side and not the other, as lattice points.
    pub first_unmatched: Vec<(u32, u32)>,
    pub side: &'static str,
}

impl std::fmt::Display for LoopDisagreement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} stored face loop(s) against {} derived from the labelling; a cycle present only \
             in the {} begins {:?}",
            self.stored_loops,
            self.derived_loops,
            self.side,
            self.first_unmatched.first()
        )
    }
}

fn dir_between(a: (u32, u32), b: (u32, u32)) -> Option<Dir> {
    let (dx, dy) = (
        i64::from(b.0) - i64::from(a.0),
        i64::from(b.1) - i64::from(a.1),
    );
    match (dx, dy) {
        (1, 0) => Some(Dir::E),
        (0, 1) => Some(Dir::S),
        (-1, 0) => Some(Dir::W),
        (0, -1) => Some(Dir::N),
        _ => None,
    }
}

/// Expand one stored loop into the lattice steps it walks.
///
/// `None` when a half-edge's path is not a run of unit steps — which the audit
/// checks separately, and which this must not panic on.
fn steps_of(d: &Dcel, lp: &[super::HalfEdgeId]) -> Option<Vec<Step>> {
    let mut out = Vec::new();
    for h in lp {
        let b = d.boundaries().get(h.boundary().index())?;
        let path: Vec<(u32, u32)> = if h.is_forward() {
            b.path.clone()
        } else {
            b.path.iter().rev().copied().collect()
        };
        for w in path.windows(2) {
            let dir = dir_between(w[0], w[1])?;
            out.push(Step {
                from: Lat {
                    x: w[0].0,
                    y: w[0].1,
                },
                dir,
            });
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// Rotate a cycle to start at its smallest step, so two rotations of one cycle
/// compare equal and two DIFFERENT orders do not.
fn canonical(mut c: Vec<Step>) -> Vec<Step> {
    let Some(k) = (0..c.len()).min_by_key(|i| c[*i]) else {
        return c;
    };
    c.rotate_left(k);
    c
}

fn as_points(c: &[Step]) -> Vec<(u32, u32)> {
    c.iter().map(|s| (s.from.x, s.from.y)).collect()
}

/// Compare the stored face loops against the loops of the labelling.
///
/// `Ok(n)` with the number of cycles compared. A stored loop that is the right
/// half-edges in the wrong ORDER walks a different lattice cycle, so it does
/// not match anything on the derived side and is reported.
pub fn loops_agree_with_the_labelling(d: &Dcel) -> Result<usize, LoopDisagreement> {
    let arr = Arrangement::new(
        d.labelling().inside(),
        d.width_px(),
        d.height_px(),
        d.connectivity(),
    );
    let derived: Vec<Vec<Step>> = orbits(&arr).into_iter().map(canonical).collect();

    let mut stored: Vec<Vec<Step>> = Vec::new();
    for f in d.faces() {
        for lp in &f.loops {
            match steps_of(d, lp) {
                Some(s) => stored.push(canonical(s)),
                // A malformed path is the audit's business, not this check's;
                // report it here as a disagreement rather than panicking, so an
                // instrument asked to judge a broken value says what it found
                // (meta-rule M-4).
                None => {
                    return Err(LoopDisagreement {
                        stored_loops: d.loop_count(),
                        derived_loops: derived.len(),
                        first_unmatched: Vec::new(),
                        side: "stored (a loop expands to no walk at all)",
                    })
                }
            }
        }
    }

    let mut want: BTreeMap<Vec<Step>, usize> = BTreeMap::new();
    for c in &derived {
        *want.entry(c.clone()).or_default() += 1;
    }
    let mut have: BTreeMap<Vec<Step>, usize> = BTreeMap::new();
    for c in &stored {
        *have.entry(c.clone()).or_default() += 1;
    }
    if want != have {
        let only_stored = stored.iter().find(|c| have.get(*c) != want.get(*c));
        let (side, first) = match only_stored {
            Some(c) => ("stored loops", as_points(c)),
            None => (
                "loops of the labelling",
                derived.first().map(|c| as_points(c)).unwrap_or_default(),
            ),
        };
        return Err(LoopDisagreement {
            stored_loops: stored.len(),
            derived_loops: derived.len(),
            first_unmatched: first,
            side,
        });
    }
    Ok(stored.len())
}

/// The longest stored loop, in HALF-EDGES, and the mean.
///
/// Published because the check above is only exercised by loops long enough to
/// have an order: a loop of one or two half-edges has no reordering that
/// changes it.
///
/// **Corrected in delta-4, and the correction is mine to make.** This said
/// "neither M5 population had them". That is false, and it was false when I
/// wrote it: the red team's figure was an UPPER BOUND — "at most 55 of 1334" —
/// and I restated a bound as a measurement of absence. Measured since: with the
/// staircase fixture REMOVED, a reordering defect still reddens the gate on
/// **fourteen corpus arms**. The corpus carries loops of three or more.
///
/// What was measured to be zero is the STRUCTURAL REGISTER, and that is what
/// justifies the fixture — not the corpus's supposed emptiness. Condition 51's
/// standard is coverage BY CONSTRUCTION at every size under both arms, which
/// fourteen incidental corpus arms do not provide and the staircase does. The
/// fixture stands; the sentence that argued for it did not.
pub fn loop_length_profile(d: &Dcel) -> (usize, usize, usize) {
    let mut longest = 0usize;
    let mut total = 0usize;
    let mut at_least_three = 0usize;
    for f in d.faces() {
        for lp in &f.loops {
            longest = longest.max(lp.len());
            total += lp.len();
            if lp.len() >= 3 {
                at_least_three += 1;
            }
        }
    }
    (longest, total, at_least_three)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubical::Labelling;
    use vice_ir::ComplementaryConnectivity;

    fn staircase(k: usize, s: usize) -> Labelling {
        let n = k * s + 2;
        let mut inside = vec![false; n * n];
        for i in 0..k {
            for y in 0..s {
                for x in 0..s {
                    inside[(1 + i * s + y) * n + (1 + i * s + x)] = true;
                }
            }
        }
        Labelling::new(n, n, inside)
    }

    /// The fixture RT5-A13 says the milestone did not have: loops of three or
    /// more half-edges, BY CONSTRUCTION.
    #[test]
    fn a_diagonal_staircase_has_loops_of_three_or_more_half_edges() {
        for conn in ComplementaryConnectivity::arms() {
            let d = super::super::Dcel::assemble(staircase(4, 2), conn);
            let (longest, _total, at_least_three) = loop_length_profile(&d);
            assert!(
                longest >= 3,
                "the longest loop is {longest} half-edges; this fixture exists to exercise the \
                 rotation system and cannot do it with loops of one or two"
            );
            assert!(at_least_three > 0);
            assert!(loops_agree_with_the_labelling(&d).is_ok());
        }
    }

    /// **RT5-A13 / M5A-D2-N1.** Swapping two half-edges inside one loop leaves
    /// a cycle of the same half-edges in a different ORDER. Every delta-2 check
    /// reproduces it; this one does not.
    #[test]
    fn reordering_a_loop_is_caught_here_and_by_nothing_that_shipped_before() {
        let conn = ComplementaryConnectivity::arms()[0];
        let d = super::super::Dcel::assemble(staircase(4, 2), conn);
        assert!(
            loops_agree_with_the_labelling(&d).is_ok(),
            "positive control"
        );

        // Find a loop with something to reorder.
        let (fi, li) = d
            .faces()
            .iter()
            .enumerate()
            .find_map(|(fi, f)| {
                f.loops
                    .iter()
                    .position(|lp| lp.len() >= 3)
                    .map(|li| (fi, li))
            })
            .expect("the staircase has a loop of three or more");

        let mut parts = d.parts().clone();
        parts.faces[fi].loops[li].swap(0, 1);
        // The site index is kept CONSISTENT with the reordering, which is what
        // makes this a defect in the ORDER rather than a corrupt index — the
        // audit's site check would otherwise catch it for the wrong reason.
        for (f_i, f) in parts.faces.iter().enumerate() {
            for (l_i, lp) in f.loops.iter().enumerate() {
                for (p, h) in lp.iter().enumerate() {
                    parts.site[h.0 as usize] = (f_i as u32, l_i as u32, p as u32);
                }
            }
        }
        let broken = d.clone().with_parts(parts);

        // Everything delta-2 shipped reproduces it.
        assert!(
            super::super::face_map_agrees(&broken).is_ok(),
            "the rebuild reads only boundaries; a reordering moves none"
        );
        // The labelling anchor moves no pixel either.
        let (w, h) = (broken.width_px(), broken.height_px());
        for y in 0..h {
            for x in 0..w {
                let want = broken.labelling().inside()[y as usize * w as usize + x as usize];
                assert_eq!(
                    broken.faces()[broken.face_of_pixel(x, y).index()].label,
                    want,
                    "the anchor is untouched by a reordering"
                );
            }
        }

        // This check is not fooled.
        let e = loops_agree_with_the_labelling(&broken)
            .expect_err("a reordered loop walks a different lattice cycle");
        assert!(e.to_string().contains("stored"), "{e}");
        // And so is the audit, now that it runs this.
        assert!(super::super::audit(&broken).is_err());
    }

    /// The empty arrangement has no loops on either side, and that agrees
    /// rather than being skipped — the branch REVIEW_M5_B N11 was about.
    #[test]
    fn the_empty_arrangement_agrees_with_its_own_absence_of_loops() {
        let d = super::super::Dcel::assemble(
            Labelling::new(8, 8, vec![false; 64]),
            ComplementaryConnectivity::arms()[0],
        );
        assert_eq!(loops_agree_with_the_labelling(&d), Ok(0));
    }
}

#[cfg(test)]
mod maximality_tests {
    use super::*;
    use crate::cubical::Labelling;
    use vice_ir::ComplementaryConnectivity;

    /// **M5A-D3-N2: splitting a chain at an interior degree-two point.**
    ///
    /// It passed everything before delta-4: `audit()` returned no violation,
    /// `loops_agree_with_the_labelling` returned true — the WALK is unchanged,
    /// only its chunking is — `face_map_agrees` returned true, and V and B grew
    /// together so the Euler identity was preserved.
    ///
    /// Both directions: the intact structure is maximal, the split one is not.
    #[test]
    fn a_chain_split_at_an_interior_point_is_not_maximal() {
        let d = super::super::Dcel::assemble(
            Labelling::new(
                9,
                9,
                (0..81)
                    .map(|i| (2..7).contains(&(i % 9)) && (2..7).contains(&(i / 9)))
                    .collect(),
            ),
            ComplementaryConnectivity::arms()[0],
        );
        assert!(super::super::audit(&d).is_ok(), "positive control");
        let b0 = &d.boundaries()[0];
        assert!(
            b0.path.len() > 4,
            "the fixture needs a chain with an interior"
        );

        // Split boundary 0 in the middle by promoting an interior point to a
        // vertex — the chain still walks the same lattice cycle, it is simply
        // no longer maximal.
        let mut parts = d.parts().clone();
        let mid = parts.boundaries[0].path.len() / 2;
        let pt = parts.boundaries[0].path[mid];
        parts.vertices.push(pt);
        parts.vertices.sort_unstable();
        let broken = d.clone().with_parts(parts);

        // The loop walk is untouched: same steps, same cycle.
        assert!(
            loops_agree_with_the_labelling(&broken).is_ok(),
            "a chain's chunking is not its walk, so this check cannot see it - which is why              maximality needed its own comparison"
        );
        let e = super::super::audit(&broken).expect_err("§12 asks for MAXIMAL chains");
        assert!(
            matches!(
                e,
                super::super::InvariantViolation::ChainIsNotMaximal { .. }
            ),
            "{e}"
        );
    }
}
