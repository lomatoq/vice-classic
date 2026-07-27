//! An INDEPENDENT digital-topology computation, for ground truth only.
//!
//! ## Why this file exists
//!
//! RT45-A2. The recall clause was published as a measurement against an
//! independently computed truth, and only the AREA INTEGRATOR was independent:
//! the signature of the truth was taken with `vice_topology::signature`, the
//! same function that signs the candidates. The red team inserted exactly what
//! §5.3 forbids by name — the same connectivity on both sides —
//!
//! ```text
//! -    let background = count_regions(l, false, conn.background()).max(1);
//! +    let background = count_regions(l, false, conn.foreground()).max(1);
//! ```
//!
//! — and got three clauses MET, exit 0, 489 tests green, and **0 of 132 arms
//! changed their GT signature**. Any error in the convention, in the hole
//! count or in the padding cancels between the two sides, because both sides
//! are the same expression.
//!
//! So the chain from scene to compared number is independent here for its
//! WHOLE length, not only at its most expensive step:
//!
//! | step | production (candidate) | ground truth (this file) |
//! |---|---|---|
//! | coverage | estimated from the render | exact-clip integrator over the scene |
//! | components | union-find, path halving | breadth-first flood fill, explicit queue |
//! | holes | union-find over the BACKGROUND, padded ring | derived from the EULER characteristic |
//! | Euler | `components - holes`, by construction | 2x2 bit-quad counts, a LOCAL formula |
//!
//! The two disagree in direction: production computes χ from the two counts,
//! this file computes one count from χ. A convention error therefore cannot
//! cancel — it changes `nD`'s sign term here and the background traversal
//! there, and the cross-check goes red.
//!
//! ## The bit-quad identity
//!
//! Over every 2x2 window of the image padded with one background ring, with
//! `n1` windows holding exactly one foreground pixel, `n3` exactly three, and
//! `nD` exactly two on a diagonal:
//!
//! ```text
//! foreground 4-connected (background 8):  X = (n1 - n3 + 2*nD) / 4
//! foreground 8-connected (background 4):  X = (n1 - n3 - 2*nD) / 4
//! ```
//!
//! The connectivity convention enters through the SIGN of the `nD` term, and
//! nowhere else. That is the whole reason this is a witness: a
//! same-connectivity mutation in the production signature cannot move this
//! number, so the two answers separate.

use vice_ir::{ComplementaryConnectivity, PixelConnectivity};

/// The three numbers, computed without `vice-topology`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndependentSignature {
    pub components: u32,
    pub holes: u32,
    pub euler: i32,
}

fn at(inside: &[bool], w: usize, h: usize, x: isize, y: isize) -> bool {
    if x < 0 || y < 0 || x >= w as isize || y >= h as isize {
        return false;
    }
    inside[y as usize * w + x as usize]
}

