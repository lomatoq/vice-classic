//! What a topology edit does to the signature (spec v1.3 §11.4, §28 M5).
//!
//! This type lived inside [`crate::continuation`] until M6, where it was a
//! four-variant enum and a detail of envelope adjacency. It is neither now: the
//! DCEL transaction executor decides acceptance against it, so it has two
//! consumers with different needs, and §4.1's rule about a module earning its
//! own file applies.

use serde::Serialize;

/// What one edit does to the topological signature, DECLARED before the edit
/// is performed.
///
/// ## Why this is a point of Z^2 and not a list of names (§28 M5, limitation 37)
///
/// Until M6 this was a four-variant enum, and `vice-bench`'s harness classified
/// an arm by matching `(dc, dh)` against those four with a `_ => return None`
/// fallthrough. That dropped **310 of 480 arms**, and the dropped subclass was
/// reported as the one §28 M5 names when it says "local COMPOUND topology
/// transactions". Both M5 reviewers recorded it as the one carried obligation
/// with no second deferral available (REVIEW_M5_A §A4, REVIEW_M5_B addenda
/// 2-7), and F-0058's own rule applied to it verbatim: a filter that decides
/// membership by looking at the answer is not a population.
///
/// The closure is a change of CRITERION, not an extension of the list, because
/// F-0048 Q2 asks what the next finding costs. A fifth named variant would have
/// answered "append a line". A point of Z^2 answers "the criterion already
/// covers it": every `(dc, dh)` is expressible, so an edit shape nobody
/// anticipated needs no new variant and no new arm anywhere.
///
/// The four unit steps survive as CONSTANTS of this type rather than as cases
/// of it. They name the four edits §11.4 discusses, and naming is all they now
/// do — no control flow branches on them.
///
/// ## What widening the type did NOT do
///
/// It did not, by itself, produce a compound population. With one edit shape a
/// filled square can only yield `(0,0)`, `(-1,0)` or `(+1,0)`, so the first run
/// after the filter came out published `transactions_compound = 0` and the 310
/// "compound" arms turned out to be IDENTITY edits, 282 of which change no
/// pixel at all. That is recorded as F-0081; the population came from a second
/// edit shape, not from this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditKind {
    /// Change in the number of foreground components.
    pub d_components: i64,
    /// Change in the number of holes.
    pub d_holes: i64,
}

impl EditKind {
    /// One fewer component: two blobs became one. A bridge closed.
    pub const BRIDGE_CLOSE: EditKind = EditKind::new(-1, 0);
    /// One more component: one blob became two. A gap opened.
    pub const GAP_OPEN: EditKind = EditKind::new(1, 0);
    /// One more hole.
    pub const HOLE_OPEN: EditKind = EditKind::new(0, 1);
    /// One fewer hole.
    pub const HOLE_FILL: EditKind = EditKind::new(0, -1);

    /// The four unit steps with the names they have carried since M4.5.
    ///
    /// This IS a literal enumerating subjects (F-0048 Q1), and it is allowed to
    /// be one because of what it is for: it supplies the exact strings the
    /// signed M4.5 and M5 artifacts already contain, so that widening the type
    /// moves no recorded byte. No decision is taken by membership in it —
    /// [`EditKind::is_unit_step`] is COMPUTED from the delta rather than looked
    /// up here, and `the_named_steps_are_exactly_the_unit_steps` requires the
    /// two to agree in both directions over an exhaustive box, so this array
    /// cannot quietly become the definition of "unit step".
    pub const UNIT_STEPS: [(EditKind, &'static str); 4] = [
        (EditKind::BRIDGE_CLOSE, "bridge_close"),
        (EditKind::GAP_OPEN, "gap_open"),
        (EditKind::HOLE_OPEN, "hole_open"),
        (EditKind::HOLE_FILL, "hole_fill"),
    ];

    pub const fn new(d_components: i64, d_holes: i64) -> EditKind {
        EditKind {
            d_components,
            d_holes,
        }
    }

    /// The signature delta between two arrangements, as a fact about them.
    pub fn between(before: (u32, u32), after: (u32, u32)) -> EditKind {
        EditKind::new(
            i64::from(after.0) - i64::from(before.0),
            i64::from(after.1) - i64::from(before.1),
        )
    }

    /// Does this edit change the signature at all?
    pub fn is_identity(self) -> bool {
        self.d_components == 0 && self.d_holes == 0
    }

    /// Is this one of the four unit steps?
    ///
    /// DERIVED from the delta, not looked up in [`EditKind::UNIT_STEPS`].
    pub fn is_unit_step(self) -> bool {
        self.d_components.abs() + self.d_holes.abs() == 1
    }

    /// How many unit steps this edit is worth: the L1 norm of the delta.
    ///
    /// A COMPOUND edit is one whose `steps()` is not 1. §11.4 calls every
    /// topology edit a compound operation; this is the number that says how
    /// compound this one is.
    pub fn steps(self) -> u64 {
        self.d_components.unsigned_abs() + self.d_holes.unsigned_abs()
    }

    /// The edit's name.
    ///
    /// The four unit steps keep the exact strings `docs/gt/DCEL_M5.json` and
    /// `docs/gt/TOPOLOGY_M4_5.json` already carry. Anything else is named FROM
    /// its delta rather than from a table, so a name exists for every point of
    /// Z^2 without anyone having written one.
    pub fn name(self) -> String {
        for (k, s) in EditKind::UNIT_STEPS {
            if k == self {
                return s.to_string();
            }
        }
        if self.is_identity() {
            return "identity".to_string();
        }
        format!("compound(c{:+},h{:+})", self.d_components, self.d_holes)
    }
}

/// Serialized as its NAME, so the four unit steps produce exactly the strings
/// the signed artifacts already contain: the old enum carried
/// `#[serde(rename_all = "snake_case")]` and produced the same four.
impl Serialize for EditKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The named array is not the definition of "unit step".**
    ///
    /// `UNIT_STEPS` is the one literal enumerating subjects that survived M6's
    /// rewrite of [`EditKind`], and F-0048 Q1 says a literal enumerating
    /// subjects closes the contents of the literal. It is kept only to supply
    /// the four names the signed artifacts carry, so the thing that must be
    /// true is that it agrees with the COMPUTED predicate in both directions:
    ///
    /// - everything named is a unit step (no name for a non-step);
    /// - every unit step is named (no step without a name).
    ///
    /// The second leg is the one that matters, and it is checked by
    /// ENUMERATING Z^2 over a box rather than by re-listing the four: add a
    /// fifth entry to the array, or change `is_unit_step`, and one of the two
    /// legs fails. The box is small on purpose — `is_unit_step` is a statement
    /// about the L1 ball of radius 1, so a radius-4 box contains every witness
    /// there is and the sweep is exhaustive rather than sampled.
    #[test]
    fn the_named_steps_are_exactly_the_unit_steps() {
        for (k, name) in EditKind::UNIT_STEPS {
            assert!(
                k.is_unit_step(),
                "{name} is named as a unit step and is not one: {k:?}"
            );
            assert_eq!(k.steps(), 1, "{name}");
            assert_eq!(k.name(), name, "the name must round-trip");
        }

        let mut named = 0usize;
        let mut computed = 0usize;
        for dc in -4i64..=4 {
            for dh in -4i64..=4 {
                let k = EditKind::new(dc, dh);
                let is_named = EditKind::UNIT_STEPS.iter().any(|(u, _)| *u == k);
                if is_named {
                    named += 1;
                }
                if k.is_unit_step() {
                    computed += 1;
                }
                assert_eq!(
                    is_named,
                    k.is_unit_step(),
                    "the array and the predicate disagree at ({dc}, {dh})"
                );
            }
        }
        assert_eq!(named, 4, "exactly four points are named");
        assert_eq!(computed, 4, "exactly four points are unit steps");
    }

