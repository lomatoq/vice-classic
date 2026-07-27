//! The cubical complex of a thresholded scalar field, and the signature it
//! carries (spec §5.3, §11.2, §11.3).
//!
//! ## What a signature here IS, and what it is NOT
//!
//! It is a COMBINATORIAL statement about a digital labelling under one
//! complementary-connectivity convention: how many foreground components,
//! how many holes, and the Euler characteristic that follows. REVIEW_M1
//! M1-N5 is the reason to say this out loud: a scene there satisfied every
//! combinatorial invariant of §12 — twins, two owners per interior
//! boundary, `V - E + F = 1 + C` — and still had no planar embedding. A
//! combinatorial signature is not an embedding certificate, and nothing in
//! this module claims otherwise. The certificate arrives with the DCEL and
//! the renderer's partition check (M5, ADR-0010).
//!
//! ## Complementary connectivity is not optional and not representable
//! ## otherwise
//!
//! §5.3: one side 4-connected, the other 8-connected. `vice-ir` already
//! makes a non-complementary pair unrepresentable by storing only the
//! foreground arm and DERIVING the background, and this module takes that
//! type rather than two integers. Both arms are legitimate hypotheses and
//! the envelope carries both.
//!
//! ## Holes are counted through the background, not through a formula
//!
//! `holes = (background components) - 1` on a labelling padded with one ring
//! of background, so:
//!
//! - the exterior is a real region, never a negative magic label (§5.3);
//! - a foreground stripe that crosses the whole canvas does not manufacture
//!   a hole, because the padding ring keeps the outside connected around it;
//! - the Euler characteristic is then `components - holes` BY CONSTRUCTION
//!   rather than by a bit-quad formula whose connectivity convention has to
//!   be kept in agreement with the union-find by hand. Two computations of
//!   "the same" number that can drift apart is the shape of REVIEW_M3 M3-N1.
//!
//! ## Saddles
//!
//! A critical 2x2 — two foreground pixels on one diagonal, two background on
//! the other — is the configuration where the answer depends on the
//! convention rather than on the image (§5.3, Latecki 1995). It must never be
//! settled by iteration order. [`SaddleResolution`] enumerates the three
//! readings this milestone offers, each applied as ONE BATCH computed from
//! the original labelling, and the count of critical cells that REMAIN is
//! published rather than assumed to be zero.

use serde::Serialize;
use sha2::{Digest, Sha256};
use vice_ir::{ComplementaryConnectivity, PixelConnectivity};

/// A binary labelling of the pixel grid: `true` = foreground.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Labelling {
    width_px: usize,
    height_px: usize,
    inside: Vec<bool>,
}

impl Labelling {
    pub fn new(width_px: usize, height_px: usize, inside: Vec<bool>) -> Labelling {
        assert_eq!(inside.len(), width_px * height_px, "labelling shape");
        Labelling {
            width_px,
            height_px,
            inside,
        }
    }
    pub fn width_px(&self) -> usize {
        self.width_px
    }
    pub fn height_px(&self) -> usize {
        self.height_px
    }
    pub fn inside(&self) -> &[bool] {
        &self.inside
    }
    pub fn count_inside(&self) -> usize {
        self.inside.iter().filter(|v| **v).count()
    }
}

/// How a critical 2x2 is read.
///
/// Not a tie-break: all three are kept as hypotheses where they differ, and
/// where they do not differ they collapse to one candidate through the
/// signature digest. §5.3 forbids exactly one thing — resolving a saddle by
/// iteration order — and none of these does that: each is a function of the
/// ORIGINAL labelling applied to every critical cell at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SaddleResolution {
    /// Leave the critical cells alone; the connectivity convention decides,
    /// which is the reading §5.3 fixes as the default.
    Thresholded,
    /// Turn every critical 2x2 fully foreground: the diagonal is JOINED for
    /// both connectivities, so the reading no longer depends on convention.
    JoinDiagonal,
    /// Turn every critical 2x2 fully background: the diagonal is SPLIT for
    /// both connectivities.
    SplitDiagonal,
}

