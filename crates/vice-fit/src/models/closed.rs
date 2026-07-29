//! Physical-loop normalization and the bounded canonical-opening policy.

use std::cmp::Ordering;

use vice_evidence::{BoundaryChain, BoundarySample};

/// Collapse coincident observations before any index-based schedule is built.
///
/// The residual code is already duplicate-invariant because a repeat has no
/// arclength, but leaving repeats in the sample list shifts every dyadic
/// support and changes which candidates exist.
pub fn dedup_coincident(chain: &BoundaryChain) -> BoundaryChain {
    let mut samples: Vec<BoundarySample> = Vec::with_capacity(chain.samples.len());
    for sample in &chain.samples {
        match samples.last_mut() {
            Some(previous) if (previous.p - sample.p).length() <= DUPLICATE_EPSILON_PX => {
                previous.weight_ds += sample.weight_ds
            }
            _ => samples.push(*sample),
        }
    }
    if samples.len() == chain.samples.len() {
        return chain.clone();
    }
    let vertices = samples.len() as u64;
    BoundaryChain {
        samples,
        vertices,
        ..chain.clone()
    }
}

/// Below this separation, in px, two consecutive samples are one observation.
///
/// This is five orders below the frozen 0.35 px observability floor and three
/// orders above f64 rounding on a 10^4 px canvas. It closes RT6-A6's 1e-9 px
/// near-duplicate without collapsing a resolvable feature.
pub const DUPLICATE_EPSILON_PX: f64 = 1e-6;

/// Maximum canonical openings evaluated for one closed chain.
///
/// The cyclicly canonical root is always present; remaining slots go to the
/// strongest cyclic persistent-turning anchors. All cuts share one candidate
/// budget.
pub const MAX_CANONICAL_CUTS: usize = 4;

fn normal_alignment(normal: vice_geom::Pt, edge: vice_geom::Pt, length: f64) -> f64 {
    if length > 0.0 {
        (normal.dot(edge) / length).abs()
    } else {
        0.0
    }
}

fn descriptor(samples: &[BoundarySample], i: usize) -> [f64; 8] {
    let n = samples.len();
    let sample = samples[i];
    let incoming_edge = sample.p - samples[(i + n - 1) % n].p;
    let outgoing_edge = samples[(i + 1) % n].p - sample.p;
    let incoming = incoming_edge.length();
    let outgoing = outgoing_edge.length();
    [
        -crate::corner::cyclic_turning(samples, i, 1).map_or(0.0, f64::abs),
        incoming,
        outgoing,
        sample.halfwidth,
        sample.weight_ds,
        sample.corr_length_px,
        normal_alignment(sample.normal, incoming_edge, incoming),
        normal_alignment(sample.normal, outgoing_edge, outgoing),
    ]
}

fn compare_cyclic_roots(samples: &[BoundarySample], a: usize, b: usize) -> Ordering {
    let n = samples.len();
    for offset in 0..n {
        let left = descriptor(samples, (a + offset) % n);
        let right = descriptor(samples, (b + offset) % n);
        for field in 0..left.len() {
            let ordering = left[field].total_cmp(&right[field]);
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
    }
    Ordering::Equal
}

fn canonical_root(samples: &[BoundarySample]) -> usize {
    (1..samples.len()).fold(0usize, |best, candidate| {
        if compare_cyclic_roots(samples, candidate, best) == Ordering::Less {
            candidate
        } else {
            best
        }
    })
}

/// Openings immediately before a cyclicly canonical root and the strongest
/// persistent corner anchors.
///
/// Neither the root nor an anchor depends on which sample the caller happened
/// to put at index zero. Descriptor sequences bind both the cyclic geometry
/// and every observation attribute consumed by candidate ranking: normal-line
/// orientation, corridor halfwidth, arclength weight and correlation length.
/// `confidence` is deliberately absent because Stage G publishes but does not
/// consume it. Anchor windows wrap across the seam. Perfectly symmetric loops,
/// where every ranking-relevant descriptor is equivalent, also receive a
/// diametrically separated second cut so the cut-invariance gate remains
/// load-bearing.
pub fn canonical_cuts(chain: &BoundaryChain) -> Vec<usize> {
    let n = chain.samples.len();
    if n == 0 {
        return Vec::new();
    }
    let root = canonical_root(&chain.samples);
    let proposals = crate::corner::cyclic_corner_proposals(&chain.samples);
    let mut saliencies = vec![0.0; n];
    for proposal in proposals {
        saliencies[proposal.sample] = proposal.saliency;
    }
    let window = crate::CORNER_ANCHOR_HALF_WINDOW.min(n.saturating_sub(1) / 2);
    let mut ranked: Vec<(usize, f64)> = (0..n)
        .filter(|sample| *sample != root && saliencies[*sample] > 0.0)
        .filter(|sample| {
            (1..=window).all(|offset| {
                saliencies[(sample + n - offset) % n] < saliencies[*sample]
                    && saliencies[(sample + offset) % n] < saliencies[*sample]
            })
        })
        .map(|sample| (sample, saliencies[sample]))
        .collect();
    ranked.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then(((a.0 + n - root) % n).cmp(&((b.0 + n - root) % n)))
    });
    let opening_before = |anchor: usize| (anchor + n - 1) % n;
    let mut cuts = vec![opening_before(root)];
    cuts.extend(
        ranked
            .into_iter()
            .take(MAX_CANONICAL_CUTS.saturating_sub(1))
            .map(|(sample, _)| sample),
    );
    if cuts.len() < 2 && n > 1 {
        cuts.push(opening_before((root + n / 2) % n));
    }
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

/// Rotate a closed chain so `cut` is first and repeat it once as an unweighted
/// geometric endpoint. The final copy carries the incoming-side normal.
pub(super) fn rotate(chain: &BoundaryChain, cut: usize) -> BoundaryChain {
    if cut == 0 && !chain.closed {
        return chain.clone();
    }
    let n = chain.samples.len();
    let mut samples: Vec<BoundarySample> = (0..n)
        .map(|index| chain.samples[(cut + index) % n])
        .collect();
    if chain.closed {
        let mut seam = chain.samples[cut];
        seam.normal = chain.samples[(cut + n - 1) % n].normal;
        seam.weight_ds = 0.0;
        samples.push(seam);
    }
    BoundaryChain {
        samples,
        ..chain.clone()
    }
}
