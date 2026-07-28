//! Local certificates (spec §12, §11.4 step six, §28 M5 "topology/isotopy
//! certificates").
//!
//! §12 asks for two different things and the difference is the whole content
//! of this file.
//!
//! **A topology certificate** is a statement about the combinatorics of an
//! arrangement before and after an edit. M5 can produce one, because both
//! arrangements exist and both are integer objects: [`TopologyCertificate`].
//!
//! **An isotopy certificate** is §12's condition for replacing a boundary
//! CHAIN with a fitted CURVE while preserving topology:
//!
//! ```text
//! endpoints/junction incidence unchanged;
//! the fitted curve lies in a certified tubular neighbourhood;
//! the tubes of non-adjacent boundaries do not intersect;
//! robust intersection predicates green.
//! ```
//!
//! Three of those four are about a curve, and there is no curve until M6's
//! typed grammar fits one. So the isotopy certificate is a TYPED REFUSAL that
//! names its missing capabilities and their owner — the same shape
//! [`crate::continuation`] uses, for the same reason: §32 rule 7 forbids a
//! placeholder API for a later milestone, and §27.6 forbids a fake arm.
//!
//! The first condition, incidence, IS computable now, and
//! [`incidence_signature`] computes it. It is published as the executed half
//! rather than folded into the refusal, because "one of four conditions holds"
//! and "the certificate exists" are different statements and the second does
//! not follow from the first. REVIEW_M1 M1-N5 is the standing reminder of what
//! that gap costs: a scene there satisfied every combinatorial invariant and
//! still had no planar embedding.

use serde::Serialize;

use super::audit::AuditReport;
use super::Dcel;
use crate::continuation::EditKind;

/// What a committed transaction proves about the topology.
///
/// Every field is an integer read off two arrangements. Nothing here is a
/// score, a bound or a likelihood, and that is deliberate: §32 rule 14 forbids
/// a topology decision made on a proxy, so the object acceptance is decided
/// against carries no proxy to be made on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopologyCertificate {
    pub edit: &'static str,
    pub components_before: u32,
    pub holes_before: u32,
    pub components_after: u32,
    pub holes_after: u32,
    /// `V - B + L` on each side. The Euler identity is `= 2C`; publishing both
    /// sides lets a reader check the arithmetic rather than trust it.
    pub euler_lhs_before: i64,
    pub euler_lhs_after: i64,
    pub skeleton_components_before: u32,
    pub skeleton_components_after: u32,
    /// Junction incidence, before and after: the number of vertices of degree
    /// greater than two, i.e. the JUNCTIONS, and the sorted degree multiset.
    ///
    /// It used to be `(vertices, sum of degrees)`, which REVIEW_M5_A N6 proved
    /// is algebraically `(|V|, 2|B|)` for every arrangement without exception -
    /// each boundary increments exactly two entries - so it was a paraphrase of
    /// two counts the `AuditReport` already published and could not distinguish
    /// two arrangements with equal `|V|` and `|B|` and different degree
    /// distributions, which is precisely what "junction incidence" means. The
    /// per-vertex degrees were computed and then discarded.
    pub junctions_before: u32,
    pub junctions_after: u32,
    pub degrees_before: Vec<u32>,
    pub degrees_after: Vec<u32>,
    /// The isotopy certificate §12 asks for when a chain is replaced by a
    /// curve. Not produced: see [`IsotopyRefusal`].
    pub isotopy: IsotopyRefusal,
}

/// Why no isotopy certificate accompanies this transaction.
///
/// Constructed only by [`isotopy_refusal`], which refuses to build an
/// unexplained one — the same guard [`crate::continuation::not_yet_applicable`]
/// carries, applied to the constructor this milestone adds rather than only to
/// the one where the rule was first needed (REVIEW_M3_5 M35-N5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IsotopyRefusal {
    pub what: &'static str,
    pub conditions_evaluated: Vec<&'static str>,
    pub missing: Vec<&'static str>,
    pub owner_milestone: &'static str,
    pub reason: &'static str,
}

