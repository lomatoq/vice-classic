//! The lattice the arrangement lives on, and the ONE rule that decides where
//! a boundary goes next (spec v1.3 §12, §5.3, §5.4).
//!
//! Everything here is integer. That is not thrift, it is the §5.4 answer: a
//! cubical arrangement's combinatorics is decided by comparisons of `u32`
//! lattice coordinates and of `bool` labels, so there is no predicate to make
//! robust and no tolerance to type. §5.4 requires robust predicates *for*
//! topology decisions; the cheapest robust predicate is the one that never
//! needs an epsilon, and choosing the representation is how M5 gets it. The
//! f64 curve geometry of §12's "curve replacement" arrives with M6's fitter,
//! and the tubular-neighbourhood test that governs it is named as not-here in
//! [`super::certificate`] rather than half-built.
//!
//! ## Why the successor is a FUNCTION and not a field
//!
//! §12 lists "face cycles closed and oriented" among the invariants. A DCEL
//! that stores `next` has a field that can hold a value making that false, and
//! then "closed" is a property of the data that something has to check. Here
//! [`Arrangement::succ`] computes the successor from the 2x2 pixel
//! neighbourhood of the head vertex. Two consequences, and they are the whole
//! reason for the choice:
//!
//! - there is no field to corrupt, so the invariant has no failure mode to
//!   check for. `next` is total by construction — every case of the match
//!   lands on a segment whose two flanking labels differ, which is exactly the
//!   condition for that segment to exist;
//! - `succ` is a PERMUTATION of the directed segments (proved by
//!   [`Arrangement::successor_is_a_permutation`], which builds the inverse and
//!   compares), and the orbits of a permutation of a finite set are cycles.
//!   "Closed" is therefore a theorem about the construction rather than a
//!   sentence in a report.
//!
//! ## The critical 2x2, and where the convention actually enters
//!
//! §5.3 forbids resolving a critical 2x2 by "a hidden iteration-order
//! tie-break", and §12 asks for "deterministic ambiguous-saddle branches". At
//! a critical 2x2 the four incident segment ends of the centre lattice point
//! can be paired two ways: every arriving step turns left, or every arriving
//! step turns right. This file takes the FIRST, unconditionally, and the
//! reason is worth stating precisely because the obvious alternative — pair
//! them by which label is 8-connected — was written first, and the exhaustive
//! sweep in [`super::audit`] rejected it.
//!
//! **The pairing is not where the convention lives.** The set of boundary
//! segments is a function of the labels alone: a segment exists exactly where
//! two adjacent pixels disagree. The convention does not add or remove one.
//! What the convention decides is which PIXELS are one region, and that is a
//! statement about faces, not about the curve system. So:
//!
//! - the curve system, its vertices and its walks are the same under both
//!   arms — one arrangement of unit segments, with the critical point an
//!   honest degree-four junction in both readings;
//! - the FACES differ: under 4-connectivity the two diagonal pixels of that
//!   label are two faces, under 8-connectivity they are one. That is computed
//!   by the face flood fill in the parent module, from the same complementary
//!   convention, and it is what makes the two arms two different `Dcel`s.
//!
//! The measurement that settled it: with the pairing made convention-dependent,
//! the Euler identity `V - B + L = 2C` failed on 4x3 labelling 18 under
//! foreground-4 — a background that should walk both squares in one loop
//! walked them in two. Under the unconditional rule the identity holds over
//! every labelling of a 4x3 and a 4x4 grid under both arms. A rule chosen
//! because it sounded like the textbook and a rule that survives the whole
//! input space are different things, and only the second is in the tree.
//!
//! The two arms still differ at a saddle — that is
//! `the_two_conventions_disagree_about_a_critical_2x2` below — and they differ
//! in the place §5.3 actually names: how many regions there are.

use vice_ir::{ComplementaryConnectivity, PixelConnectivity};

/// A corner of the pixel grid. `(0,0)` is the top-left corner of pixel
/// `(0,0)`, so a `W x H` canvas has `(W+1) x (H+1)` of them (§5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Lat {
    pub x: u32,
    pub y: u32,
}

