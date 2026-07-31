use vice_geom::Pt;

fn point_segment_distance(point: Pt, a: Pt, b: Pt) -> f64 {
    let segment = b - a;
    let length_sq = segment.length_sq();
    if length_sq == 0.0 {
        point.dist(a)
    } else {
        let t = ((point - a).dot(segment) / length_sq).clamp(0.0, 1.0);
        point.dist(a + segment * t)
    }
}

#[derive(Debug, Clone, Copy)]
struct SegmentBounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl SegmentBounds {
    fn of_segment(a: Pt, b: Pt) -> Self {
        Self {
            min_x: a.x.min(b.x),
            min_y: a.y.min(b.y),
            max_x: a.x.max(b.x),
            max_y: a.y.max(b.y),
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    fn distance_sq(self, point: Pt) -> f64 {
        let dx = if point.x < self.min_x {
            self.min_x - point.x
        } else if point.x > self.max_x {
            point.x - self.max_x
        } else {
            0.0
        };
        let dy = if point.y < self.min_y {
            self.min_y - point.y
        } else if point.y > self.max_y {
            point.y - self.max_y
        } else {
            0.0
        };
        dx.mul_add(dx, dy * dy)
    }
}

#[derive(Debug)]
enum SegmentNode {
    Leaf {
        bounds: SegmentBounds,
        segments: Vec<(Pt, Pt)>,
    },
    Branch {
        bounds: SegmentBounds,
        left: Box<SegmentNode>,
        right: Box<SegmentNode>,
    },
}

impl SegmentNode {
    const LEAF_SEGMENTS: usize = 8;

    fn bounds(&self) -> SegmentBounds {
        match self {
            Self::Leaf { bounds, .. } | Self::Branch { bounds, .. } => *bounds,
        }
    }

    fn build(mut segments: Vec<(Pt, Pt)>) -> Self {
        let bounds = segments
            .iter()
            .map(|(a, b)| SegmentBounds::of_segment(*a, *b))
            .reduce(SegmentBounds::union)
            .expect("a segment index is never empty");
        if segments.len() <= Self::LEAF_SEGMENTS {
            return Self::Leaf { bounds, segments };
        }
        let split_x = bounds.max_x - bounds.min_x >= bounds.max_y - bounds.min_y;
        segments.sort_by(|(a0, a1), (b0, b1)| {
            let ac = if split_x { a0.x + a1.x } else { a0.y + a1.y };
            let bc = if split_x { b0.x + b1.x } else { b0.y + b1.y };
            ac.total_cmp(&bc)
                .then_with(|| a0.x.total_cmp(&b0.x))
                .then_with(|| a0.y.total_cmp(&b0.y))
                .then_with(|| a1.x.total_cmp(&b1.x))
                .then_with(|| a1.y.total_cmp(&b1.y))
        });
        let right = segments.split_off(segments.len() / 2);
        Self::Branch {
            bounds,
            left: Box::new(Self::build(segments)),
            right: Box::new(Self::build(right)),
        }
    }

    fn nearest(&self, point: Pt, best: &mut f64) {
        if self.bounds().distance_sq(point) >= *best * *best {
            return;
        }
        match self {
            Self::Leaf { segments, .. } => {
                for (a, b) in segments {
                    let distance = point_segment_distance(point, *a, *b);
                    *best = (*best).min(distance);
                }
            }
            Self::Branch { left, right, .. } => {
                let left_distance = left.bounds().distance_sq(point);
                let right_distance = right.bounds().distance_sq(point);
                if left_distance <= right_distance {
                    left.nearest(point, best);
                    right.nearest(point, best);
                } else {
                    right.nearest(point, best);
                    left.nearest(point, best);
                }
            }
        }
    }
}

#[derive(Debug)]
pub(super) struct PolylineIndex(Option<SegmentNode>);

impl PolylineIndex {
    pub(super) fn new(polyline: &[Pt]) -> Self {
        let segments = polyline
            .windows(2)
            .map(|segment| (segment[0], segment[1]))
            .collect::<Vec<_>>();
        Self((!segments.is_empty()).then(|| SegmentNode::build(segments)))
    }

    pub(super) fn distance(&self, point: Pt) -> f64 {
        let Some(root) = &self.0 else {
            return f64::INFINITY;
        };
        let mut best = f64::INFINITY;
        root.nearest(point, &mut best);
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brute(point: Pt, polyline: &[Pt]) -> f64 {
        polyline
            .windows(2)
            .map(|segment| point_segment_distance(point, segment[0], segment[1]))
            .fold(f64::INFINITY, f64::min)
    }

    #[test]
    fn segment_index_is_exactly_equivalent_to_brute_force_distance() {
        let polylines = [
            vec![
                Pt::new(0.0, 0.0),
                Pt::new(8.0, 0.0),
                Pt::new(8.0, 6.0),
                Pt::new(0.0, 6.0),
                Pt::new(0.0, 0.0),
            ],
            (0..40)
                .map(|index| {
                    let x = f64::from(index) * 0.25;
                    Pt::new(x, (x * 0.7).sin() * 2.0 + 3.0)
                })
                .collect(),
        ];
        for polyline in &polylines {
            let index = PolylineIndex::new(polyline);
            for yi in -8..=32 {
                for xi in -8..=48 {
                    let point = Pt::new(f64::from(xi) * 0.25, f64::from(yi) * 0.25);
                    assert_eq!(
                        index.distance(point).to_bits(),
                        brute(point, polyline).to_bits()
                    );
                }
            }
        }
    }
}
