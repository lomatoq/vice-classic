//! The ONE public door to a corpus fixture, and the opaque handles it hands
//! out.
//!
//! ## Why this module exists, and why the previous two answers were not enough
//!
//! The door has now been closed four times, and the first three all failed the
//! same way — a check that READS THE SOURCE cannot enumerate the ways a value
//! leaves a crate:
//!
//! | attempt | mechanism | how it was walked around |
//! |---|---|---|
//! | M4 C135 | one legal population + a scan for two literals | `use … as …` |
//! | M4.5 C154 | `pub(crate)` on those two names + the same scan | `authored_groups()` and `all_adversarial_groups()` were still `pub` — 12 groups, 2 sealed |
//! | M4.5 C170 | seal every accessor, scan every `pub fn` by its RETURN TEXT | `pub type Fixtures = Vec<GtSourceGroup>` and `pub struct Basket { pub groups: Vec<GtSourceGroup> }` — **60 groups, ALL 22 sealed-audit**, with `fmt` clean, `clippy -D warnings` silent and all seven hygiene tests green |
//!
//! The third attempt is the one worth dwelling on: it was strictly better than
//! its predecessor and still worse than the original defect, because the new
//! doors reached the WHOLE wide population instead of a fifth of it. A type
//! alias, a public field, a trait item, `impl Trait`, a closure, a `static`, a
//! `Deref` — a text model will not list them, and the list is the defect.
//!
//! ## Two questions, not one
//!
//! The fourth attempt (C188) made `GtSourceGroup`, `GtScene` and
//! `AmbiguityPair` `pub(crate)` and denied `private_interfaces` /
//! `private_bounds`, so ANY public item that would hand one out — under any of
//! those syntaxes — is a compile error rather than a lint a scan has to
//! notice. That answers **what may cross the boundary**, and the compiler
//! answers it.
//!
//! It did NOT answer **how wide the thing that crosses is**, and this handle
//! was the proof: `FrozenPopulation::new` was `pub(crate)`, and `pub(crate)`
//! restricts nobody inside `vice-bench`. Four lines in any module of the crate
//!
//! ```ignore
//! pub fn wide() -> FrozenPopulation {
//!     FrozenPopulation::new(crate::gt::corpus::all_groups().unwrap())
//! }
//! ```
//!
//! handed an integration test **60 groups, 22 of them sealed-audit, 63
//! scenes**, against the legal 22 and 24 — measured, not argued, and the same
//! number REVIEW_M4_5 addendum 2 reproduced in a clean clone. A door named
//! after the legal population that carries the whole corpus is worse than an
//! obviously wide door, because it reads correct.
//!
//! ## What closes the second question
//!
//! 1. There is no `new`. The field is private and [`FrozenPopulation::of`] is
//!    private to THIS module, so the only mint site in the workspace is
//!    [`frozen_calibration_population`] below, twelve lines away and reviewed
//!    with it. Same for [`FrozenScene`], whose field is private.
//! 2. `of` REFUSES. It asks the split policy about every group it is given and
//!    returns an error naming the first one that is not `development`. That is
//!    not a scan and not a list: mint a handle from the wide corpus and it
//!    fails, wherever the call is written and whatever it is called.
//!
//! ## The residual, stated because it is not closed
//!
//! A future module inside `vice-bench` can still declare its OWN public struct
//! with a private `Vec<GtSourceGroup>` field and re-implement render and
//! rasterize on it. That compiles, and no mechanism here stops it: a crate
//! boundary would, but the corpus and the harnesses that legitimately walk it
//! (`corridor`, `oracle`, `topology`) live in one crate by construction, and
//! splitting them would have to make `GtSourceGroup` public in the new crate,
//! which is strictly worse. What this module closes is every EXISTING path and
//! the two ways to widen this handle; what it leaves open is writing a second
//! handle from scratch, which is a new public API and shows up as one.
//!
//! The source scan survives as a second echelon and says so in its own doc
//! comment. It models a habit. This module is the proof.

use crate::gt::degradation::{render_cell, DegradationCell, RenderedFixture};
use crate::gt::raster::{rasterize, CoverageStack, Psf, RasterProfile, ViewTransform};
use crate::gt::split::{Split, SPLIT_POLICY_V1};
use crate::gt::{GtScene, GtSourceGroup};

/// The legal calibration population, as an opaque handle.
///
/// The only value of this type in the workspace comes from
/// [`frozen_calibration_population`]. There is no method that returns a
/// `GtSourceGroup`, so a caller outside the crate cannot widen the population
/// it was given, and no caller inside the crate can mint a wider one.
pub struct FrozenPopulation {
    groups: Vec<GtSourceGroup>,
}

