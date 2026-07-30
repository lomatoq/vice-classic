use super::*;
use crate::refit::{g1_readings, RefitNode, FEASIBLE_HALFWIDTHS};

fn samples_from(points: &[Pt]) -> Vec<BoundarySample> {
    points
        .iter()
        .map(|p| BoundarySample {
            p: *p,
            normal: Pt::new(0.0, -1.0),
            halfwidth: 0.35,
            confidence: 1.0,
            weight_ds: 1.0,
            corr_length_px: 1.0,
        })
        .collect()
}

/// A chain of two cubics through a smooth node, initialised away from the
/// samples. The solve must reduce the residual AND the result must still be
/// G1-exact — the second is the whole point, and a solve that improved the
/// fit by breaking the join would pass the first alone.
#[test]
fn the_joint_solve_reduces_the_residual_and_keeps_g1_exact() {
    // Samples on a sine, which neither single cubic reproduces.
    let pts: Vec<Pt> = (0..=60)
        .map(|i| {
            let x = i as f64 * 0.5;
            Pt::new(x, 10.0 * (x / 30.0 * std::f64::consts::PI).sin())
        })
        .collect();
    let s = samples_from(&pts);
    let init = RefitChain {
        nodes: vec![
            RefitNode {
                pos: pts[0],
                tangent_rad: None,
            },
            RefitNode {
                pos: pts[30],
                tangent_rad: Some(0.0),
            },
            RefitNode {
                pos: pts[60],
                tangent_rad: None,
            },
        ],
        segments: vec![
            RefitSegment::Cubic {
                head: Handle::Free(pts[10]),
                tail: Handle::Shared { length_px: 5.0 },
            },
            RefitSegment::Cubic {
                head: Handle::Shared { length_px: 5.0 },
                tail: Handle::Free(pts[50]),
            },
        ],
    };
    let out = joint_constrained_refit(&init, &s).expect("feasible");
    println!(
        "residual {:.5} -> {:.5} over {} parameters, pass {} kept, worst d_n {:.5} px",
        out.residual_before,
        out.residual_after,
        out.parameters,
        out.pass_kept,
        out.worst_normal_deviation_px
    );
    assert!(
        out.residual_after < out.residual_before * 0.5,
        "the joint solve moved the residual from {} to {}",
        out.residual_before,
        out.residual_after
    );
    let lowered = out.chain.lower().expect("lowers");
    let worst = g1_readings(&lowered, out.chain.start(), out.chain.end())
        .iter()
        .map(|r| r.spread_rad)
        .fold(0.0f64, f64::max);
    assert!(
        worst < 1e-12,
        "the solve returned a chain with a G1 spread of {worst} rad"
    );
}

/// **§14.3's "path invalid".** A grammar the evidence cannot support is
/// REFUSED with the numbers, not returned with a bad residual.
#[test]
fn a_path_the_evidence_cannot_support_is_refused_with_its_numbers() {
    let pts: Vec<Pt> = (0..=40)
        .map(|i| {
            let a = i as f64 / 40.0 * std::f64::consts::TAU;
            Pt::new(30.0 * a.cos(), 30.0 * a.sin())
        })
        .collect();
    let s = samples_from(&pts);
    // One straight line across a full circle: no parameter can save it.
    let init = RefitChain {
        nodes: vec![
            RefitNode {
                pos: pts[0],
                tangent_rad: None,
            },
            RefitNode {
                pos: pts[40],
                tangent_rad: None,
            },
        ],
        segments: vec![RefitSegment::Line],
    };
    match joint_constrained_refit(&init, &s) {
        Err(RefitRefusal::OutsideCorridor {
            worst_deviation_px,
            allowed_px,
        }) => {
            assert!(worst_deviation_px > allowed_px);
            assert!((allowed_px - FEASIBLE_HALFWIDTHS * 0.35).abs() < 1e-12);
        }
        other => panic!("expected OutsideCorridor, got {other:?}"),
    }
}

#[test]
fn bounded_jacobian_does_not_bound_the_final_corridor_certificate() {
    let points = (0..17)
        .map(|index| {
            if index == 6 {
                Pt::new(index as f64, 8.0)
            } else {
                Pt::new(index as f64, 0.0)
            }
        })
        .collect::<Vec<_>>();
    let samples = samples_from(&points);
    assert!(
        representative_solve_samples(&samples, 4, &[0, 16])
            .iter()
            .all(|sample| sample.p.y == 0.0),
        "the positive control must place the defect outside the Jacobian subset"
    );
    let init = RefitChain {
        nodes: vec![
            RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
            RefitNode {
                pos: Pt::new(16.0, 0.0),
                tangent_rad: None,
            },
        ],
        segments: vec![RefitSegment::Line],
    };
    let refusal = joint_constrained_refit_bounded(&init, &samples, 4, &[0, 16])
        .expect_err("the unsampled spike must still fail full certification");
    assert!(matches!(
        refusal,
        RefitRefusal::OutsideCorridor {
            worst_deviation_px,
            allowed_px
        } if worst_deviation_px > allowed_px
    ));
}

