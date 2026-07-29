//! The edit SHAPES the DCEL harness applies, and the one function that puts
//! them through `apply` (spec v1.3 §11.4, §28 M5).
//!
//! Split out of `dcel/mod.rs` in M6 when that file reached §4.1's 800-line cap.
//!
//! ## Why there is more than one shape
//!
//! M5 had exactly one — a filled square at the canvas centre — and STATUS_M5
//! limitation 34 named the consequence: "one transaction shape, chosen from the
//! canvas". A filled square can only ever move the signature by `(0,0)`,
//! `(-1,0)` or `(+1,0)`, so the COMPOUND subclass §28 M5 names was empty by
//! construction and the harness reported its absence as an exclusion
//! (F-0081). Each shape below exists because a measurement said the previous
//! set could not reach a class:
//!
//! | shape | reaches | why it was added |
//! |---|---|---|
//! | filled square | `identity`, `gap_open`, `bridge_close` | M5's original |
//! | annulus | `hole_open` and four compound deltas | `transactions_compound` read 0 (F-0081) |
//! | hole fill | `hole_fill` | no declaration in 960 had a negative hole component, on a population where 72 arms carry a hole |
//!
//! ## Canvas-derived versus arrangement-derived, and why the third is allowed
//!
//! The first two are derived from the CANVAS: the same edit for every arm, so
//! the population cannot be selected by what happens to work. The third cannot
//! be — a hole is wherever the scene put it, and no fixed rectangle finds one.
//! It is still not a SEARCH over edits, which is what limitation 34 warns
//! against: it takes the lexicographically first hole, it is defined for every
//! arm that has one, its population is published, and its declared delta is
//! read from the independent chain and checked by `apply` exactly like the
//! others. What would be forbidden is trying several edits and keeping the one
//! that commits; nothing here does that.

use vice_ir::PixelConnectivity;
use vice_topology::continuation::EditKind;
use vice_topology::dcel::{apply, Edit, Roi};
use vice_topology::{Dcel, Outcome, TX_CONFIG_V1};

use super::knockouts::RoiKnockout;
use super::ArmTransaction;
use crate::gt::raster::ViewTransform;

/// One transaction per arm: fill a small square at the centre of the canvas.
///
/// The edit is derived from the CANVAS rather than from the arrangement, so it
/// is the same edit for every arm and the population is not selected by what
/// happens to work.
///
/// ## What M6 changed, and why it had to (limitations 37 and 44)
///
/// Until M6 the declared kind came out of a four-arm `match` with
/// `_ => return None`, so **310 of 480 arms never reached `apply` at all** —
/// and the dropped subclass was exactly the one §28 M5 names, "local COMPOUND
/// topology transactions". Both reviewers recorded it with no second deferral
/// available. Two things changed here:
///
/// 1. **No arm is dropped for its signature delta.** `EditKind` is a point of
///    Z^2 now, so every arm's edit is declarable and every arm is attempted.
///    The only remaining exclusion is the size guard below, and it is counted.
///
/// 2. **The declaration and the check no longer share a provenance.** The old
///    code built the edited arrangement with `Dcel::assemble` and read the
///    delta off it — then `apply` rebuilt with `Dcel::assemble` and compared.
///    That is F-0048 Q4: the guard shared its origin with the mechanism, so a
///    defect inside `assemble` moved both sides together and the comparison
///    could not see it. The declaration now comes from
///    [`crate::topology::independent::signature_of`] — breadth-first flood fill
///    plus an Euler count from 2x2 bit-quads, which shares no code with the
///    DCEL — so `apply`'s agreement is a claim about two independent readings
///    of the same labelling rather than about a copy.
pub(super) fn transaction_for(
    base: &Dcel,
    t: &ViewTransform,
    roi_k: RoiKnockout,
) -> Option<ArmTransaction> {
    let (w, h) = (t.width_px, t.height_px);
    if w < 16 || h < 16 {
        return None;
    }
    let s = (w / 8).max(2);
    let roi = Roi {
        x0: w / 2 - s / 2,
        y0: h / 2 - s / 2,
        x1: w / 2 + s / 2,
        y1: h / 2 + s / 2,
    };
    let set: Vec<(u32, u32, bool)> = (roi.x0..roi.x1)
        .flat_map(|x| (roi.y0..roi.y1).map(move |y| (x, y, true)))
        .collect();
    attempt(base, w, h, roi, set, roi_k)
}

