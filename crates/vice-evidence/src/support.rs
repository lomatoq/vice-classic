//! Why nothing in this crate can become a second pixel likelihood
//! (spec §10.2, §17.2, §32 rule 12).
//!
//! §10.2 is the rule this module exists for:
//!
//! > Mixture alpha, corridor and boundary samples are computed from the SAME
//! > pixels as the final render likelihood. They therefore cannot be added a
//! > second time as an independent `E_evidence` without a probabilistic
//! > factorization.
//!
//! and it lists what evidence IS for: hypothesis generation, pruning,
//! trust-region sizes, proposal cost, uncertainty diagnostics.
//!
//! A comment saying "do not add this to the render likelihood" is a request.
//! The mechanism here is three things instead:
//!
//! 1. **Every surrogate number DERIVES the pixels it came from.** A
//!    [`SurrogateScore`] is a value, a [`SurrogateRole`] and an
//!    [`ObservationSupport`], and the only public constructor,
//!    [`SurrogateScore::over`], takes the COMPUTATION and reads the support
//!    off it. A support cannot be presented beside a value.
//!
//!    That is a correction. The first version took the support as a third
//!    independent argument and [`ObservationSupport::of_indices`] dropped
//!    out-of-range indices silently, so REVIEW_M4 (M4-N4) added two
//!    whole-image quantities under a support covering zero pixels and the
//!    combiner below allowed it. Both halves are closed: the constructor
//!    derives, and an index outside the image is a typed refusal.
//!
//!    It is worth naming what the defect WAS, because it is not new: a
//!    number and its provenance as two independent parameters is the shape
//!    of M35-N3, which this milestone removed from `oracle::key` under
//!    condition B1 and left standing here, one module away. The general
//!    rule is in the ledger (F-0029).
//!
//!    [`ObservationSource`] is SEALED, so the derivation cannot be reopened
//!    by implementing the trait either — a caller who wants a support has to
//!    have run a computation in this crate that produced one.
//! 2. **Addition is defined only on DISJOINT supports.** The only combiner
//!    is [`SurrogateScore::add_disjoint`], and it returns
//!    [`DoubleCounted`] — naming §10.2 and the number of shared pixels —
//!    whenever the two supports overlap. Two quantities over the same image
//!    always overlap, so "add the evidence to the render likelihood" is not
//!    an operation that exists.
//! 3. **No surrogate is published in the units of the final posterior.**
//!    §14.5/§17 put the final score in bits; nothing in this crate is named
//!    or serialized in bits, and
//!    `analysis::tests::no_evidence_field_is_published_in_posterior_units`
//!    walks the serialized report to keep it that way rather than trusting
//!    that nobody adds such a field later.
//!
//! What this module does NOT claim: that a caller cannot subtract two `f64`s
//! by hand. Nothing in Rust prevents arithmetic. The claim is that the
//! result is a bare number which no field of any M4 artifact accepts,
//! because every evidence field is typed `SurrogateScore` and every
//! `SurrogateScore` carries its support — exactly the shape of the claim
//! `oracle::key` makes for causal deltas.

use serde::Serialize;
use sha2::{Digest, Sha256};

/// The set of observed pixels a quantity was computed from.
///
/// Identity of the image is part of the support: two quantities over
/// DIFFERENT images are not "disjoint" in any useful sense — they are
/// incomparable, and combining them is refused for that reason rather than
/// silently allowed because their index sets happen not to overlap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationSupport {
    image_sha256: String,
    pixels: usize,
    bits: Vec<u64>,
    covered: u64,
}

impl ObservationSupport {
    /// The whole image.
    pub fn whole_image(image_sha256: &str, pixels: usize) -> ObservationSupport {
        ObservationSupport::of_indices(image_sha256, pixels, 0..pixels)
            .expect("0..pixels is in range by construction")
    }

    /// An explicit subset.
    ///
    /// An index outside the image is REFUSED, not dropped. Dropping it was
    /// the second half of REVIEW_M4 M4-N4: a caller could hand in indices
    /// that all missed and receive a support covering nothing, which then
    /// combined with anything because it overlapped nothing.
    pub fn of_indices(
        image_sha256: &str,
        pixels: usize,
        indices: impl IntoIterator<Item = usize>,
    ) -> Result<ObservationSupport, SupportError> {
        let mut bits = vec![0u64; pixels.div_ceil(64)];
        let mut covered = 0u64;
        for i in indices {
            if i >= pixels {
                return Err(SupportError::OutOfRange { index: i, pixels });
            }
            let (w, b) = (i / 64, i % 64);
            if bits[w] & (1u64 << b) == 0 {
                bits[w] |= 1u64 << b;
                covered += 1;
            }
        }
        Ok(ObservationSupport {
            image_sha256: image_sha256.to_string(),
            pixels,
            bits,
            covered,
        })
    }

