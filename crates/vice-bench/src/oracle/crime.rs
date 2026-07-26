//! Inverse-crime contamination as a lattice that survives aggregation
//! (spec §27.1, §28 M3.5 gate "inverse-crime warning visible").
//!
//! M3 applied the inverse-crime discipline to RENDERS: the production
//! renderer is a flagged corpus profile and its renders are dropped from the
//! reliability sample. The oracle suite needs the discipline on the other
//! side too, because an oracle arm has TWO sides — the observation it is
//! judged against, and the reference backend that produced the model — and
//! either of them can be the system's own forward model.
//!
//! Two design rules, both learned rather than invented:
//!
//! 1. **Derived, never passed.** `RenderOutcome.inverse_crime` was a plain
//!    `bool` set by the caller, and REVIEW_M3 M3-N5 showed a value could be
//!    smuggled past the filter by constructing the struct by hand. Here the
//!    status is computed from the profile TYPES by [`InverseCrime::of`], and
//!    there is no constructor that takes a status.
//! 2. **The fold is the only combiner.** A warning that has to be copied
//!    upwards by hand is a warning that gets lost at the first aggregation
//!    someone adds later. [`InverseCrime::fold`] is the only way to combine,
//!    every aggregate in this crate is built by it, and `Clean` is the
//!    identity — so an aggregate over a contaminated arm cannot come out
//!    clean.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::gt::raster::RasterProfile;

/// Why one arm's residual measures shared assumptions rather than accuracy.
///
/// Each variant names a distinct mechanism, because a report that says only
/// "inverse crime" cannot be acted on: the fix differs per reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrimeReason {
    /// The reference backend IS the production forward model (§27.1).
    BackendIsProductionRenderer,
    /// The observation was produced by the production forward model.
    ObservationIsProductionRenderer,
    /// Backend and observation are the same implementation, so the residual
    /// is the serialization step alone and says nothing about the renderer.
    SharedImplementation,
}

impl CrimeReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            CrimeReason::BackendIsProductionRenderer => "backend_is_production_renderer",
            CrimeReason::ObservationIsProductionRenderer => "observation_is_production_renderer",
            CrimeReason::SharedImplementation => "shared_implementation",
        }
    }

    /// The sentence a report prints. Written so a reader who has never seen
    /// §27.1 still knows what the number is worth.
    pub fn warning(&self) -> &'static str {
        match self {
            CrimeReason::BackendIsProductionRenderer => {
                "INVERSE CRIME: the reference backend is the production forward model \
                 (vice-render). The residual bounds the system against itself and is not \
                 evidence about accuracy (spec 27.1)."
            }
            CrimeReason::ObservationIsProductionRenderer => {
                "INVERSE CRIME: the observation was produced by the production forward model \
                 (vice-render), so it is a diagnostic image and never ground truth (spec 27.1)."
            }
            CrimeReason::SharedImplementation => {
                "INVERSE CRIME: model and observation come from the SAME rasterizer \
                 implementation, so this residual isolates serialization and says nothing \
                 about the renderer."
            }
        }
    }
}

/// Contamination status of one arm or of any aggregate over arms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum InverseCrime {
    /// Neither side shares an implementation with the system under test.
    Clean,
    /// At least one side does; the reasons are carried, not summarised away.
    Contaminated { reasons: BTreeSet<CrimeReason> },
}

impl InverseCrime {
    /// The status of one arm, DERIVED from the two profiles.
    ///
    /// Walking the class rather than the obvious case: contamination can
    /// enter through the backend, through the observation, or through the
    /// two being the same implementation. All three are checked here and
    /// each has its own test.
    pub fn of(backend: RasterProfile, observation: RasterProfile) -> InverseCrime {
        let mut reasons = BTreeSet::new();
        if backend.is_inverse_crime() {
            reasons.insert(CrimeReason::BackendIsProductionRenderer);
        }
        if observation.is_inverse_crime() {
            reasons.insert(CrimeReason::ObservationIsProductionRenderer);
        }
        if backend == observation {
            reasons.insert(CrimeReason::SharedImplementation);
        }
        if reasons.is_empty() {
            InverseCrime::Clean
        } else {
            InverseCrime::Contaminated { reasons }
        }
    }

    pub fn is_contaminated(&self) -> bool {
        matches!(self, InverseCrime::Contaminated { .. })
    }

    /// The join of the lattice, and the ONLY combiner. `Clean` is the
    /// identity, so a fold that meets one contaminated arm stays
    /// contaminated no matter how many clean arms follow.
    pub fn fold(&self, other: &InverseCrime) -> InverseCrime {
        match (self, other) {
            (InverseCrime::Clean, InverseCrime::Clean) => InverseCrime::Clean,
            (InverseCrime::Contaminated { reasons }, InverseCrime::Clean)
            | (InverseCrime::Clean, InverseCrime::Contaminated { reasons }) => {
                InverseCrime::Contaminated {
                    reasons: reasons.clone(),
                }
            }
            (
                InverseCrime::Contaminated { reasons: a },
                InverseCrime::Contaminated { reasons: b },
            ) => InverseCrime::Contaminated {
                reasons: a.union(b).copied().collect(),
            },
        }
    }

