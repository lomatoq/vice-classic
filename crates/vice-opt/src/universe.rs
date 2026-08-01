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

use sha2::{Digest, Sha256};

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
    /// Whole-loop constrained siblings of the free typed chain (§15).
    pub loop_primitives: Vec<Family>,
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
                loop_primitives: vec![
                    Family::admissible("circle", "closed loop with one centre and radius"),
                    Family::admissible(
                        "ellipse",
                        "closed loop with two radii and an axis angle",
                    ),
                    Family::admissible("rect", "axis-aligned rectangle"),
                    Family::admissible("rotated_rect", "oriented rectangle"),
                    Family::admissible(
                        "rounded_rect",
                        "oriented rectangle with one shared corner radius",
                    ),
                    Family::admissible("capsule", "two semicircles joined by parallel lines"),
                    Family::admissible(
                        "regular_polygon",
                        "regular polygon with side count in 3..=12",
                    ),
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
                    Family::admissible("equal_radius", "two arcs share one radius parameter"),
                    Family::admissible("concentric", "two arcs share one centre parameter"),
                    Family::admissible(
                        "parallel_perpendicular",
                        "two line directions are equal or differ by one quarter turn",
                    ),
                    Family::admissible(
                        "shared_baseline",
                        "two lines lie on the same infinite supporting line",
                    ),
                    Family::admissible(
                        "mirror_symmetry",
                        "a closed typed line loop is projected onto a finite bilateral correspondence",
                    ),
                    Family::admissible(
                        "repeated_transforms",
                        "two typed line spans share one exact translation vector",
                    ),
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

    /// The M7 production universe.
    ///
    /// This is derived from, but does not mutate, the frozen M6 `v1()`
    /// declaration. M7 changes executable topology operators, scene-level
    /// relation generators, and search-bound claims; spec 1.5 requires a new
    /// content hash and full recalibration for those changes.
    pub fn m7() -> SupportedModelUniverseV1 {
        let mut universe = Self::v1();
        // v8 adds the bounded, explicitly priced observed-polyline member and
        // rekeys the palette/fitter/scheduler implementation. Candidate
        // membership, confidence, and unexplored mass are calibrated anew.
        universe.version = "m7-v8";
        universe.topology.operators = vec![
            Family::admissible(
                "topology_merge",
                "atomically replace a complete refit by a lower-component topology arm",
            ),
            Family::admissible(
                "topology_split",
                "atomically replace a complete refit by a higher-component topology arm",
            ),
            Family::admissible(
                "topology_bridge",
                "atomically choose the connected arm of a critical bridge envelope",
            ),
            Family::admissible(
                "topology_hole",
                "atomically insert or remove a hole through a complete topology refit",
            ),
        ];
        universe.relations.families = vec![
            Family::admissible("equal_radius", "two arcs share one radius parameter"),
            Family::admissible("concentric", "two arcs share one centre parameter"),
            Family::admissible(
                "parallel_perpendicular",
                "two line directions are equal or differ by one quarter turn",
            ),
            Family::admissible(
                "shared_baseline",
                "two lines lie on the same infinite supporting line",
            ),
            Family::admissible(
                "mirror_symmetry",
                "independently segmented closed components share one bilateral axis and bidirectional correspondence corridor",
            ),
            Family::admissible(
                "repeated_transforms",
                "independently segmented closed components share one exact translation transform",
            ),
        ];
        universe.search = SearchUniverse {
            truncation_rules: vec![
                "dense chains protect the same k=4 primary path court for Fast and Quality at deterministic 32/64/96/128-sample levels; a certified miss opens one bounded k=16 recovery, search stops after the first level with a certified model, attempts at most 3 paths per level, and retains at most 2 certified models per physical chain; Quality remains wider in topology/formation materialization, beam width, and continuous optimization, and every skipped level/path is reported as unexplored mass",
                "proposal-path Jacobians use at most 64 mandatory-breakpoint-preserving samples and final continuous refits use at most 128; every retained model is physically recoded, Stage-H compared, binding-isotopy checked, and certified in both corridor directions on all observations",
                "when every compact line/arc/Bezier grammar solve refuses, or as an explicitly priced alternative to one that constructs no valid scene, a full-observation line-chain member may retain at most 128 segments; it is simplified inside half the certified corridor and binding headroom, refined until exact non-adjacent line crossings are absent, charged for every retained anchor, segment, corner and residual, and must pass the same full two-sided corridor, binding-isotopy, scene, quantization and delivery certificates",
                "Stage-H relation Jacobians use at most 16 mandatory-breakpoint-preserving samples while relation residual code and both corridor checks use all observations",
                "observed binding tubes include the maximum evidence halfwidth, half a physical sample interval, the frozen 1/64 px verifier tessellation certificate, and one 0.05 px fitter chord certificate; an optional Stage-H sibling outside that tube or more than one fitter chord certificate worse than its certified free sibling falls back to that free sibling",
                "opaque label-swapped mixture surrogates within 1e-9 canonicalize to the border-supported H2 reading while all palette hypotheses remain published",
                "event-level M4.5 contours own topology while the canonical coverage-0.5 contour owns fit geometry whenever it binds bijectively to that topology",
                "elliptic-arc fitting is omitted and charged to empirical unexplored search mass",
                "M4.5 supplies a finite critical-connectivity envelope; arms pruned by the prefit budget are reported as unexplored",
                "the deterministic diverse beam is capped by serialized-materialization work units (Quality 8, Fast 5), candidate and memory budgets; elapsed targets (Quality 8500 ms, Fast 1000 ms) are reported but never decide candidate membership; topology and formation seed quotas are mandatory",
                "continuous scene optimization uses one deterministic evidence-solved start, relation-preserving geometry similarity blocks plus paint blocks, and quantized serialized exact acceptance; Quality uses at most 4 trust-region rounds with 4 backtracks per block and Fast uses at most 2 rounds with 4 backtracks; unreached materializations and optimizer alternatives remain explicit unexplored mass",
                "after exact scoring, a topology or formation candidate whose observed-support isotopy displacement is more than one certified fitter chord worse than the best verified canonical-topology baseline receives a typed support-monotonicity refusal and cannot become a delivery",
                "only hypotheses with equal canonical delivery bytes collapse into one posterior delivery-equivalence class",
                "topology and geometry complexity caps bound every materialized scene",
                "geometry outside the numeric domain is refused before scoring",
            ],
            unexplored_mass_bound: BoundStatus::EmpiricallyCalibrated,
            retained_mass_bound: BoundStatus::EmpiricallyCalibrated,
            reliability_tier:
                "R1 empirical selective reliability; no search-certified R2 completeness claim",
        };
        universe
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
        families("geometry.loop_primitives", &self.geometry.loop_primitives);
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
    hex::encode(Sha256::digest(u.canonical_json().as_bytes()))
}