/// The SECOND transaction shape: a square annulus at the canvas centre.
///
/// ## Why a second shape exists at all
///
/// Removing the `_ => return None` filter let all 480 arms reach `apply`, and
/// the first thing the published `transactions_compound` count said was
/// **zero**. The 310 arms the M5 harness reported as
/// `transaction_arms_excluded_as_compound` are not compound: 282 of them change
/// no pixel (the centred square is already inside the foreground) and the other
/// 28 move no signature count, so all 310 declare the IDENTITY delta `(0, 0)`.
/// The label was arithmetically defensible — `(0,0)` is not a single `±1` — and
/// materially wrong, because everyone who read it, this author included, took
/// it to name the subclass §28 M5 calls "local COMPOUND topology transactions".
///
/// The cause is STATUS_M5 limitation 34: ONE transaction shape, and a filled
/// square can only ever produce `(0,0)`, `(-1,0)` or `(+1,0)`. A compound
/// population cannot exist under one shape, so widening `EditKind` was
/// necessary and not sufficient.
///
/// An annulus is compound BY CONSTRUCTION on a background region: the wall
/// becomes one new component and the void it encloses becomes one new hole, so
/// the delta is `(+1, +1)` — two unit steps in one transaction. Like the filled
/// square it is derived from the CANVAS and not from the arrangement, so the
/// population is still not selected by what happens to work: on an arm whose
/// centre is already foreground the annulus does something else, and whatever
/// it does is declared from the independent chain and checked by `apply`.
pub(super) fn ring_transaction_for(
    base: &Dcel,
    t: &ViewTransform,
    roi_k: RoiKnockout,
) -> Option<ArmTransaction> {
    let (w, h) = (t.width_px, t.height_px);
    if w < 16 || h < 16 {
        return None;
    }
    // Outer side at least 5 so that a one-pixel wall leaves a non-empty void.
    let s = (w / 6).max(5);
    let roi = Roi {
        x0: w / 2 - s / 2,
        y0: h / 2 - s / 2,
        x1: w / 2 + s / 2,
        y1: h / 2 + s / 2,
    };
    let set: Vec<(u32, u32, bool)> = (roi.x0..roi.x1)
        .flat_map(|x| (roi.y0..roi.y1).map(move |y| (x, y)))
        .map(|(x, y)| {
            let on_wall = x == roi.x0 || x + 1 == roi.x1 || y == roi.y0 || y + 1 == roi.y1;
            (x, y, on_wall)
        })
        .collect();
    attempt(base, w, h, roi, set, roi_k)
}

/// Declare an edit from the INDEPENDENT chain and put it through `apply`.
///
/// Shared by both shapes so that the declaration's provenance is stated once:
/// the delta comes from breadth-first flood fill plus a bit-quad Euler count,
/// and `apply` recomputes it through the DCEL. The two share no code, which is
/// what makes their agreement a cross-check rather than a comparison of a copy
/// against itself (F-0048 Q4; the old code read both sides off `Dcel::assemble`).
fn attempt(
    base: &Dcel,
    w: u32,
    h: u32,
    roi: Roi,
    mut set: Vec<(u32, u32, bool)>,
    roi_k: RoiKnockout,
) -> Option<ArmTransaction> {
    if roi_k == RoiKnockout::Reach {
        set.push((0, 0, true));
    }

    // The declaration, from the INDEPENDENT chain. `apply` will recompute the
    // same delta through the DCEL and refuse the transaction if the two
    // disagree, which is now a real cross-check rather than a rebuild compared
    // against itself.
    let before = base.labelling().inside().to_vec();
    let mut after = before.clone();
    for (x, y, v) in &set {
        if *x < w && *y < h {
            after[*y as usize * w as usize + *x as usize] = *v;
        }
    }
    let conn = base.connectivity();
    let (wz, hz) = (w as usize, h as usize);
    let sig_before = crate::topology::independent::signature_of(&before, wz, hz, conn);
    let sig_after = crate::topology::independent::signature_of(&after, wz, hz, conn);
    let kind = EditKind::between(
        (sig_before.components, sig_before.holes),
        (sig_after.components, sig_after.holes),
    );

    let edit = Edit { kind, roi, set };
    let out = apply(base, &edit, &TX_CONFIG_V1);
    let rep = out.report();
    Some(ArmTransaction {
        declared: rep.declared.clone(),
        declared_steps: rep.declared_steps,
        committed: rep.committed,
        refusal: match &out {
            Outcome::RolledBack { reason, .. } => Some(reason.to_string()),
            Outcome::Committed { .. } => None,
        },
        refusal_kind: match &out {
            Outcome::RolledBack { reason, .. } => Some(reason.name()),
            Outcome::Committed { .. } => None,
        },
        roi_area_px: rep.roi.area_px(),
        pixels_changed: rep.pixels_changed,
        unrelated_chains: rep.unrelated_chains,
        unrelated_chains_that_moved: rep.unrelated_chains_that_moved,
        components_before: rep.base.map(|b| b.foreground_faces),
        components_after: rep.candidate.map(|c| c.foreground_faces),
        holes_before: rep.base.map(|b| b.holes),
        holes_after: rep.candidate.map(|c| c.holes),
    })
}