/// The four axis directions, in the fixed order used by every scan here.
///
/// The order is part of the artifact: it decides which half-edge of a loop is
/// its canonical representative, and §5.5 makes that a Tier A concern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dir {
    E,
    S,
    W,
    N,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::E, Dir::S, Dir::W, Dir::N];

    pub fn index(self) -> usize {
        match self {
            Dir::E => 0,
            Dir::S => 1,
            Dir::W => 2,
            Dir::N => 3,
        }
    }

    pub fn from_index(i: usize) -> Dir {
        Dir::ALL[i % 4]
    }

    /// Screen-space delta. y is DOWN (§5.1), which is why "left" below looks
    /// inverted against the algebraic convention; docs/COORDINATE_CONVENTION.md
    /// §1 carries the same caveat for the same reason.
    pub fn delta(self) -> (i64, i64) {
        match self {
            Dir::E => (1, 0),
            Dir::S => (0, 1),
            Dir::W => (-1, 0),
            Dir::N => (0, -1),
        }
    }

    pub fn turn_left(self) -> Dir {
        Dir::from_index((self.index() + 3) % 4)
    }

    pub fn turn_right(self) -> Dir {
        Dir::from_index((self.index() + 1) % 4)
    }

    pub fn reverse(self) -> Dir {
        Dir::from_index((self.index() + 2) % 4)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Dir::E => "E",
            Dir::S => "S",
            Dir::W => "W",
            Dir::N => "N",
        }
    }
}

/// One directed unit step of the boundary: leave `from` heading `dir`.
///
/// This IS the half-edge at the lattice level. Its twin is
/// `Step { from: head, dir: dir.reverse() }` — computed, never stored, so
/// §12's "every half-edge has a twin" has no representation in which it is
/// false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Step {
    pub from: Lat,
    pub dir: Dir,
}

impl Step {
    pub fn head(self) -> Lat {
        let (dx, dy) = self.dir.delta();
        Lat {
            x: (i64::from(self.from.x) + dx) as u32,
            y: (i64::from(self.from.y) + dy) as u32,
        }
    }

    /// The twin. A total function of the step, with no stored counterpart.
    pub fn twin(self) -> Step {
        Step {
            from: self.head(),
            dir: self.dir.reverse(),
        }
    }
}

/// A pixel address that may lie one ring outside the canvas.
///
/// The ring is always background, which is what makes the exterior a real
/// region rather than a special case: §5.3 requires the exterior to be a
/// genuine `FaceId` and not "a negative magic label", and the cheapest way to
/// keep that promise is for the exterior to be produced by the same walk that
/// produces every other face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Px {
    pub x: i64,
    pub y: i64,
}

/// The labelled grid, with the outside ring already part of it.
pub struct Arrangement<'a> {
    inside: &'a [bool],
    pub width_px: u32,
    pub height_px: u32,
    pub conn: ComplementaryConnectivity,
}

