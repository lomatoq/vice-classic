//! §14.5's explicit code lengths, in physical bits.
//!
//! ## What this is, and the one term it does NOT contain
//!
//! §14.5 writes the final score as
//!
//! ```text
//! L_total = -log2 p(observed bytes | scene, formation, noise)
//!         + L_topology + L_geometry + L_paint + L_relations + L_formation
//! ```
//!
//! and lists the minimal explicit code: prefix code family, combinatorial code
//! for breakpoints/corners, parameter code `log2(range / calibrated precision)`,
//! topology/face-count code, formation family/parameter code, robust quantized
//! residual likelihood.
//!
//! **The first term — the pixel-byte likelihood — is NOT here and is not
//! approximated here.** It is the full-resolution correlation-aware likelihood
//! of §28 M7, and writing a surrogate for it under the same name is exactly what
//! §17.2 forbids ("запрет двойного учёта") once M7 arrives. What this module
//! computes is the BOUNDARY-OBSERVATION residual code — §14.5's own last bullet,
//! "robust quantized residual likelihood" — over the Stage F corridor, plus
//! `L_topology` for the breakpoint/corner structure and `L_geometry` for the
//! anchors and family parameters. [`ChainCode::total_bits`] is therefore
//! **`L_geometry + L_topology + L_relations + L_residual`, and it says so**;
//! `L_paint` and `L_formation` are properties of a whole scene, not of a chain.
//!
//! ## `BIC_eff` does not appear, at all
//!
//! §14.5: "Raw-sample BIC не является final selector" and `BIC_eff` "не может
//! promote feature". There is no `k * log(n)` in this module and no free
//! multiplier anywhere in it. Every number is a code length over a declared
//! range at a calibrated precision, and the only quantity that is not derived
//! from the chain itself is [`GeometryCodeTable`], which comes from the frozen
//! `configs/GATES_V1.toml [geometry_code_table]`.
//!
//! ## One frozen number, everything else derived from it
//!
//! The table has three values, and only the first carries a length scale.
//! Rather than repeat the calibrated precision as a second constant — a guard
//! sharing its key with the mechanism, which is F-0048 Q4 — the precision is
//! RECOVERED from `bits_per_anchor` by inverting its own definition
//! ([`GeometryCodeTable::coordinate_precision_px`]). There is exactly one
//! length constant in this crate's code table and it is the frozen one.

use serde::Serialize;

use crate::span::SpanFamily;

/// The canvas dimension `bits_per_anchor` is stated at, in px.
///
/// The universe's `geometry.canvas_dim_px.hi`. Held here because it is part of
/// the definition of the frozen number rather than a second calibration, and
/// cross-checked against the universe itself by `vice-bench` — neither side
/// derives the other.
pub const REFERENCE_CANVAS_DIM_PX: f64 = 16384.0;

/// Join kinds an interior node may take: `corner` and `smooth_g1`.
///
/// The universe's `geometry.join_kinds` count, cross-checked from the
/// `vice-bench` side. A node's join therefore costs `log2(2) = 1` bit, and that
/// bit is the ENTIRE cost difference between a corner and a smooth join — which
/// is what makes the choice an MDL decision rather than a threshold on a corner
/// saliency (§14.1: "proposal confidence, не hard label").
pub const JOIN_KINDS: usize = 2;

/// `bits_per_anchor`, `bits_per_segment_family`, `bits_per_relation` — the
/// three keys of `configs/GATES_V1.toml [geometry_code_table]`.
///
/// Minted only by [`GeometryCodeTable::new`], which refuses non-finite and
/// non-positive values. A code length of zero says a parameter is free, and the
/// placeholder this section shipped as had all three at `0.0`; a table that
/// silently kept those values would price an unbounded grammar at nothing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GeometryCodeTable {
    bits_per_anchor: f64,
    bits_per_segment_family: f64,
    bits_per_relation: f64,
}