/// Connected components by BREADTH-FIRST FLOOD FILL with an explicit queue.
///
/// Deliberately not a union-find: the production side is a disjoint-set forest
/// with path halving, and two implementations of the same idea would share its
/// mistakes.
fn components(inside: &[bool], w: usize, h: usize, c: PixelConnectivity) -> u32 {
    let offsets: &[(isize, isize)] = match c {
        PixelConnectivity::Four => &[(1, 0), (-1, 0), (0, 1), (0, -1)],
        PixelConnectivity::Eight => &[
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ],
    };
    let mut seen = vec![false; inside.len()];
    let mut queue: std::collections::VecDeque<(isize, isize)> = std::collections::VecDeque::new();
    let mut count = 0u32;
    for start in 0..inside.len() {
        if !inside[start] || seen[start] {
            continue;
        }
        count += 1;
        seen[start] = true;
        queue.clear();
        queue.push_back(((start % w) as isize, (start / w) as isize));
        while let Some((x, y)) = queue.pop_front() {
            for (dx, dy) in offsets {
                let (nx, ny) = (x + dx, y + dy);
                if !at(inside, w, h, nx, ny) {
                    continue;
                }
                let i = ny as usize * w + nx as usize;
                if seen[i] {
                    continue;
                }
                seen[i] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    count
}

/// Euler characteristic from 2x2 bit-quad counts.
fn euler(inside: &[bool], w: usize, h: usize, c: PixelConnectivity) -> i32 {
    let (mut n1, mut n3, mut nd) = (0i64, 0i64, 0i64);
    // Windows over the image padded with one background ring, so the border
    // is counted exactly once and the outside is background — the same
    // convention the production side gets from its padding ring.
    for y in -1..h as isize {
        for x in -1..w as isize {
            let q = [
                at(inside, w, h, x, y),
                at(inside, w, h, x + 1, y),
                at(inside, w, h, x, y + 1),
                at(inside, w, h, x + 1, y + 1),
            ];
            match q.iter().filter(|v| **v).count() {
                1 => n1 += 1,
                3 => n3 += 1,
                // Diagonal iff the two set pixels are not edge-adjacent.
                2 if (q[0] && q[3]) || (q[1] && q[2]) => nd += 1,
                _ => {}
            }
        }
    }
    let signed = match c {
        PixelConnectivity::Four => n1 - n3 + 2 * nd,
        PixelConnectivity::Eight => n1 - n3 - 2 * nd,
    };
    debug_assert_eq!(
        signed % 4,
        0,
        "the bit-quad sum is always a multiple of four"
    );
    (signed / 4) as i32
}

/// The independent signature of a labelling under one convention.
///
/// `holes` is DERIVED (`components - euler`) rather than counted, which is the
/// direction production does not take.
pub fn signature_of(
    inside: &[bool],
    width_px: usize,
    height_px: usize,
    conn: ComplementaryConnectivity,
) -> IndependentSignature {
    let comps = components(inside, width_px, height_px, conn.foreground());
    let x = euler(inside, width_px, height_px, conn.foreground());
    IndependentSignature {
        components: comps,
        holes: (comps as i32 - x).max(0) as u32,
        euler: x,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_topology::{signature, Labelling};

    /// A labelling with its shape.
    type Fixture = (Vec<bool>, usize, usize);

    fn rows(r: &[&str]) -> Fixture {
        let h = r.len();
        let w = r[0].len();
        (
            r.iter().flat_map(|s| s.chars().map(|c| c == '#')).collect(),
            w,
            h,
        )
    }

    /// The DIAGONAL RING: the witness the red team used, and the fixture the
    /// old positive test did not have.
    ///
    /// `|x-5| + |y-5| == 4` on 11x11. Under 4-connected foreground it is
    /// sixteen isolated pixels and the 8-connected background leaks between
    /// them, so there is NO hole; under 8-connected foreground it is one loop
    /// and the 4-connected background inside it IS a hole. An ordinary ring
    /// reads (1, 1) under both conventions and therefore witnesses nothing.
    fn diagonal_ring() -> Fixture {
        let n = 11usize;
        let mut v = vec![false; n * n];
        for y in 0..n {
            for x in 0..n {
                let d = (x as i32 - 5).abs() + (y as i32 - 5).abs();
                v[y * n + x] = d == 4;
            }
        }
        (v, n, n)
    }

    #[test]
    fn the_diagonal_ring_separates_the_two_conventions() {
        let (v, w, h) = diagonal_ring();
        let [four, eight] = ComplementaryConnectivity::arms();
        let a = signature_of(&v, w, h, four);
        let b = signature_of(&v, w, h, eight);
        println!("independent fg=4 {a:?}");
        println!("independent fg=8 {b:?}");
        assert_eq!(
            (a.components, a.holes),
            (16, 0),
            "4-connected foreground: sixteen pixels, and the 8-connected background leaks \
             between them"
        );
        assert_eq!(
            (b.components, b.holes),
            (1, 1),
            "8-connected foreground: one loop around a 4-connected background hole"
        );
        assert_ne!(
            (a.components, a.holes),
            (b.components, b.holes),
            "if the two conventions agreed here the fixture would witness nothing"
        );
    }

    /// The cross-check: on every witness, the independent chain and the
    /// production one agree, under BOTH conventions.
    ///
    /// This is the test RT45-A2 asks for. The red-team mutation
    /// (`conn.background()` -> `conn.foreground()` in
    /// `vice_topology::cubical::signature`) moves the production side on the
    /// diagonal ring and leaves this side untouched, so the assertion fails —
    /// where the whole 132-arm corpus did not notice.
    #[test]
    fn the_independent_chain_agrees_with_the_production_signature() {
        let mut fixtures: Vec<(&str, Fixture)> = vec![
            ("single pixel", rows(&["...", ".#.", "..."])),
            ("solid block", rows(&["...", ".##", ".##"])),
            ("ring", rows(&["#####", "#...#", "#.#.#", "#...#", "#####"])),
            (
                "two blobs",
                rows(&["##.##", "##.##", ".....", "##.##", "##.##"]),
            ),
            ("saddle", rows(&["#.", ".#"])),
            ("stripe across", rows(&[".....", "#####", "....."])),
            ("checkerboard", rows(&["#.#.", ".#.#", "#.#.", ".#.#"])),
            ("empty", rows(&["..", ".."])),
            ("full", rows(&["##", "##"])),
            ("diagonal chain", rows(&["#...", ".#..", "..#.", "...#"])),
            (
                "nested rings",
                rows(&[
                    "#######", "#.....#", "#.###.#", "#.#.#.#", "#.###.#", "#.....#", "#######",
                ]),
            ),
        ];
        fixtures.push(("diagonal ring", diagonal_ring()));

        let mut compared = 0;
        for (name, (v, w, h)) in &fixtures {
            for arm in ComplementaryConnectivity::arms() {
                let mine = signature_of(v, *w, *h, arm);
                let theirs = signature(&Labelling::new(*w, *h, v.clone()), arm);
                assert_eq!(
                    (mine.components, mine.holes, mine.euler),
                    (theirs.components, theirs.holes, theirs.euler),
                    "{name} under fg={}: the independent chain says {:?}, the production \
                     signature says {:?}. Two chains that disagree mean one of them is wrong, \
                     and neither is allowed to be the referee for the other",
                    match arm.foreground() {
                        PixelConnectivity::Four => 4,
                        PixelConnectivity::Eight => 8,
                    },
                    mine,
                    (theirs.components, theirs.holes, theirs.euler)
                );
                compared += 1;
            }
        }
        assert!(
            compared >= 20,
            "only {compared} comparisons; the witness set is not being walked"
        );
        println!("{compared} independent-vs-production comparisons agree");
    }

    /// The witness set contains a fixture on which the two CONVENTIONS
    /// disagree — otherwise the cross-check above could pass under a tree
    /// where the convention is ignored entirely.
    #[test]
    fn the_witness_set_contains_a_convention_sensitive_fixture() {
        let [four, eight] = ComplementaryConnectivity::arms();
        let mut sensitive = 0;
        for (v, w, h) in [
            diagonal_ring(),
            rows(&["#.", ".#"]),
            rows(&["#...", ".#..", "..#.", "...#"]),
        ] {
            let a = signature_of(&v, w, h, four);
            let b = signature_of(&v, w, h, eight);
            if (a.components, a.holes) != (b.components, b.holes) {
                sensitive += 1;
            }
        }
        assert!(
            sensitive >= 2,
            "the witness set must contain fixtures where the two admissible conventions give \
             different answers, or it cannot detect a tree that confuses them"
        );
    }
}
