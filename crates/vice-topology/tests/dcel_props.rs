//! Properties of the shared DCEL over the sizes and shapes it is actually
//! applied to (spec v1.3 §12, §27.5, §28 M5).
//!
//! ## Why this file exists at all
//!
//! F-0054 / F-9 is the standing class of this project: **an instrument's
//! domain of proof must be compared with its domain of use, and the comparison
//! must be by COMPOSITION rather than by boundary.** M4.5's judge was proved
//! on twelve hand-written fixtures no larger than 7x7 and decided questions on
//! renders up to 512 px; the gap was found by the red team, and freezing it
//! cost the milestone a delta.
//!
//! So the DCEL's proof domain is built as three axes, on purpose, and each one
//! is named:
//!
//! | axis | how it is covered | what would be missed without it |
//! |---|---|---|
//! | **exhaustive** | every labelling of 4x3 and 4x4, both convention arms ([`vice_topology::audit_every_labelling`]) | any defect reachable at small size — there is no subclass left to hide in |
//! | **size** | the corpus sizes 32, 64, 128 and the declared-but-unreached 256 and 512 | a defect that only triggers above the fixture ceiling — F-8 in M4.5 |
//! | **structure** | five structural fixtures AT EVERY SIZE: annulus, nested annulus, bridge, two components, triple junction | the register the corpus actually lives in. REVIEW_M4_5 M45-N42 / condition 51: the class `(1,1)` appeared in 450 random comparisons ONCE, by chance |
//!
//! The third axis is condition 51, and its acceptance criterion is that the
//! class `(1,1)` is present BY CONSTRUCTION rather than by chance.
//! [`structural_fixtures`] returns it at every size, and
//! `the_structural_register_is_covered_by_construction` asserts the classes
//! rather than hoping for them.

use std::collections::BTreeSet;

use vice_ir::{ComplementaryConnectivity, PixelConnectivity};
use vice_topology::continuation::EditKind;
use vice_topology::dcel::{apply, audit, is_the_assembly_of_its_own_labelling, Edit, Outcome, Roi};
use vice_topology::{
    audit_every_labelling, signature, structural_fixtures, with_a_distant_witness, Dcel, Labelling,
    TX_CONFIG_V1,
};

/// The sizes the corpus uses, plus the two the size list declares and no
/// fixture reached in M4.5. F-8 is that gap; here it is not a gap.
pub const SIZES_PX: [usize; 5] = [32, 64, 128, 256, 512];

/// The sizes cheap enough for the default test path. The two large ones run
/// under `--ignored` in release, which is where the corpus-wide measurements
/// already live.
pub const FAST_SIZES_PX: [usize; 3] = [32, 64, 128];

fn arms() -> [ComplementaryConnectivity; 2] {
    ComplementaryConnectivity::arms()
}

