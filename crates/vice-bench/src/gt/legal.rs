//! The ONE public door to a corpus fixture, and the opaque handles it hands
//! out.
//!
//! ## Why this module exists, and why the previous two answers were not enough
//!
//! The door has now been closed six times, and the first five all failed the
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
//! ```text
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
//! ## The residual, at the price it actually costs
//!
//! ONE description, here, and STATUS_M4_5 points at this one rather than
//! carrying a second (condition 27). Two earlier versions of this paragraph
//! were wrong in the same direction - they priced the residual too high - and
//! the decision not to close the class rested on that price, which is why the
//! wording is now the cheapest form anyone has demonstrated rather than the
//! first form that came to mind.
//!
//! What is NOT closed, in the cheapest known form:
//!
//! - ONE `pub fn` in an already-declared module, with no new type, returning
//!   `Vec<u8>` from `rgba8()`, `Vec<Vec<f64>>` from coverage, or a `String` from
//!   a foreign crate's `Debug`. Measured: 22 of 22 sealed-audit groups and
//!   258 048 bytes (RT45-A19, M45-N23). No visibility modifier reaches this:
//!   the type seal ends at the first type this project does not declare, and
//!   `Vec<u8>` is that type.
//!
//! What answers it instead, because types cannot:
//! `hygiene.rs::only_declared_modules_call_the_render_pipeline` derives the
//! pipeline by return type and declares WHICH modules may call it. That is a
//! declared set and therefore reviewable, which is weaker than a compiler error
//! and is said here rather than implied.
//!
//! The source scan survives as a second echelon and says so in its own doc
//! comment. It models a habit. This module is the proof.

use crate::gt::degradation::{render_cell, DegradationCell, RenderedFixture, ResizeChain};
use crate::gt::raster::{rasterize, CoverageStack, Psf, RasterProfile, ViewTransform};
use crate::gt::split::{Split, SPLIT_POLICY_V1};
use crate::gt::{GtScene, GtSourceGroup};
use vice_ir::BlendSpace;

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

    /// The rasterizer profiles a frozen coefficient may be measured with, and
    /// the ONLY source of a [`LegalProfile`] in the workspace.
    ///
    /// This is the second clause of D1, and until delta-3 it was guarded by
    /// nothing (RT45-A15). The split policy holds `tiny-skia` out of
    /// `development` entirely, because §28 M4 asks whether the system
    /// generalizes to an engine the calibration did NOT see — and a
    /// coefficient measured with it has seen its own test.
    pub fn legal_profiles(&self) -> Vec<LegalProfile> {
        legal_raster_profiles()
            .into_iter()
            .map(LegalProfile)
            .collect()
    }

    /// The cells of the frozen §27.2 matrix a frozen coefficient may use.
    ///
    /// Three of the five frozen measurements used to pick a cell out of
    /// `matrix_v1()` by a literal id, and swapping ONE of those literals to
    /// `s64_ptiny-skia_…` measured a frozen `vice-evidence` coefficient with
    /// the held-out engine while nine hygiene tests stayed green (RT45-A15).
    /// They select from HERE now, so the same literal resolves to nothing.
    pub fn legal_cells(&self) -> Vec<LegalCell> {
        let legal = legal_raster_profiles();
        crate::gt::degradation::matrix_v1()
            .into_iter()
            .filter(|c| legal.contains(&c.profile))
            .map(|cell| LegalCell { cell })
            .collect()
    }
}

/// The rasterizer profiles the split policy leaves in `development`.
///
/// Derived from the policy, never listed: `held_out_profiles` is the frozen
/// declaration and this asks it, so an engine held out tomorrow is excluded
/// tomorrow without editing this module.
fn legal_raster_profiles() -> Vec<RasterProfile> {
    RasterProfile::ALL
        .iter()
        .copied()
        .filter(|p| SPLIT_POLICY_V1.profile_allowed(Split::Development, p.as_str()))
        .collect()
}