/// The THIRD edit shape: fill the lexicographically first hole.
///
/// ## The measurement that made it necessary
///
/// After the annulus was added, three of the four named unit steps occurred and
/// `hole_fill` still never did. The governor's question was the right one — is
/// it unreachable BY CONSTRUCTION, or does the population simply not carry it?
/// Measured on `docs/gt/DCEL_M5.json`, the answer was neither:
///
/// ```text
/// arms whose base arrangement carries a hole ....... 72 of 480
/// declarations with a negative hole component ...... 0 of 960
/// ```
///
/// The population carries holes in quantity. What could not reach them was the
/// SHAPE: both existing shapes sit at the canvas centre, and a hole is wherever
/// the scene put it. So the deficiency was in the shape family, not in the
/// corpus and not in `apply` — and "unreachable by construction" would have
/// been false had it been assumed.
///
/// A hole is a background component that does not touch the border. Filling one
/// moves the signature by exactly `(0, -1)`, which is `EditKind::HOLE_FILL`.
/// The background connectivity is the COMPLEMENT of the foreground's, which is
/// the whole point of `ComplementaryConnectivity` — using the foreground rule
/// for both would count a diagonal background leak as sealed and fill something
/// that is not a hole.
pub(super) fn hole_fill_transaction_for(
    base: &Dcel,
    t: &ViewTransform,
    roi_k: RoiKnockout,
) -> Option<ArmTransaction> {
    let (w, h) = (t.width_px, t.height_px);
    if w < 16 || h < 16 {
        return None;
    }
    let (wz, hz) = (w as usize, h as usize);
    let inside = base.labelling().inside();
    let hole = first_hole(inside, wz, hz, base.connectivity().background())?;

    // The ROI is the hole's bounding box. Derived from the thing being edited,
    // like every other ROI here, and `apply` still refuses any pixel outside it.
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for &i in &hole {
        let (x, y) = ((i % wz) as u32, (i / wz) as u32);
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + 1);
        y1 = y1.max(y + 1);
    }
    let roi = Roi { x0, y0, x1, y1 };
    let set: Vec<(u32, u32, bool)> = hole
        .iter()
        .map(|&i| ((i % wz) as u32, (i / wz) as u32, true))
        .collect();
    attempt(base, w, h, roi, set, roi_k)
}

/// The pixels of the lexicographically first hole, or `None` if there is none.
///
/// A hole is a background component with no pixel on the canvas border. The
/// scan order fixes WHICH hole without consulting the outcome, so the choice is
/// canonical rather than searched: the same labelling always yields the same
/// hole, and an arm with two holes does not get to pick the one that commits.
fn first_hole(inside: &[bool], w: usize, h: usize, bg: PixelConnectivity) -> Option<Vec<usize>> {
    let offsets: &[(isize, isize)] = match bg {
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
    for start in 0..inside.len() {
        if inside[start] || seen[start] {
            continue;
        }
        // Flood the background component containing `start`, recording whether
        // it reaches the border. Breadth-first with an explicit queue.
        let mut queue = std::collections::VecDeque::new();
        let mut cells = Vec::new();
        let mut touches_border = false;
        seen[start] = true;
        queue.push_back(start);
        while let Some(i) = queue.pop_front() {
            cells.push(i);
            let (x, y) = ((i % w) as isize, (i / w) as isize);
            if x == 0 || y == 0 || x as usize + 1 == w || y as usize + 1 == h {
                touches_border = true;
            }
            for (dx, dy) in offsets {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                if !inside[j] && !seen[j] {
                    seen[j] = true;
                    queue.push_back(j);
                }
            }
        }
        if !touches_border {
            return Some(cells);
        }
    }
    None
}
