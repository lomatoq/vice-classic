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

/// Where the stored vertex set and the vertex set of the labelling differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexDisagreement {
    /// Points the structure calls vertices and the labelling does not: a chain
    /// was split where nothing meets. OVER-splitting.
    pub stored_only: Vec<(u32, u32)>,
    /// Points the labelling makes vertices and the structure does not: a
    /// junction swallowed inside a chain. UNDER-splitting.
    pub derived_only: Vec<(u32, u32)>,
    pub stored: usize,
    pub derived: usize,
}

impl std::fmt::Display for VertexDisagreement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.stored_only.is_empty() && self.derived_only.is_empty() {
            return write!(
                f,
                "the vertex SET is right and its ORDER is not: {} vertices, stored in an order the                  canonical one does not produce (REVIEW_M5_A D5-N1)",
                self.stored
            );
        }
        write!(
            f,
            "{} stored vertices against {} the labelling requires; {} point(s) are vertices only \
             in the structure (a chain split where nothing meets: {:?}) and {} only in the \
             labelling (a junction swallowed inside a chain: {:?})",
            self.stored,
            self.derived,
            self.stored_only.len(),
            self.stored_only.first(),
            self.derived_only.len(),
            self.derived_only.first()
        )
    }
}

/// The vertex set §12's MAXIMAL chains require, computed from the labelling.
///
/// A lattice point is a vertex exactly when it is a junction — degree other
/// than two — and, for a loop that carries no junction at all, the canonical
/// smallest point on it, because a chain needs endpoints.
///
/// This is the same rule `assemble` follows, run again from the INPUT. That is
/// the whole point of it existing: see [`vertices_agree_with_the_labelling`].
pub fn vertices_of_the_labelling(d: &Dcel) -> Vec<(u32, u32)> {
    let arr = Arrangement::new(
        d.labelling().inside(),
        d.width_px(),
        d.height_px(),
        d.connectivity(),
    );
    let mut set: std::collections::BTreeSet<Lat> = std::collections::BTreeSet::new();
    for lp in orbits(&arr) {
        let mut has_junction = false;
        for s in &lp {
            if arr.degree(s.from) != 2 {
                set.insert(s.from);
                has_junction = true;
            }
        }
        if !has_junction {
            if let Some(v) = lp.iter().map(|s| s.from).min() {
                set.insert(v);
            }
        }
    }
    set.into_iter().map(|v| (v.x, v.y)).collect()
}