    /// **A compound edit is expressible and names itself.**
    ///
    /// The point of the M6 rewrite: no `match` arm exists for these and none is
    /// needed. The name is derived, so a delta nobody anticipated still has a
    /// distinct, stable name — which is what a report row needs.
    #[test]
    fn a_compound_edit_is_expressible_and_names_itself_distinctly() {
        let compound = [
            EditKind::new(-2, 0),
            EditKind::new(2, 0),
            EditKind::new(-1, 1),
            EditKind::new(1, -1),
            EditKind::new(0, 3),
            EditKind::new(-3, -2),
        ];
        let mut names = std::collections::BTreeSet::new();
        for k in compound {
            assert!(!k.is_unit_step(), "{k:?} must not read as a unit step");
            assert!(!k.is_identity());
            assert!(k.steps() >= 2, "{k:?} is worth {} steps", k.steps());
            assert!(names.insert(k.name()), "names must be distinct: {k:?}");
        }
        assert_eq!(names.len(), compound.len());

        // The identity is neither a unit step nor compound, and it is named.
        let id = EditKind::new(0, 0);
        assert!(id.is_identity());
        assert!(!id.is_unit_step());
        assert_eq!(id.steps(), 0);
        assert_eq!(id.name(), "identity");
    }

    /// **`between` is the delta and nothing else.** Both directions, so a sign
    /// error cannot hide behind a symmetric fixture.
    #[test]
    fn between_is_the_signed_difference_in_both_directions() {
        assert_eq!(EditKind::between((2, 0), (1, 0)), EditKind::BRIDGE_CLOSE);
        assert_eq!(EditKind::between((1, 0), (2, 0)), EditKind::GAP_OPEN);
        assert_eq!(EditKind::between((1, 0), (1, 1)), EditKind::HOLE_OPEN);
        assert_eq!(EditKind::between((1, 1), (1, 0)), EditKind::HOLE_FILL);
        assert_eq!(EditKind::between((3, 2), (1, 4)), EditKind::new(-2, 2));
        assert!(EditKind::between((5, 5), (5, 5)).is_identity());
    }

    /// The four names are exactly the strings the signed artifacts carry, so
    /// widening the type moved no recorded byte. Checked against literals
    /// rather than against `UNIT_STEPS`, because comparing the array to itself
    /// would measure nothing.
    #[test]
    fn the_four_unit_steps_serialize_as_the_artifacts_already_record_them() {
        let j = |k: EditKind| serde_json::to_string(&k).unwrap();
        assert_eq!(j(EditKind::BRIDGE_CLOSE), "\"bridge_close\"");
        assert_eq!(j(EditKind::GAP_OPEN), "\"gap_open\"");
        assert_eq!(j(EditKind::HOLE_OPEN), "\"hole_open\"");
        assert_eq!(j(EditKind::HOLE_FILL), "\"hole_fill\"");
        assert_eq!(j(EditKind::new(-1, -1)), "\"compound(c-1,h-1)\"");
        assert_eq!(j(EditKind::new(0, 0)), "\"identity\"");
    }
}