impl SaddleResolution {
    pub fn as_str(self) -> &'static str {
        match self {
            SaddleResolution::Thresholded => "thresholded",
            SaddleResolution::JoinDiagonal => "join_diagonal",
            SaddleResolution::SplitDiagonal => "split_diagonal",
        }
    }
    pub const ALL: &'static [SaddleResolution] = &[
        SaddleResolution::Thresholded,
        SaddleResolution::JoinDiagonal,
        SaddleResolution::SplitDiagonal,
    ];
}

/// Top-left indices of every critical 2x2 of a labelling.
///
/// Critical means the two diagonals disagree with each other: exactly one
/// diagonal is foreground. That is the configuration whose connected
/// components differ between the 4- and 8-connected readings.
pub fn critical_cells(l: &Labelling) -> Vec<usize> {
    let (w, h) = (l.width_px, l.height_px);
    let mut out = Vec::new();
    if w < 2 || h < 2 {
        return out;
    }
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            let i = y * w + x;
            let (a, b, c, d) = (
                l.inside[i],
                l.inside[i + 1],
                l.inside[i + w],
                l.inside[i + w + 1],
            );
            if (a && d && !b && !c) || (b && c && !a && !d) {
                out.push(i);
            }
        }
    }
    out
}

/// Threshold a field at `level` and apply one saddle resolution.
///
/// The resolution is a BATCH: the critical cells are found on the labelling
/// as thresholded, and every one of them is rewritten from that snapshot, so
/// no cell can see another cell's rewrite. Iteration order is therefore not
/// an input, which is what §11.2 demands of plateaus and ties.
///
/// One pass does not guarantee a well-composed result — rewriting a cell can
/// make a NEIGHBOURING 2x2 critical — and this module does not pretend
/// otherwise: [`residual_critical_cells`] is published on every hypothesis as
/// an ambiguity flag. Claiming well-composedness would be a statement wider
/// than the mechanism.
pub fn threshold(
    values: &[f64],
    width_px: usize,
    height_px: usize,
    level: f64,
    saddle: SaddleResolution,
) -> Labelling {
    let base = Labelling::new(
        width_px,
        height_px,
        values.iter().map(|v| *v >= level).collect(),
    );
    if saddle == SaddleResolution::Thresholded {
        return base;
    }
    let cells = critical_cells(&base);
    let mut out = base.inside.clone();
    let want = saddle == SaddleResolution::JoinDiagonal;
    for i in cells {
        for j in [i, i + 1, i + width_px, i + width_px + 1] {
            out[j] = want;
        }
    }
    Labelling::new(width_px, height_px, out)
}

/// Critical cells that survive the resolution. Zero means the labelling is
/// well-composed; anything else is an ambiguity the envelope carries.
pub fn residual_critical_cells(l: &Labelling) -> u32 {
    critical_cells(l).len() as u32
}

/// The combinatorial signature of one labelling under one convention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopologySignature {
    pub components: u32,
    pub holes: u32,
    pub euler: i32,
    pub foreground_connectivity: &'static str,
    pub background_connectivity: &'static str,
    /// sha256 over the labelling itself: shape, convention and run lengths.
    /// Contains no float, so it is the same on every platform — the
    /// F-0022 rule ("a function of a float is a float") applied in the
    /// direction that lets a structural projection keep it.
    pub digest: String,
}

fn conn_name(c: PixelConnectivity) -> &'static str {
    match c {
        PixelConnectivity::Four => "4",
        PixelConnectivity::Eight => "8",
    }
}

fn offsets(c: PixelConnectivity) -> &'static [(isize, isize)] {
    match c {
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
    }
}

struct Dsu(Vec<usize>);