    pub fn image_sha256(&self) -> &str {
        &self.image_sha256
    }
    pub fn pixels(&self) -> usize {
        self.pixels
    }
    pub fn covered(&self) -> u64 {
        self.covered
    }
    pub fn contains(&self, i: usize) -> bool {
        i < self.pixels && self.bits[i / 64] & (1u64 << (i % 64)) != 0
    }

    /// Number of pixels in both supports. Zero for supports over different
    /// images is NOT reported: see [`SurrogateScore::add_disjoint`].
    pub fn shared_pixels(&self, other: &ObservationSupport) -> u64 {
        if self.image_sha256 != other.image_sha256 {
            return 0;
        }
        self.bits
            .iter()
            .zip(&other.bits)
            .map(|(a, b)| (a & b).count_ones() as u64)
            .sum()
    }

    /// A stable digest of the support, for artifacts. The bitset itself is
    /// megabytes on a large image and belongs in memory, not in a report.
    pub fn digest(&self) -> String {
        let mut h = Sha256::new();
        h.update(b"vice-classic/observation-support/v1");
        h.update(self.image_sha256.as_bytes());
        h.update((self.pixels as u64).to_le_bytes());
        for w in &self.bits {
            h.update(w.to_le_bytes());
        }
        hex::encode(h.finalize())
    }

    pub fn summary(&self) -> SupportSummary {
        SupportSummary {
            image_sha256: self.image_sha256.clone(),
            pixels: self.pixels as u64,
            covered: self.covered,
            digest: self.digest(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SupportSummary {
    pub image_sha256: String,
    pub pixels: u64,
    pub covered: u64,
    pub digest: String,
}

/// What §10.2 permits a surrogate quantity to be used for.
///
/// There is deliberately no `ObservationLikelihood` variant: the final
/// observation likelihood is one exact term over the serialized scene
/// (§10.2, §17.1), it is produced by the renderer and the posterior, and it
/// is not something this crate can hand out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurrogateRole {
    HypothesisGeneration,
    Pruning,
    TrustRegion,
    ProposalCost,
    Diagnostic,
}

impl SurrogateRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            SurrogateRole::HypothesisGeneration => "hypothesis_generation",
            SurrogateRole::Pruning => "pruning",
            SurrogateRole::TrustRegion => "trust_region",
            SurrogateRole::ProposalCost => "proposal_cost",
            SurrogateRole::Diagnostic => "diagnostic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SupportError {
    #[error(
        "pixel index {index} is outside an image of {pixels} pixels: refusing to build a support          that silently omits it"
    )]
    OutOfRange { index: usize, pixels: usize },
}

/// A computation in this crate that read pixels.
///
/// The trait exists so that [`SurrogateScore::over`] can DERIVE a support
/// rather than accept one. It is SEALED: the supertrait is private, so no
/// downstream crate can implement it, and therefore no downstream crate can
/// state a support that no computation produced.
///
/// The seal is the difference between this trait and
/// `oracle::key::KeyedMeasurement`, which REVIEW_M4 M4-N7 records as still
/// implementable by a caller who wants to declare a key rather than derive
/// one. Same defect class, closed here, open there; M12 owns the port.
pub trait ObservationSource: sealed::Sealed {
    /// The pixels the computation actually read.
    fn observation_support(&self) -> ObservationSupport;
}

pub(crate) mod sealed {
    /// Private supertrait. Implementing [`super::ObservationSource`] requires
    /// naming this, which is impossible outside `vice-evidence`.
    pub trait Sealed {}
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DoubleCounted {
    #[error(
        "refusing to combine two surrogate scores that share {shared} of {pixels} observed \
         pixels: spec 10.2 forbids counting the same pixels twice, and this crate has no \
         probabilistic factorization that would make it legitimate"
    )]
    SharedPixels { shared: u64, pixels: u64 },
    #[error(
        "refusing to combine surrogate scores over different images ({a} vs {b}): they are \
         incomparable, not disjoint"
    )]
    DifferentImages { a: String, b: String },
    #[error("refusing to combine surrogate scores with different roles ({a} vs {b})")]
    DifferentRoles { a: &'static str, b: &'static str },
}

