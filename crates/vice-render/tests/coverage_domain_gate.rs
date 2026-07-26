//! Numeric-domain gate for the coverage accumulator (F-0008; REDTEAM_M2
//! F-M2-R2).
//!
//! Instrument: the **translation oracle** the red team used, brought
//! in-tree. A shape is placed at offset `base = 2^k`; the ground truth is
//! the SAME shape at the origin. The subtraction `x - base` is EXACT for
//! `base <= x <= 2*base` (Sterbenz), so the two copies are geometrically
//! identical bit-for-bit and any difference in their renders is pure
//! arithmetic error of the accumulator — measured against a reference
//! whose own error is ~1e-16, not against another f64 approximation.
//!
//! Why this class of test did not exist before: the in-tree translation
//! test used an AXIS-ALIGNED rectangle, and axis-aligned edges take the
//! `sx == ex` fast path where the trapezoid is exact for free. The defect
//! lived only on SLOPED edges, so the fixture, not the claim, was what
//! made the old test pass. Every oracle shape here therefore has sloped
//! edges.

mod common;

use vice_geom::Pt;
use vice_render::domain::NumericDomain;
use vice_render::{polygon_coverage, PartitionTolerances};

/// A non-dyadic, sloped quadrilateral at offset `base`, together with its
/// EXACT translate to the origin.
///
/// The origin copy is obtained by SUBTRACTING `base` (exact by Sterbenz for
/// `base <= x <= 2*base`), never by re-evaluating the literals: re-evaluating
/// would round differently and the oracle would then be comparing two
/// different shapes instead of one shape at two offsets.
fn oracle_pair(base: f64) -> (Vec<Pt>, Vec<Pt>) {
    let pts = [(0.3, 0.7), (21.55, 4.2), (17.9, 19.35), (2.15, 13.6)];
    let mut big: Vec<Pt> = pts.iter().map(|&(x, y)| Pt::new(base + x, y)).collect();
    big.push(big[0]);
    let small: Vec<Pt> = big.iter().map(|p| Pt::new(p.x - base, p.y)).collect();
    (big, small)
}

/// Worst per-pixel |Δ| between the shape at `base` and the same shape at
/// the origin, over the window the shape occupies.
fn oracle_error(base: f64) -> f64 {
    let (big, small) = oracle_pair(base);
    // Sanity: the two copies must be an EXACT translation of each other,
    // otherwise the oracle is measuring geometry, not arithmetic.
    for (b, s) in big.iter().zip(&small) {
        assert_eq!(s.x + base, b.x, "translation must be exact (Sterbenz)");
        assert_eq!(b.y, s.y);
    }

    let cols = 24u32;
    let base_col = base as u32;
    let w_big = base_col + cols;
    let cov_big = polygon_coverage(&[&big], w_big, 0, 24).expect("closed loops");
    let cov_small = polygon_coverage(&[&small], cols, 0, 24).expect("closed loops");

    let mut worst = 0.0f64;
    for r in 0..24usize {
        for c in 0..cols as usize {
            let a = cov_big[r * w_big as usize + base_col as usize + c];
            let b = cov_small[r * cols as usize + c];
            worst = worst.max((a - b).abs());
        }
    }
    worst
}

/// Inside the frozen M2 domain the accumulator's error must stay under the
/// domain's derived bound — the bound that is now a property of a TYPE
/// rather than an unstated assumption.
///
/// The claim is deliberately LAYERED, because a single flat number would be
/// false at one end or vacuous at the other:
///
/// - domain-wide (coords up to 2^16): `<= domain.coverage_error_bound_px()`
///   = 2.3e-10, measured worst 2.4e-12 (≈95x margin);
/// - small-coordinate regime (up to 2^13, which covers every canvas this
///   core targets): `<= 1e-12`, measured worst 5.5e-13.
///
/// Pre-fix measurements for the same oracle (REDTEAM_M2 table): 2^13 →
/// 1.271e-12, 2^14 → 2.505e-12, 2^16 → 4.542e-12. The column-local
/// trapezoid roughly halves each of these; the remainder is the
/// representation limit of the edge position itself (ulp(M)), which no
/// integration scheme can undo — hence a domain, not a tighter formula.
#[test]
fn translation_oracle_holds_across_the_frozen_domain() {
    let domain = NumericDomain::m2_default();
    let derived = domain.coverage_error_bound_px();
    for k in [6u32, 10, 12, 13, 14, 15, 16] {
        let base = f64::from(1u32 << k);
        let err = oracle_error(base);
        println!("oracle 2^{k}: worst |delta| = {err:e}");
        assert!(
            err <= derived,
            "2^{k}: {err:e} exceeds the domain's derived bound {derived:e}"
        );
        if k <= 13 {
            assert!(
                err <= 1e-12,
                "2^{k}: {err:e} exceeds the small-coordinate bound 1e-12"
            );
        }
    }
}

/// The bound the partition checker enforces must dominate the accumulator
/// error everywhere in the domain, with margin. This is the property that
/// was false before: 1e-9 was enforced while the error reached 1.73e-9.
#[test]
fn enforced_tolerance_dominates_the_measured_error_in_domain() {
    let tol = PartitionTolerances::default().sum_abs_tol;
    assert_eq!(tol, PartitionTolerances::FROZEN_FLOOR);
    let worst = [12u32, 14, 16]
        .into_iter()
        .map(|k| oracle_error(f64::from(1u32 << k)))
        .fold(0.0f64, f64::max);
    println!("worst in-domain oracle error = {worst:e}, enforced tol = {tol:e}");
    assert!(
        worst * 100.0 < tol,
        "enforced tolerance {tol:e} must dominate measured {worst:e} by >=100x"
    );
}

/// Outside the domain the error grows with coordinate magnitude — that is
/// a property of f64, not a defect. The point of the typed domain is that
/// such scenes are REFUSED by the render path rather than silently
/// returning numbers the guards cannot check. Here we show the growth is
/// real (so the domain is not theatre) and, in `partition_gate`, that the
/// refusal happens.
#[test]
fn outside_the_domain_the_error_grows_with_magnitude() {
    let in_domain = oracle_error(f64::from(1u32 << 16));
    let far_out = oracle_error(f64::from(1u32 << 24));
    println!("2^16 -> {in_domain:e}, 2^24 -> {far_out:e}");
    assert!(
        far_out > in_domain * 8.0,
        "error must visibly grow outside the domain: {in_domain:e} -> {far_out:e}"
    );
    // And it exceeds what the partition checker would enforce — the exact
    // situation the domain check now prevents from being reached.
    assert!(far_out > 1e-11, "2^24 error {far_out:e} is large by design");
}
