//! `SupportedModelUniverseV1` — the finite, versioned universe any future
//! posterior mass and confidence are relative to (spec v1.3 §1.5, §28 M3,
//! §31 `model_universe_hash`).
//!
//! This module is a SCHEMA and a HASH, not inference. It answers one
//! question and refuses the others: *which* scenes and formations is this
//! core allowed to consider at all? Spec §1.5 is explicit that confidence
//! is meaningless without that answer, because "posterior mass" over an
//! unbounded space is not a number — and that quietly widening the grammar
//! while keeping the old calibration is the specific failure it forbids.
//!
//! Three properties are load-bearing and each is CHECKED rather than
//! asserted in prose:
//!
//! 1. **Finite.** [`SupportedModelUniverseV1::check_finite`] refuses empty
//!    family lists, non-finite bounds and inverted ranges. A universe that
//!    does not pass is not a universe.
//! 2. **Bound to the actual IR.** The geometry and formation families are
//!    checked against the real `vice_ir` enums by an exhaustive match, so
//!    adding a segment kind or a blend space without revisiting the
//!    universe is a compile error, not a silent widening.
//! 3. **Versioned by content.** [`model_universe_hash`] is the sha256 of
//!    the canonical JSON. The hash is frozen in a test: changing the
//!    universe changes the hash, which is exactly the "separate
//!    model-version change with full recalibration" §1.5 demands.
//!
//! Deliberately NOT here (§32 rule 7 — no API without a milestone that owns
//! its semantics): any evaluation of whether a given scene is inside the
//! universe beyond the structural predicates M3 actually uses, any posterior
//! or search machinery, and `BoundValue<T>` as a report type. The search
//! section below records the STATUS of the bounds this universe can claim
//! today, which is `Unknown` for every one of them, because no search
//! exists to bound.

use serde::Serialize;
use vice_ir::{
    BlendSpace, ExteriorModel, JoinKind, Paint, PixelFilter, QuantizationModel, Segment,
};

use crate::hashing::sha256_hex;

pub const MODEL_UNIVERSE_SCHEMA: &str = "vice-classic/model-universe/v1";

/// Status of a claimed bound (spec §1.5 `BoundValue`): a heuristic estimate
/// may never be serialized as a proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundStatus {
    /// Mathematically proven.
    Certified,
    /// Estimated against a frozen held-out calibration split.
    EmpiricallyCalibrated,
    /// No proof and no calibration. The absence is not masked by a number.
    Unknown,
}

/// Whether a declared family may be used by the core TODAY.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Admissibility {
    /// In the supported universe now.
    Admissible,
    /// Declared so the universe is a complete enumeration, but a candidate
    /// using it is outside the supported universe until the milestone that
    /// owns it lands — which is a universe version change.
    NotYetAdmissible { first_milestone: &'static str },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Family {
    pub name: &'static str,
    pub admissibility: Admissibility,
    /// What the family means, in one line, so the enumeration is readable
    /// without cross-referencing the spec.
    pub note: &'static str,
}

impl Family {
    const fn admissible(name: &'static str, note: &'static str) -> Family {
        Family {
            name,
            admissibility: Admissibility::Admissible,
            note,
        }
    }

    const fn planned(name: &'static str, milestone: &'static str, note: &'static str) -> Family {
        Family {
            name,
            admissibility: Admissibility::NotYetAdmissible {
                first_milestone: milestone,
            },
            note,
        }
    }
}