/// A number derived from observed pixels, carrying those pixels and the role
/// §10.2 allows it to play.
#[derive(Debug, Clone, PartialEq)]
pub struct SurrogateScore {
    role: SurrogateRole,
    value: f64,
    support: ObservationSupport,
}

impl SurrogateScore {
    /// The only public constructor: the support is read off the computation
    /// the number came from, so the two cannot disagree.
    pub fn over<S: ObservationSource>(
        role: SurrogateRole,
        value: f64,
        source: &S,
    ) -> SurrogateScore {
        SurrogateScore {
            role,
            value,
            support: source.observation_support(),
        }
    }

    /// Crate-internal, for the one place where the support IS what is being
    /// built: `infer_mixture` derives it from the tensor it has just read,
    /// and the `Flat2Evidence` that would implement [`ObservationSource`]
    /// does not exist yet at that point. Tests of the combiner itself also
    /// use it, because their subject is the support algebra.
    pub(crate) fn with_support(
        role: SurrogateRole,
        value: f64,
        support: ObservationSupport,
    ) -> SurrogateScore {
        SurrogateScore {
            role,
            value,
            support,
        }
    }

    pub fn role(&self) -> SurrogateRole {
        self.role
    }

    /// The value, in whatever units the producing module documents — never
    /// in bits, and never comparable with a posterior (§10.2).
    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn support(&self) -> &ObservationSupport {
        &self.support
    }

    /// The ONLY combiner. Defined on disjoint supports of one image and
    /// refused otherwise.
    pub fn add_disjoint(&self, other: &SurrogateScore) -> Result<SurrogateScore, DoubleCounted> {
        if self.support.image_sha256 != other.support.image_sha256 {
            return Err(DoubleCounted::DifferentImages {
                a: self.support.image_sha256.clone(),
                b: other.support.image_sha256.clone(),
            });
        }
        if self.role != other.role {
            return Err(DoubleCounted::DifferentRoles {
                a: self.role.as_str(),
                b: other.role.as_str(),
            });
        }
        let shared = self.support.shared_pixels(&other.support);
        if shared > 0 {
            return Err(DoubleCounted::SharedPixels {
                shared,
                pixels: self.support.pixels as u64,
            });
        }
        let mut bits = self.support.bits.clone();
        for (w, o) in bits.iter_mut().zip(&other.support.bits) {
            *w |= o;
        }
        let covered = self.support.covered + other.support.covered;
        Ok(SurrogateScore {
            role: self.role,
            value: self.value + other.value,
            support: ObservationSupport {
                image_sha256: self.support.image_sha256.clone(),
                pixels: self.support.pixels,
                bits,
                covered,
            },
        })
    }

    pub fn summary(&self) -> SurrogateSummary {
        SurrogateSummary {
            role: self.role,
            value: self.value,
            support: self.support.summary(),
            not_a_likelihood: NOT_A_LIKELIHOOD,
        }
    }
}

