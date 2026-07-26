//! Digital topology convention (spec v1.3 §5.3).
//!
//! Binary raster topology always uses COMPLEMENTARY connectivity: if the
//! foreground is 4-connected the background is 8-connected and vice versa.
//! Using the same connectivity on both sides makes Euler/hole reasoning
//! inconsistent (checkerboard paradox) and is forbidden.
//!
//! The type below stores only the foreground arm; the background arm is
//! DERIVED, so a non-complementary pair is unrepresentable by construction.
//! Both arms are legitimate candidate hypotheses; near-saddle inputs must
//! spawn both (M4.5), never resolve the tie by iteration order.

/// Pixel adjacency for one side of a binary partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelConnectivity {
    /// Edge-adjacency only.
    Four,
    /// Edge + corner adjacency.
    Eight,
}

impl PixelConnectivity {
    /// The complementary connectivity of the opposite side.
    pub fn complement(self) -> PixelConnectivity {
        match self {
            PixelConnectivity::Four => PixelConnectivity::Eight,
            PixelConnectivity::Eight => PixelConnectivity::Four,
        }
    }
}

/// A complementary foreground/background connectivity pair. Only the
/// foreground arm is stored; `background()` is always the complement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComplementaryConnectivity {
    foreground: PixelConnectivity,
}

impl ComplementaryConnectivity {
    pub fn new(foreground: PixelConnectivity) -> Self {
        ComplementaryConnectivity { foreground }
    }

    pub fn foreground(self) -> PixelConnectivity {
        self.foreground
    }

    pub fn background(self) -> PixelConnectivity {
        self.foreground.complement()
    }

    /// Both admissible arms, in a fixed deterministic order.
    pub fn arms() -> [ComplementaryConnectivity; 2] {
        [
            ComplementaryConnectivity::new(PixelConnectivity::Four),
            ComplementaryConnectivity::new(PixelConnectivity::Eight),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_is_always_the_complement() {
        for arm in ComplementaryConnectivity::arms() {
            assert_ne!(arm.foreground(), arm.background());
            assert_eq!(arm.foreground().complement(), arm.background());
            assert_eq!(arm.background().complement(), arm.foreground());
        }
    }

    #[test]
    fn both_arms_are_available_and_distinct() {
        let [a, b] = ComplementaryConnectivity::arms();
        assert_eq!(a.foreground(), PixelConnectivity::Four);
        assert_eq!(b.foreground(), PixelConnectivity::Eight);
        assert_ne!(a, b);
    }
}