impl<'a> Arrangement<'a> {
    pub fn new(
        inside: &'a [bool],
        width_px: u32,
        height_px: u32,
        conn: ComplementaryConnectivity,
    ) -> Arrangement<'a> {
        assert_eq!(
            inside.len(),
            width_px as usize * height_px as usize,
            "labelling length must match its declared shape"
        );
        Arrangement {
            inside,
            width_px,
            height_px,
            conn,
        }
    }

    /// The label of a pixel, with everything outside the canvas background.
    pub fn label(&self, p: Px) -> bool {
        if p.x < 0 || p.y < 0 || p.x >= i64::from(self.width_px) || p.y >= i64::from(self.height_px)
        {
            return false;
        }
        self.inside[p.y as usize * self.width_px as usize + p.x as usize]
    }

    /// Connectivity of the region a label names (§5.3 complementary pair).
    pub fn conn_of(&self, label: bool) -> PixelConnectivity {
        if label {
            self.conn.foreground()
        } else {
            self.conn.background()
        }
    }

    /// The pixel on the left of a directed step, and the one on its right.
    ///
    /// Left of direction `(dx, dy)` under a y-DOWN frame is `(dy, -dx)`.
    pub fn flanks(&self, s: Step) -> (Px, Px) {
        let (x, y) = (i64::from(s.from.x), i64::from(s.from.y));
        match s.dir {
            // (x,y)->(x+1,y): above is pixel (x, y-1), below is (x, y).
            Dir::E => (Px { x, y: y - 1 }, Px { x, y }),
            Dir::W => (Px { x: x - 1, y }, Px { x: x - 1, y: y - 1 }),
            // (x,y)->(x,y+1): the pixel to the east is (x, y).
            Dir::S => (Px { x, y }, Px { x: x - 1, y }),
            Dir::N => (Px { x: x - 1, y: y - 1 }, Px { x, y: y - 1 }),
        }
    }

    /// Does a boundary segment exist along this step? It does exactly when the
    /// two pixels it separates carry different labels.
    pub fn exists(&self, s: Step) -> bool {
        if !self.in_lattice(s.from) || !self.in_lattice(s.head()) {
            return false;
        }
        let (l, r) = self.flanks(s);
        self.label(l) != self.label(r)
    }

    pub fn in_lattice(&self, p: Lat) -> bool {
        p.x <= self.width_px && p.y <= self.height_px
    }

    /// Number of boundary segments incident to a lattice point.
    ///
    /// Zero, two, three or four. Never one: a boundary cannot stop, because a
    /// segment exists iff two labels differ and the four pixels around a
    /// lattice point contribute an even number of such disagreements around
    /// the cycle. Degree three or four makes the point a JUNCTION and hence a
    /// vertex of the DCEL; degree four is exactly the critical 2x2 of §5.3.
    pub fn degree(&self, v: Lat) -> u32 {
        Dir::ALL
            .iter()
            .filter(|d| self.exists(Step { from: v, dir: **d }))
            .count() as u32
    }

    /// **The rule.** Where the boundary continues, keeping the same face on
    /// the left.
    ///
    /// `s` must exist. The result always exists too, and the doc comment on
    /// each arm says why, because "the successor is total" is the property
    /// that makes the orbits cycles.
    pub fn succ(&self, s: Step) -> Step {
        debug_assert!(self.exists(s), "succ of a step that is not a boundary");
        let (left, _right) = self.flanks(s);
        let inside = self.label(left);
        let b = s.head();

        let straight = Step {
            from: b,
            dir: s.dir,
        };
        let (ahead_left, ahead_right) = self.flanks(straight);
        let al = self.label(ahead_left) == inside;
        let ar = self.label(ahead_right) == inside;

        if !al {
            // The region on the left has ended ahead. Turning left keeps it on
            // the left: the new step's left pixel is the CURRENT left pixel,
            // and its right pixel is `ahead_left`, which differs. Exists.
            //
            // `ar` true here is the critical 2x2 of §5.3: the region touches
            // itself across the corner, and the four ends of this lattice
            // point admit two pairings. This takes the left one in both
            // readings — see the module docs for the measurement that chose
            // it, and for where the convention does enter.
            Step {
                from: b,
                dir: s.dir.turn_left(),
            }
        } else if !ar {
            // The corridor continues: same face left, other face right.
            straight
        } else {
            // Both pixels ahead belong to the face on our left, so the
            // boundary wraps around the pixel on our right. Turning right
            // puts `ahead_right` on the left and our current right pixel on
            // the right; those differ, so it exists.
            Step {
                from: b,
                dir: s.dir.turn_right(),
            }
        }
    }

    /// Every directed boundary step of this labelling, in a canonical order.
    pub fn steps(&self) -> Vec<Step> {
        let mut out = Vec::new();
        for y in 0..=self.height_px {
            for x in 0..=self.width_px {
                for d in Dir::ALL {
                    let s = Step {
                        from: Lat { x, y },
                        dir: d,
                    };
                    if self.exists(s) {
                        out.push(s);
                    }
                }
            }
        }
        out
    }

    /// The property the orbit decomposition rests on, as a computation rather
    /// than as a claim: `succ` hits every step exactly once.
    ///
    /// A caller can run this on any labelling; [`super::audit`] runs it on
    /// every arrangement it audits, and the exhaustive tests run it over every
    /// labelling of a small grid. It is checked in BOTH directions — a
    /// permutation with a fixed point or a collision fails, and so does an
    /// EMPTY step set, because an empty relation is vacuously injective and
    /// vacuous success is indistinguishable from success from the outside
    /// (F-0039).
    pub fn successor_is_a_permutation(&self) -> Result<usize, String> {
        let steps = self.steps();
        if steps.is_empty() {
            return Err(
                "no boundary steps: a labelling with no interface has no arrangement to \
                        check, and reporting that as a pass would be vacuous"
                    .to_string(),
            );
        }
        let set: std::collections::BTreeSet<Step> = steps.iter().copied().collect();
        let mut seen: std::collections::BTreeMap<Step, Step> = std::collections::BTreeMap::new();
        for s in &steps {
            let t = self.succ(*s);
            if !set.contains(&t) {
                return Err(format!(
                    "succ({:?} {}) left the boundary at {:?} {}",
                    s.from,
                    s.dir.as_str(),
                    t.from,
                    t.dir.as_str()
                ));
            }
            if let Some(prev) = seen.insert(t, *s) {
                return Err(format!(
                    "succ is not injective: {:?} {} and {:?} {} both go to {:?} {}",
                    prev.from,
                    prev.dir.as_str(),
                    s.from,
                    s.dir.as_str(),
                    t.from,
                    t.dir.as_str()
                ));
            }
        }
        Ok(steps.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arr<'a>(inside: &'a [bool], w: u32, h: u32, fg_four: bool) -> Arrangement<'a> {
        let conn = ComplementaryConnectivity::new(if fg_four {
            PixelConnectivity::Four
        } else {
            PixelConnectivity::Eight
        });
        Arrangement::new(inside, w, h, conn)
    }

    /// The twin is a function, and applying it twice is the identity. There is
    /// no field here that could say otherwise.
    #[test]
    fn the_twin_of_the_twin_is_the_step() {
        for x in 0..4u32 {
            for y in 0..4u32 {
                for d in Dir::ALL {
                    let s = Step {
                        from: Lat { x, y },
                        dir: d,
                    };
                    assert_eq!(s.twin().twin(), s);
                    assert_ne!(s.twin(), s);
                }
            }
        }
    }

    /// One foreground pixel in the middle: four segments, one loop of four,
    /// and `succ` is a permutation.
    #[test]
    fn a_single_pixel_has_a_four_step_loop() {
        let mut inside = vec![false; 9];
        inside[4] = true;
        let a = arr(&inside, 3, 3, true);
        assert_eq!(a.steps().len(), 8, "four segments, two directions each");
        assert_eq!(a.successor_is_a_permutation(), Ok(8));
    }

    /// The critical 2x2. The curve system is the SAME under both arms — the
    /// segments are a function of the labels and the pairing is
    /// unconditional — and that is asserted here rather than assumed, because
    /// the first version of this file claimed the opposite and the exhaustive
    /// sweep disagreed.
    ///
    /// Where the two arms differ is the face count, and that belongs to the
    /// parent module: `the_two_conventions_disagree_about_a_critical_2x2`.
    #[test]
    fn a_critical_vertex_has_degree_four_and_one_pairing_under_both_arms() {
        // 2x2: foreground on the main diagonal.
        let inside = vec![true, false, false, true];
        let centre = Lat { x: 1, y: 1 };

        let four = arr(&inside, 2, 2, true);
        let eight = arr(&inside, 2, 2, false);
        assert_eq!(four.degree(centre), 4, "the centre is a critical 2x2");
        assert_eq!(eight.degree(centre), 4);

        // Arrive at the centre heading east along the top-left pixel's lower
        // edge: left pixel is (0,0) = foreground.
        let s = Step {
            from: Lat { x: 0, y: 1 },
            dir: Dir::E,
        };
        assert!(four.exists(s));
        let (l, _) = four.flanks(s);
        assert!(four.label(l), "the region on the left is the foreground");

        assert_eq!(four.succ(s).dir, Dir::N);
        assert_eq!(eight.succ(s).dir, Dir::N, "the pairing is unconditional");
        assert_eq!(four.steps(), eight.steps(), "same segments, both arms");

        assert!(four.successor_is_a_permutation().is_ok());
        assert!(eight.successor_is_a_permutation().is_ok());
    }

    /// The permutation property, over EVERY labelling of a 4x3 grid and both
    /// convention arms: 4096 labellings, 8192 arrangements.
    ///
    /// This is the shape the whole milestone uses in place of a fixture list.
    /// F-0054/F-9 is that a judge proved on hand-written witnesses is proved
    /// on those witnesses; the answer here is that the witness set IS the
    /// input space, so there is no subclass left for a defect to hide in.
    #[test]
    fn the_successor_is_a_permutation_on_every_labelling_of_a_small_grid() {
        let (w, h) = (4u32, 3u32);
        let n = (w * h) as usize;
        let mut checked = 0usize;
        let mut degenerate = 0usize;
        for bits in 0u32..(1 << n) {
            let inside: Vec<bool> = (0..n).map(|i| bits & (1 << i) != 0).collect();
            for fg_four in [true, false] {
                let a = arr(&inside, w, h, fg_four);
                match a.successor_is_a_permutation() {
                    Ok(_) => checked += 1,
                    // The all-background labelling has no interface at all.
                    Err(e) if inside.iter().all(|b| !*b) => {
                        assert!(e.contains("no boundary steps"));
                        degenerate += 1;
                    }
                    Err(e) => panic!("bits={bits} fg_four={fg_four}: {e}"),
                }
            }
        }
        assert_eq!(degenerate, 2, "exactly the empty labelling, once per arm");
        assert_eq!(checked, 2 * ((1 << n) - 1));
    }
}