impl GeometryCodeTable {
    pub fn new(
        bits_per_anchor: f64,
        bits_per_segment_family: f64,
        bits_per_relation: f64,
    ) -> Option<GeometryCodeTable> {
        let ok = [bits_per_anchor, bits_per_segment_family, bits_per_relation]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0);
        ok.then_some(GeometryCodeTable {
            bits_per_anchor,
            bits_per_segment_family,
            bits_per_relation,
        })
    }

    /// Bits to code one anchor (two coordinates) on a canvas of `canvas_dim_px`.
    ///
    /// `bits_per_anchor + 2 log2(canvas / reference)`: exact arithmetic on the
    /// frozen number, so the frozen number is what governs behaviour rather
    /// than a rounded copy of something the code recomputes. Coding at the
    /// image's own canvas rather than at the universe's maximum matters: on a
    /// 256 px canvas the difference is 12 bits per anchor, which is several
    /// segments' worth of selection pressure.
    pub fn anchor_bits(&self, canvas_dim_px: f64) -> f64 {
        self.bits_per_anchor + 2.0 * (canvas_dim_px / REFERENCE_CANVAS_DIM_PX).log2()
    }

    /// Bits to code one scalar coordinate-like parameter.
    pub fn coordinate_bits(&self, canvas_dim_px: f64) -> f64 {
        0.5 * self.anchor_bits(canvas_dim_px)
    }

    /// The calibrated precision `bits_per_anchor` was formed at, in px,
    /// RECOVERED by inverting `bits_per_anchor = 2 log2(reference / precision)`.
    ///
    /// Not a second constant. If the frozen number moves, this moves with it,
    /// which is the difference between a derivation and a copy.
    pub fn coordinate_precision_px(&self) -> f64 {
        REFERENCE_CANVAS_DIM_PX / (self.bits_per_anchor / 2.0).exp2()
    }

    pub fn bits_per_anchor(&self) -> f64 {
        self.bits_per_anchor
    }
    pub fn bits_per_segment_family(&self) -> f64 {
        self.bits_per_segment_family
    }
    pub fn bits_per_relation(&self) -> f64 {
        self.bits_per_relation
    }

    /// `L_geometry` for one segment: naming its family, then its own scalar
    /// parameters and flags.
    pub fn segment_bits(&self, family: SpanFamily, canvas_dim_px: f64) -> f64 {
        self.bits_per_segment_family
            + family.free_parameters() as f64 * self.coordinate_bits(canvas_dim_px)
            + family.flag_bits()
    }
}

/// The frozen table, as the CODE has it.
///
/// The three values are cross-checked against `configs/GATES_V1.toml` by
/// `vice_bench::gates::tests::every_frozen_value_agrees_with_the_code_that_uses_it`,
/// and their DERIVATIONS are checked separately by `vice-bench`, which is the
/// only place that can see the model universe and the identifiability gate at
/// once:
///
/// | value | derivation |
/// |---|---|
/// | `bits_per_anchor = 31.029146` | `2 log2(16384 / 0.35)` — the universe's `canvas_dim_px.hi` over the frozen `[identifiability] observability_floor_px`, which is "smallest salient LENGTH in render px whose parameters stay recoverable" and is therefore exactly §14.5's "calibrated precision" |
/// | `bits_per_segment_family = 2.321928` | `log2(5)` — a uniform prefix code over the universe's five admissible segment families. Uniform because no frequency calibration exists; a frequency-weighted code is a later model-version change, not a tweak |
/// | `bits_per_relation = 2.584963` | `log2(6)` — a uniform prefix code over the universe's six relation families |
pub const GEOMETRY_CODE_TABLE_V1: GeometryCodeTable = GeometryCodeTable {
    bits_per_anchor: 31.029146,
    bits_per_segment_family: 2.321928,
    bits_per_relation: 2.584963,
};

/// `log2` of the binomial coefficient, by summing logarithms so it is finite
/// for the chain lengths this crate sees rather than overflowing a `u64`.
pub fn log2_binomial(n: usize, k: usize) -> f64 {
    if k > n {
        return f64::INFINITY;
    }
    let k = k.min(n - k);
    let mut acc = 0.0f64;
    for i in 0..k {
        acc += ((n - i) as f64).log2() - ((i + 1) as f64).log2();
    }
    acc
}

