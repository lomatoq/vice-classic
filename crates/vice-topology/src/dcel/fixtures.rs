//! The structural register: five fixtures at any size (REVIEW_M4_5 condition
//! 51).
//!
//! ## Why this is production code and not a test helper
//!
//! Two consumers need the same five shapes: the crate's own property tests and
//! the §28 M5 gate harness in `vice-bench`. F-0045 is the standing lesson about
//! what happens when a property crosses a boundary as a COPY instead of as a
//! CALL — a frozen noise coefficient was diluted by 29 % because a predicate
//! became a list of three strings on the far side of a new crate boundary. So
//! the generator lives here, once, and both sides call it.
//!
//! ## What condition 51 asked for and what is here
//!
//! REVIEW_M4_5 §11: *"either five structural fixtures per size (annulus, nested
//! annulus, bridge, two components, triple junction), or a tenth frozen item
//! with a price. Acceptance: the class `(1,1)` appears in the differential run
//! by construction, not by chance."*
//!
//! Four of the five are here under their own names. The fifth is not, and the
//! reason is arithmetic rather than an implementation limit: **a binary
//! labelling has no triple junction.** The degree of a lattice point is the
//! number of disagreeing pairs around the 2x2 surrounding it, which is a cycle
//! of four, so the degree is even — 0, 2 or 4, never 3. Three regions meeting
//! at a point needs three labels, and the multicolor RAG arrives with M8 (§11.5,
//! §28 M8).
//!
//! In its place is [`Fixture`] `diagonal_pinch`: the degree-four critical 2x2
//! of §5.3, which is the only junction a binary labelling has. It is the more
//! useful of the two for this milestone, because it is the only fixture whose
//! class DIFFERS between the two complementary conventions — and that makes it
//! the population STATUS_M4_5 limitation 18 says the corpus does not have
//! ("zero of 132 arms where the conventions would diverge"). Without it, the
//! §28 M5 clause "no final-topology claim from proxy" would be a statement
//! about sets of size one.

use crate::cubical::Labelling;

/// The sizes the structural register is built at on the default test path:
/// **the single declaration**, and the reason it lives here rather than in the
/// test that reads it.
///
/// This closes STATUS_M5 limitation 53 (REVIEW_M5_A A7.2, REVIEW_M5_B N-sizes,
/// found independently). The list existed twice: as `FAST_SIZES_PX` in this
/// crate's `dcel_props`, and as the distinct `size_px` of the frozen
/// degradation cells the `vice-bench` harness runs (`dcel::structural_sizes`).
/// Two declarations of one list is the form F-0078 names: **a derivation is
/// only as derived as its least-derived input.** Grow the harness's cell list
/// and the crate's tests keep checking the stale three, silently narrowing
/// their own domain.
///
/// It is a `const` and not a derivation because the dependency runs one way:
/// `vice-topology` does not depend on `vice-bench` and must not. So the
/// arrangement is the one the frozen gate constants already use — one
/// declaration here, and a mechanism on the consuming side that REDDENS when
/// the harness disagrees (`the_harness_structural_sizes_are_the_registers`,
/// verified by knockout: shortened to `[32, 64]` it fails with `left: [32, 64,
/// 128], right: [32, 64]`). The literal is not removed; it is reduced to one,
/// with a judge over the gap that used to be silent.
///
/// The two large sizes are deliberately absent: 256 and 512 run under
/// `--ignored` in release, and `dcel_props::SIZES_PX` is the axis that carries
/// them. This const is the FAST axis, which is the one the harness's cells
/// match.
pub const STRUCTURAL_SIZES_PX: [usize; 3] = [32, 64, 128];

/// One structural fixture: its name, its labelling, and the class it is BUILT
/// to have under each convention arm.
///
/// The class is carried per arm rather than once, because the fifth fixture
/// exists precisely to have two different answers. A fixture whose class drifts
/// fails the test that reads this, so the field is an assertion about the
/// generator rather than a summary of whatever came out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixture {
    pub name: &'static str,
    pub labelling: Labelling,
    /// `(components, holes)` under foreground-4.
    pub class_fg4: (u32, u32),
    /// `(components, holes)` under foreground-8.
    pub class_fg8: (u32, u32),
}