/// A rasterizer profile a frozen coefficient may be measured with.
///
/// Opaque, and the only way to obtain one is
/// [`FrozenPopulation::legal_profiles`]. That is the whole mechanism: the
/// legality of a profile becomes a property of the HANDLE rather than a habit
/// of the caller, so `rasterize(&t, RasterProfile::TinySkia, Psf::Box)` through
/// the legal door is a type error rather than a silent measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegalProfile(RasterProfile);

impl LegalProfile {
    pub fn as_str(&self) -> &'static str {
        self.0.as_str()
    }

    /// Does this profile draw INDEPENDENTLY of our own integrator?
    ///
    /// Delegated to `RasterProfile`, deliberately, and the delegation is the
    /// finding: porting the frozen measurements to this handle, I replaced the
    /// call to `is_independent_engine()` with a list of three engine names and
    /// put `vice-render` — our OWN renderer — in it. The measurement then
    /// pooled a self-comparison at `RMS 0.00000` and the clean-bucket noise
    /// scale fell from 25.57 codes to 18.079. A frozen coefficient would have
    /// been diluted by a comparison of the renderer with itself.
    ///
    /// F-0042 in one move, committed while fixing F-0042: a property became a
    /// list, and the list was wrong. The property is asked, never restated.
    pub fn is_independent_engine(&self) -> bool {
        self.0.is_independent_engine()
    }
}

/// A degradation cell a frozen coefficient may be measured at.
///
/// Its profile is a [`LegalProfile`], so there is no way to name an illegal one
/// — not by literal id, not by constructing the cell field by field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LegalCell {
    cell: DegradationCell,
}

impl LegalCell {
    /// The frozen §27.2 id of this cell.
    pub fn id(&self) -> String {
        self.cell.id()
    }

    pub fn size_px(&self) -> u32 {
        self.cell.size_px
    }

    /// Build a cell at a LEGAL profile. The only constructor, and it cannot be
    /// handed anything else, because `LegalProfile` has no public constructor.
    pub fn at(
        profile: &LegalProfile,
        size_px: u32,
        psf: Psf,
        blend: BlendSpace,
        resize: ResizeChain,
        contrast: f64,
    ) -> LegalCell {
        LegalCell {
            cell: DegradationCell {
                size_px,
                subpixel_dx: 0.0,
                subpixel_dy: 0.0,
                profile: profile.0,
                psf,
                blend,
                resize,
                contrast,
            },
        }
    }
}

/// What a render of a legal scene hands out: exactly the three quantities the
/// five frozen measurements consume, and nothing that reaches the corpus.
///
/// RT45-A14: sealing `GtSourceGroup` closed what the corpus IS and left what it
/// PRODUCES public. `RenderedFixture` had every field `pub` and `render_cell`
/// was `pub(crate)`, so six lines in an already-declared module handed an
/// integration test 63 renders over all 22 sealed-audit groups — no new type,
/// no re-implemented pipeline. The currency of F-0026 is the render, not the
/// group, so the seal now stands on the render.
pub struct LegalRender {
    fixture: RenderedFixture,
}

impl LegalRender {
    pub fn width_px(&self) -> u32 {
        self.fixture.width_px
    }

    pub fn height_px(&self) -> u32 {
        self.fixture.height_px
    }

    pub fn rgba8(&self) -> &[u8] {
        &self.fixture.rgba8
    }
}

/// The same for coverage: per-face coverage and the dimensions, nothing else.
pub struct LegalCoverage {
    stack: CoverageStack,
}

impl LegalCoverage {
    pub fn width_px(&self) -> u32 {
        self.stack.width_px
    }

    pub fn height_px(&self) -> u32 {
        self.stack.height_px
    }

