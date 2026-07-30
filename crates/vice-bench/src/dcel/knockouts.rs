//! The knockouts: one per §28 M5 clause, plus the population control.
//!
//! In their own module for the §4.1 size rule, and the split is the honest one:
//! `dcel/mod.rs` RUNS the harness, this file describes the defects the harness
//! is required to notice. Every one of them is a corruption a cold context
//! actually executed against this tree, kept here as a callable control rather
//! than as a sentence about a past finding — which is F-7 and the red team's
//! tenth obligation.

/// Whether the M5 stage is allowed to reduce a set of topologies to one.
///
/// It is not, and this type is how "it is not" becomes falsifiable: §32 rule 14
/// forbids a topology winner from the M5 proxy, and a clause asserting that it
/// does not happen is worth exactly as much as the world in which it does.
/// [`ProxyKnockout::Select`] is that world, it is reachable from the harness,
/// and the test `the_proxy_knockout_takes_the_row_down` runs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyKnockout {
    /// Production: the stage carries every topology through.
    Off,
    /// Knockout: the stage keeps the candidate with the smallest surrogate
    /// cost and drops the rest. Never used outside the control.
    Select,
}

/// Whether the transaction's edit is allowed to reach outside its ROI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoiKnockout {
    Off,
    /// Knockout: add one pixel far from the declared region.
    Reach,
}

/// Whether the arrangement's class is reported as the arrangement computed it.
///
/// Clause 2 had NO knockout, which REDTEAM_M5 §3 lists as one of the three
/// mechanisms that fail F-0048: `RunKnockouts` was one field per clause that
/// happened to have a knockout, so "add a field" was the answer to Q2 and two
/// of the four clauses had no world in which they are false.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassKnockout {
    Off,
    /// Knockout: report one more component than the arrangement has, so the
    /// DCEL and the independent chain disagree.
    Shift,
}

/// Whether the pixel-to-face map is left as `assemble` built it.
///
/// This is REDTEAM_M5 RT5-A1 itself, wired in as a control: rotate every entry
/// of `face_of_padded_px` above 16 px. Before delta-1 it passed 530 tests, four
/// `[MET]` clauses and a byte-identical artifact. Clause 4 must now go red.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceMapKnockout {
    Off,
    /// Knockout: the red team's own ten-line edit (RT5-A1).
    Rotate,
    /// Knockout: REVIEW_M5_A D1-N1 / REDTEAM_M5 RT5-A9 — a relabelling that
    /// keeps EVERY count and every structural relation and attaches the wrong
    /// label to a face's pixels. It passed the whole of delta-1 with a
    /// byte-identical artifact and 529 of 1089 pixels reporting the wrong ink.
    SwapLabels,
}

/// Whether the structural register keeps the fixture that carries long loops.
///
/// The red side of the ORIENTED clause's population floor: with the staircase
/// gone the register's share falls to zero and clause 4 must go NOT MET.
///
/// **TWO legs, not three, and delta-4 claimed three** (REDTEAM_M5 RT5-A19,
/// REVIEW_M5_A D4-N2). With a floor above zero, `count == 0` ANALYTICALLY
/// implies `!row`, so "red" and "empty" are one demonstration wearing two
/// names — the RT5-A2 shape moved from a gate row onto its own control.
///
/// - **red / empty (one leg):** this knockout. The population goes to zero and
///   the row goes NOT MET;
/// - **idle (independent, and it holds):** the count comes from
///   `loop_length_profile` over real loop lengths rather than from a constant,
///   so it cannot be satisfied without loops;
///   `the_oriented_clause_has_a_population_and_it_is_split_by_source` asserts
///   the longest is genuinely three or more.
///
/// A third leg would need a run where the population is NON-zero and still
/// below the floor. That is now reachable — the floor is six and the register
/// produces exactly six — but only by removing a size from the register, which
/// is a change to the register rather than a knockout over it. Recorded as what
/// it is instead of counted as a leg it is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterKnockout {
    Off,
    /// Knockout: drop every fixture carrying a loop of three or more.
    DropLongLoops,
}

/// Remove one real transaction shape at the point where arms are measured.
///
/// This is STATUS_M6 limitation 54's control: the old compound knockout
/// mutated aggregate report counts after measurement and therefore could not
/// detect deletion of the ring shape itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKnockout {
    Off,
    DropRing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunKnockouts {
    pub proxy: ProxyKnockout,
    pub roi: RoiKnockout,
    pub class: ClassKnockout,
    pub face_map: FaceMapKnockout,
    pub register: RegisterKnockout,
    pub shape: ShapeKnockout,
}

impl RunKnockouts {
    /// One knockout per §28 M5 clause, in clause order.
    ///
    /// The DESTRUCTURING is the mechanism: a field added without an entry does
    /// not compile, and `every_gate_clause_has_a_knockout_that_reddens_it`
    /// compares the length of this against the length of the gate table, which
    /// is derived from `gate_table()` rather than written here. A fifth clause
    /// with no knockout fails that test.
    pub fn one_per_clause() -> Vec<(&'static str, RunKnockouts)> {
        let RunKnockouts {
            proxy: _,
            roi: _,
            class: _,
            face_map: _,
            register: _,
            shape: _,
        } = PRODUCTION;
        vec![
            (
                "no final-topology claim from proxy",
                RunKnockouts {
                    proxy: ProxyKnockout::Select,
                    ..PRODUCTION
                },
            ),
            (
                "candidate recall maintained after budget pruning",
                RunKnockouts {
                    class: ClassKnockout::Shift,
                    ..PRODUCTION
                },
            ),
            (
                "no unrelated graph mutation",
                RunKnockouts {
                    shape: ShapeKnockout::DropRing,
                    ..PRODUCTION
                },
            ),
            (
                "no dangling/invalid faces",
                RunKnockouts {
                    face_map: FaceMapKnockout::Rotate,
                    ..PRODUCTION
                },
            ),
        ]
    }
}

pub const PRODUCTION: RunKnockouts = RunKnockouts {
    proxy: ProxyKnockout::Off,
    roi: RoiKnockout::Off,
    class: ClassKnockout::Off,
    face_map: FaceMapKnockout::Off,
    register: RegisterKnockout::Off,
    shape: ShapeKnockout::Off,
};