#[test]
fn a_curve_excursion_hidden_from_the_forward_leg_is_detected_in_reverse() {
    let s = samples_from(&[Pt::new(0.0, 0.0), Pt::new(5.0, 0.0), Pt::new(10.0, 0.0)]);
    let poly = [
        Pt::new(0.0, 0.0),
        Pt::new(5.0, 0.0),
        Pt::new(10.0, 0.0),
        Pt::new(10.0, 100.0),
    ];
    assert!(
        s.iter()
            .all(|sample| euclidean_deviation(sample.p, &poly) == 0.0),
        "the old forward-only corridor leg sees no defect"
    );
    assert!(!model_to_evidence_corridor(&poly, &s).feasible());
}

#[test]
fn a_narrow_local_corridor_cannot_borrow_a_wide_samples_allowance() {
    let init = RefitChain {
        nodes: vec![
            RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
            RefitNode {
                pos: Pt::new(10.0, 0.0),
                tangent_rad: None,
            },
        ],
        segments: vec![RefitSegment::Line],
    };
    let s = vec![
        BoundarySample {
            p: Pt::new(0.0, 0.0),
            normal: Pt::new(0.0, 1.0),
            halfwidth: 1.0,
            confidence: 1.0,
            weight_ds: 1.0,
            corr_length_px: 1.0,
        },
        BoundarySample {
            p: Pt::new(3.0, 1.0),
            normal: Pt::new(0.0, 1.0),
            halfwidth: 1.0,
            confidence: 1.0,
            weight_ds: 1.0,
            corr_length_px: 1.0,
        },
        BoundarySample {
            p: Pt::new(7.0, 0.8),
            normal: Pt::new(0.0, 1.0),
            halfwidth: 0.1,
            confidence: 1.0,
            weight_ds: 1.0,
            corr_length_px: 1.0,
        },
        BoundarySample {
            p: Pt::new(10.0, 0.0),
            normal: Pt::new(0.0, 1.0),
            halfwidth: 1.0,
            confidence: 1.0,
            weight_ds: 1.0,
            corr_length_px: 1.0,
        },
    ];
    assert!(matches!(
        joint_constrained_refit(&init, &s),
        Err(RefitRefusal::OutsideCorridor {
            worst_deviation_px,
            allowed_px,
        }) if (worst_deviation_px - 0.8).abs() < 1e-12
            && (allowed_px - 0.3).abs() < 1e-12
    ));
}

#[test]
fn a_reverse_excursion_uses_the_halfwidth_at_its_projection() {
    let init = RefitChain {
        nodes: vec![
            RefitNode {
                pos: Pt::new(0.0, 0.0),
                tangent_rad: None,
            },
            RefitNode {
                pos: Pt::new(5.0, 0.0),
                tangent_rad: None,
            },
            RefitNode {
                pos: Pt::new(7.0, 2.0),
                tangent_rad: None,
            },
            RefitNode {
                pos: Pt::new(10.0, 0.0),
                tangent_rad: None,
            },
        ],
        segments: vec![RefitSegment::Line, RefitSegment::Line, RefitSegment::Line],
    };
    let mut s = samples_from(&[Pt::new(0.0, 0.0), Pt::new(5.0, 0.0), Pt::new(10.0, 0.0)]);
    s[0].halfwidth = 1.0;
    s[1].halfwidth = 0.1;
    s[2].halfwidth = 0.1;
    assert!(matches!(
        joint_constrained_refit(&init, &s),
        Err(RefitRefusal::OutsideCorridor {
            worst_deviation_px,
            allowed_px,
        }) if (worst_deviation_px - 2.0).abs() < 1e-12
            && (allowed_px - 0.3).abs() < 1e-12
    ));
}

/// The solve never returns something worse than what it was handed, at any
/// pass budget. Without the best-of rule a fixed iteration count is a bet.
#[test]
fn the_solve_never_returns_a_worse_chain_than_its_input() {
    let pts: Vec<Pt> = (0..=40).map(|i| Pt::new(i as f64, 0.0)).collect();
    let s = samples_from(&pts);
    let init = RefitChain {
        nodes: vec![
            RefitNode {
                pos: pts[0],
                tangent_rad: None,
            },
            RefitNode {
                pos: pts[40],
                tangent_rad: None,
            },
        ],
        segments: vec![RefitSegment::Line],
    };
    let out = joint_constrained_refit(&init, &s).expect("a line on a line is feasible");
    assert!(out.residual_after <= out.residual_before);
    assert_eq!(
        out.parameters, 0,
        "a single line between held ends is rigid"
    );
    assert_eq!(out.pass_kept, 0);
}