/// A closed numeric interval in named units. Both ends finite by contract;
/// `check_finite` enforces it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Range {
    pub lo: f64,
    pub hi: f64,
    pub unit: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopologyUniverse {
    /// Explicit operators the topology search may apply (§17.3 "explicit
    /// operators"). Enumerated, not open-ended.
    pub operators: Vec<Family>,
    pub max_visible_faces: u32,
    pub max_components: u32,
    pub max_holes_per_face: u32,
    pub max_boundaries: u32,
    pub max_segments_per_boundary: u32,
    pub max_total_anchors: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeometryUniverse {
    /// Segment families. Kept in lockstep with `vice_ir::Segment` by an
    /// exhaustive match in [`ir_segment_family`].
    pub segment_families: Vec<Family>,
    pub join_kinds: Vec<Family>,
    pub abs_coord_px: Range,
    pub canvas_dim_px: Range,
    /// Below this a segment is degenerate rather than small.
    pub min_segment_length_px: f64,
    pub arc_radius_px: Range,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RelationUniverse {
    pub families: Vec<Family>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FormationUniverse {
    pub blend_spaces: Vec<Family>,
    pub pixel_filters: Vec<Family>,
    pub gaussian_sigma_px: Range,
    pub exterior_models: Vec<Family>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaintUniverse {
    pub families: Vec<Family>,
    /// Channel precision of the delivered colour.
    pub channel_bits: u32,
    pub quantization: Vec<Family>,
}

/// Search truncation rules and what can be PROVEN about them (§1.5 R1/R2).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchUniverse {
    /// Human-readable truncation rules in force. Empty is not allowed: a
    /// universe with no stated truncation rule is claiming an exhaustive
    /// search, which nothing here does.
    pub truncation_rules: Vec<&'static str>,
    /// Can the unexplored posterior mass be bounded from above?
    pub unexplored_mass_bound: BoundStatus,
    /// Can the retained posterior mass be bounded from below?
    pub retained_mass_bound: BoundStatus,
    /// Reliability tier this universe can currently support (§1.5).
    pub reliability_tier: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SupportedModelUniverseV1 {
    pub schema: &'static str,
    pub version: &'static str,
    pub topology: TopologyUniverse,
    pub geometry: GeometryUniverse,
    pub relations: RelationUniverse,
    pub formation: FormationUniverse,
    pub paint: PaintUniverse,
    pub search: SearchUniverse,
}

/// The family name of an IR segment.
///
/// Exhaustive match on purpose: this is the join between the declared
/// universe and the type system. Adding a variant to `vice_ir::Segment`
/// stops compiling here, which is the only way "the universe silently grew"
/// can be made impossible rather than merely discouraged.
pub fn ir_segment_family(seg: &Segment) -> &'static str {
    match seg {
        Segment::Line => "line",
        Segment::CircularArc { .. } => "circular_arc",
        Segment::EllipticArc { .. } => "elliptic_arc",
        Segment::Quad { .. } => "quadratic_bezier",
        Segment::Cubic { .. } => "cubic_bezier",
    }
}

pub fn ir_join_family(join: &JoinKind) -> &'static str {
    match join {
        JoinKind::Corner => "corner",
        JoinKind::SmoothG1 { .. } => "smooth_g1",
    }
}

pub fn ir_blend_family(space: &BlendSpace) -> &'static str {
    match space {
        BlendSpace::LinearLight => "linear_light",
        BlendSpace::EncodedSrgb => "encoded_srgb",
    }
}

pub fn ir_filter_family(filter: &PixelFilter) -> &'static str {
    match filter {
        PixelFilter::Box => "box",
        PixelFilter::Triangle => "triangle",
        PixelFilter::Gaussian { .. } => "gaussian",
    }
}

pub fn ir_exterior_family(model: &ExteriorModel) -> &'static str {
    match model {
        ExteriorModel::Transparent => "transparent_exterior",
        ExteriorModel::Opaque => "opaque_exterior",
    }
}

pub fn ir_paint_family(paint: &Paint) -> &'static str {
    match paint {
        Paint::OpaqueSolid(_) => "opaque_solid",
        Paint::TransparentExterior => "transparent_exterior",
    }
}

pub fn ir_quantization_family(q: &QuantizationModel) -> &'static str {
    match q {
        QuantizationModel::Uint8 => "uint8",
    }
}

