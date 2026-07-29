use vice_evidence::{BoundaryChain, BoundarySample};
use vice_fit::{
    canonical_cuts, k_best_boundary_models, FIT_BUDGET_V1, GATE_MAX_CUT_ROTATION_DELTA_BITS,
    K_DISCRETE_PATHS, MAX_CANONICAL_CUTS,
};
use vice_geom::Pt;

fn irregular_loop(seed: usize) -> Vec<Pt> {
    let count = 7usize;
    let mut vertices = Vec::new();
    for i in 0..count {
        let angle =
            std::f64::consts::TAU * i as f64 / count as f64 + 0.03 * ((seed + i * 3) % 5) as f64;
        let radius = 24.0 + ((seed * 17 + i * 11) % 23) as f64;
        vertices.push(Pt::new(64.0, 64.0) + Pt::new(radius * angle.cos(), radius * angle.sin()));
    }
    let per_edge = 5usize;
    let mut points = Vec::new();
    for i in 0..count {
        let a = vertices[i];
        let b = vertices[(i + 1) % count];
        for j in 0..per_edge {
            points.push(a + (b - a) * (j as f64 / per_edge as f64));
        }
    }
    points
}

fn chain(points: &[Pt]) -> BoundaryChain {
    let n = points.len();
    let mut samples = Vec::new();
    let mut length = 0.0;
    for i in 0..n {
        let prev = points[(i + n - 1) % n];
        let next = points[(i + 1) % n];
        let back = (points[i] - prev).length();
        let forward = (next - points[i]).length();
        let tangent = next - prev;
        let tangent_length = tangent.length();
        let weight_ds = 0.5 * (back + forward);
        length += weight_ds;
        samples.push(BoundarySample {
            p: points[i],
            normal: Pt::new(-tangent.y / tangent_length, tangent.x / tangent_length),
            halfwidth: 0.35,
            confidence: 1.0,
            weight_ds,
            corr_length_px: 1.0,
        });
    }
    BoundaryChain {
        samples,
        closed: true,
        length_px: length,
        corr_length_px: 1.0,
        vertices: n as u64,
    }
}

#[test]
fn canonical_cut_set_tracks_the_same_physical_samples_after_rotation() {
    let points = irregular_loop(0);
    let base = chain(&points);
    let base_points: std::collections::BTreeSet<(u64, u64)> = canonical_cuts(&base)
        .into_iter()
        .map(|cut| {
            let point = base.samples[cut].p;
            (point.x.to_bits(), point.y.to_bits())
        })
        .collect();
    assert!((2..=MAX_CANONICAL_CUTS).contains(&base_points.len()));

    for rotation in [1usize, 4, 9, 17] {
        let rotated: Vec<_> = (0..points.len())
            .map(|i| points[(rotation + i) % points.len()])
            .collect();
        let rotated = chain(&rotated);
        let cut_points: std::collections::BTreeSet<(u64, u64)> = canonical_cuts(&rotated)
            .into_iter()
            .map(|cut| {
                let point = rotated.samples[cut].p;
                (point.x.to_bits(), point.y.to_bits())
            })
            .collect();
        assert_eq!(cut_points, base_points, "rotation {rotation}");
    }
}

#[test]
fn rotated_irregular_loops_select_the_same_model() {
    for seed in 0..5 {
        let points = irregular_loop(seed);
        let mut answers = Vec::new();
        for rotation in [0usize, 1, 4, 9, 17] {
            let rotated: Vec<_> = (0..points.len())
                .map(|i| points[(rotation + i) % points.len()])
                .collect();
            let run =
                k_best_boundary_models(&chain(&rotated), &FIT_BUDGET_V1, 128.0, K_DISCRETE_PATHS)
                    .expect("loop fits");
            let model = &run.models[0];
            answers.push((
                rotation,
                model.families.clone(),
                model.smooth.clone(),
                model.code.total_bits(),
            ));
        }
        let reference = &answers[0];
        for answer in &answers[1..] {
            assert_eq!(
                answer.1, reference.1,
                "seed {seed}, rotations {} and {} select different families; {answers:?}",
                reference.0, answer.0
            );
            assert_eq!(
                answer.2, reference.2,
                "seed {seed}, rotations {} and {} select different joins; {answers:?}",
                reference.0, answer.0
            );
            assert!(
                (answer.3 - reference.3).abs() < GATE_MAX_CUT_ROTATION_DELTA_BITS,
                "seed {seed} rotation {} moved {} -> {} bits; {answers:?}",
                answer.0,
                reference.3,
                answer.3
            );
        }
    }
}
