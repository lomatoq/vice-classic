//! `Vec2`/`Pt`: the only 2D vector/point type of the core.
//!
//! Convention (spec v1.3 §5.1): x grows to the right, y grows DOWN. All
//! internal geometry is f64. `Pt` is an alias of `Vec2` — positions and
//! displacements share one representation; the distinction is documented at
//! call sites, not encoded in a second type (M1 has no call site that would
//! pay for the conversion noise).
//!
//! Clean-room implementation: written from the definitions, no donor code.

use std::cmp::Ordering;
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// True exactly for `-0.0` (IEEE negative zero). Canonical serialization
/// forbids negative zero (spec §5.5), and `f64::==` cannot see it.
pub fn is_negative_zero(v: f64) -> bool {
    v == 0.0 && v.is_sign_negative()
}

/// 2D vector / point in the fixed frame (x right, y down), f64.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

/// A position in canvas coordinates. Same representation as [`Vec2`].
pub type Pt = Vec2;

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Vec2 { x, y }
    }

    /// Dot product.
    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }

    /// z-component of the 3D cross product of `self` and `o`.
    ///
    /// Sign convention is algebraic (right-handed axes). NOTE: in the image
    /// frame y points DOWN, so a positive cross corresponds to a turn that
    /// looks CLOCKWISE on screen. Topology code must use
    /// [`crate::predicates::orient2d`] instead of the sign of this value.
    pub fn cross(self, o: Vec2) -> f64 {
        self.x * o.y - self.y * o.x
    }

    pub fn length_sq(self) -> f64 {
        self.dot(self)
    }

    pub fn length(self) -> f64 {
        self.length_sq().sqrt()
    }

    pub fn dist_sq(self, o: Vec2) -> f64 {
        (o - self).length_sq()
    }

    pub fn dist(self, o: Vec2) -> f64 {
        self.dist_sq(o).sqrt()
    }

    /// Both components finite (no NaN, no ±Inf).
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Either component is IEEE `-0.0`.
    pub fn has_negative_zero(self) -> bool {
        is_negative_zero(self.x) || is_negative_zero(self.y)
    }

    /// Total lexicographic order: by x, then by y, using `f64::total_cmp`.
    /// On canonical (finite, no `-0.0`) values this coincides with the
    /// numeric order and is a total order — used for deterministic sorting.
    pub fn lex_cmp(self, o: Vec2) -> Ordering {
        self.x.total_cmp(&o.x).then(self.y.total_cmp(&o.y))
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        *self = *self + o;
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, o: Vec2) {
        *self = *self - o;
    }
}

impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f64) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
}

impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2::new(-self.x, -self.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_ops() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, -1.0);
        assert_eq!(a + b, Vec2::new(4.0, 1.0));
        assert_eq!(a - b, Vec2::new(-2.0, 3.0));
        assert_eq!(a * 2.0, Vec2::new(2.0, 4.0));
        assert_eq!(-a, Vec2::new(-1.0, -2.0));
        assert_eq!(a.dot(b), 1.0);
        assert_eq!(a.cross(b), -7.0);
        assert_eq!(Vec2::new(3.0, 4.0).length(), 5.0);
        assert_eq!(Vec2::new(1.0, 1.0).dist(Vec2::new(4.0, 5.0)), 5.0);
    }

    #[test]
    fn cross_sign_matches_algebraic_convention() {
        // (1,0) x (0,1) = +1 algebraically; in the y-down frame (0,1) points
        // down, so the positive cross is the screen-clockwise turn.
        assert_eq!(Vec2::new(1.0, 0.0).cross(Vec2::new(0.0, 1.0)), 1.0);
    }

    #[test]
    fn negative_zero_detection() {
        assert!(is_negative_zero(-0.0));
        assert!(!is_negative_zero(0.0));
        assert!(!is_negative_zero(-1.0));
        assert!(Vec2::new(-0.0, 1.0).has_negative_zero());
        assert!(!Vec2::new(0.0, 1.0).has_negative_zero());
    }

    #[test]
    fn finiteness() {
        assert!(Vec2::new(1.0, 2.0).is_finite());
        assert!(!Vec2::new(f64::NAN, 0.0).is_finite());
        assert!(!Vec2::new(0.0, f64::INFINITY).is_finite());
    }

    #[test]
    fn lex_cmp_is_total_and_lexicographic() {
        let a = Vec2::new(1.0, 5.0);
        let b = Vec2::new(2.0, 0.0);
        let c = Vec2::new(1.0, 6.0);
        assert_eq!(a.lex_cmp(b), Ordering::Less);
        assert_eq!(a.lex_cmp(c), Ordering::Less);
        assert_eq!(a.lex_cmp(a), Ordering::Equal);
        assert_eq!(b.lex_cmp(a), Ordering::Greater);
    }
}
