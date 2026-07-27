//! One topology candidate and everything §11.3 says it must carry.
//!
//! ```text
//! cubical signature
//! components/holes/Euler
//! event persistence
//! formation/palette provenance
//! lower-bound data cost
//! topology/region code length
//! ambiguity flags
//! ```
//!
//! ## The two cost bounds, and what each is a bound OVER
//!
//! §5.4 fixes the arithmetic of a comparison: `A` is certifiably better than
//! `B` only when `A.hi < B.lo`, and overlapping intervals keep both. So a
//! candidate needs a low end and a high end, and each has to be a bound over
//! a NAMED set, or the comparison is an epsilon quietly choosing a topology —
//! which §5.4 forbids by name.
//!
//! **`lower`** is certified over *every* scene consistent with this labelling
//! at this level. Such a scene has coverage `g` with `g_i >= level` where the
//! label is inside and `g_i <= level` where it is outside, so the smallest
//! achievable `sum (g_i - a_i)^2` is the projection of the observed `a` onto
//! that half-space product:
//!
//! ```text
//! lower = sum_{inside}  max(0, level - a_i)^2
//!       + sum_{outside} max(0, a_i - level)^2
//! ```
//!
//! No scene with this topology at this level can cost less. That is a
//! theorem, not a calibration.
//!
//! **`achievable`** is the cost of ONE explicit scene: the union of the
//! labelled pixel squares — a rectilinear polygon, which is a legal Flat2
//! visible scene — rendered through the formation's own kernel. It is an
//! upper bound only if that scene really has this candidate's topology, so
//! that is CHECKED rather than assumed: the predicted coverage is
//! re-thresholded and its signature compared, and when it differs the bound
//! is `None` and the candidate can be dominated but can dominate nobody.
//!
//! The honest caveat, stated because a bound whose class is not named is the
//! defect this project keeps finding: the pixel-union polygon is a legal
//! scene but has thousands of anchors, so `achievable` bounds the
//! unrestricted rectilinear family and not a complexity-limited
//! `SupportedModelUniverse` (§1.5). What that could cost is measured rather
//! than argued: if this bound ever prunes a correct candidate, the §28 M4.5
//! recall clause falls, which is the whole reason the gate is recall.
//!
//! ## These are surrogates
//!
//! Both numbers order and prune hypotheses. Neither is in the units of the
//! final observation likelihood and neither may ever be added to one (§10.2,
//! §32 rule 12). Every hypothesis carries the sentence saying so.

use serde::Serialize;
use vice_ir::ComplementaryConnectivity;

use crate::cubical::{
    residual_critical_cells, signature, Labelling, SaddleResolution, TopologySignature,
};
use crate::events::LevelOrigin;
use crate::field::{FieldKind, SeparableKernel};

/// The sentence every surrogate in this crate carries (§10.2).
pub const NOT_A_LIKELIHOOD: &str = "surrogate cost in coverage units: it orders and prunes \
                                    topology hypotheses and is NOT a term of the observation \
                                    likelihood (spec 10.2)";

/// Where a candidate came from, in full. §11.3 calls it
/// "formation/palette provenance"; the field, the connectivity arm, the
/// saddle reading and the level's ORIGIN are here too, because without them
/// the per-field and no-fixed-threshold measurements have nothing to group
/// by.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Provenance {
    pub palette_id: String,
    pub formation_id: String,
    pub field: FieldKind,
    pub foreground_connectivity: &'static str,
    pub saddle: SaddleResolution,
    pub level: f64,
    pub level_origins: Vec<&'static str>,
    /// True when the ONLY reason this level was generated is a fixed smoke
    /// probe. The §28 M4.5 clause about magic thresholds is measured by
    /// counting candidates for which this is true.
    pub level_from_fixed_probe_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CostBounds {
    /// Certified lower bound over every scene with this labelling at this
    /// level.
    pub lower: f64,
    /// Cost of one explicit scene that HAS this topology, when the check
    /// confirms it does.
    pub achievable: Option<f64>,
    pub not_a_likelihood: &'static str,
}

