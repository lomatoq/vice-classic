//! Physical-loop normalization and the bounded canonical-opening policy.

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
/// Sample zero is always present; remaining slots go to the strongest
/// persistent corner anchors. All cuts share one candidate budget.
pub const MAX_CANONICAL_CUTS: usize = 4;

/// Sample zero plus the strongest persistent corner anchors.
pub fn canonical_cuts(chain: &BoundaryChain) -> Vec<usize> {
    let proposals = crate::corner::corner_proposals(&chain.samples);
    let anchors = crate::corner::corner_anchors(&proposals, crate::CORNER_ANCHOR_HALF_WINDOW);
    let mut ranked: Vec<(usize, f64)> = anchors
        .into_iter()
        .filter(|sample| *sample != 0)
        .map(|sample| {
            let saliency = proposals
                .iter()
                .find(|proposal| proposal.sample == sample)
                .map_or(0.0, |proposal| proposal.saliency);
            (sample, saliency)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut cuts = vec![0usize];
    cuts.extend(
        ranked
            .into_iter()
            .take(MAX_CANONICAL_CUTS.saturating_sub(1))
            .map(|(sample, _)| sample),
    );
    cuts.sort_unstable();
    cuts.dedup();
    cuts
}

pub(super) fn cut_is_jet_smooth(chain: &BoundaryChain, cut: usize) -> bool {
    let n = chain.samples.len();
    if n < 3 || cut >= n {
        return false;
    }
    let point = chain.samples[cut].p;
    let incoming = point - chain.samples[(cut + n - 1) % n].p;
    let outgoing = chain.samples[(cut + 1) % n].p - point;
    if incoming.length_sq() <= 0.0 || outgoing.length_sq() <= 0.0 {
        return false;
    }
    crate::grammar::jet_compatible(
        crate::grammar::jet_class(incoming.y.atan2(incoming.x)),
        crate::grammar::jet_class(outgoing.y.atan2(outgoing.x)),
    )
}

/// Rotate a closed chain so `cut` is first and repeat it once as an unweighted
/// geometric endpoint. The final copy carries the incoming-side normal.
pub fn rotate(chain: &BoundaryChain, cut: usize) -> BoundaryChain {
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