/// The sentence that travels with every surrogate number in an artifact.
pub const NOT_A_LIKELIHOOD: &str =
    "surrogate over the SAME pixels the final render likelihood judges; spec 10.2 forbids adding \
     it to that likelihood as a second term";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SurrogateSummary {
    pub role: SurrogateRole,
    pub value: f64,
    pub support: SupportSummary,
    pub not_a_likelihood: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The §10.2 mechanism, on the state where it matters: two evidence
    /// quantities over one image ALWAYS share pixels, so combining them is
    /// refused and the refusal counts the shared pixels.
    #[test]
    fn two_scores_over_the_same_image_cannot_be_added() {
        let a = SurrogateScore::with_support(
            SurrogateRole::Pruning,
            1.0,
            ObservationSupport::whole_image("img", 100),
        );
        let b = SurrogateScore::with_support(
            SurrogateRole::Pruning,
            2.0,
            ObservationSupport::whole_image("img", 100),
        );
        match a.add_disjoint(&b) {
            Err(DoubleCounted::SharedPixels { shared, pixels }) => {
                assert_eq!(shared, 100);
                assert_eq!(pixels, 100);
            }
            other => panic!("{other:?}"),
        }
        // Even ONE shared pixel is refused: the rule is about counting a
        // pixel twice, not about counting most of them twice.
        let left = SurrogateScore::with_support(
            SurrogateRole::Pruning,
            1.0,
            ObservationSupport::of_indices("img", 100, 0..51).unwrap(),
        );
        let right = SurrogateScore::with_support(
            SurrogateRole::Pruning,
            1.0,
            ObservationSupport::of_indices("img", 100, 50..100).unwrap(),
        );
        assert!(matches!(
            left.add_disjoint(&right),
            Err(DoubleCounted::SharedPixels { shared: 1, .. })
        ));
    }

    /// The control without which the refusal above would be indistinguishable
    /// from a combiner that never works: disjoint supports DO combine, and
    /// the union is the support of the result.
    #[test]
    fn disjoint_supports_do_combine_and_carry_the_union() {
        let left = SurrogateScore::with_support(
            SurrogateRole::ProposalCost,
            1.5,
            ObservationSupport::of_indices("img", 100, 0..50).unwrap(),
        );
        let right = SurrogateScore::with_support(
            SurrogateRole::ProposalCost,
            2.5,
            ObservationSupport::of_indices("img", 100, 50..100).unwrap(),
        );
        let sum = left
            .add_disjoint(&right)
            .expect("disjoint supports combine");
        assert!((sum.value() - 4.0).abs() < 1e-12);
        assert_eq!(sum.support().covered(), 100);
        assert_eq!(sum.role(), SurrogateRole::ProposalCost);
        assert_ne!(sum.support().digest(), left.support().digest());
    }

    /// Different images are INCOMPARABLE, not disjoint. Without this, two
    /// evidence numbers from two different runs would add cleanly and the
    /// support would stop meaning anything.
    #[test]
    fn scores_over_different_images_are_refused_rather_than_treated_as_disjoint() {
        let a = SurrogateScore::with_support(
            SurrogateRole::Diagnostic,
            1.0,
            ObservationSupport::of_indices("A", 10, 0..5).unwrap(),
        );
        let b = SurrogateScore::with_support(
            SurrogateRole::Diagnostic,
            1.0,
            ObservationSupport::of_indices("B", 10, 5..10).unwrap(),
        );
        assert!(matches!(
            a.add_disjoint(&b),
            Err(DoubleCounted::DifferentImages { .. })
        ));
    }

    #[test]
    fn roles_do_not_mix() {
        let a = SurrogateScore::with_support(
            SurrogateRole::Pruning,
            1.0,
            ObservationSupport::of_indices("img", 10, 0..5).unwrap(),
        );
        let b = SurrogateScore::with_support(
            SurrogateRole::TrustRegion,
            1.0,
            ObservationSupport::of_indices("img", 10, 5..10).unwrap(),
        );
        assert!(matches!(
            a.add_disjoint(&b),
            Err(DoubleCounted::DifferentRoles { .. })
        ));
    }

    /// The support is a set, and the digest is a function of that set.
    #[test]
    fn the_support_is_a_set_and_its_digest_follows_it() {
        let s = ObservationSupport::of_indices("img", 10, [3usize, 3, 7]).unwrap();
        assert_eq!(s.covered(), 2, "a repeated index is not a second member");
        assert!(s.contains(3) && s.contains(7) && !s.contains(4));
        let same = ObservationSupport::of_indices("img", 10, [7usize, 3]).unwrap();
        assert_eq!(s.digest(), same.digest(), "order must not matter");
        let other = ObservationSupport::of_indices("img", 10, [3usize, 8]).unwrap();
        assert_ne!(s.digest(), other.digest());
        assert_eq!(s.summary().covered, 2);
    }

    /// REVIEW_M4 M4-N4, second half. The reviewer built a support out of
    /// indices that were all out of range, got a support over zero pixels,
    /// and added two whole-image numbers under it because zero pixels
    /// overlap nothing. An out-of-range index is a typed refusal now.
    #[test]
    fn an_index_outside_the_image_is_refused_rather_than_dropped() {
        let e = ObservationSupport::of_indices("img", 10, [3usize, 99]).unwrap_err();
        assert!(matches!(
            e,
            SupportError::OutOfRange {
                index: 99,
                pixels: 10
            }
        ));
        // The refusal is specific, not a constructor that rejects everything.
        assert_eq!(
            ObservationSupport::of_indices("img", 10, [3usize, 9])
                .unwrap()
                .covered(),
            2
        );
        assert_eq!(ObservationSupport::whole_image("img", 10).covered(), 10);
    }

    /// Every published surrogate carries the sentence that says what it is
    /// not, so a reader of an artifact does not have to know §10.2 already.
    #[test]
    fn a_published_surrogate_says_it_is_not_a_likelihood() {
        let s = SurrogateScore::with_support(
            SurrogateRole::Diagnostic,
            0.25,
            ObservationSupport::whole_image("img", 4),
        );
        let json = serde_json::to_string(&s.summary()).unwrap();
        assert!(json.contains("10.2"), "{json}");
        assert!(!json.contains("bits"), "{json}");
    }
}
