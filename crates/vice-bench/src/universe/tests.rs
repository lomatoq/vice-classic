//! Tests for the supported model universe.
//!
//! Split out of `universe.rs` in M6 when promoting four relation families past
//! §4.1's 800-line cap. The seam is the same one `gates/mod.rs` used: the
//! SCHEMA and its hash on one side, the judges on the other.

use super::*;
use vice_ir::{
    BlendSpace, ExteriorModel, JoinKind, Paint, PixelFilter, QuantizationModel, Segment,
};

#[test]
fn v1_is_finite_and_versioned() {
    let u = SupportedModelUniverseV1::v1();
    u.check_finite().expect("V1 must be a finite universe");
    assert_eq!(u.schema, MODEL_UNIVERSE_SCHEMA);
    assert_eq!(u.version, "v1");
}

#[test]
fn m7_is_a_distinct_finite_r1_model_version() {
    let m6 = SupportedModelUniverseV1::v1();
    let m7 = SupportedModelUniverseV1::m7();
    m7.check_finite().expect("M7 must be a finite universe");
    assert_eq!(m7.schema, MODEL_UNIVERSE_SCHEMA);
    assert_eq!(m7.version, "m7-v4");
    assert_ne!(model_universe_hash(&m7), model_universe_hash(&m6));
    assert_eq!(
        m7.search.unexplored_mass_bound,
        BoundStatus::EmpiricallyCalibrated
    );
    assert_eq!(
        m7.search.retained_mass_bound,
        BoundStatus::EmpiricallyCalibrated
    );
    assert!(m7.search.reliability_tier.starts_with("R1 "));
    assert!(
        m7.search
            .reliability_tier
            .contains("no search-certified R2"),
        "the empirical tier must not imply certified completeness"
    );
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
    // M6 activates all six §15 relation families. Parallel/perpendicular share
    // one universe prefix but remain two typed constrained hypotheses.
    assert_eq!(
        SupportedModelUniverseV1::admissible_names(&u.relations.families),
        vec![
            "equal_radius",
            "concentric",
            "parallel_perpendicular",
            "shared_baseline",
            "mirror_symmetry",
            "repeated_transforms"
        ],
        "the admissible relation set is what `vice_fit::RelationKind` generates hypotheses for"
    );
    assert_eq!(
        SupportedModelUniverseV1::admissible_names(&u.geometry.loop_primitives),
        vice_fit::LoopPrimitiveKind::ALL
            .iter()
            .map(|kind| kind.universe_name())
            .collect::<Vec<_>>(),
        "the admissible whole-loop set is exactly what vice-fit generates"
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

/// **A §1.5 model-version change, recorded rather than merely applied.**
///
/// §1.5: "Изменение universe — отдельная model-version change с полной
/// recalibration; нельзя молча расширить grammar и сохранить старый
/// confidence threshold."
///
/// WAS `fdcd283a…7359`. IS `47903d73…f097`.
///
/// The final M6 closure admits the previously deferred `mirror_symmetry` and
/// `repeated_transforms` families and binds selected constrained geometry.
/// The hash below is re-frozen by the corresponding §27.7 gate-only commit.
///
/// **What recalibration is owed, and by whom.** §1.5 attaches confidence
/// thresholds and search-mass bounds to the universe version. This tree has
/// neither: `search.unexplored_mass_bound` and `search.retained_mass_bound`
/// are both `BoundStatus::Unknown` (asserted two tests above), and no
/// confidence threshold exists — §28 M7 owns the selective-delivery
/// calibration. So the recalibration debt this change creates is not a
/// number that must be re-measured now; it is that **M7 must calibrate
/// against `47903d73…`, not against `fdcd283a…`**, and a calibration
/// inherited across this hash would be a calibration for a smaller grammar.
/// That is the whole of the obligation and it is stated here so M7 does not
/// have to reconstruct it.
const FROZEN_V1_HASH: &str = "47903d7374d54683e60c318239d75adabcc2eef5fc80ad9d7822e8176990f097";

/// M7 is a separate model version. `m7-v4` also binds the declared closure
/// edge in closed-chain isotopy certification. It is frozen before its first
/// calibration; no confidence value from an earlier M7 version is carried.
const FROZEN_M7_HASH: &str = "4b16559803e03689be485ad0269738b82b5897f8d42ed528ebcbeb3bb5efc914";

#[test]
fn m7_model_universe_hash_is_frozen() {
    let h = model_universe_hash(&SupportedModelUniverseV1::m7());
    println!("M7_MODEL_UNIVERSE_SHA256={h}");
    assert_eq!(
        h, FROZEN_M7_HASH,
        "the M7 universe changed; mint a new model version and recalibrate"
    );
}

#[test]
fn m7_resource_envelope_declaration_matches_executable_constants() {
    let universe = SupportedModelUniverseV1::m7();
    let rules = universe.search.truncation_rules.join("\n");
    assert!(rules.contains("32/64/96/128-sample"));
    assert!(rules.contains(&format!(
        "at most {} paths per level",
        vice_fit::MAX_CERTIFICATION_ATTEMPTS_PER_LEVEL_V1
    )));
    assert!(rules.contains(&format!(
        "at most {} certified models",
        vice_fit::MAX_CERTIFIED_MODELS_PER_CHAIN_V1
    )));
    assert!(rules.contains(&format!(
        "Jacobians use at most {} mandatory",
        vice_fit::PROPOSAL_CONTINUOUS_REFIT_SAMPLE_CAP_V1
    )));
    assert!(rules.contains(&format!(
        "refits use at most {}",
        vice_fit::CONTINUOUS_REFIT_SAMPLE_CAP_V1
    )));
    assert!(rules.contains(&format!(
        "Jacobians use at most {} mandatory",
        vice_fit::RELATION_REFIT_SAMPLE_CAP_V1
    )));
    let quality = vice_core::CoreConfig::development_for(vice_core::Preset::Quality);
    let fast = vice_core::CoreConfig::development_for(vice_core::Preset::Fast);
    assert!(rules.contains(&quality.beam.budget.max_elapsed_ms.to_string()));
    assert!(rules.contains(&fast.beam.budget.max_elapsed_ms.to_string()));
    assert!(rules.contains(&format!(
        "at most {} trust-region rounds with {} backtracks",
        quality.trust_region.max_rounds, quality.trust_region.max_backtracks
    )));
    assert_eq!(
        quality
            .verification
            .render_options
            .budget
            .chord_tolerance
            .px(),
        vice_fit::BINDING_CERTIFICATION_CHORD_TOLERANCE_PX_V1
    );
    assert!(rules.contains("1/64"));
    assert!(rules.contains(&vice_fit::BINDING_RELATION_RESCUE_MARGIN_PX_V1.to_string()));
}

#[test]
fn m7_topology_operators_match_executable_transaction_kinds() {
    use vice_opt::TransactionKind;

    let u = SupportedModelUniverseV1::m7();
    let declared = SupportedModelUniverseV1::admissible_names(&u.topology.operators);
    let executable = [
        TransactionKind::TopologyMerge,
        TransactionKind::TopologySplit,
        TransactionKind::TopologyBridge,
        TransactionKind::TopologyHole,
    ]
    .map(TransactionKind::universe_name)
    .to_vec();
    assert_eq!(declared, executable);
}

#[test]
fn every_m7_transaction_kind_has_a_stable_unique_universe_name() {
    use std::collections::BTreeSet;
    use vice_opt::TransactionKind;

    let names: BTreeSet<_> = TransactionKind::ALL
        .iter()
        .copied()
        .map(TransactionKind::universe_name)
        .collect();
    assert_eq!(names.len(), TransactionKind::ALL.len());
    assert!(names.iter().all(|name| !name.is_empty()));
}

/// **The default is "an admissible relation family has a hypothesis
/// generator"**, the same inverted default
/// `every_admissible_segment_family_has_a_fitter_or_a_declared_reason`
/// applies to geometry.
///
/// Without it, promoting a family to admissible is one line and buys a
/// grammar nothing generates — the widening §1.5 forbids, arriving through
/// the door that is cheapest to open.
#[test]
fn every_admissible_relation_family_has_a_hypothesis_generator() {
    use vice_fit::RelationKind;
    let u = SupportedModelUniverseV1::v1();
    let admissible = SupportedModelUniverseV1::admissible_names(&u.relations.families);
    assert!(
        !admissible.is_empty(),
        "the declared universe admits no relation family, so the loops below compare nothing"
    );
    let mut generated: Vec<&str> = RelationKind::ALL
        .iter()
        .map(|k| k.universe_name())
        .collect();
    generated.sort_unstable();
    generated.dedup();

    for name in &admissible {
        assert!(
            generated.contains(name),
            "the universe admits relation `{name}` and `vice_fit::relation` forms no                  hypothesis for it: the grammar is wider than anything that generates it"
        );
    }
    for name in &generated {
        assert!(
            admissible.contains(name),
            "`vice_fit::relation` forms hypotheses for `{name}`, which the universe does not                  admit; accepting one would put a candidate outside the supported universe"
        );
    }

    // The planned side, named with its owner, so a family cannot sit
    // pointing at a milestone that has finished (which is what M6 inherited
    // for all six of these).
    for f in &u.relations.families {
        if let Admissibility::NotYetAdmissible { first_milestone } = f.admissibility {
            assert_ne!(
                first_milestone, "M6",
                "relation `{}` still names M6 as its first milestone after M6 delivered Stage                      H; either it is admissible or its owner has moved",
                f.name
            );
        }
    }
}

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

/// **The geometry code table is DERIVED, and this is where the derivation is
/// checked**, because `vice-bench` is the only crate that can see the model
/// universe and the frozen identifiability gate at once.
///
/// §14.5 asks for a "parameter code `log2(range / calibrated precision)`" and a
/// "prefix code family". Both are computable from things this repository has
/// already frozen, and neither is a number anyone chose:
///
/// | value | range | precision |
/// |---|---|---|
/// | `bits_per_anchor` | the universe's `canvas_dim_px.hi` | `[identifiability] observability_floor_px`, "smallest salient LENGTH in render px whose parameters stay recoverable" |
/// | `bits_per_segment_family` | the admissible segment families | uniform, no frequency calibration exists |
/// | `bits_per_relation` | the relation families | uniform |
///
/// `vice-fit` holds the values and this holds the derivation, so neither side
/// derives the other (F-0048 Q4). The tolerance is the rounding of the six
/// decimals the gate file carries.
#[test]
fn the_geometry_code_table_agrees_with_the_universe_it_codes_over() {
    use vice_fit::{GEOMETRY_CODE_TABLE_V1 as t, JOIN_KINDS, REFERENCE_CANVAS_DIM_PX};

    let u = SupportedModelUniverseV1::v1();
    assert_eq!(
        REFERENCE_CANVAS_DIM_PX, u.geometry.canvas_dim_px.hi,
        "`vice-fit` states `bits_per_anchor` at a canvas the universe does not declare"
    );
    assert_eq!(
        JOIN_KINDS,
        u.geometry.join_kinds.len(),
        "the join code prices a number of join kinds the universe does not declare"
    );

    // The calibrated precision comes from the gate file, not from a literal
    // here: a second copy of 0.35 would be the guard sharing a key with the
    // mechanism.
    let g = crate::gates::GatesFile::load(std::path::Path::new("../../configs/GATES_V1.toml"))
        .expect("gates load");
    let floor = g
        .doc
        .sections
        .get("identifiability")
        .and_then(|s| s.values.get("observability_floor_px"))
        .and_then(|v| v.as_float())
        .expect("[identifiability] observability_floor_px");

    let want_anchor = 2.0 * (u.geometry.canvas_dim_px.hi / floor).log2();
    assert!(
        (t.bits_per_anchor() - want_anchor).abs() < 5e-6,
        "bits_per_anchor {} against 2 log2({} / {floor}) = {want_anchor}",
        t.bits_per_anchor(),
        u.geometry.canvas_dim_px.hi
    );
    assert!(
        (t.coordinate_precision_px() - floor).abs() < 1e-5,
        "the table inverts to a precision of {} px against the frozen floor of {floor}",
        t.coordinate_precision_px()
    );

    let seg = SupportedModelUniverseV1::admissible_names(&u.geometry.segment_families).len();
    assert!(
        (t.bits_per_segment_family() - (seg as f64).log2()).abs() < 5e-6,
        "bits_per_segment_family {} against log2({seg})",
        t.bits_per_segment_family()
    );
    let rel = u.relations.families.len();
    assert!(
        (t.bits_per_relation() - (rel as f64).log2()).abs() < 5e-6,
        "bits_per_relation {} against log2({rel})",
        t.bits_per_relation()
    );

    // Both directions on the derivation itself: a table of zeros — the
    // placeholder this section shipped as — must not be constructible, or a
    // §14.5 code would price an unbounded grammar at nothing.
    assert!(vice_fit::GeometryCodeTable::new(0.0, 0.0, 0.0).is_none());
}

/// The pricing-surface hash, printed so the freeze commit can quote it and a
/// reviewer can recompute it. Run with `--nocapture`.
#[test]
fn the_pricing_surface_hash_is_printed_for_the_freeze() {
    let surface = vice_fit::pricing_surface_v1();
    let hash = crate::hashing::sha256_hex(surface.as_bytes());
    println!("--- pricing surface v1 ---");
    println!("{surface}");
    println!("sha256 {hash}");
    // The surface enumerates something: 4 families, 16 free-scalar rows.
    assert!(surface.lines().count() > 30, "the surface shrank");
}