/// The five structural fixtures at one size.
pub fn structural_fixtures(n: usize) -> Vec<Fixture> {
    let lab = |f: &dyn Fn(usize, usize) -> bool| {
        Labelling::new(n, n, (0..n * n).map(|i| f(i % n, i / n)).collect())
    };
    let c = n as f64 / 2.0;
    let r =
        |x: usize, y: usize| ((x as f64 + 0.5 - c).powi(2) + (y as f64 + 0.5 - c).powi(2)).sqrt();
    let unit = (n as f64 / 16.0).max(1.0);
    let unit_px = unit as usize;
    let _ = unit_px;
    let half = 2.0 * unit;
    let (left, right) = (c - 4.0 * unit, c + 4.0 * unit);
    let block = |x: usize, y: usize, cx: f64, cy: f64, hw: f64| {
        ((x as f64 + 0.5) - cx).abs() <= hw && ((y as f64 + 0.5) - cy).abs() <= hw
    };
    vec![
        Fixture {
            name: "annulus",
            labelling: lab(&|x, y| {
                let d = r(x, y);
                d >= 2.0 * unit && d <= 6.0 * unit
            }),
            class_fg4: (1, 1),
            class_fg8: (1, 1),
        },
        Fixture {
            name: "nested_annulus",
            labelling: lab(&|x, y| {
                let d = r(x, y);
                (d >= 4.0 * unit && d <= 6.0 * unit) || d <= 1.5 * unit
            }),
            class_fg4: (2, 1),
            class_fg8: (2, 1),
        },
        Fixture {
            name: "bridge",
            labelling: lab(&|x, y| {
                let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
                let neck = (fx - c).abs() <= 4.0 * unit && (fy - c).abs() <= 0.5 * unit.max(1.0);
                block(x, y, left, c, half) || block(x, y, right, c, half) || neck
            }),
            class_fg4: (1, 0),
            class_fg8: (1, 0),
        },
        Fixture {
            name: "two_components",
            labelling: lab(&|x, y| block(x, y, left, c, half) || block(x, y, right, c, half)),
            class_fg4: (2, 0),
            class_fg8: (2, 0),
        },
        Fixture {
            // RT5-A13's other half. The check for §12's ORIENTED clause is
            // only exercised by loops long enough to HAVE an order, and
            // THIS REGISTER had zero, measured. The corpus does carry them —
            // with this fixture removed a reordering defect still reddens the
            // gate on fourteen corpus arms — and the delta-3 comment claiming
            // otherwise restated the red team's UPPER BOUND ("at most 55 of
            // 1334") as a measurement of absence. Corrected in delta-4.
            //
            // The fixture's justification is condition 51's standard, which is
            // coverage BY CONSTRUCTION at every size under both arms; fourteen
            // incidental corpus arms are not that.
            //
            // A staircase of blocks touching corner to corner puts one
            // degree-four vertex between each consecutive pair, so the loop
            // that walks the whole run is split into that many chains. Four
            // blocks give three junctions and loops of three or more
            // half-edges BY CONSTRUCTION, at every size and under both arms.
            name: "diagonal_staircase",
            labelling: lab(&|x, y| {
                let s = (unit as usize).max(2);
                let k = 4usize;
                let span = k * s;
                let off = (n.saturating_sub(span)) / 2;
                let (mut hit, mut i) = (false, 0usize);
                while i < k {
                    let (bx, by) = (off + i * s, off + i * s);
                    if x >= bx && x < bx + s && y >= by && y < by + s {
                        hit = true;
                    }
                    i += 1;
                }
                hit
            }),
            class_fg4: (4, 0),
            class_fg8: (1, 0),
        },
        Fixture {
            name: "diagonal_pinch",
            labelling: lab(&|x, y| {
                let s = (2.0 * unit).max(2.0);
                let a =
                    (x as f64) < c && (x as f64) >= c - s && (y as f64) < c && (y as f64) >= c - s;
                let b =
                    (x as f64) >= c && (x as f64) < c + s && (y as f64) >= c && (y as f64) < c + s;
                a || b
            }),
            class_fg4: (2, 0),
            class_fg8: (1, 0),
        },
    ]
}

/// A labelling with an UNRELATED witness in a far corner.
///
/// The §28 M5 clause "no unrelated graph mutation" is a statement about the part
/// of the graph an edit does not touch. A fixture where the edit's region plus
/// its halo covers the canvas leaves that clause with an empty population, and
/// an empty population is indistinguishable from a passing one from the outside
/// (F-0039). The witness is put there on purpose, and the tests assert it is
/// there.
pub fn with_a_distant_witness(l: &Labelling, n: usize) -> Labelling {
    let mut inside = l.inside().to_vec();
    let s = (n / 8).max(2);
    for y in 0..s {
        for x in 0..s {
            inside[y * n + x] = true;
        }
    }
    Labelling::new(n, n, inside)
}