impl Dsu {
    fn new(n: usize) -> Dsu {
        Dsu((0..n).collect())
    }
    fn find(&mut self, x: usize) -> usize {
        let mut x = x;
        while self.0[x] != x {
            self.0[x] = self.0[self.0[x]];
            x = self.0[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) -> bool {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return false;
        }
        // Smaller index survives: a canonical rule, so the labels do not
        // depend on the order edges arrive in.
        let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
        self.0[hi] = lo;
        true
    }
}

/// Count connected components of `select` over a padded grid.
///
/// Padded by one ring which is always BACKGROUND, so `count_regions(false)`
/// counts the outside plus every hole, and the outside is exactly one region
/// however the foreground touches the canvas edge.
fn count_regions(l: &Labelling, want: bool, c: PixelConnectivity) -> u32 {
    let (w, h) = (l.width_px + 2, l.height_px + 2);
    let mut grid = vec![false; w * h];
    for y in 0..l.height_px {
        for x in 0..l.width_px {
            grid[(y + 1) * w + (x + 1)] = l.inside[y * l.width_px + x];
        }
    }
    let mut dsu = Dsu::new(w * h);
    let offs = offsets(c);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if grid[i] != want {
                continue;
            }
            for (dx, dy) in offs {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                if grid[j] == want {
                    dsu.union(i, j);
                }
            }
        }
    }
    let mut roots = std::collections::BTreeSet::new();
    for (i, cell) in grid.iter().enumerate() {
        if *cell == want {
            let r = dsu.find(i);
            roots.insert(r);
        }
    }
    roots.len() as u32
}

/// The signature of a labelling under one complementary-connectivity arm.
pub fn signature(l: &Labelling, conn: ComplementaryConnectivity) -> TopologySignature {
    let components = count_regions(l, true, conn.foreground());
    // The padded ring guarantees at least one background region.
    let background = count_regions(l, false, conn.background()).max(1);
    let holes = background - 1;
    TopologySignature {
        components,
        holes,
        euler: components as i32 - holes as i32,
        foreground_connectivity: conn_name(conn.foreground()),
        background_connectivity: conn_name(conn.background()),
        digest: digest(l, conn),
    }
}