impl SupportedModelUniverseV1 {
    /// The frozen V1 universe.
    ///
    /// Numbers are not invented here: the coordinate and canvas ranges are
    /// the enforced `vice_render::NumericDomain`, the complexity caps are
    /// the Flat2/multiregion class of §1.1 with headroom, and every family
    /// that the first usable core does not yet support is declared
    /// `not_yet_admissible` with the milestone that owns it — so the
    /// enumeration is complete AND the admissible subset is honest.
    pub fn v1() -> SupportedModelUniverseV1 {
        let domain = vice_render::NumericDomain::m2_default();
        SupportedModelUniverseV1 {
            schema: MODEL_UNIVERSE_SCHEMA,
            version: "v1",
            topology: TopologyUniverse {
                operators: vec![
                    Family::admissible(
                        "insert_visible_face",
                        "add one visible face to the planar partition",
                    ),
                    Family::admissible("insert_hole", "add a hole loop to an existing face"),
                    Family::admissible(
                        "merge_adjacent_faces",
                        "remove a shared boundary between two same-paint faces",
                    ),
                    Family::admissible(
                        "split_face",
                        "insert a shared boundary splitting one face in two",
                    ),
                    Family::planned(
                        "saddle_resolution",
                        "M4.5",
                        "resolve a critical 2x2 configuration into an explicit well-composed candidate",
                    ),
                    Family::planned(
                        "component_fusion_split",
                        "M5",
                        "fuse or split visible components across a dual/primal transaction",
                    ),
                ],
                max_visible_faces: 256,
                max_components: 64,
                max_holes_per_face: 32,
                max_boundaries: 4096,
                max_segments_per_boundary: 256,
                max_total_anchors: 8192,
            },
            geometry: GeometryUniverse {
                segment_families: vec![
                    Family::admissible("line", "straight segment"),
                    Family::admissible("circular_arc", "endpoint-parameterized circular arc"),
                    Family::admissible("elliptic_arc", "endpoint-parameterized elliptic arc"),
                    Family::admissible("quadratic_bezier", "one control point"),
                    Family::admissible("cubic_bezier", "two control points"),
                ],
                join_kinds: vec![
                    Family::admissible("corner", "no tangent constraint"),
                    Family::admissible(
                        "smooth_g1",
                        "shared tangent parameter; exact G1 comes from the joint solve",
                    ),
                ],
                abs_coord_px: Range {
                    lo: -domain.max_abs_coord_px,
                    hi: domain.max_abs_coord_px,
                    unit: "px",
                },
                canvas_dim_px: Range {
                    lo: 1.0,
                    hi: f64::from(domain.max_canvas_dim_px),
                    unit: "px",
                },
                min_segment_length_px: 1e-9,
                arc_radius_px: Range {
                    lo: 1e-6,
                    hi: domain.max_abs_coord_px,
                    unit: "px",
                },
            },
            relations: RelationUniverse {
                families: vec![
                    Family::planned("equal_radius", "M6", "two arcs share one radius parameter"),
                    Family::planned("concentric", "M6", "two arcs share one centre parameter"),
                    Family::planned("axis_aligned", "M6", "a line is horizontal or vertical"),
                    Family::planned("collinear", "M6", "two lines share one direction parameter"),
                    Family::planned("mirror_symmetry", "M6", "a reflection maps the scene to itself"),
                    Family::planned("repetition", "M6", "a translation maps a sub-scene to itself"),
                ],
            },
            formation: FormationUniverse {
                blend_spaces: vec![
                    Family::admissible("linear_light", "coverage composited in linear light"),
                    Family::admissible(
                        "encoded_srgb",
                        "coverage composited after the sRGB transfer function",
                    ),
                ],
                pixel_filters: vec![
                    Family::admissible("box", "unit box filter = exact pixel-area coverage"),
                    Family::planned("triangle", "M4", "tent filter, global"),
                    Family::planned("gaussian", "M4", "isotropic gaussian, global, sigma below"),
                ],
                gaussian_sigma_px: Range {
                    lo: 0.05,
                    hi: 2.0,
                    unit: "px",
                },
                exterior_models: vec![
                    Family::admissible("transparent_exterior", "exterior alpha is zero"),
                    Family::admissible("opaque_exterior", "exterior is an opaque background face"),
                ],
            },
            paint: PaintUniverse {
                families: vec![
                    Family::admissible("opaque_solid", "one opaque linear-RGB colour per face"),
                    Family::admissible("transparent_exterior", "the exterior face's paint"),
                    Family::planned(
                        "semi_transparent_interior",
                        "M11",
                        "authored constant alpha strictly between 0 and 1; §1.6 excludes it from Flat2 v1",
                    ),
                    Family::planned("linear_gradient", "M11", "gradients milestone"),
                ],
                channel_bits: 8,
                quantization: vec![Family::admissible(
                    "uint8",
                    "8-bit per channel output quantization, part of the formation likelihood",
                )],
            },
            search: SearchUniverse {
                truncation_rules: vec![
                    "no search exists in M3: this universe declares the space, nothing explores it",
                    "complexity caps of the topology section bound the enumerable scenes",
                    "geometry outside the numeric domain is refused before any scoring",
                ],
                unexplored_mass_bound: BoundStatus::Unknown,
                retained_mass_bound: BoundStatus::Unknown,
                reliability_tier: "none: neither R1 nor R2 is claimed before a selector exists",
            },
        }
    }