    /// Fold a whole sequence. Empty folds to `Clean`, which is correct: an
    /// aggregate over no arms carries no contamination because it carries
    /// no measurement either.
    pub fn fold_all<'a>(items: impl Iterator<Item = &'a InverseCrime>) -> InverseCrime {
        items.fold(InverseCrime::Clean, |acc, x| acc.fold(x))
    }

    /// The warnings this status obliges a report to print, in a stable
    /// order. Empty exactly when clean.
    pub fn warnings(&self) -> Vec<&'static str> {
        match self {
            InverseCrime::Clean => Vec::new(),
            InverseCrime::Contaminated { reasons } => reasons.iter().map(|r| r.warning()).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each of the three mechanisms is detected on its own, and a clean
    /// pairing exists — without which every later assertion would be true
    /// for the wrong reason (meta-rule M-2).
    #[test]
    fn contamination_is_derived_from_the_profiles_by_every_mechanism() {
        let clean = InverseCrime::of(RasterProfile::ExactClip, RasterProfile::TinySkia);
        assert_eq!(clean, InverseCrime::Clean);
        assert!(clean.warnings().is_empty());

        let backend = InverseCrime::of(RasterProfile::ViceRender, RasterProfile::TinySkia);
        assert!(backend.is_contaminated());
        assert!(matches!(&backend, InverseCrime::Contaminated { reasons }
            if reasons.contains(&CrimeReason::BackendIsProductionRenderer)
                && !reasons.contains(&CrimeReason::SharedImplementation)));

        let observation = InverseCrime::of(RasterProfile::ExactClip, RasterProfile::ViceRender);
        assert!(
            matches!(&observation, InverseCrime::Contaminated { reasons }
            if reasons.contains(&CrimeReason::ObservationIsProductionRenderer))
        );

        let shared = InverseCrime::of(RasterProfile::ExactClip, RasterProfile::ExactClip);
        assert!(matches!(&shared, InverseCrime::Contaminated { reasons }
            if reasons.contains(&CrimeReason::SharedImplementation)
                && reasons.len() == 1));

        // Both production sides at once: all three reasons, none dropped.
        let both = InverseCrime::of(RasterProfile::ViceRender, RasterProfile::ViceRender);
        match &both {
            InverseCrime::Contaminated { reasons } => assert_eq!(reasons.len(), 3),
            other => panic!("{other:?}"),
        }
    }

    /// The property the gate depends on: one contaminated arm among many
    /// clean ones contaminates the fold, in EITHER order, and the reason
    /// survives.
    #[test]
    fn one_contaminated_arm_contaminates_the_whole_fold() {
        let clean = InverseCrime::Clean;
        let dirty = InverseCrime::of(RasterProfile::ViceRender, RasterProfile::TinySkia);

        let many_then_one: Vec<&InverseCrime> = vec![&clean, &clean, &clean, &dirty];
        let one_then_many: Vec<&InverseCrime> = vec![&dirty, &clean, &clean, &clean];
        for order in [many_then_one, one_then_many] {
            let folded = InverseCrime::fold_all(order.into_iter());
            assert!(folded.is_contaminated(), "the fold lost the contamination");
            assert_eq!(folded.warnings().len(), 1);
            assert!(folded.warnings()[0].contains("INVERSE CRIME"));
        }

        // And the control: all clean folds to clean, so the assertion above
        // is not true of every input.
        let all_clean = InverseCrime::fold_all([&clean, &clean, &clean].into_iter());
        assert_eq!(all_clean, InverseCrime::Clean);
        assert!(InverseCrime::fold_all(std::iter::empty()) == InverseCrime::Clean);
    }

    /// Reasons UNION rather than replace: two different mechanisms in one
    /// aggregate must both reach the reader.
    #[test]
    fn folding_unions_the_reasons_instead_of_keeping_the_first() {
        let a = InverseCrime::of(RasterProfile::ViceRender, RasterProfile::TinySkia);
        let b = InverseCrime::of(RasterProfile::ExactClip, RasterProfile::ExactClip);
        let f = a.fold(&b);
        match &f {
            InverseCrime::Contaminated { reasons } => {
                assert!(reasons.contains(&CrimeReason::BackendIsProductionRenderer));
                assert!(reasons.contains(&CrimeReason::SharedImplementation));
                assert_eq!(reasons.len(), 2);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(f.warnings().len(), 2);
    }
}
