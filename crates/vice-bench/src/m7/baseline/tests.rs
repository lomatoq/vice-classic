use super::*;

#[test]
fn typed_baseline_refusal_is_a_catastrophic_blind_opponent_not_missing_input() {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let gates = GatesFile::load_for_a_gate_decision(&repository.join("configs/GATES_V1.toml"))
        .expect("committed gates load");
    let release = M7ReleaseGates::from_file(&gates).expect("release gates parse");
    let mut row = super::super::tests::synthetic_row("typed-baseline-refusal");
    row.internal_baseline_refusals = vec![
        "Preseal: curve separation cannot be certified at the requested tessellation budget".into(),
    ];
    assert!(baseline_row_catastrophic(&row, release));
    let metrics = baseline_metrics(&[&row], release);
    assert_eq!(metrics.catastrophic, 1);
    assert!(metrics.boundary_p95_sum > release.boundary_p95_px);
    assert!(metrics.boundary_p99_sum > release.boundary_p99_px);
    assert!(metrics.boundary_max > release.boundary_max_px);
    assert_ne!(
        baseline_bundle_digest(&[&row]),
        bundle_digest([].into_iter())
    );
}

#[test]
fn blind_judge_is_symmetric_under_presentation_swap() {
    let better = BlindMetrics {
        catastrophic: 0,
        boundary_p95_sum: 1.0,
        boundary_p99_sum: 2.0,
        boundary_max: 3.0,
        palette_sum: 0,
        curve_segments: 4,
        delivery_bytes: 100,
    };
    let worse = BlindMetrics {
        boundary_p95_sum: 2.0,
        ..better.clone()
    };
    assert_eq!(judge_blind(&better, &worse), BlindChoice::Left);
    assert_eq!(judge_blind(&worse, &better), BlindChoice::Right);
}

#[test]
fn exact_binomial_tail_has_known_values() {
    assert!((one_sided_binomial_tail(10, 10) - 1.0 / 1024.0).abs() < 1e-12);
    assert!((one_sided_binomial_tail(0, 10) - 1.0).abs() < 1e-12);
}