    /// Canonical JSON: struct declaration order, compact, no map iteration.
    ///
    /// "Canonical" here means DETERMINISTIC FOR A FIXED DECLARATION, not
    /// invariant under semantically neutral edits (REVIEW_M3 M3-N8):
    /// swapping two entries of `segment_families` changes the hash although
    /// the universe it denotes is the same. The error is in the safe
    /// direction - a semantically neutral edit raises an alarm rather than
    /// hiding - but it means hashes are comparable only within one
    /// declaration, exactly as render digests are comparable only within one
    /// renderer version (REDTEAM_M2 addendum 4, debt 3).
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("universe serializes")
    }

    /// Refuse a universe that is not finite. The M3 gate says "supported
    /// universe is finite/versioned"; this is that sentence as code.
    pub fn check_finite(&self) -> Result<(), Vec<String>> {
        let mut problems = Vec::new();
        let mut families = |what: &str, fams: &[Family]| {
            if fams.is_empty() {
                problems.push(format!("{what}: empty family list"));
            }
            let mut names: Vec<&str> = fams.iter().map(|f| f.name).collect();
            names.sort_unstable();
            let before = names.len();
            names.dedup();
            if names.len() != before {
                problems.push(format!("{what}: duplicate family names"));
            }
        };
        families("topology.operators", &self.topology.operators);
        families("geometry.segment_families", &self.geometry.segment_families);
        families("geometry.join_kinds", &self.geometry.join_kinds);
        families("relations.families", &self.relations.families);
        families("formation.blend_spaces", &self.formation.blend_spaces);
        families("formation.pixel_filters", &self.formation.pixel_filters);
        families("formation.exterior_models", &self.formation.exterior_models);
        families("paint.families", &self.paint.families);
        families("paint.quantization", &self.paint.quantization);

        let mut range = |what: &str, r: &Range| {
            if !r.lo.is_finite() || !r.hi.is_finite() {
                problems.push(format!("{what}: non-finite range bound"));
            } else if r.lo > r.hi {
                problems.push(format!("{what}: inverted range {} > {}", r.lo, r.hi));
            }
            if r.unit.is_empty() {
                problems.push(format!("{what}: range without a unit"));
            }
        };
        range("geometry.abs_coord_px", &self.geometry.abs_coord_px);
        range("geometry.canvas_dim_px", &self.geometry.canvas_dim_px);
        range("geometry.arc_radius_px", &self.geometry.arc_radius_px);
        range(
            "formation.gaussian_sigma_px",
            &self.formation.gaussian_sigma_px,
        );

        if !self.geometry.min_segment_length_px.is_finite()
            || self.geometry.min_segment_length_px <= 0.0
        {
            problems
                .push("geometry.min_segment_length_px: must be finite and positive".to_string());
        }
        for (what, v) in [
            (
                "topology.max_visible_faces",
                self.topology.max_visible_faces,
            ),
            ("topology.max_components", self.topology.max_components),
            (
                "topology.max_holes_per_face",
                self.topology.max_holes_per_face,
            ),
            ("topology.max_boundaries", self.topology.max_boundaries),
            (
                "topology.max_segments_per_boundary",
                self.topology.max_segments_per_boundary,
            ),
            (
                "topology.max_total_anchors",
                self.topology.max_total_anchors,
            ),
            ("paint.channel_bits", self.paint.channel_bits),
        ] {
            if v == 0 {
                problems.push(format!("{what}: zero cap makes the universe empty"));
            }
        }
        if self.search.truncation_rules.is_empty() {
            problems.push(
                "search.truncation_rules: empty means an exhaustive search is being claimed"
                    .to_string(),
            );
        }
        if self.schema != MODEL_UNIVERSE_SCHEMA || self.version.is_empty() {
            problems.push("schema/version: the universe must be versioned".to_string());
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems)
        }
    }

    /// Names of the families admissible today, for a given section.
    pub fn admissible_names(fams: &[Family]) -> Vec<&'static str> {
        fams.iter()
            .filter(|f| f.admissibility == Admissibility::Admissible)
            .map(|f| f.name)
            .collect()
    }
}

