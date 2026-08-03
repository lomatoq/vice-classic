use super::*;

fn point_segment_distance(p: Pt, a: Pt, b: Pt) -> f64 {
    let d = b - a;
    let length_sq = d.length_sq();
    if length_sq == 0.0 {
        p.dist(a)
    } else {
        let t = ((p - a).dot(d) / length_sq).clamp(0.0, 1.0);
        p.dist(a + d * t)
    }
}

pub(super) fn directed_polyline_distance(points: &[Pt], target: &[Pt]) -> f64 {
    points.iter().fold(0.0f64, |worst, point| {
        let best = target
            .windows(2)
            .map(|segment| point_segment_distance(*point, segment[0], segment[1]))
            .fold(f64::INFINITY, f64::min);
        worst.max(best)
    })
}

fn segment_distance(a: Pt, b: Pt, c: Pt, d: Pt) -> f64 {
    if closed_segments_intersect(a, b, c, d) {
        0.0
    } else {
        point_segment_distance(a, c, d)
            .min(point_segment_distance(b, c, d))
            .min(point_segment_distance(c, a, b))
            .min(point_segment_distance(d, a, b))
    }
}

fn endpoint_vertex(
    scene: &VectorScene,
    boundary: usize,
    point_index: usize,
    point_count: usize,
) -> Option<VertexId> {
    let b = &scene.graph.boundaries[boundary];
    if point_index == 0 {
        Some(b.start_vertex)
    } else if point_index + 1 == point_count {
        Some(b.end_vertex)
    } else {
        None
    }
}

fn allowed_touch(
    scene: &VectorScene,
    mesh: &vice_render::RenderMesh,
    ba: usize,
    sa: usize,
    bb: usize,
    sb: usize,
) -> bool {
    let pa = &mesh.boundary_polylines[ba].points;
    let pb = &mesh.boundary_polylines[bb].points;
    let a = [pa[sa], pa[sa + 1]];
    let b = [pb[sb], pb[sb + 1]];
    if ba == bb {
        let adjacent = sa.abs_diff(sb) == 1
            || (scene.graph.boundaries[ba].start_vertex == scene.graph.boundaries[ba].end_vertex
                && ((sa == 0 && sb + 2 == pa.len()) || (sb == 0 && sa + 2 == pa.len())));
        if !adjacent {
            return false;
        }
        let Some(shared) = a.into_iter().find(|p| b.contains(p)) else {
            return false;
        };
        let other_a = if a[0] == shared { a[1] } else { a[0] };
        let other_b = if b[0] == shared { b[1] } else { b[0] };
        return !shared_endpoint_segments_overlap(shared, other_a, other_b);
    }
    for (ia, point_a) in [(sa, a[0]), (sa + 1, a[1])] {
        for (ib, point_b) in [(sb, b[0]), (sb + 1, b[1])] {
            if point_a != point_b {
                continue;
            }
            let Some(va) = endpoint_vertex(scene, ba, ia, pa.len()) else {
                continue;
            };
            let Some(vb) = endpoint_vertex(scene, bb, ib, pb.len()) else {
                continue;
            };
            if va == vb {
                let other_a = if a[0] == point_a { a[1] } else { a[0] };
                let other_b = if b[0] == point_b { b[1] } else { b[0] };
                return !shared_endpoint_segments_overlap(point_a, other_a, other_b);
            }
        }
    }
    false
}

pub(super) fn verify_curve_separation(
    scene: &VectorScene,
    mesh: &vice_render::RenderMesh,
    margin: f64,
) -> Result<u64, VerificationError> {
    let mut checks = 0u64;
    for ba in 0..mesh.boundary_polylines.len() {
        let pa = &mesh.boundary_polylines[ba];
        for bb in ba..mesh.boundary_polylines.len() {
            let pb = &mesh.boundary_polylines[bb];
            for sa in 0..pa.points.len() - 1 {
                for sb in 0..pb.points.len() - 1 {
                    if ba == bb && sa == sb {
                        continue;
                    }
                    checks += 1;
                    let (a, b) = (pa.points[sa], pa.points[sa + 1]);
                    let (c, d) = (pb.points[sb], pb.points[sb + 1]);
                    let allowed = allowed_touch(scene, mesh, ba, sa, bb, sb);
                    if closed_segments_intersect(a, b, c, d) {
                        if allowed {
                            continue;
                        }
                        return Err(VerificationError::Intersection);
                    }
                    if allowed {
                        continue;
                    }
                    let certified_margin = pa.max_deviation_px + pb.max_deviation_px + margin;
                    if segment_distance(a, b, c, d) <= certified_margin {
                        return Err(VerificationError::UncertifiedCurveSeparation);
                    }
                }
            }
        }
    }
    Ok(checks)
}

pub(super) fn verify_bindings(
    scene: &VectorScene,
    bindings: &[BoundaryBinding],
    topology: &str,
) -> Result<(), VerificationError> {
    if bindings.len() != scene.graph.boundaries.len() {
        return Err(VerificationError::BoundaryBinding);
    }
    let mut boundaries = BTreeSet::new();
    let mut chains = BTreeSet::new();
    let mut dcel_boundaries = BTreeSet::new();
    let mut canvas_closures = 0usize;
    for binding in bindings {
        if binding.boundary.index() >= scene.graph.boundaries.len()
            || binding.topology_signature_sha256 != topology
            || !boundaries.insert(binding.boundary)
        {
            return Err(VerificationError::BoundaryBinding);
        }
        match &binding.origin {
            BoundaryBindingOrigin::ObservedDcel {
                observed_chain_sha256,
                dcel_boundary_sha256,
            } => {
                if !chains.insert(observed_chain_sha256.as_str())
                    || !dcel_boundaries.insert(dcel_boundary_sha256.as_str())
                {
                    return Err(VerificationError::BoundaryBinding);
                }
            }
            BoundaryBindingOrigin::CanvasClosure { canvas_sha256 } => {
                canvas_closures += 1;
                if canvas_closures > 1
                    || canvas_sha256 != &canvas_closure_sha256(scene.canvas)
                    || !is_exact_canvas_closure(scene, binding)
                {
                    return Err(VerificationError::BoundaryBinding);
                }
            }
            BoundaryBindingOrigin::CanvasSegment {
                canvas_sha256,
                dcel_boundary_sha256,
            } => {
                if canvas_sha256 != &canvas_closure_sha256(scene.canvas)
                    || !dcel_boundaries.insert(dcel_boundary_sha256.as_str())
                    || !is_exact_canvas_segment(scene, binding)
                {
                    return Err(VerificationError::BoundaryBinding);
                }
            }
        }
    }
    Ok(())
}
