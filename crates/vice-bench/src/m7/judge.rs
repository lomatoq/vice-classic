use super::*;

pub(super) fn judge_witness(
    truth_scene: &GtScene,
    cell: &DegradationCell,
    witness: &vice_core::CalibrationWitness,
) -> Result<(TopologyComparison, BoundaryTail, u8), String> {
    let selected = vice_ir::parse_scene(&witness.scene_json)
        .map_err(|error| format!("parse selected scene: {error}"))?;
    let selected = ValidatedScene::new(selected)
        .map_err(|error| format!("validate selected scene: {error}"))?;
    let selected_mesh = CertifiedMesh::from_scene(&selected, RenderOptions::default())
        .map_err(|error| format!("certify selected scene: {error}"))?;
    let selected_truth = PartitionTruth::measure(&selected, &selected_mesh)
        .map_err(|error| format!("measure selected partition: {error}"))?;
    let truth = truth_scene.partition_truth();
    let topology = TopologyComparison {
        truth_visible_faces: truth.visible_faces,
        selected_visible_faces: selected_truth.visible_faces,
        truth_components: truth.components,
        selected_components: selected_truth.components,
        truth_holes: truth.holes,
        selected_holes: selected_truth.holes,
        truth_exterior: truth.exterior_model.to_string(),
        selected_exterior: selected_truth.exterior_model.to_string(),
        exact: truth.visible_faces == selected_truth.visible_faces
            && truth.components == selected_truth.components
            && truth.holes == selected_truth.holes
            && truth.exterior_model == selected_truth.exterior_model,
    };
    let scale = f64::from(cell.size_px) / f64::from(AUTHORING_CANVAS_PX);
    let truth_segments = mesh_segments(
        truth_scene.certified(),
        scale,
        cell.subpixel_dx,
        cell.subpixel_dy,
    );
    let selected_segments = mesh_segments(&selected_mesh, 1.0, 0.0, 0.0);
    let mut distances = directed_distances(&truth_segments, &selected_segments);
    distances.extend(directed_distances(&selected_segments, &truth_segments));
    if distances.is_empty() {
        return Err("boundary court received no finite distance samples".into());
    }
    distances.sort_by(f64::total_cmp);
    let boundary = BoundaryTail {
        samples: distances.len() as u64,
        p95_px: quantile(&distances, 0.95),
        p99_px: quantile(&distances, 0.99),
        max_px: *distances.last().expect("nonempty"),
    };
    let paint_delta = palette_code_delta(&truth.palette, &selected_truth.palette);
    Ok((topology, boundary, paint_delta))
}

fn mesh_segments(mesh: &CertifiedMesh, scale: f64, dx: f64, dy: f64) -> Vec<(Pt, Pt)> {
    mesh.mesh()
        .boundary_polylines
        .iter()
        .flat_map(|boundary| boundary.points.windows(2))
        .map(|pair| {
            (
                Pt::new(pair[0].x * scale + dx, pair[0].y * scale + dy),
                Pt::new(pair[1].x * scale + dx, pair[1].y * scale + dy),
            )
        })
        .filter(|(a, b)| a.is_finite() && b.is_finite() && *a != *b)
        .collect()
}