/// **Condition 51.** The structural register is covered BY CONSTRUCTION.
///
/// The acceptance criterion REVIEW_M4_5 §11 states is that the class `(1, 1)`
/// appears because the fixture set puts it there, not because a sampler
/// happened to land on it. This asserts the class of every fixture at every
/// fast size under both convention arms, so a generator that stopped producing
/// an annulus fails here rather than quietly narrowing the population.
#[test]
fn the_structural_register_is_covered_by_construction() {
    let mut classes: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut checked = 0usize;
    let mut convention_dependent = 0usize;
    for n in FAST_SIZES_PX {
        for f in structural_fixtures(n) {
            if f.class_fg4 != f.class_fg8 {
                convention_dependent += 1;
            }
            for conn in arms() {
                let four = conn.foreground() == PixelConnectivity::Four;
                let want = if four { f.class_fg4 } else { f.class_fg8 };
                let sig = signature(&f.labelling, conn);
                assert_eq!(
                    (sig.components, sig.holes),
                    want,
                    "{} at {n} px under fg-{} is ({}, {}), not the class it is built to be",
                    f.name,
                    if four { "4" } else { "8" },
                    sig.components,
                    sig.holes
                );
                classes.insert(want);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, FAST_SIZES_PX.len() * 5 * 2);
    assert!(
        classes.contains(&(1, 1)),
        "condition 51: the annulus class must be present by construction, saw {classes:?}"
    );
    assert!(classes.contains(&(2, 1)), "{classes:?}");
    assert!(classes.contains(&(2, 0)), "{classes:?}");
    // And the fixture set is not one class wearing five names.
    assert!(classes.len() >= 4, "{classes:?}");
    // One fixture must answer differently under the two arms, or the whole
    // register would be blind to the convention it is supposed to exercise.
    assert_eq!(
        convention_dependent,
        FAST_SIZES_PX.len(),
        "exactly one fixture per size is convention-dependent"
    );
}

/// The audit is green on every structural fixture at every fast size, under
/// both arms — and the population is non-trivial, which is asserted rather
/// than assumed: an audit that ran on nothing would pass this test otherwise
/// (F-0039).
#[test]
fn the_audit_holds_on_the_structural_register_at_corpus_sizes() {
    let mut audited = 0usize;
    let mut junctions_seen = 0u32;
    for n in FAST_SIZES_PX {
        for f in structural_fixtures(n) {
            for conn in arms() {
                let d = Dcel::assemble(f.labelling.clone(), conn);
                let r = audit(&d).unwrap_or_else(|e| panic!("{} at {n}: {e}", f.name));
                assert!(
                    is_the_assembly_of_its_own_labelling(&d),
                    "{} at {n}: not its own assembly",
                    f.name
                );
                assert!(
                    r.boundaries > 0 && r.loops > 0,
                    "{} at {n}: empty arrangement",
                    f.name
                );
                // A junction is a lattice point of degree four, and it shows up
                // as a vertex whose incidence exceeds two. `incidence` is
                // (vertices, sum of degrees); with no junction every vertex is
                // the artificial one of a closed chain and the sum is exactly
                // twice the count.
                let (v, deg) = vice_topology::dcel::incidence_signature(&d);
                if deg > 2 * v {
                    junctions_seen += 1;
                }
                audited += 1;
            }
        }
    }
    assert_eq!(audited, FAST_SIZES_PX.len() * 5 * 2);
    assert_eq!(
        junctions_seen as usize,
        FAST_SIZES_PX.len() * 2,
        "exactly the diagonal pinch, at every size and under both arms, should carry a junction; \
         a register with none would exercise only simple closed curves"
    );
}

/// The transaction machinery on the structural register: closing the gap of
/// `two_components` is a real edit, it commits, and the chains outside the
/// region are byte-identical.
///
/// The knockout is in the same test and in both directions: the same edit with
/// one pixel added outside the ROI must be REFUSED, and the clean edit must be
/// ACCEPTED. A locality check that refused everything would satisfy half of
/// this and measure nothing.
#[test]
fn a_local_transaction_on_a_corpus_sized_fixture_leaves_the_rest_alone() {
    for n in FAST_SIZES_PX {
        let unit = (n as f64 / 16.0).max(1.0) as u32;
        let c = (n / 2) as u32;
        let fixtures = structural_fixtures(n);
        let two = &fixtures[3];
        assert_eq!(two.name, "two_components");
        let witnessed = with_a_distant_witness(&two.labelling, n);
        let base = Dcel::assemble(witnessed, arms()[0]);
        assert_eq!(
            base.foreground_faces(),
            3,
            "two blocks and the corner witness"
        );

        let roi = Roi {
            x0: c - 3 * unit,
            y0: c - unit,
            x1: c + 3 * unit,
            y1: c + unit,
        };
        let set: Vec<(u32, u32, bool)> = (roi.x0..roi.x1)
            .flat_map(|x| (roi.y0..roi.y1).map(move |y| (x, y, true)))
            .collect();
        let edit = Edit {
            kind: EditKind::BridgeClose,
            roi,
            set: set.clone(),
        };
        let out = apply(&base, &edit, &TX_CONFIG_V1);
        let rep = out.report();
        let new = out
            .committed()
            .unwrap_or_else(|| panic!("{n} px: the bridge must close, got {rep:?}"));
        assert_eq!(
            new.foreground_faces(),
            2,
            "the two blocks merge; the witness stays"
        );
        assert_eq!(rep.unrelated_chains_that_moved, 0);
        assert!(
            rep.unrelated_chains > 0,
            "{n} px: nothing lies outside the region, so the clause has no population"
        );

        // The other direction.
        let mut reaching = set;
        reaching.push((0, 0, true));
        let bad = Edit {
            kind: EditKind::BridgeClose,
            roi,
            set: reaching,
        };
        assert!(
            matches!(
                apply(&base, &bad, &TX_CONFIG_V1),
                Outcome::RolledBack { .. }
            ),
            "{n} px: an edit reaching outside its ROI must not commit"
        );
    }
}

/// The two convention arms disagree about a critical 2x2, and the place they
/// disagree is the FACE COUNT.
///
/// §5.3's whole content is that the answer at a saddle depends on the
/// convention. If both arms produced the same arrangement the convention would
/// be decoration, so this is asserted on a fixture built to contain nothing
/// but the ambiguity.
#[test]
fn the_two_conventions_disagree_about_a_critical_2x2() {
    // Two pixels touching at a corner, and nothing else.
    let l = Labelling::new(
        4,
        4,
        (0..16)
            .map(|i| {
                let (x, y) = (i % 4, i / 4);
                (x == 1 && y == 1) || (x == 2 && y == 2)
            })
            .collect(),
    );
    let four = Dcel::assemble(
        l.clone(),
        ComplementaryConnectivity::new(PixelConnectivity::Four),
    );
    let eight = Dcel::assemble(l, ComplementaryConnectivity::new(PixelConnectivity::Eight));
    assert!(audit(&four).is_ok());
    assert!(audit(&eight).is_ok());
    assert_eq!(four.foreground_faces(), 2, "4-connected ink is pinched");
    assert_eq!(
        eight.foreground_faces(),
        1,
        "8-connected ink passes through"
    );
    assert_ne!(four.faces().len(), eight.faces().len());
}

/// The exhaustive axis at 4x4: 65 536 labellings, 131 070 arrangements, every
/// one assembled and audited.
///
/// `#[ignore]` because it is minutes in debug, and it runs in release in CI
/// beside the other corpus-wide measurements. The 4x3 sweep is in the crate's
/// unit tests and runs by default, so the axis is never entirely absent from
/// the fast path.
#[test]
#[ignore = "exhaustive 2^16 sweep; runs in CI in release"]
fn the_audit_is_green_over_every_labelling_of_a_four_by_four() {
    let r = audit_every_labelling(4, 4).expect("exhaustive audit at 4x4");
    assert_eq!(r.arrangements_audited, 2 * (1u64 << 16));
    assert!(r.distinct_classes >= 8, "{:?}", r.classes);
    assert!(r.classes.contains(&(1, 1)));
    assert!(r.labellings_with_a_critical_cell > 1000);
    println!(
        "4x4 exhaustive: {} arrangements, {} classes {:?}, {} labellings with a critical 2x2",
        r.arrangements_audited, r.distinct_classes, r.classes, r.labellings_with_a_critical_cell
    );
}

/// The size axis at its declared ceiling. F-8 was a judge proved to 128 px
/// deciding questions at 512; the DCEL's proof domain reaches the top of
/// `SIZES_PX` instead, and the cost of that is the reason this one is
/// `#[ignore]`d rather than the reason it does not exist.
#[test]
#[ignore = "256 and 512 px; runs in CI in release"]
fn the_audit_holds_on_the_structural_register_at_every_declared_size() {
    for n in SIZES_PX {
        for f in structural_fixtures(n) {
            for conn in arms() {
                let four = conn.foreground() == PixelConnectivity::Four;
                let want = if four { f.class_fg4 } else { f.class_fg8 };
                let sig = signature(&f.labelling, conn);
                assert_eq!((sig.components, sig.holes), want, "{} at {n}", f.name);
                let d = Dcel::assemble(f.labelling.clone(), conn);
                let r = audit(&d).unwrap_or_else(|e| panic!("{} at {n}: {e}", f.name));
                println!(
                    "{} {n}px fg-{}: V={} B={} L={} F={} ({}, {})",
                    f.name,
                    if four { 4 } else { 8 },
                    r.vertices,
                    r.boundaries,
                    r.loops,
                    r.faces,
                    r.foreground_faces,
                    r.holes
                );
            }
        }
    }
}