impl FrozenPopulation {
    /// Private to this module, and it refuses.
    ///
    /// Both halves matter. Private is what makes this the only mint site;
    /// refusing is what makes a wide mint fail even here, so the guarantee
    /// does not rest on the twelve lines below staying correct.
    fn of(groups: Vec<GtSourceGroup>) -> Result<FrozenPopulation, String> {
        let policy = &SPLIT_POLICY_V1;
        if let Some(bad) = groups
            .iter()
            .find(|g| policy.split_of_group(g) != Split::Development)
        {
            return Err(format!(
                "a frozen measurement was handed group {} from the {} split; only `development` \
                 may set a frozen coefficient (spec §27.1, REVIEW_M4_5 condition 14, RT45-A9)",
                bad.id,
                policy.split_of_group(bad).as_str()
            ));
        }
        Ok(FrozenPopulation { groups })
    }

    /// Independent source groups in the population.
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// The FIRST scene of every group.
    ///
    /// Four of the five frozen measurements want exactly this: one scene per
    /// independent group, because §27.4 makes the group the unit of trial and
    /// scenes inside one group are not independent of each other.
    pub fn first_scenes(&self) -> Vec<FrozenScene<'_>> {
        self.groups
            .iter()
            .filter_map(|g| g.scenes.first())
            .map(|scene| FrozenScene { scene })
            .collect()
    }

    /// Every scene of every group.
    pub fn all_scenes(&self) -> Vec<FrozenScene<'_>> {
        self.groups
            .iter()
            .flat_map(|g| g.scenes.iter())
            .map(|scene| FrozenScene { scene })
            .collect()
    }
}

/// One scene of the legal population, as an opaque handle.
///
/// It can be RENDERED and RASTERIZED and it can say its own id. It cannot be
/// turned back into a `GtScene`, so nothing downstream of it can reach the
/// group, the split, or the rest of the corpus.
pub struct FrozenScene<'a> {
    scene: &'a GtScene,
}

impl FrozenScene<'_> {
    pub fn id(&self) -> &str {
        self.scene.id()
    }

    /// Render this scene under one degradation cell.
    pub fn render(&self, cell: &DegradationCell) -> Result<RenderedFixture, String> {
        render_cell(self.scene, cell, 1)
    }

    /// Rasterize this scene's certified mesh under one profile and PSF.
    pub fn rasterize(
        &self,
        t: &ViewTransform,
        profile: RasterProfile,
        psf: Psf,
    ) -> Result<CoverageStack, String> {
        rasterize(self.scene.certified(), t, profile, psf)
    }
}

/// The ONE public door to a corpus fixture (REVIEW_M4_5 condition 14,
/// addendum 2; REDTEAM RT45-A9).
///
/// Development groups, development-legal profiles, nothing else — see
/// [`crate::corridor::frozen_calibration_groups`] for why the held-out
/// rasterizer is excluded as well. It is defined HERE, next to the private
/// constructor, rather than in `corridor`, because a mint site that lives in
/// another module needs a `pub(crate)` constructor, and `pub(crate)` is what
/// this whole module exists to stop being enough.
pub fn frozen_calibration_population() -> Result<FrozenPopulation, String> {
    FrozenPopulation::of(crate::corridor::frozen_calibration_groups()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The refusal is a measurement, not a promise.
    ///
    /// This is the C188 attack, written here rather than described: mint the
    /// handle from the wide corpus, the way any module of `vice-bench` could
    /// when the constructor was `pub(crate)`. It reached 60 groups / 22
    /// sealed-audit / 63 scenes. It now returns `Err`.
    #[test]
    fn the_handle_refuses_a_population_that_is_not_development() {
        let wide = crate::gt::corpus::all_groups().expect("the corpus");
        let policy = &SPLIT_POLICY_V1;
        let sealed = wide
            .iter()
            .filter(|g| policy.split_of_group(g) == Split::SealedAudit)
            .count();
        assert!(
            sealed > 0,
            "the corpus has no sealed-audit group at all, so this test would pass on a corpus \
             that cannot leak - the attack it models would have nothing to reach"
        );
        let Err(err) = FrozenPopulation::of(wide) else {
            panic!(
                "the wide corpus was accepted into the frozen-measurement handle; this is RT45-A9 \
                 exactly, and it reached {sealed} sealed-audit groups the last time it was open"
            );
        };
        assert!(
            err.contains("only `development`"),
            "unexpected refusal: {err}"
        );
    }

    /// And the legal population is not refused by the same predicate — a
    /// constructor that rejected everything would pass the test above.
    #[test]
    fn the_legal_population_passes_the_same_predicate() {
        let pop = frozen_calibration_population().expect("the legal population");
        assert!(
            pop.group_count() > 0 && !pop.first_scenes().is_empty(),
            "the one legal door hands out an empty population"
        );
    }
}