impl CostBounds {
    /// §5.4: certifiably better means the intervals do not overlap.
    pub fn certifiably_dominates(&self, other: &CostBounds) -> bool {
        match self.achievable {
            Some(hi) => hi < other.lower,
            None => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct AmbiguityFlags {
    /// Critical 2x2 cells the saddle resolution did not remove. Non-zero
    /// means the two connectivity arms can still disagree about this
    /// labelling.
    pub residual_critical_cells: u32,
    /// The persistence that justifies this candidate's level.
    pub event_persistence: f64,
    /// True when this candidate exists only because of a fixed probe.
    pub level_from_fixed_probe_only: bool,
    /// True when a candidate with the same signature exists under the OTHER
    /// connectivity arm. Filled by the envelope, which is the only place
    /// that can see both.
    pub both_connectivity_arms_agree: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopologyHypothesis {
    pub signature: TopologySignature,
    pub provenance: Provenance,
    pub cost: CostBounds,
    /// Description length of the topology and its regions, in bits.
    pub topology_code_bits: f64,
    pub ambiguity: AmbiguityFlags,
    #[serde(skip)]
    pub labelling: Labelling,
}

/// Everything a candidate needs beside its labelling.
///
/// A struct rather than eleven positional arguments, and not only because
/// clippy counts: a caller that swaps two `&str` of the same type is exactly
/// the mistake FAILURE_LEDGER F-0013 records under "the numerator and the
/// denominator changed places", and named fields make that unwritable.
#[derive(Debug, Clone, Copy)]
pub struct CandidateContext<'a> {
    pub conn: ComplementaryConnectivity,
    pub alpha: &'a [f64],
    pub level: f64,
    pub level_origins: &'a [LevelOrigin],
    pub persistence: f64,
    pub kernel: Option<&'a SeparableKernel>,
    pub palette_id: &'a str,
    pub formation_id: &'a str,
    pub field: FieldKind,
    pub saddle: SaddleResolution,
}

/// Build a candidate from a labelling and the observation it came from.
pub fn from_events(labelling: Labelling, ctx: &CandidateContext<'_>) -> TopologyHypothesis {
    let CandidateContext {
        conn,
        alpha,
        level,
        level_origins,
        persistence,
        kernel,
        palette_id,
        formation_id,
        field,
        saddle,
    } = *ctx;
    let sig = signature(&labelling, conn);
    let cost = cost_bounds(&labelling, conn, alpha, level, kernel, &sig);
    let fixed_only = !level_origins.iter().any(|o| o.is_event_driven());
    TopologyHypothesis {
        topology_code_bits: topology_code_bits(&labelling, &sig),
        ambiguity: AmbiguityFlags {
            residual_critical_cells: residual_critical_cells(&labelling),
            event_persistence: persistence,
            level_from_fixed_probe_only: fixed_only,
            both_connectivity_arms_agree: false,
        },
        provenance: Provenance {
            palette_id: palette_id.to_string(),
            formation_id: formation_id.to_string(),
            field,
            foreground_connectivity: sig.foreground_connectivity,
            saddle,
            level,
            level_origins: level_origins.iter().map(|o| o.as_str()).collect(),
            level_from_fixed_probe_only: fixed_only,
        },
        signature: sig,
        cost,
        labelling,
    }
}

/// The two bounds of §11.3, each over the set the module doc names.
pub fn cost_bounds(
    l: &Labelling,
    conn: ComplementaryConnectivity,
    alpha: &[f64],
    level: f64,
    kernel: Option<&SeparableKernel>,
    sig: &TopologySignature,
) -> CostBounds {
    let mut lower = 0.0f64;
    for (i, inside) in l.inside().iter().enumerate() {
        let a = alpha[i];
        let d = if *inside {
            (level - a).max(0.0)
        } else {
            (a - level).max(0.0)
        };
        lower += d * d;
    }

    let indicator: Vec<f64> = l
        .inside()
        .iter()
        .map(|v| if *v { 1.0 } else { 0.0 })
        .collect();
    // A box PSF of one pixel IS the pixel integration, so its pixel-grid
    // kernel is the identity and the indicator is already the prediction.
    let predicted = match kernel {
        Some(k) => k.apply(&indicator, l.width_px(), l.height_px()),
        None => indicator,
    };
    let realized = crate::cubical::threshold(
        &predicted,
        l.width_px(),
        l.height_px(),
        level,
        SaddleResolution::Thresholded,
    );
    let achievable = if signature(&realized, conn).digest == sig.digest {
        Some(
            predicted
                .iter()
                .zip(alpha)
                .map(|(p, a)| (p - a) * (p - a))
                .sum(),
        )
    } else {
        None
    };

    CostBounds {
        lower,
        achievable,
        not_a_likelihood: NOT_A_LIKELIHOOD,
    }
}

/// Description length of the topology and its regions, in bits.
///
/// Three terms, all of them properties of the labelling rather than of any
/// fitted curve — a fitted description length is §14.5's job and needs a
/// grammar (M6):
///
/// - the counts, at `log2(n+1)` bits each;
/// - one start position per region, at `2*log2(max(w,h))` bits;
/// - the crack boundary as a chain code, at `log2(3)` bits per step, because
///   from any boundary step the next one is one of three (the fourth would
///   retrace).
pub fn topology_code_bits(l: &Labelling, sig: &TopologySignature) -> f64 {
    let (w, h) = (l.width_px(), l.height_px());
    let inside = l.inside();
    let mut steps = 0u64;
    for y in 0..h {
        for x in 0..w {
            let c = inside[y * w + x];
            // A crack edge on the right and on the bottom, plus the canvas
            // border where the foreground runs into it.
            let right = if x + 1 < w {
                inside[y * w + x + 1]
            } else {
                false
            };
            let down = if y + 1 < h {
                inside[(y + 1) * w + x]
            } else {
                false
            };
            if c != right {
                steps += 1;
            }
            if c != down {
                steps += 1;
            }
            if x == 0 && c {
                steps += 1;
            }
            if y == 0 && c {
                steps += 1;
            }
        }
    }
    let regions = f64::from(sig.components + sig.holes);
    let pos_bits = 2.0 * (w.max(h) as f64).log2();
    f64::from(sig.components + 1).log2()
        + f64::from(sig.holes + 1).log2()
        + regions * pos_bits
        + steps as f64 * 3.0f64.log2()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubical::threshold;
    use crate::field::forward_kernel;
    use vice_ir::PixelFilter;

    fn arm() -> ComplementaryConnectivity {
        ComplementaryConnectivity::arms()[0]
    }

    fn disk(w: usize, h: usize, r: f64) -> Vec<f64> {
        let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
        (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
                let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                (r + 0.5 - d).clamp(0.0, 1.0)
            })
            .collect()
    }

    /// The lower bound is a bound: no scene with this labelling can cost
    /// less, so in particular the labelling's OWN best prediction cannot.
    /// And it is zero exactly when the observation already satisfies the
    /// labelling's constraints, which is the only case where nothing is
    /// owed.
    #[test]
    fn the_lower_bound_is_zero_only_when_the_observation_already_satisfies_the_labelling() {
        let (w, h) = (16, 16);
        let a = disk(w, h, 5.0);
        let l = threshold(&a, w, h, 0.5, SaddleResolution::Thresholded);
        let sig = signature(&l, arm());
        let c = cost_bounds(&l, arm(), &a, 0.5, None, &sig);
        assert!(
            c.lower.abs() < 1e-12,
            "thresholding a field at L cannot violate its own constraints: {}",
            c.lower
        );
        // A labelling that asserts the opposite everywhere owes a lot.
        let flipped = Labelling::new(w, h, l.inside().iter().map(|v| !v).collect());
        let fsig = signature(&flipped, arm());
        let fc = cost_bounds(&flipped, arm(), &a, 0.5, None, &fsig);
        assert!(fc.lower > 1.0, "the inverted labelling owes: {}", fc.lower);
    }

    /// Domination is §5.4 arithmetic and nothing else: overlapping intervals
    /// keep both, and a candidate with no upper bound dominates nobody.
    #[test]
    fn domination_is_the_interval_rule_and_a_missing_upper_bound_dominates_nobody() {
        let n = NOT_A_LIKELIHOOD;
        let a = CostBounds {
            lower: 1.0,
            achievable: Some(2.0),
            not_a_likelihood: n,
        };
        let b = CostBounds {
            lower: 3.0,
            achievable: Some(4.0),
            not_a_likelihood: n,
        };
        let overlapping = CostBounds {
            lower: 1.5,
            achievable: Some(3.5),
            not_a_likelihood: n,
        };
        let unbounded = CostBounds {
            lower: 0.1,
            achievable: None,
            not_a_likelihood: n,
        };
        assert!(a.certifiably_dominates(&b));
        assert!(!b.certifiably_dominates(&a));
        assert!(
            !a.certifiably_dominates(&overlapping),
            "2.0 is not below 1.5"
        );
        assert!(
            !overlapping.certifiably_dominates(&b),
            "3.5 is not below 3.0"
        );
        assert!(
            !unbounded.certifiably_dominates(&b),
            "a candidate whose achievable cost is unknown may be dominated but may not dominate"
        );
        assert!(b.certifiably_dominates(&CostBounds {
            lower: 5.0,
            achievable: None,
            not_a_likelihood: n
        }));
    }

    /// The upper bound is CHECKED, not assumed: under a wide kernel the
    /// pixel-union scene does not reproduce a one-pixel structure, and the
    /// bound is then absent rather than wrong.
    #[test]
    fn the_upper_bound_is_absent_when_the_explicit_scene_has_a_different_topology() {
        let (w, h) = (11usize, 11usize);
        let mut a = vec![0.0f64; w * h];
        // Two blobs and a one-pixel neck: under a sigma=1 gaussian the
        // pixel-union's own coverage loses the neck.
        for y in 4..7 {
            for x in 0..4 {
                a[y * w + x] = 1.0;
            }
            for x in 7..11 {
                a[y * w + x] = 1.0;
            }
        }
        a[5 * w + 4] = 1.0;
        a[5 * w + 5] = 1.0;
        a[5 * w + 6] = 1.0;
        let l = threshold(&a, w, h, 0.5, SaddleResolution::Thresholded);
        let sig = signature(&l, arm());
        assert_eq!(sig.components, 1, "the neck joins them");
        let k = forward_kernel(PixelFilter::Gaussian { sigma_px: 1.0 }).unwrap();
        let c = cost_bounds(&l, arm(), &a, 0.5, Some(&k), &sig);
        assert!(
            c.achievable.is_none(),
            "the explicit scene loses the neck under this kernel, so it does not bound THIS \
             topology's cost"
        );
        // With no kernel (box, identity) the explicit scene IS the
        // labelling, so the bound exists. The control that stops the test
        // above from passing on a broken checker.
        let c2 = cost_bounds(&l, arm(), &a, 0.5, None, &sig);
        assert!(c2.achievable.is_some());
    }

    /// The code length grows with topological and geometric complexity, and
    /// it is a function of the labelling only.
    #[test]
    fn the_code_length_ranks_a_ring_above_a_disk() {
        let (w, h) = (24usize, 24usize);
        let a = disk(w, h, 8.0);
        let inner = disk(w, h, 4.0);
        let solid = threshold(&a, w, h, 0.5, SaddleResolution::Thresholded);
        let ring = Labelling::new(
            w,
            h,
            solid
                .inside()
                .iter()
                .zip(inner.iter())
                .map(|(o, i)| *o && *i < 0.5)
                .collect(),
        );
        let sd = topology_code_bits(&solid, &signature(&solid, arm()));
        let rd = topology_code_bits(&ring, &signature(&ring, arm()));
        assert_eq!(signature(&ring, arm()).holes, 1);
        assert!(
            rd > sd,
            "a ring costs more bits than a disk: {rd:.1} against {sd:.1}"
        );
        assert!(sd > 0.0);
    }
}