    pub fn per_face(&self) -> &[Vec<f64>] {
        &self.stack.per_face
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

    /// Render this scene under one LEGAL degradation cell.
    pub fn render(&self, cell: &LegalCell) -> Result<LegalRender, String> {
        render_cell(self.scene, &cell.cell, 1).map(|fixture| LegalRender { fixture })
    }

    /// Rasterize this scene's certified mesh under one LEGAL profile and a PSF.
    pub fn rasterize(
        &self,
        t: &ViewTransform,
        profile: &LegalProfile,
        psf: Psf,
    ) -> Result<LegalCoverage, String> {
        rasterize(self.scene.certified(), t, profile.0, psf).map(|stack| LegalCoverage { stack })
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

    /// The SECOND clause of D1, which until delta-3 was guarded by nothing.
    ///
    /// `corridor::frozen_calibration_groups` states D1 as two clauses: the
    /// sealed audit is never touched, AND the held-out rasterizer is never
    /// seen. The first was closed by the constructor's refusal; the second was
    /// a habit of two callers out of five, and swapping ONE literal cell id in
    /// an existing frozen measurement measured a frozen `vice-evidence`
    /// coefficient with `tiny-skia` while nine hygiene tests stayed green
    /// (RT45-A15). Both halves are measured here.
    #[test]
    fn no_legal_profile_or_cell_carries_the_held_out_engine() {
        let held_out = SPLIT_POLICY_V1.held_out_profiles;

        // RT45-A20: the checks BELOW compare `!held_out.contains(p.as_str())`,
        // which is the same string join the mechanism itself computes, so they
        // measure consistency rather than the property. Renaming
        // `RasterProfile::TinySkia => "tinyskia"` made the held-out engine a
        // legal profile and six legal cells while that guard stayed green.
        //
        // These two do not go through `as_str` at all:
        //
        //   - every held-out NAME must resolve to a variant that exists. A name
        //     nobody can resolve holds nothing out, and nothing checked that.
        //   - the legal set and the held-out set must PARTITION the enum. A
        //     rename moves a variant from one side to the other without
        //     changing either count, and the sum stops matching.
        for h in held_out {
            assert!(
                RasterProfile::from_id(h).is_some(),
                "the split policy holds out {h:?}, which resolves to no RasterProfile variant. A                  held-out name that names nothing holds nothing out (RT45-A20)"
            );
        }
        assert_eq!(
            RasterProfile::ALL.len(),
            legal_raster_profiles().len() + held_out.len(),
            "the legal profiles and the held-out profiles do not partition RasterProfile::ALL:              {} legal + {} held out against {} variants. A variant that is in neither is legal by              accident, and a rename is how it gets there (RT45-A20)",
            legal_raster_profiles().len(),
            held_out.len(),
            RasterProfile::ALL.len()
        );

        assert!(
            !held_out.is_empty(),
            "the split policy holds no profile out, so this test would pass on a policy that              cannot leak - the clause it guards would have nothing to guard"
        );
        let pop = frozen_calibration_population().expect("the legal population");

        let profiles = pop.legal_profiles();
        assert!(!profiles.is_empty(), "no profile is legal at all");
        for p in &profiles {
            assert!(
                !held_out.contains(&p.as_str()),
                "the held-out engine {} is offered as a LEGAL profile",
                p.as_str()
            );
        }

        // And the cell selector cannot name one either. This is the exact
        // literal the red team swapped in.
        let cells = pop.legal_cells();
        assert!(!cells.is_empty(), "no cell is legal at all");
        assert!(
            cells
                .iter()
                .any(|c| c.id() == "s64_praqote_box_lin_none_dx0.00dy0.00_c1.00"),
            "the legal cell set lost the cell a frozen measurement actually uses, so the check              below would pass for the wrong reason"
        );
        assert!(
            !cells
                .iter()
                .any(|c| c.id() == "s64_ptiny-skia_box_lin_none_dx0.00dy0.00_c1.00"),
            "the held-out engine is reachable by literal cell id through the legal handle"
        );
        for c in &cells {
            for h in held_out {
                assert!(
                    !c.id().contains(h),
                    "legal cell {} names the held-out engine {h}",
                    c.id()
                );
            }
        }
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
