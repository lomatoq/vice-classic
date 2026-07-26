//! The TYPED numeric domain of the renderer (ADR-0013).
//!
//! Why this type exists (F-0008). The red-team pass found three separate
//! MAJOR defects with one root: *the numeric domain of the system was
//! nowhere expressed as a type*. The renderer froze a per-pixel tolerance
//! of 1e-9 justified "for canvases up to 2^12 px", while the only
//! enforced limit (`MAX_COVERAGE_ELEMENTS`) admitted canvases up to 2^26
//! and validation admitted ANY finite coordinate. Between the justified
//! domain and the admitted domain stood nothing, and inside that gap the
//! renderer returned wrong values SILENTLY — the error is common-mode
//! across faces, so both partition guards are structurally blind to it.
//!
//! A tolerance is only meaningful together with the domain on which it was
//! proven. This type carries that domain, [`PartitionTolerances`] is
//! derived FROM it, and the render path refuses anything outside it with a
//! typed error rather than returning numbers whose error is unbounded.
//!
//! [`PartitionTolerances`]: crate::partition::PartitionTolerances

/// The region of coordinate space and canvas size on which the renderer's
/// numeric error bounds are proven and enforced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumericDomain {
    /// Largest admissible `|x|`, `|y|` of any tessellated mesh point (px).
    /// Bounds the representation error of an edge position: a coordinate
    /// of magnitude `M` is only known to `ulp(M)`, and no integration
    /// scheme can recover accuracy the input does not carry.
    pub max_abs_coord_px: f64,
    /// Largest admissible canvas dimension (px), per axis.
    pub max_canvas_dim_px: u32,
}

impl NumericDomain {
    /// The frozen M2 domain.
    ///
    /// `max_canvas_dim_px = 2^14` (16384): the raster class this core
    /// targets (logos, icons, clipart) sits far below it, and it keeps the
    /// per-axis size inside the range where `ulp` of a canvas coordinate
    /// is negligible against the partition tolerances.
    ///
    /// `max_abs_coord_px = 2^16` (65536) = 4x the canvas bound: geometry
    /// legitimately leaves the canvas (ADR-0009 — control-polygon
    /// overshoot, and M7 moving handles across the edge), so the
    /// coordinate bound must be strictly more generous than the canvas
    /// bound. At that bound `coverage_error_bound_px` is 2.33e-10: a 4.3x
    /// margin under the frozen 1e-9 partition tolerance, and itself a ~95x
    /// conservative envelope over the measured worst case of 2.4e-12
    /// (`tests/coverage_domain_gate.rs`).
    pub const fn m2_default() -> NumericDomain {
        NumericDomain {
            max_abs_coord_px: 65536.0,
            max_canvas_dim_px: 1 << 14,
        }
    }

    /// A domain with an explicitly chosen coordinate bound. Callers who
    /// need a wider domain get a correspondingly WIDER derived tolerance
    /// (see [`Self::coverage_error_bound_px`]) — the trade is explicit,
    /// never silent.
    pub fn with_max_abs_coord_px(max_abs_coord_px: f64) -> Option<NumericDomain> {
        (max_abs_coord_px.is_finite() && max_abs_coord_px > 0.0).then_some(NumericDomain {
            max_abs_coord_px,
            ..NumericDomain::m2_default()
        })
    }

    /// Certified-style upper bound (px of pixel area) on the per-pixel
    /// coverage error attributable to coordinate magnitude inside this
    /// domain.
    ///
    /// Form: `16 · eps · M`, where `M` is the larger of the coordinate and
    /// canvas bounds. Basis: with the column-local trapezoid (F-0008) the
    /// integration itself contributes only a few ulp(1); what remains
    /// scales with `ulp(M) = M · 2^-52`. The red team measured the
    /// pre-fix law at ≈ 1.1e-16 · M (7.04e-13 at M = 2^12 rising to
    /// 1.73e-9 at M = 2^24); `16 · eps ≈ 3.6e-15` per unit is ~32x that
    /// slope, and the in-tree translation oracle
    /// (`coverage_domain_gate.rs`) asserts the measured error stays under
    /// this bound across the domain.
    pub fn coverage_error_bound_px(&self) -> f64 {
        let m = self.max_abs_coord_px.max(f64::from(self.max_canvas_dim_px));
        16.0 * f64::EPSILON * m
    }

    /// Is a single coordinate inside the domain?
    pub fn contains_coord(&self, v: f64) -> bool {
        v.is_finite() && v.abs() <= self.max_abs_coord_px
    }
}

impl Default for NumericDomain {
    fn default() -> Self {
        NumericDomain::m2_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_domain_keeps_the_frozen_tolerance_valid() {
        let d = NumericDomain::m2_default();
        // The frozen 1e-9 must dominate the derived bound with real margin
        // (measured 4.3x), so the frozen constant is a consequence of the
        // domain rather than a wish independent of it.
        let bound = d.coverage_error_bound_px();
        assert!(
            bound * 4.0 < 1e-9,
            "derived bound {bound:e} must sit >=4x below the frozen 1e-9"
        );
        assert!(d.contains_coord(65536.0));
        assert!(!d.contains_coord(65536.0_f64.next_up()));
        assert!(!d.contains_coord(f64::INFINITY));
        assert!(!d.contains_coord(f64::NAN));
        assert!(d.contains_coord(-1024.0));
    }

    #[test]
    fn wider_domain_widens_the_bound_explicitly() {
        let wide = NumericDomain::with_max_abs_coord_px(f64::from(1u32 << 24)).unwrap();
        assert!(wide.coverage_error_bound_px() > 1e-9);
        assert!(NumericDomain::with_max_abs_coord_px(0.0).is_none());
        assert!(NumericDomain::with_max_abs_coord_px(f64::NAN).is_none());
    }
}