/// The only constructor.
///
/// # Panics
///
/// When `missing` is empty. A refusal that does not say what is missing is a
/// placeholder with better manners (§27.6).
pub fn isotopy_refusal(
    what: &'static str,
    conditions_evaluated: &[&'static str],
    missing: &[&'static str],
    owner_milestone: &'static str,
    reason: &'static str,
) -> IsotopyRefusal {
    assert!(
        !missing.is_empty(),
        "an isotopy refusal must name what is missing: an unexplained refusal is a placeholder \
         with better manners (spec 27.6)"
    );
    IsotopyRefusal {
        what,
        conditions_evaluated: conditions_evaluated.to_vec(),
        missing: missing.to_vec(),
        owner_milestone,
        reason,
    }
}

/// The standing refusal for curve replacement.
pub fn curve_replacement_isotopy() -> IsotopyRefusal {
    isotopy_refusal(
        "topology-preserving curve replacement",
        &["endpoints and junction incidence unchanged"],
        &[
            "fitted_curve",
            "certified_tubular_neighbourhood",
            "tube_disjointness",
            "robust_curve_intersection_predicates",
        ],
        "M6",
        "spec 12 permits replacing a boundary chain by a fitted curve only under four \
         conditions. The first is combinatorial and is evaluated and published here; the other \
         three are statements about a CURVE, and spec 14 fits the first curve in M6. Spec 12 also \
         says what happens when the sufficient isotopy condition is not proved: the operation \
         requires a full arrangement rebuild and verification, which is exactly what \
         dcel::transaction::apply does",
    )
}

/// The sorted multiset of vertex degrees, from the boundary endpoints.
///
/// This is what §12's first isotopy condition compares across an edit. The sum
/// of these is `2|B|` unconditionally and therefore carries nothing; the
/// multiset does (REVIEW_M5_A N6).
pub fn degree_multiset(d: &Dcel) -> Vec<u32> {
    let mut degree = vec![0u32; d.vertices().len()];
    for b in d.boundaries() {
        degree[b.start.index()] += 1;
        degree[b.end.index()] += 1;
    }
    degree.sort_unstable();
    degree
}

/// Vertices of degree greater than two: the JUNCTIONS.
///
/// In a binary labelling every lattice point has even degree — 0, 2 or 4 — so
/// this counts the critical 2x2 cells of §5.3 that survived as vertices.
pub fn junction_count(d: &Dcel) -> u32 {
    degree_multiset(d).iter().filter(|k| **k > 2).count() as u32
}

/// Build the certificate for a committed transaction.
pub fn topology_certificate(
    base: &Dcel,
    candidate: &Dcel,
    kind: EditKind,
    base_audit: &AuditReport,
    cand_audit: &AuditReport,
) -> TopologyCertificate {
    TopologyCertificate {
        edit: kind.as_str(),
        components_before: base_audit.foreground_faces,
        holes_before: base_audit.holes,
        components_after: cand_audit.foreground_faces,
        holes_after: cand_audit.holes,
        euler_lhs_before: i64::from(base_audit.vertices) - i64::from(base_audit.boundaries)
            + i64::from(base_audit.loops),
        euler_lhs_after: i64::from(cand_audit.vertices) - i64::from(cand_audit.boundaries)
            + i64::from(cand_audit.loops),
        skeleton_components_before: base_audit.skeleton_components,
        skeleton_components_after: cand_audit.skeleton_components,
        junctions_before: junction_count(base),
        junctions_after: junction_count(candidate),
        degrees_before: degree_multiset(base),
        degrees_after: degree_multiset(candidate),
        isotopy: curve_replacement_isotopy(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unexplained isotopy refusal is not constructible.
    #[test]
    #[should_panic(expected = "must name what is missing")]
    fn an_isotopy_refusal_with_no_missing_capability_is_a_bug() {
        let _ = isotopy_refusal("something", &[], &[], "M6", "because");
    }

    /// The standing refusal names four conditions, one evaluated and three
    /// missing, and the owner is the milestone that fits the first curve.
    /// Counted so that "the executable part" cannot quietly become zero, and
    /// so that the refusal cannot quietly become empty.
    #[test]
    fn the_isotopy_refusal_names_one_evaluated_condition_and_three_missing() {
        let r = curve_replacement_isotopy();
        assert_eq!(r.conditions_evaluated.len(), 1);
        assert_eq!(
            r.missing.len(),
            3 + 1,
            "three curve conditions plus the curve itself"
        );
        assert_eq!(r.owner_milestone, "M6");
        assert!(r.reason.contains("full arrangement rebuild"));
    }
}
