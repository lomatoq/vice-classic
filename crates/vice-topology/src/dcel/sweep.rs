//! The exhaustive sweep: every labelling of a small grid, under both convention
//! arms, assembled and audited.
//!
//! Split out of `audit.rs` for the §4.1 size rule, and the split is the honest
//! one: `audit` judges ONE arrangement, this file enumerates the INPUT SPACE.
//!
//! The distinction is not cosmetic — it is exactly the boundary REVIEW_M5_A
//! D1-N1 drew. An exhaustive sweep over inputs says nothing about a field no
//! predicate reads, and nothing at all about a defect INSIDE `assemble`, which
//! produces a self-consistent wrong value that is not a perturbation of
//! anything. Those are the audit's job, and the anchor against the labelling is
//! what does it.

use serde::Serialize;
use vice_ir::ComplementaryConnectivity;

use super::audit::{audit, is_the_assembly_of_its_own_labelling};
use super::Dcel;
use crate::cubical::Labelling;

/// Assemble every labelling of a `w x h` grid under both convention arms and
/// audit each one.
///
/// This is the answer to F-0054 / F-9 in the only form that closes the class:
/// the witness set IS the input space, so there is no subclass left in which a
/// defect could be unreachable. `(w, h) = (4, 4)` is 65 536 labellings and
/// 131 072 arrangements.
pub fn audit_every_labelling(w: u32, h: u32) -> Result<ExhaustiveReport, String> {
    let n = (w * h) as usize;
    assert!(
        n <= 20,
        "the exhaustive sweep is 2^n; {n} bits is not a sweep"
    );
    let mut audited = 0u64;
    let mut empty = 0u64;
    let mut classes: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
    let mut critical = 0u64;
    for bits in 0u64..(1u64 << n) {
        let inside: Vec<bool> = (0..n).map(|i| bits & (1 << i) != 0).collect();
        let has_critical = !crate::cubical::critical_cells(&Labelling::new(
            w as usize,
            h as usize,
            inside.clone(),
        ))
        .is_empty();
        for conn in ComplementaryConnectivity::arms() {
            let l = Labelling::new(w as usize, h as usize, inside.clone());
            let is_empty = l.count_inside() == 0;
            if is_empty {
                // Covered rather than skipped, since C243: the corpus reaches
                // this state and the sweep that was supposed to leave no
                // subclass unreached was excluding one BY CONSTRUCTION.
                empty += 1;
            }
            let d = Dcel::assemble(l, conn);
            let r = audit(&d).map_err(|e| format!("bits={bits} conn={conn:?}: {e}"))?;
            if !is_the_assembly_of_its_own_labelling(&d) {
                return Err(format!("bits={bits} conn={conn:?}: not its own assembly"));
            }
            classes.insert((r.foreground_faces, r.holes));
            audited += 1;
        }
        if has_critical {
            critical += 1;
        }
    }
    Ok(ExhaustiveReport {
        width_px: w,
        height_px: h,
        arrangements_audited: audited,
        empty_arrangements_covered: empty,
        distinct_classes: classes.len() as u32,
        classes: classes.into_iter().collect(),
        labellings_with_a_critical_cell: critical,
    })
}

/// What an exhaustive sweep saw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExhaustiveReport {
    pub width_px: u32,
    pub height_px: u32,
    pub arrangements_audited: u64,
    /// Labellings with no interface at all. Audited like every other one since
    /// C243 — the corpus reaches this state (`adv/sliver`) and a sweep that
    /// excluded it was leaving a subclass unreached by construction, which is
    /// the very class F-0054 / F-9 names.
    pub empty_arrangements_covered: u64,
    pub distinct_classes: u32,
    pub classes: Vec<(u32, u32)>,
    pub labellings_with_a_critical_cell: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The audit is green on every arrangement of every labelling of a 4x3
    /// grid, under both conventions, and the sweep SEES more than one
    /// topological class — an exhaustive run over a space with one answer in
    /// it would prove nothing.
    #[test]
    fn the_audit_is_green_over_a_whole_small_input_space() {
        let r = audit_every_labelling(4, 3).expect("exhaustive audit");
        assert_eq!(r.arrangements_audited, 2 * (1 << 12));
        assert_eq!(r.empty_arrangements_covered, 2);
        assert!(r.distinct_classes >= 6, "classes seen: {:?}", r.classes);
        assert!(
            r.classes.contains(&(1, 1)),
            "the ring class must be in a 4x3 sweep: {:?}",
            r.classes
        );
        assert!(
            r.labellings_with_a_critical_cell > 0,
            "a sweep with no critical 2x2 never exercises the convention branch"
        );
    }
}
