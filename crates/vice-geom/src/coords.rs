//! The ONLY pixel-grid ↔ continuous-canvas transform module.
//!
//! Frozen conventions (spec v1.3 §5.1, docs/COORDINATE_CONVENTION.md):
//! - frame: x right, y down;
//! - pixel `(ix, iy)` occupies the closed box `[ix, ix+1] × [iy, iy+1]`;
//! - pixel center is `(ix + 0.5, iy + 0.5)`;
//! - canvas of a `W × H` image is `[0, W] × [0, H]`;
//! - internal geometry is f64.
//!
//! Ad-hoc `±0.5` adjustments anywhere else in the pipeline are forbidden;
//! every future crate must call these functions.

use serde::{Deserialize, Serialize};

use crate::vec2::Pt;

/// Center of pixel `(ix, iy)`: `(ix + 0.5, iy + 0.5)`.
pub fn pixel_center(ix: u32, iy: u32) -> Pt {
    Pt::new(f64::from(ix) + 0.5, f64::from(iy) + 0.5)
}

/// The closed unit box `[ix, ix+1] × [iy, iy+1]` of pixel `(ix, iy)`.
pub fn pixel_box(ix: u32, iy: u32) -> Aabb {
    Aabb::from_min_max(
        Pt::new(f64::from(ix), f64::from(iy)),
        Pt::new(f64::from(ix) + 1.0, f64::from(iy) + 1.0),
    )
}

/// The canvas box `[0, W] × [0, H]` of a `W × H` image.
pub fn canvas_box(width_px: u32, height_px: u32) -> Aabb {
    Aabb::from_min_max(Pt::ZERO, Pt::new(f64::from(width_px), f64::from(height_px)))
}

/// Closed axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Aabb {
    min: Pt,
    max: Pt,
}

impl Aabb {
    /// `min` must be component-wise <= `max` (checked in debug builds).
    pub fn from_min_max(min: Pt, max: Pt) -> Aabb {
        debug_assert!(min.x <= max.x && min.y <= max.y, "inverted Aabb");
        Aabb { min, max }
    }

    /// Smallest box containing all points; `None` for an empty slice.
    pub fn from_points(points: &[Pt]) -> Option<Aabb> {
        let first = *points.first()?;
        let mut b = Aabb {
            min: first,
            max: first,
        };
        for p in &points[1..] {
            b.min.x = b.min.x.min(p.x);
            b.min.y = b.min.y.min(p.y);
            b.max.x = b.max.x.max(p.x);
            b.max.y = b.max.y.max(p.y);
        }
        Some(b)
    }

    pub fn min(&self) -> Pt {
        self.min
    }

    pub fn max(&self) -> Pt {
        self.max
    }

    pub fn union(&self, o: &Aabb) -> Aabb {
        Aabb {
            min: Pt::new(self.min.x.min(o.min.x), self.min.y.min(o.min.y)),
            max: Pt::new(self.max.x.max(o.max.x), self.max.y.max(o.max.y)),
        }
    }

    /// Grow the box by `by >= 0` on every side, then widen each bound by one
    /// ulp outward. The extra ulp makes the result a certified OUTER bound
    /// even though `min - by` / `max + by` round to nearest: an inflated box
    /// used as a conservative curve enclosure must never under-cover.
    pub fn inflate_outward(&self, by: f64) -> Aabb {
        debug_assert!(by >= 0.0 && by.is_finite());
        Aabb {
            min: Pt::new((self.min.x - by).next_down(), (self.min.y - by).next_down()),
            max: Pt::new((self.max.x + by).next_up(), (self.max.y + by).next_up()),
        }
    }

    /// True when the CLOSED boxes share no point. Used as a certificate of
    /// non-intersection for the curves they enclose.
    pub fn strictly_disjoint(&self, o: &Aabb) -> bool {
        self.max.x < o.min.x || o.max.x < self.min.x || self.max.y < o.min.y || o.max.y < self.min.y
    }

    /// Closed-box point containment.
    pub fn contains(&self, p: Pt) -> bool {
        self.min.x <= p.x && p.x <= self.max.x && self.min.y <= p.y && p.y <= self.max.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_center_is_plus_half() {
        assert_eq!(pixel_center(0, 0), Pt::new(0.5, 0.5));
        assert_eq!(pixel_center(7, 3), Pt::new(7.5, 3.5));
    }

    #[test]
    fn pixel_box_is_unit_closed_box() {
        let b = pixel_box(2, 3);
        assert_eq!(b.min(), Pt::new(2.0, 3.0));
        assert_eq!(b.max(), Pt::new(3.0, 4.0));
        assert!(b.contains(pixel_center(2, 3)));
        // Closed box: corners belong to the pixel.
        assert!(b.contains(Pt::new(2.0, 3.0)));
        assert!(b.contains(Pt::new(3.0, 4.0)));
    }

    #[test]
    fn canvas_box_spans_zero_to_wh() {
        let b = canvas_box(64, 32);
        assert_eq!(b.min(), Pt::ZERO);
        assert_eq!(b.max(), Pt::new(64.0, 32.0));
    }

    #[test]
    fn adjacent_pixels_share_a_closed_edge() {
        // [x, x+1] boxes: pixel (0,0) and (1,0) share the x=1 edge.
        let a = pixel_box(0, 0);
        let b = pixel_box(1, 0);
        assert!(!a.strictly_disjoint(&b));
        let c = pixel_box(2, 0);
        assert!(a.strictly_disjoint(&c));
    }

    #[test]
    fn from_points_and_union() {
        let b =
            Aabb::from_points(&[Pt::new(1.0, 5.0), Pt::new(-2.0, 3.0), Pt::new(0.0, 9.0)]).unwrap();
        assert_eq!(b.min(), Pt::new(-2.0, 3.0));
        assert_eq!(b.max(), Pt::new(1.0, 9.0));
        assert!(Aabb::from_points(&[]).is_none());

        let u = b.union(&pixel_box(10, 10));
        assert_eq!(u.min(), Pt::new(-2.0, 3.0));
        assert_eq!(u.max(), Pt::new(11.0, 11.0));
    }

    #[test]
    fn inflate_outward_is_a_certified_outer_bound() {
        let b = Aabb::from_min_max(Pt::new(0.1, 0.2), Pt::new(0.3, 0.4));
        let by = 0.7;
        let g = b.inflate_outward(by);
        // Strictly wider than the naively rounded arithmetic on every side,
        // so the exact-real inflated corners are certainly covered.
        assert!(g.min().x < 0.1 - by);
        assert!(g.min().y < 0.2 - by);
        assert!(g.max().x > 0.3 + by);
        assert!(g.max().y > 0.4 + by);
    }
}