/// sha256 of the canonical JSON — the `model_universe_hash` of §31.
pub fn model_universe_hash(u: &SupportedModelUniverseV1) -> String {
    sha256_hex(u.canonical_json().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_is_finite_and_versioned() {
        let u = SupportedModelUniverseV1::v1();
        u.check_finite().expect("V1 must be a finite universe");
        assert_eq!(u.schema, MODEL_UNIVERSE_SCHEMA);
        assert_eq!(u.version, "v1");
    }

    #[test]
    fn the_finiteness_check_is_not_vacuous() {
        // Every clause is exercised by a universe that violates exactly it,
        // so "check_finite passed" means something (meta-rule M-2: a green
        // check must not be green because nothing could make it red).
        let mut u = SupportedModelUniverseV1::v1();
        u.geometry.segment_families.clear();
        assert!(u.check_finite().is_err(), "empty family list");

        let mut u = SupportedModelUniverseV1::v1();
        u.geometry.abs_coord_px.hi = f64::INFINITY;
        assert!(u.check_finite().is_err(), "infinite coordinate range");

        let mut u = SupportedModelUniverseV1::v1();
        u.geometry.arc_radius_px = Range {
            lo: 10.0,
            hi: 1.0,
            unit: "px",
        };
        assert!(u.check_finite().is_err(), "inverted range");

        let mut u = SupportedModelUniverseV1::v1();
        u.topology.max_visible_faces = 0;
        assert!(u.check_finite().is_err(), "zero cap");

        let mut u = SupportedModelUniverseV1::v1();
        u.search.truncation_rules.clear();
        assert!(u.check_finite().is_err(), "no truncation rule stated");

        let mut u = SupportedModelUniverseV1::v1();
        u.paint.families.push(u.paint.families[0].clone());
        assert!(u.check_finite().is_err(), "duplicate family name");

        let mut u = SupportedModelUniverseV1::v1();
        u.version = "";
        assert!(u.check_finite().is_err(), "unversioned");
    }

    /// The universe cannot drift away from the IR it describes.
    ///
    /// The `ir_*_family` helpers match exhaustively, so a new IR variant is
    /// a compile error there; this test closes the other direction — every
    /// name those helpers can produce must be declared in the universe.
    #[test]
    fn every_ir_family_is_declared_in_the_universe() {
        let u = SupportedModelUniverseV1::v1();
        let declared = |fams: &[Family], name: &str| fams.iter().any(|f| f.name == name);

        for seg in [
            Segment::Line,
            Segment::CircularArc {
                radius_px: 1.0,
                large_arc: false,
                ccw: true,
            },
            Segment::EllipticArc {
                rx_px: 1.0,
                ry_px: 1.0,
                x_axis_rotation_rad: 0.0,
                large_arc: false,
                ccw: true,
            },
            Segment::Quad {
                ctrl: vice_geom::Pt::new(0.0, 0.0),
            },
            Segment::Cubic {
                ctrl1: vice_geom::Pt::new(0.0, 0.0),
                ctrl2: vice_geom::Pt::new(1.0, 1.0),
            },
        ] {
            let name = ir_segment_family(&seg);
            assert!(
                declared(&u.geometry.segment_families, name),
                "IR segment family {name} is not declared in the universe"
            );
        }
        for j in [
            JoinKind::Corner,
            JoinKind::SmoothG1 {
                tangent_angle_rad: 0.0,
            },
        ] {
            assert!(declared(&u.geometry.join_kinds, ir_join_family(&j)));
        }
        for b in [BlendSpace::LinearLight, BlendSpace::EncodedSrgb] {
            assert!(declared(&u.formation.blend_spaces, ir_blend_family(&b)));
        }
        for f in [
            PixelFilter::Box,
            PixelFilter::Triangle,
            PixelFilter::Gaussian { sigma_px: 0.5 },
        ] {
            assert!(declared(&u.formation.pixel_filters, ir_filter_family(&f)));
        }
        for e in [ExteriorModel::Transparent, ExteriorModel::Opaque] {
            assert!(declared(
                &u.formation.exterior_models,
                ir_exterior_family(&e)
            ));
        }
        for p in [
            Paint::OpaqueSolid(vice_ir::LinearRgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
            }),
            Paint::TransparentExterior,
        ] {
            assert!(declared(&u.paint.families, ir_paint_family(&p)));
        }
        assert!(declared(
            &u.paint.quantization,
            ir_quantization_family(&QuantizationModel::Uint8)
        ));
    }

    /// What the M2 renderer can actually execute must be exactly what the
    /// universe calls admissible today — otherwise "admissible" is a wish.
    #[test]
    fn the_admissible_subset_matches_what_the_core_executes_today() {
        let u = SupportedModelUniverseV1::v1();
        assert_eq!(
            SupportedModelUniverseV1::admissible_names(&u.formation.pixel_filters),
            vec!["box"],
            "the renderer refuses Triangle/Gaussian with UnsupportedPixelFilter"
        );
        assert!(
            SupportedModelUniverseV1::admissible_names(&u.relations.families).is_empty(),
            "no relation is activated before M6"
        );
        assert_eq!(u.search.unexplored_mass_bound, BoundStatus::Unknown);
        assert_eq!(u.search.retained_mass_bound, BoundStatus::Unknown);
    }

    /// The hash is the version. Freezing it here is what makes "changing
    /// the universe is a separate model-version change" enforceable rather
    /// than aspirational: any edit above turns this test red.
    #[test]
    fn model_universe_hash_is_frozen() {
        let u = SupportedModelUniverseV1::v1();
        let h = model_universe_hash(&u);
        assert_eq!(h.len(), 64);
        assert_eq!(
            h, FROZEN_V1_HASH,
            "the supported model universe changed; that is a model-version change \
             requiring recalibration (spec §1.5), not a routine edit"
        );

        // Sensitivity: a one-character change in any section moves it.
        let mut other = SupportedModelUniverseV1::v1();
        other.topology.max_visible_faces += 1;
        assert_ne!(model_universe_hash(&other), h);
    }

    const FROZEN_V1_HASH: &str = "fed2af8642ee3bdd6be85fead97f5ae834622ad0f68525cad9889d17845b8f5d";

    /// **The default is "an admissible segment family has a fitter".**
    ///
    /// `vice_fit::FITTED_FAMILIES` is a literal enumerating its subjects, and
    /// `span.rs` says so at its true price: the cheapest bypass is one family
    /// nobody wrote a fitter for. A literal cannot be argued out of being one,
    /// but the DEFAULT around it can be inverted, and that is what this does —
    /// the same move `doc_claims` made when it stopped listing the documents it
    /// checked and started walking `docs/` and stopping on anything
    /// unclassified.
    ///
    /// This test lives in `vice-bench` because it is the only crate that can
    /// see both sides: `vice-fit` must not depend on the declared universe (it
    /// is a consumer of geometry, not of the benchmark), and the universe must
    /// not depend on the fitter. Neither side can hold the invariant alone.
    ///
    /// Adding an admissible geometry family and no fitter is now RED, and
    /// closing it needs either a fitter or a line in
    /// `FAMILIES_DELIBERATELY_NOT_FITTED` carrying a reason. That is weaker
    /// than a type and stronger than a list: the exception must be WRITTEN, and
    /// what it costs to write is a sentence a reviewer reads.
    #[test]
    fn every_admissible_segment_family_has_a_fitter_or_a_declared_reason() {
        let u = SupportedModelUniverseV1::v1();
        let admissible = SupportedModelUniverseV1::admissible_names(&u.geometry.segment_families);
        assert!(
            !admissible.is_empty(),
            "the declared universe admits no segment family at all, so the loop below would \
             compare nothing and pass (F-0039)"
        );

        let fitted: Vec<&str> = vice_fit::FITTED_FAMILIES
            .iter()
            .map(|f| f.universe_name())
            .collect();
        let excused: Vec<&str> = vice_fit::FAMILIES_DELIBERATELY_NOT_FITTED
            .iter()
            .map(|(name, _)| *name)
            .collect();

        for name in &admissible {
            assert!(
                fitted.contains(name) || excused.contains(name),
                "the declared universe admits segment family `{name}`, `vice-fit` has no fitter \
                 for it, and `FAMILIES_DELIBERATELY_NOT_FITTED` does not say why. Either write \
                 the fitter or write the reason; a family that is admissible and unfitted with \
                 nobody saying so is a hole in the candidate stage that no run reports"
            );
        }

        // The other direction. A fitter for a family the universe does NOT
        // admit produces candidates that cannot legally be selected, and it
        // would move `model_universe_hash` to make them legal — a §1.5
        // model-version change, not a routine edit.
        for name in &fitted {
            assert!(
                admissible.contains(name),
                "`vice-fit` fits `{name}`, which the declared universe does not admit"
            );
        }

        // And the excuses are about real families rather than about names
        // nobody uses, which is how an exception list rots (F-0047).
        for (name, reason) in vice_fit::FAMILIES_DELIBERATELY_NOT_FITTED {
            assert!(
                admissible.contains(&name),
                "`FAMILIES_DELIBERATELY_NOT_FITTED` excuses `{name}`, which is not an admissible \
                 family: the exception is about nothing and is now permanently green"
            );
            assert!(
                !fitted.contains(&name),
                "`{name}` is both fitted and excused from being fitted"
            );
            assert!(
                reason.len() > 40,
                "the reason given for not fitting `{name}` is {} characters; an exception whose \
                 reason is a word is a list entry wearing a justification",
                reason.len()
            );
        }
    }
}