/// How many INDEPENDENT observations a sample stands for.
///
/// `weight_ds / corr_length_px`: the physical arclength the sample represents,
/// divided by §13's MEASURED correlation length along the chain. Not one, and
/// not one per sample — the number of samples is a property of the resampling
/// step, and a code length that counted samples would charge twice for the same
/// evidence when the step halves and would double when a sample is duplicated.
///
/// §14.5 requires the selection to be invariant to `0.25 / 0.5 / 1.0 px`
/// sampling and to duplicate samples, and this ratio is what makes it so: the
/// sum of `weight_ds` over a span is its arclength at any step, and a duplicated
/// sample carries `weight_ds` near zero. It is also the correlation-aware count
/// §17.1 asks for rather than an iid one.
///
/// `None` when the correlation length is not strictly positive: an observation
/// count divided by zero is not a large number, it is absent (F-0075).
pub fn independent_observations(weight_ds_px: f64, corr_length_px: f64) -> Option<f64> {
    (corr_length_px.is_finite() && corr_length_px > 0.0 && weight_ds_px.is_finite())
        .then(|| (weight_ds_px / corr_length_px).max(0.0))
}

/// The robust quantized residual code for ONE observation, in bits.
///
/// A Gaussian of standard deviation `halfwidth` quantized at `precision`,
/// with the square replaced by §14.4's Huber loss so a single outlying sample
/// cannot buy an arbitrary number of segments:
///
/// ```text
/// bits = rho(d_n / h) / (2 ln 2) + log2(h sqrt(2 pi) / precision)
/// ```
///
/// The second term is the normalising constant, and it is included rather than
/// dropped because `total_bits` claims to be a number of BITS. It is identical
/// for every candidate over the same sample, so it cancels in every comparison
/// — which is why dropping it would have been undetectable, and is why it is
/// stated here that it changes no ranking.
pub fn residual_bits(d_n_px: f64, halfwidth_px: f64, precision_px: f64) -> f64 {
    let u = d_n_px / halfwidth_px;
    crate::cost::rho(u) / (2.0 * std::f64::consts::LN_2)
        + (halfwidth_px * std::f64::consts::TAU.sqrt() / precision_px).log2()
}

/// The explicit code length of one grammar path over one chain, term by term.
///
/// Every field is a real number of bits and they are published separately,
/// because a single total cannot be argued with and four terms can.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct ChainCode {
    /// Anchors and per-segment family/parameter codes.
    pub geometry_bits: f64,
    /// The combinatorial code for WHICH samples are breakpoints, the code for
    /// how many there are, and one bit per interior node for its join kind.
    pub topology_bits: f64,
    /// `L_relations`: `bits_per_relation` per relation asserted about this
    /// chain. Zero when the chain asserts none.
    pub relation_bits: f64,
    /// The robust quantized residual code over the chain's samples, each
    /// charged exactly ONCE (§17.2).
    pub residual_bits: f64,
}