/// **§12's MAXIMAL chains, in BOTH directions.**
///
/// ## What delta-4 closed and what it did not
///
/// Non-maximality has two directions and delta-4's check saw one:
///
/// | | violation | delta-4 |
/// |---|---|---|
/// | **under-splitting** | a junction lies INSIDE a chain | caught |
/// | **over-splitting** | a chain END is not a junction | **not caught** |
///
/// Two contexts refuted the closure claim independently and neither by reading:
/// reviewer A re-ran their delta-3 experiment verbatim against the delta-4 HEAD
/// and got `audit -> None`, `loops_agree -> true` at a break point of lattice
/// degree two; reviewer B rebuilt the split independently on 9x9 and repeated it
/// at 512 px with the same result. The in-tree test that was recorded as closing
/// it performed a PROMOTION — raising an interior point to a vertex while
/// leaving the chain whole — which is a different transformation from the one
/// it was written about.
///
/// ## The cause, and it is the fourth repetition of one class
///
/// The delta-4 check read `d.vertices()` — the STORED set, an output of the same
/// `assemble` — while the comment beside it said the vertex set comes from
/// lattice degree and therefore shares no provenance. That sentence described
/// how `assemble` BUILDS the set, not what the check READS. It is RT5-A9's form
/// a fourth time, inside the fix for a finding whose entire content was
/// provenance, and it is why Q4 must be read off the CODE rather than off the
/// intention.
///
/// ## What this does instead
///
/// The legal vertex set is derived from the labelling by
/// [`vertices_of_the_labelling`] and compared with the stored one. Equality
/// fails in both directions at once: an added vertex is over-splitting, a
/// missing one is under-splitting. Nothing here reads `d.vertices()` except as
/// the side being judged.
pub fn vertices_agree_with_the_labelling(d: &Dcel) -> Result<usize, VertexDisagreement> {
    let derived: std::collections::BTreeSet<(u32, u32)> =
        vertices_of_the_labelling(d).into_iter().collect();
    let stored: std::collections::BTreeSet<(u32, u32)> = d.vertices().iter().copied().collect();
    if derived == stored {
        // ORDER, not only membership (REVIEW_M5_A D5-N1). The determinism
        // paragraph of `dcel` promises a canonical order and nothing checked
        // that `assemble` produces one: permuting `vertices` with a consistent
        // remap of every `start`/`end` left `audit` returning None. The derived
        // set is a `BTreeSet`, so its iteration order IS the canonical one, and
        // comparing sequences rather than sets costs nothing extra.
        let want: Vec<(u32, u32)> = derived.iter().copied().collect();
        if d.vertices() != want.as_slice() {
            return Err(VertexDisagreement {
                stored_only: Vec::new(),
                derived_only: Vec::new(),
                stored: d.vertices().len(),
                derived: want.len(),
            });
        }
        return Ok(derived.len());
    }
    Err(VertexDisagreement {
        stored_only: stored.difference(&derived).copied().collect(),
        derived_only: derived.difference(&stored).copied().collect(),
        stored: stored.len(),
        derived: derived.len(),
    })
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

    fn square() -> super::super::Dcel {
        super::super::Dcel::assemble(
            Labelling::new(
                9,
                9,
                (0..81)
                    .map(|i| (2..7).contains(&(i % 9)) && (2..7).contains(&(i / 9)))
                    .collect(),
            ),
            ComplementaryConnectivity::arms()[0],
        )
    }

    /// **M5A-D4-N1 = M5B-N18: a REAL split, not a promotion.**
    ///
    /// Delta-4's test raised an interior point to a vertex and left the chain
    /// whole. That is a different transformation from the one it was recorded
    /// as closing, and reviewer A named the method rule against themselves: a
    /// finding shipped as PROSE loses whichever half the prose did not
    /// distinguish. So this performs the corruption the reviewers performed —
    /// the chain is actually CUT in two at a point of lattice degree two, with
    /// every index kept consistent.
    ///
    /// Both directions, and the negative one is the point: every check that
    /// shipped before delta-5 reproduces this.
    #[test]
    fn a_chain_cut_at_a_degree_two_point_is_over_split_and_is_caught() {
        let d = square();
        assert!(super::super::audit(&d).is_ok(), "positive control");
        assert_eq!(d.boundaries().len(), 1, "the square is one closed chain");

        let mut parts = d.parts().clone();
        let path = parts.boundaries[0].path.clone();
        let mid = path.len() / 2;
        let cut = path[mid];
        // A genuine SPLIT: one chain becomes two, meeting at `cut`, which the
        // labelling does not make a vertex.
        let v_new = parts.vertices.len() as u32;
        parts.vertices.push(cut);
        let (first, second) = (path[..=mid].to_vec(), path[mid..].to_vec());
        let ends = (
            parts.boundaries[0].start,
            parts.boundaries[0].end,
            parts.boundaries[0].owners,
        );
        parts.boundaries[0].path = first;
        parts.boundaries[0].end = super::super::VertexId(v_new);
        parts.boundaries.push(super::super::Boundary {
            owners: ends.2,
            start: super::super::VertexId(v_new),
            end: ends.1,
            path: second,
        });
        let _ = ends.0;
        // Keep the loops and the site index consistent with the new chain, so
        // this is a defect in MAXIMALITY and not a corrupt index that some
        // other check would catch for the wrong reason.
        let h_new_f = super::super::HalfEdgeId::new(super::super::BoundaryId(1), true);
        let h_new_b = super::super::HalfEdgeId::new(super::super::BoundaryId(1), false);
        for f in parts.faces.iter_mut() {
            for lp in f.loops.iter_mut() {
                let mut out = Vec::new();
                for h in lp.iter() {
                    out.push(*h);
                    if h.boundary().0 == 0 {
                        out.push(if h.is_forward() { h_new_f } else { h_new_b });
                    }
                }
                *lp = out;
            }
        }
        parts.site.resize(parts.boundaries.len() * 2, (0, 0, 0));
        for (f_i, f) in parts.faces.iter().enumerate() {
            for (l_i, lp) in f.loops.iter().enumerate() {
                for (p, h) in lp.iter().enumerate() {
                    parts.site[h.0 as usize] = (f_i as u32, l_i as u32, p as u32);
                }
            }
        }
        let broken = d.clone().with_parts(parts);

        // Everything that shipped before delta-5 reproduces it.
        assert!(
            super::super::face_map_agrees(&broken).is_ok(),
            "the rebuild walks segments, and a cut adds no segment"
        );
        assert!(
            loops_agree_with_the_labelling(&broken).is_ok(),
            "the WALK is unchanged; only its chunking is - which is exactly why delta-4's              interior-point test could not see this"
        );

        // The derived vertex set does not.
        let e = vertices_agree_with_the_labelling(&broken)
            .expect_err("the cut point is a vertex the labelling does not make one");
        assert_eq!(e.stored_only.len(), 1);
        assert_eq!(e.stored_only[0], cut);
        assert!(
            e.derived_only.is_empty(),
            "nothing was swallowed, only added"
        );
        assert!(super::super::audit(&broken).is_err());
    }

    /// **M5A-D5-N1: the vertex ORDER, not only the set.**
    ///
    /// Permuting `vertices` with a consistent remap of every `start`/`end` left
    /// `audit` returning nothing before delta-6. The class the reviewer named:
    /// everything bound to the input is bound as a SET or a CYCLIC SEQUENCE,
    /// and the freedoms left over — index and order — are bound only to
    /// themselves.
    #[test]
    fn a_consistently_permuted_vertex_list_is_caught_by_order() {
        let d = super::super::Dcel::assemble(
            Labelling::new(
                6,
                6,
                (0..36)
                    .map(|i| {
                        let (x, y) = (i % 6, i / 6);
                        (x < 3 && y < 3) || (x >= 3 && y >= 3)
                    })
                    .collect(),
            ),
            ComplementaryConnectivity::arms()[0],
        );
        assert!(super::super::audit(&d).is_ok(), "positive control");
        if d.vertices().len() < 2 {
            return; // nothing to permute; the register covers the rest
        }
        let mut parts = d.parts().clone();
        parts.vertices.swap(0, 1);
        for b in parts.boundaries.iter_mut() {
            let remap = |v: super::super::VertexId| match v.0 {
                0 => super::super::VertexId(1),
                1 => super::super::VertexId(0),
                other => super::super::VertexId(other),
            };
            b.start = remap(b.start);
            b.end = remap(b.end);
        }
        let broken = d.clone().with_parts(parts);
        let e = vertices_agree_with_the_labelling(&broken)
            .expect_err("the vertex order is part of the canonical form");
        assert!(e.stored_only.is_empty() && e.derived_only.is_empty(), "{e}");
        assert!(super::super::audit(&broken).is_err());
    }

    /// The other direction: a junction swallowed INSIDE a chain. Delta-4 caught
    /// this one, and it must stay caught.
    #[test]
    fn a_junction_swallowed_inside_a_chain_is_under_split_and_is_caught() {
        let d = super::super::Dcel::assemble(
            Labelling::new(
                6,
                6,
                (0..36)
                    .map(|i| {
                        let (x, y) = (i % 6, i / 6);
                        (x < 3 && y < 3) || (x >= 3 && y >= 3)
                    })
                    .collect(),
            ),
            ComplementaryConnectivity::arms()[0],
        );
        assert!(super::super::audit(&d).is_ok(), "positive control");
        assert!(
            !d.vertices().is_empty(),
            "the diagonal pinch must produce a junction"
        );

        let mut parts = d.parts().clone();
        let removed = parts.vertices.remove(0);
        let broken = d.clone().with_parts(parts);
        let e = vertices_agree_with_the_labelling(&broken)
            .expect_err("a junction the labelling requires was removed");
        assert_eq!(e.derived_only, vec![removed]);
    }

    /// The vertex set the labelling requires is the one `assemble` produces, on
    /// every fixture of the structural register at every fast size.
    #[test]
    fn the_derived_vertex_set_matches_the_assembled_one_across_the_register() {
        for n in [32usize, 64, 128] {
            for f in crate::dcel::structural_fixtures(n) {
                for conn in ComplementaryConnectivity::arms() {
                    let d = super::super::Dcel::assemble(f.labelling.clone(), conn);
                    assert!(
                        vertices_agree_with_the_labelling(&d).is_ok(),
                        "{} at {n}",
                        f.name
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod branch_label_tests {
    /// **RT5-A18 / N17: labels unique by construction, without source position.**
    ///
    /// N17 wanted a new branch reusing an existing label to be impossible.
    /// Delta-4 bought that with `line!()`, which put source line numbers into a
    /// signed Tier A artifact and made its bytes a function of one file's
    /// layout. Uniqueness does not need a position: the labels are literals and
    /// this requires them to be pairwise distinct.
    ///
    /// Both directions: the scan must FIND the labels (an empty scan is
    /// vacuously distinct, F-0039), and duplicates must fail.
    ///
    /// **Residual, named where the strength is claimed, with its price.** Two
    /// holes were found separately and both are closed here: the scan read ONE
    /// hardcoded file, so a branch elsewhere was invisible for ZERO lines
    /// (RT5-A22), and it matched literals, so `const R: &str = "empty";` hid a
    /// duplicate for TWO (REVIEW_M5_B E18b). It now walks the whole module tree
    /// and REFUSES a non-literal rather than skipping it.
    ///
    /// What remains: a label produced by a macro that expands to a literal, or
    /// a branch in a file outside `src/dcel`. Cheapest known bypass: **one
    /// line**, a `branch:` written through a macro. Same class as the
    /// serde-attribute scan and the same closure — a derive that emits the
    /// labels and their distinctness proof together.
    #[test]
    fn every_judge_branch_has_a_distinct_label() {
        // EVERY file of the dcel module tree, not one hardcoded path.
        // REDTEAM_M5 RT5-A22: the scan read `audit.rs` alone, so a branch in
        // `loops.rs`, `crossing.rs` or a new file was invisible to it at a cost
        // of ZERO lines. The walk is over the directory now, and it asserts it
        // found more than one file so an empty walk cannot pass.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/dcel");
        let mut files = 0usize;
        let mut src = String::new();
        for e in std::fs::read_dir(&dir).expect("src/dcel").flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "rs") {
                src.push_str(&std::fs::read_to_string(&p).expect("read"));
                files += 1;
            }
        }
        assert!(
            files > 4,
            "the scan found {files} files; it is not covering the module"
        );
        let mut labels: Vec<String> = Vec::new();
        for line in src.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("branch: ") {
                let v = rest.trim_end_matches(',').trim();
                // A LITERAL is required. `branch: SOME_CONST` hides a duplicate
                // for two lines (REVIEW_M5_B E18b), so an indirection is a
                // failure here rather than something the scan quietly skips.
                assert!(
                    v.starts_with('"') && v.ends_with('"'),
                    "branch label {v:?} is not a string literal. An indirection -                      `const X: &str = \"empty\";` - hides a duplicate from this scan for two                      lines, so it is refused rather than skipped"
                );
                labels.push(v.trim_matches('"').to_string());
            }
        }
        assert!(
            labels.len() >= 2,
            "found {} branch labels; the scan is not reading the judge",
            labels.len()
        );
        let mut sorted: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            before,
            "two branches of the judge share a label: {labels:?}. A branch reusing an existing \
             name hides from the probe for zero lines, which is what N17 was about"
        );
        assert!(
            !labels.iter().any(|l| l.contains('@')),
            "a branch label carries a source position: {labels:?}. That lands in the signed \
             artifact and makes its bytes a function of source layout (RT5-A18)"
        );
    }
}