pub(super) fn directed_distances(source: &[(Pt, Pt)], target: &[(Pt, Pt)]) -> Vec<f64> {
    if target.is_empty() {
        return Vec::new();
    }
    let spatial_index = SegmentIndex::build(target.to_vec());
    let mut out = Vec::new();
    for &(a, b) in source {
        let length = a.dist(b);
        let pieces = (length / BOUNDARY_SAMPLE_STEP_PX).ceil().max(1.0) as usize;
        for sample_index in 0..=pieces {
            let t = sample_index as f64 / pieces as f64;
            let point = a * (1.0 - t) + b * t;
            let nearest = spatial_index.nearest(point);
            if nearest.is_finite() {
                out.push(nearest);
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Bounds {
    min: Pt,
    max: Pt,
}

impl Bounds {
    fn of(segments: &[(Pt, Pt)]) -> Self {
        let mut bounds = Self {
            min: Pt::new(f64::INFINITY, f64::INFINITY),
            max: Pt::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
        };
        for &(a, b) in segments {
            for point in [a, b] {
                bounds.min.x = bounds.min.x.min(point.x);
                bounds.min.y = bounds.min.y.min(point.y);
                bounds.max.x = bounds.max.x.max(point.x);
                bounds.max.y = bounds.max.y.max(point.y);
            }
        }
        bounds
    }

    fn distance(self, point: Pt) -> f64 {
        let dx = if point.x < self.min.x {
            self.min.x - point.x
        } else if point.x > self.max.x {
            point.x - self.max.x
        } else {
            0.0
        };
        let dy = if point.y < self.min.y {
            self.min.y - point.y
        } else if point.y > self.max.y {
            point.y - self.max.y
        } else {
            0.0
        };
        dx.hypot(dy)
    }
}

pub(super) enum SegmentIndex {
    Leaf {
        bounds: Bounds,
        segments: Vec<(Pt, Pt)>,
    },
    Branch {
        bounds: Bounds,
        left: Box<SegmentIndex>,
        right: Box<SegmentIndex>,
    },
}

impl SegmentIndex {
    pub(super) fn build(mut segments: Vec<(Pt, Pt)>) -> Self {
        let bounds = Bounds::of(&segments);
        if segments.len() <= 8 {
            return Self::Leaf { bounds, segments };
        }
        let split_x = bounds.max.x - bounds.min.x >= bounds.max.y - bounds.min.y;
        segments.sort_by(|left, right| {
            let midpoint = |segment: &(Pt, Pt)| {
                if split_x {
                    segment.0.x + segment.1.x
                } else {
                    segment.0.y + segment.1.y
                }
            };
            midpoint(left).total_cmp(&midpoint(right))
        });
        let right_segments = segments.split_off(segments.len() / 2);
        Self::Branch {
            bounds,
            left: Box::new(Self::build(segments)),
            right: Box::new(Self::build(right_segments)),
        }
    }

    fn bounds(&self) -> Bounds {
        match self {
            Self::Leaf { bounds, .. } | Self::Branch { bounds, .. } => *bounds,
        }
    }

    pub(super) fn nearest(&self, point: Pt) -> f64 {
        self.nearest_bounded(point, f64::INFINITY)
    }

    fn nearest_bounded(&self, point: Pt, best: f64) -> f64 {
        if self.bounds().distance(point) >= best {
            return best;
        }
        match self {
            Self::Leaf { segments, .. } => segments
                .iter()
                .map(|&(a, b)| point_segment_distance(point, a, b))
                .fold(best, f64::min),
            Self::Branch { left, right, .. } => {
                let (first, second) =
                    if left.bounds().distance(point) <= right.bounds().distance(point) {
                        (left, right)
                    } else {
                        (right, left)
                    };
                let best = first.nearest_bounded(point, best);
                second.nearest_bounded(point, best)
            }
        }
    }
}

pub(super) fn point_segment_distance(point: Pt, a: Pt, b: Pt) -> f64 {
    let delta = b - a;
    let denominator = delta.dot(delta);
    if denominator <= 0.0 {
        return point.dist(a);
    }
    let t = ((point - a).dot(delta) / denominator).clamp(0.0, 1.0);
    point.dist(a + delta * t)
}

pub(super) fn quantile(sorted: &[f64], q: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * q).ceil() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn palette_code_delta(truth: &[[f64; 3]], selected: &[[f64; 3]]) -> u8 {
    if truth.len() != selected.len() || truth.is_empty() {
        return u8::MAX;
    }
    truth
        .iter()
        .map(|expected| {
            selected
                .iter()
                .map(|actual| {
                    (0..3)
                        .map(|channel| {
                            linear_to_srgb_u8(expected[channel])
                                .abs_diff(linear_to_srgb_u8(actual[channel]))
                        })
                        .max()
                        .unwrap_or(0)
                })
                .min()
                .unwrap_or(u8::MAX)
        })
        .max()
        .unwrap_or(u8::MAX)
}

pub(super) fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        writer
            .write_image_data(rgba)
            .map_err(|error| error.to_string())?;
        writer.finish().map_err(|error| error.to_string())?;
    }
    Ok(bytes)
}
