use super::*;

#[test]
fn smoke_court_runs_both_multicolor_families_through_exact_delivery() {
    let report = measure_court(M8CourtScope::Smoke, 4).unwrap();
    assert!(report.source_groups >= 3);
    assert!(report
        .rows
        .iter()
        .any(|row| row.shape_family == "dot_cluster"));
    assert!(report
        .rows
        .iter()
        .any(|row| row.shape_family == "triple_junction"));
    assert_eq!(report.model_universe_sha256.len(), 64);
    for family in ["dot_cluster", "triple_junction"] {
        assert!(report
            .rows
            .iter()
            .any(|row| row.shape_family == family && row.accepted));
    }
    assert!(report.rows.iter().filter(|row| !row.accepted).all(|row| {
        row.refusal
            .as_deref()
            .is_some_and(|reason| reason.contains("refused"))
    }));
}

#[test]
fn clustered_split_never_places_one_cluster_in_both_courts() {
    for variant in 0..100 {
        assert_ne!(
            M8CourtScope::Calibration.admits_variant(variant),
            M8CourtScope::SealedAudit.admits_variant(variant)
        );
    }
}

#[test]
fn formal_population_has_three_origins_and_disjoint_source_groups() {
    let calibration = eligible_court_population(M8CourtScope::Calibration, 20).unwrap();
    let sealed = eligible_court_population(M8CourtScope::SealedAudit, 20).unwrap();
    for population in [&calibration, &sealed] {
        let origins = population
            .iter()
            .map(|group| group.origin.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            origins,
            BTreeSet::from(["procedural", "authored", "adversarial"])
        );
    }
    let calibration_ids = calibration
        .iter()
        .map(|group| group.id.as_str())
        .collect::<BTreeSet<_>>();
    assert!(sealed
        .iter()
        .all(|group| !calibration_ids.contains(group.id.as_str())));
}

#[test]
fn both_formal_splits_exercise_independent_sources_through_exact_delivery() {
    for scope in [M8CourtScope::Calibration, M8CourtScope::SealedAudit] {
        let report = measure_court_shard(scope, 1, 0, 1).unwrap();
        for origin in ["authored", "adversarial"] {
            let rows = report
                .rows
                .iter()
                .filter(|row| row.fixture_origin == origin)
                .collect::<Vec<_>>();
            assert!(!rows.is_empty(), "{scope:?} has no {origin} source");
            assert!(
                rows.iter().any(|row| row.accepted),
                "{scope:?} accepts no {origin} source: {rows:#?}"
            );
        }
    }
}

#[test]
fn a_self_declared_459_row_release_is_not_a_formal_court() {
    let forged = M8CourtReport {
        schema: M8_COURT_SCHEMA.into(),
        scope: M8CourtScope::SealedAudit,
        procedural_generation: M8_PROCEDURAL_GENERATION,
        variants_per_family: M8_VARIANTS_PER_FAMILY,
        cluster_size: M8_CLUSTER_SIZE,
        profile: M8CourtScope::SealedAudit.profile().as_str().into(),
        shard_index: 0,
        shard_count: M8_FORMAL_SHARDS,
        included_shards: (0..M8_FORMAL_SHARDS).collect(),
        candidate_sha: "a".repeat(40),
        runner_sha256: "b".repeat(64),
        execution_ids: (0..M8_FORMAL_SHARDS)
            .map(|index| format!("forged-{index}"))
            .collect(),
        model_universe_sha256: vice_opt::model_universe_hash(
            &vice_opt::SupportedModelUniverseV1::m8(),
        ),
        exact_config_sha256: exact_config_digest(&court_exact_config()),
        corpus_commitment_sha256: "c".repeat(64),
        source_groups: 459,
        accepted_groups: 0,
        rows: Vec::new(),
    };
    assert!(validate_formal_header(&forged, M8CourtScope::SealedAudit).is_err());
}

#[test]
fn merge_refuses_reused_execution_attestations() {
    let shard = |index| M8CourtReport {
        schema: M8_COURT_SCHEMA.into(),
        scope: M8CourtScope::Smoke,
        procedural_generation: M8_PROCEDURAL_GENERATION,
        variants_per_family: 1,
        cluster_size: M8_CLUSTER_SIZE,
        profile: M8CourtScope::Smoke.profile().as_str().into(),
        shard_index: index,
        shard_count: 2,
        included_shards: vec![index],
        candidate_sha: "a".repeat(40),
        runner_sha256: "b".repeat(64),
        execution_ids: vec!["same-execution".into()],
        model_universe_sha256: "c".repeat(64),
        exact_config_sha256: "d".repeat(64),
        corpus_commitment_sha256: format!("{index:064x}"),
        source_groups: 0,
        accepted_groups: 0,
        rows: Vec::new(),
    };
    assert!(merge_courts(vec![shard(0), shard(1)]).is_err());
}
