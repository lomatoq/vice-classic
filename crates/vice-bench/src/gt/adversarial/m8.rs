use super::*;

pub(super) fn m8_adversarial_groups() -> Vec<GtSourceGroup> {
    let c = canvas();
    let mut out = Vec::new();

    // Three differently painted components with deliberately unequal areas.
    // A palette/label permutation must not let the smallest component vanish
    // or borrow the paint of a larger neighbour.
    {
        let mut b = builder();
        for (paint, bounds) in [
            (ink(0.90, 0.05, 0.04), (0.08, 0.12, 0.34, 0.40)),
            (ink(0.04, 0.82, 0.10), (0.58, 0.10, 0.91, 0.47)),
            (ink(0.05, 0.12, 0.92), (0.27, 0.59, 0.74, 0.88)),
        ] {
            let face = b.add_face(paint);
            b.add_polygon_ring(
                &rect(c * bounds.0, c * bounds.1, c * bounds.2, c * bounds.3),
                face,
                SceneBuilder::EXTERIOR,
            )
            .expect("multicolour adversary");
        }
        out.push(single_group(
            "adv/multicolor-permutation",
            "adversarial/multicolor-permutation",
            b,
            "three unequal components with permutation-sensitive paints",
            vec![SalientFeature::ThinFeature {
                min_width_px: c * 0.26,
            }],
        ));
    }

    // One supported but much smaller third paint component challenges
    // calibrated abstention without crossing the five-pixel court floor.
    {
        let mut b = builder();
        for (paint, bounds) in [
            (ink(0.88, 0.06, 0.06), (0.08, 0.18, 0.42, 0.78)),
            (ink(0.06, 0.80, 0.12), (0.55, 0.16, 0.91, 0.76)),
            (ink(0.08, 0.16, 0.90), (0.45, 0.82, 0.55, 0.92)),
        ] {
            let face = b.add_face(paint);
            b.add_polygon_ring(
                &rect(c * bounds.0, c * bounds.1, c * bounds.2, c * bounds.3),
                face,
                SceneBuilder::EXTERIOR,
            )
            .expect("small third paint adversary");
        }
        out.push(single_group(
            "adv/multicolor-small-third",
            "adversarial/multicolor-small-third",
            b,
            "two large paints and one supported small third paint",
            vec![SalientFeature::ThinFeature {
                min_width_px: c * 0.10,
            }],
        ));
    }

    // Four paints with two nearly aligned rows challenge component identity
    // and paint cardinality without an information-lost gap.
    {
        let mut b = builder();
        for (paint, bounds) in [
            (ink(0.84, 0.08, 0.08), (0.08, 0.18, 0.29, 0.46)),
            (ink(0.08, 0.74, 0.12), (0.31, 0.18, 0.52, 0.46)),
            (ink(0.08, 0.14, 0.88), (0.54, 0.18, 0.75, 0.46)),
            (ink(0.78, 0.62, 0.06), (0.77, 0.18, 0.92, 0.46)),
        ] {
            let face = b.add_face(paint);
            b.add_polygon_ring(
                &rect(c * bounds.0, c * bounds.1, c * bounds.2, c * bounds.3),
                face,
                SceneBuilder::EXTERIOR,
            )
            .expect("four-paint adversary");
        }
        out.push(single_group(
            "adv/multicolor-four-component",
            "adversarial/multicolor-four-component",
            b,
            "four nearby components with distinct paints",
            vec![SalientFeature::ThinFeature {
                min_width_px: c * 0.15,
            }],
        ));
    }

    out
}