impl ChainCode {
    /// `L_geometry + L_topology + L_relations + L_residual`.
    ///
    /// **Not §14.5's `L_total`**: the pixel-byte likelihood, `L_paint` and
    /// `L_formation` are absent, and the first of those is M7's. Named
    /// `total_bits` and documented rather than named `l_total`, so nothing
    /// downstream can read this as the final score §14.5 defines.
    pub fn total_bits(&self) -> f64 {
        self.geometry_bits + self.topology_bits + self.relation_bits + self.residual_bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frozen_table_recovers_the_precision_it_was_formed_at() {
        let p = GEOMETRY_CODE_TABLE_V1.coordinate_precision_px();
        assert!(
            (p - 0.35).abs() < 1e-6,
            "bits_per_anchor = {} inverts to a precision of {p} px, not the frozen \
             observability floor of 0.35",
            GEOMETRY_CODE_TABLE_V1.bits_per_anchor()
        );
    }

    #[test]
    fn the_frozen_table_agrees_with_its_own_derivations() {
        let want_anchor = 2.0 * (REFERENCE_CANVAS_DIM_PX / 0.35).log2();
        assert!(
            (GEOMETRY_CODE_TABLE_V1.bits_per_anchor() - want_anchor).abs() < 1e-5,
            "frozen {} against 2 log2(16384 / 0.35) = {want_anchor}",
            GEOMETRY_CODE_TABLE_V1.bits_per_anchor()
        );
        assert!(
            (GEOMETRY_CODE_TABLE_V1.bits_per_segment_family() - 5f64.log2()).abs() < 1e-5,
            "frozen {} against log2(5)",
            GEOMETRY_CODE_TABLE_V1.bits_per_segment_family()
        );
        assert!(
            (GEOMETRY_CODE_TABLE_V1.bits_per_relation() - 6f64.log2()).abs() < 1e-5,
            "frozen {} against log2(6)",
            GEOMETRY_CODE_TABLE_V1.bits_per_relation()
        );
    }

    /// A table of zeros — which is the placeholder this section shipped as —
    /// cannot be minted. Without this the DP would price an unbounded grammar
    /// at nothing and select the most complex chain every time.
    #[test]
    fn a_table_of_zeros_is_not_a_code_table() {
        assert!(GeometryCodeTable::new(0.0, 0.0, 0.0).is_none());
        assert!(GeometryCodeTable::new(31.0, 0.0, 2.5).is_none());
        assert!(GeometryCodeTable::new(f64::NAN, 2.0, 2.0).is_none());
        assert!(GeometryCodeTable::new(31.0, 2.3, 2.6).is_some());
    }

    /// The canvas adjustment is the whole reason the code is not a constant per
    /// anchor, so both directions are measured.
    #[test]
    fn a_smaller_canvas_codes_an_anchor_in_fewer_bits() {
        let t = GEOMETRY_CODE_TABLE_V1;
        let big = t.anchor_bits(REFERENCE_CANVAS_DIM_PX);
        let small = t.anchor_bits(256.0);
        assert!((big - t.bits_per_anchor()).abs() < 1e-12);
        assert!(
            (big - small - 12.0).abs() < 1e-9,
            "16384 px codes an anchor in {big} bits and 256 px in {small}; the difference should \
             be 2 log2(64) = 12"
        );
        // And it is the precision, not the canvas, that sets the scale: at a
        // canvas equal to the precision an anchor costs nothing.
        assert!(t.anchor_bits(t.coordinate_precision_px()).abs() < 1e-6);
    }

    #[test]
    fn a_family_with_more_parameters_costs_more_bits() {
        let t = GEOMETRY_CODE_TABLE_V1;
        let b = |f| t.segment_bits(f, 256.0);
        assert!(b(SpanFamily::Line) < b(SpanFamily::CircularArc));
        assert!(b(SpanFamily::CircularArc) < b(SpanFamily::Quad));
        assert!(b(SpanFamily::Quad) < b(SpanFamily::Cubic));
        // A line is the family name and nothing else.
        assert!((b(SpanFamily::Line) - t.bits_per_segment_family()).abs() < 1e-12);
    }

    /// The combinatorial code is the real `log2 C(n, k)`, checked against a
    /// value computed the direct way at a size where the direct way is exact.
    #[test]
    fn the_breakpoint_code_is_the_binomial() {
        assert!((log2_binomial(10, 0)).abs() < 1e-12);
        assert!((log2_binomial(10, 10)).abs() < 1e-12);
        assert!((log2_binomial(10, 3) - 120f64.log2()).abs() < 1e-9);
        assert!((log2_binomial(52, 5) - 2_598_960f64.log2()).abs() < 1e-9);
        assert!(log2_binomial(3, 5).is_infinite());
    }

    /// The residual code is monotone in the deviation and grows LINEARLY beyond
    /// one corridor — the Huber knee — so one bad sample cannot pay for an
    /// unbounded number of segments.
    #[test]
    fn the_residual_code_is_robust_beyond_one_corridor() {
        let (h, p) = (0.35, 0.35);
        let at = |d| residual_bits(d, h, p);
        assert!(at(0.0) < at(h));
        assert!(at(h) < at(2.0 * h));
        let quadratic_would_be = at(0.0) + (10.0f64).powi(2) / (2.0 * std::f64::consts::LN_2);
        assert!(
            at(10.0 * h) < quadratic_would_be / 4.0,
            "at ten corridors the robust code charges {} bits against the {quadratic_would_be} a \
             square would",
            at(10.0 * h)
        );
    }
}