/// sha256 over shape, convention and the run lengths of the labelling.
fn digest(l: &Labelling, conn: ComplementaryConnectivity) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"vice-classic/cubical-signature/v1\x1f");
    hasher.update((l.width_px as u64).to_be_bytes());
    hasher.update((l.height_px as u64).to_be_bytes());
    hasher.update(conn_name(conn.foreground()).as_bytes());
    hasher.update(b"\x1f");
    let mut run: u32 = 0;
    let mut cur = false;
    for v in &l.inside {
        if *v == cur {
            run += 1;
        } else {
            hasher.update(run.to_be_bytes());
            cur = *v;
            run = 1;
        }
    }
    hasher.update(run.to_be_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_rows(rows: &[&str]) -> Labelling {
        let h = rows.len();
        let w = rows[0].len();
        let inside: Vec<bool> = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| c == '#'))
            .collect();
        Labelling::new(w, h, inside)
    }

    fn arms() -> [ComplementaryConnectivity; 2] {
        ComplementaryConnectivity::arms()
    }

    /// A ring is one component with one hole, under BOTH arms, and Euler
    /// follows. The fixture that would be wrong under a non-complementary
    /// convention.
    #[test]
    fn a_ring_is_one_component_with_one_hole_under_both_arms() {
        let l = from_rows(&["#####", "#...#", "#.#.#", "#...#", "#####"]);
        for arm in arms() {
            let s = signature(&l, arm);
            assert_eq!(s.components, 2, "the ring and the pip inside it");
            assert_eq!(s.holes, 1, "the annulus between them");
            assert_eq!(s.euler, 1);
            assert_ne!(s.foreground_connectivity, s.background_connectivity);
        }
    }

    /// A stripe crossing the canvas splits the background in two, and that
    /// is NOT a hole. The padding ring is what makes this true, and without
    /// it every full-bleed edge would manufacture one.
    #[test]
    fn a_stripe_across_the_canvas_makes_no_hole() {
        let l = from_rows(&[".....", "#####", "....."]);
        for arm in arms() {
            let s = signature(&l, arm);
            assert_eq!(s.components, 1);
            assert_eq!(s.holes, 0, "the outside stays one region around the stripe");
            assert_eq!(s.euler, 1);
        }
    }

    /// The checkerboard saddle: the two arms DISAGREE, which is the whole
    /// reason §5.3 forbids using one connectivity on both sides, and the
    /// reason both arms are hypotheses rather than a default plus a comment.
    #[test]
    fn the_two_arms_disagree_on_a_saddle_and_that_is_the_point() {
        let l = from_rows(&["#.", ".#"]);
        let [four, eight] = arms();
        assert_eq!(signature(&l, four).components, 2, "4-connected: two pixels");
        assert_eq!(signature(&l, eight).components, 1, "8-connected: one blob");
        assert_eq!(critical_cells(&l).len(), 1);
    }

    /// Both explicit resolutions remove the critical cell they are applied
    /// to, in one batch, and they disagree about the answer — which is what
    /// makes them two hypotheses rather than a tie-break.
    #[test]
    fn both_saddle_resolutions_are_batch_and_disagree() {
        let v = vec![0.9, 0.1, 0.1, 0.9];
        let t = threshold(&v, 2, 2, 0.5, SaddleResolution::Thresholded);
        let j = threshold(&v, 2, 2, 0.5, SaddleResolution::JoinDiagonal);
        let s = threshold(&v, 2, 2, 0.5, SaddleResolution::SplitDiagonal);
        assert_eq!(residual_critical_cells(&t), 1);
        assert_eq!(residual_critical_cells(&j), 0);
        assert_eq!(residual_critical_cells(&s), 0);
        assert_eq!(j.count_inside(), 4);
        assert_eq!(s.count_inside(), 0);
        for arm in arms() {
            // A resolved saddle reads the same under both conventions,
            // which is what "well composed" buys.
            assert_eq!(signature(&j, arm).components, 1);
            assert_eq!(signature(&s, arm).components, 0);
        }
    }

    /// The batch is a batch: two critical cells sharing a pixel are both
    /// rewritten from the ORIGINAL labelling, so neither can see the other's
    /// rewrite and the result cannot depend on which came first.
    #[test]
    fn overlapping_critical_cells_are_resolved_from_the_original_labelling() {
        // Two saddles sharing the middle column.
        let l = from_rows(&["#.#", ".#.", "#.#"]);
        assert_eq!(critical_cells(&l).len(), 4);
        let v: Vec<f64> = l
            .inside()
            .iter()
            .map(|b| if *b { 1.0 } else { 0.0 })
            .collect();
        let j = threshold(&v, 3, 3, 0.5, SaddleResolution::JoinDiagonal);
        let s = threshold(&v, 3, 3, 0.5, SaddleResolution::SplitDiagonal);
        assert_eq!(j.count_inside(), 9, "join fills every cell it touches");
        assert_eq!(s.count_inside(), 0, "split empties every cell it touches");
        // And the reverse-order construction agrees, which is the property
        // an iteration-order tie-break would not have.
        let mut reversed = l.inside().to_vec();
        for i in critical_cells(&l).into_iter().rev() {
            for k in [i, i + 1, i + 3, i + 4] {
                reversed[k] = true;
            }
        }
        assert_eq!(reversed, j.inside());
    }

    /// The digest separates what it must and identifies what it must.
    #[test]
    fn the_digest_is_a_function_of_the_labelling_and_the_convention_only() {
        let a = from_rows(&["##.", "..."]);
        let b = from_rows(&["##.", "..."]);
        let c = from_rows(&[".##", "..."]);
        let [four, eight] = arms();
        assert_eq!(digest(&a, four), digest(&b, four));
        assert_ne!(digest(&a, four), digest(&c, four));
        assert_ne!(
            digest(&a, four),
            digest(&a, eight),
            "the convention is part of the signature: the same pixels under a different \
             convention are a different topological reading"
        );
        assert!(digest(&a, four).chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// An empty and a full labelling are the two degenerate readings, and
    /// they are counted rather than crashing: the envelope removes them as
    /// INVALID, which is a decision with a name.
    #[test]
    fn the_degenerate_labellings_have_signatures() {
        let empty = from_rows(&["..", ".."]);
        let full = from_rows(&["##", "##"]);
        for arm in arms() {
            assert_eq!(signature(&empty, arm).components, 0);
            assert_eq!(signature(&empty, arm).holes, 0);
            assert_eq!(signature(&full, arm).components, 1);
            assert_eq!(signature(&full, arm).holes, 0);
        }
    }
}
