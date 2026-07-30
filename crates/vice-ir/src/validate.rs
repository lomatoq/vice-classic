//! Typed validation of the canonical IR (spec v1.3 §12 invariants, §5.5
//! number rules). An invalid scene is a TYPED reject, never a panic.
//!
//! What M1 checks:
//! - canonical number rules: no NaN, no ±Inf, no `-0.0` anywhere (§5.5);
//! - canvas/formation parameter sanity;
//! - all structural/combinatorial §12 invariants: twins, two owners per
//!   interior boundary, face-side consistency, closed `next` cycles that
//!   match the face loops, no dangling cracks, no isolated vertices,
//!   duplicate-vertex/duplicate-boundary exclusion, Euler count;
//! - segment feasibility (positive radii, representable arcs, canonical
//!   angle ranges, no zero-length spans);
//! - CHAIN-POINT DISTINCTNESS: every point where chains meet must be a
//!   single shared graph vertex — an interior node may not coincide with
//!   any graph vertex or any other interior node (unrepresented junction;
//!   REVIEW_M1 blocker M1-N1). After this invariant, position equality
//!   between chain points IS point identity, which makes the position-based
//!   segment-adjacency test below sound;
//! - non-adjacent boundary intersection: EXACT for line-line pairs (robust
//!   predicates), certified-disjoint via conservative boxes otherwise.
//!
//! Honest M1 boundary (ADR-0005, STATUS_M1): a non-line segment pair whose
//! conservative boxes overlap is UNDETERMINED in M1 — it is neither
//! rejected nor certified. Full certified curve-curve intersection is M2+.
//! Geometric loop orientation (outer CCW vs holes) also needs curve
//! machinery and is checked from M2 on; M1 checks the combinatorial side
//! only.

use vice_geom::{is_negative_zero, Pt};

use crate::interference::check_segment_interference;

use crate::curve::{JoinKind, Segment};
use crate::graph::{BoundaryId, FaceId, HalfEdgeId, Paint, PlanarGraph, VertexId};
use crate::scene::{PixelFilter, VectorScene};

/// Scene-level typed rejection.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SceneError {
    #[error("canvas dimensions must be nonzero, got {width_px}x{height_px}")]
    EmptyCanvas { width_px: u32, height_px: u32 },
    #[error("non-finite number at {location}")]
    NonFinite { location: String },
    #[error("negative zero at {location} (forbidden in canonical scenes)")]
    NegativeZero { location: String },
    #[error("formation: {reason}")]
    Formation { reason: String },
    #[error(transparent)]
    Graph(#[from] GraphError),
}

/// Graph-level typed rejection (§12 invariants).
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GraphError {
    #[error("{what} id {id} out of range (len {len})")]
    IdOutOfRange {
        what: &'static str,
        id: u32,
        len: usize,
    },
    #[error("exterior face {0:?} must have TransparentExterior paint")]
    ExteriorNotTransparent(FaceId),
    #[error("paint color component out of [0,1] at face {0:?}")]
    PaintOutOfRange(FaceId),
    #[error("face {0:?} has no loops but the graph is not the empty scene")]
    FaceWithoutLoops(FaceId),
    #[error("vertices {0:?} and {1:?} have identical positions")]
    DuplicateVertexPosition(VertexId, VertexId),
    #[error("vertex {0:?} is not an endpoint of any boundary")]
    IsolatedVertex(VertexId),
    #[error("unrepresented junction: {a} and {b} occupy the same position; every point where chains meet must be a single shared graph vertex")]
    UnrepresentedJunction { a: ChainPointRef, b: ChainPointRef },
    #[error("boundary {0:?} has face {1:?} on both sides (dangling crack)")]
    DanglingCrack(BoundaryId, FaceId),
    #[error("boundaries {0:?} and {1:?} are identical (same endpoints and curve)")]
    DuplicateBoundary(BoundaryId, BoundaryId),
    #[error("boundary {0:?} has an empty segment list")]
    EmptyChain(BoundaryId),
    #[error(
        "boundary {boundary:?}: {nodes} interior nodes for {segments} segments (want segments - 1)"
    )]
    ChainArityMismatch {
        boundary: BoundaryId,
        nodes: usize,
        segments: usize,
    },
    #[error("boundary {boundary:?} segment {segment}: consecutive chain points coincide")]
    DegenerateSegment {
        boundary: BoundaryId,
        segment: usize,
    },
    #[error("boundary {boundary:?} segment {segment}: {reason}")]
    InvalidSegmentParams {
        boundary: BoundaryId,
        segment: usize,
        reason: String,
    },
    #[error("boundary {boundary:?} interior node {node}: tangent angle outside (-pi, pi]")]
    TangentAngleOutOfRange { boundary: BoundaryId, node: usize },
    #[error("open boundary {0:?} must not declare a closure join")]
    ClosureJoinOnOpenBoundary(BoundaryId),
    #[error("self-loop boundary {0:?} must declare its closure join")]
    MissingClosureJoin(BoundaryId),
    #[error("boundary {0:?} closure tangent angle outside (-pi, pi]")]
    ClosureTangentAngleOutOfRange(BoundaryId),
    #[error("half-edge count {half_edges} != 2 * boundary count {boundaries}")]
    HalfEdgeCount {
        half_edges: usize,
        boundaries: usize,
    },
    #[error("half-edge {0:?}: twin violation ({1})")]
    TwinViolation(HalfEdgeId, String),
    #[error("boundary {0:?} does not have exactly one forward and one reverse half-edge")]
    BoundaryHalfEdgeCover(BoundaryId),
    #[error("half-edge {0:?}: face does not match its boundary side")]
    HalfEdgeFaceMismatch(HalfEdgeId),
    #[error("next is not a permutation: half-edge {0:?} has {1} predecessors")]
    NextNotPermutation(HalfEdgeId, usize),
    #[error("half-edge {h:?} -> next {n:?}: target vertex {t:?} != origin vertex {o:?}")]
    NextEndpointMismatch {
        h: HalfEdgeId,
        n: HalfEdgeId,
        t: VertexId,
        o: VertexId,
    },
    #[error("half-edge cycle mixes faces {0:?} and {1:?}")]
    CycleFaceMismatch(FaceId, FaceId),
    #[error("face {0:?}: loops do not match the actual half-edge cycles ({1})")]
    FaceLoopsMismatch(FaceId, String),
    #[error("Euler check failed: V={v} E={e} F={f} components={c} (want V-E+F = 1+C)")]
    EulerMismatch {
        v: usize,
        e: usize,
        f: usize,
        c: usize,
    },
    #[error(
        "boundaries {a:?} (segment {sa}) and {b:?} (segment {sb}) intersect (exact line-line test)"
    )]
    BoundariesIntersect {
        a: BoundaryId,
        sa: usize,
        b: BoundaryId,
        sb: usize,
    },
    #[error("boundaries {a:?} (segment {sa}) and {b:?} (segment {sb}) overlap collinearly beyond a shared endpoint")]
    CollinearOverlap {
        a: BoundaryId,
        sa: usize,
        b: BoundaryId,
        sb: usize,
    },
}

/// Validate a whole scene: number rules first, then canvas/formation, then
/// every §12 graph invariant that M1 can check.
pub fn validate_scene(scene: &VectorScene) -> Result<(), SceneError> {
    check_numbers(scene)?;
    if scene.canvas.width_px == 0 || scene.canvas.height_px == 0 {
        return Err(SceneError::EmptyCanvas {
            width_px: scene.canvas.width_px,
            height_px: scene.canvas.height_px,
        });
    }
    if let PixelFilter::Gaussian { sigma_px } = scene.formation.pixel_filter {
        if sigma_px <= 0.0 {
            return Err(SceneError::Formation {
                reason: format!("gaussian sigma_px must be > 0, got {sigma_px}"),
            });
        }
    }
    validate_graph(&scene.graph)?;
    Ok(())
}

/// A scene that HAS passed [`validate_scene`] — the typed form of the
/// precondition that used to live only in doc comments (REVIEW_M1 M1-N10).
///
/// The inner scene is private and immutable through this wrapper, so the
/// only way to hold a `ValidatedScene` is to have gone through
/// [`ValidatedScene::new`]. Consumers whose soundness depends on the §12
/// invariants (the interference worklist's position-identity adjacency,
/// the M2 renderer's shared-boundary tessellation) take `&ValidatedScene`
/// instead of documenting "must be validated" and hoping.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedScene {
    scene: VectorScene,
}

impl ValidatedScene {
    /// Validate and wrap. The ONLY constructor.
    pub fn new(scene: VectorScene) -> Result<Self, SceneError> {
        validate_scene(&scene)?;
        Ok(ValidatedScene { scene })
    }

    pub fn scene(&self) -> &VectorScene {
        &self.scene
    }

    pub fn graph(&self) -> &PlanarGraph {
        &self.scene.graph
    }

    /// Give the scene back (dropping the validation evidence).
    pub fn into_inner(self) -> VectorScene {
        self.scene
    }
}

/// Validate the graph invariants alone (numbers must already be canonical).
pub fn validate_graph(g: &PlanarGraph) -> Result<(), GraphError> {
    check_ids(g)?;
    check_faces(g)?;
    check_vertices(g)?;
    check_boundaries(g)?;
    // Runs after the per-segment checks so a zero-length span is reported
    // as DegenerateSegment, and BEFORE the interference stage, whose
    // position-based adjacency is only sound once this invariant holds.
    check_chain_point_distinctness(g)?;
    check_half_edges(g)?;
    check_cycles_and_loops(g)?;
    check_euler(g)?;
    check_segment_interference(g)?;
    Ok(())
}

/// A reference to one chain point: either a graph vertex or an interior
/// node of a boundary's curve chain. Used by
/// [`GraphError::UnrepresentedJunction`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChainPointRef {
    Vertex(VertexId),
    InteriorNode { boundary: BoundaryId, node: usize },
}

impl std::fmt::Display for ChainPointRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChainPointRef::Vertex(v) => write!(f, "graph vertex {}", v.0),
            ChainPointRef::InteriorNode { boundary, node } => {
                write!(f, "interior node {} of boundary {}", node, boundary.0)
            }
        }
    }
}

/// §12 "robust junctions": in a planar arrangement, ANY point where chains
/// meet must be a shared graph vertex. Therefore all chain points — graph
/// vertices AND interior nodes of every curve chain — must be pairwise
/// distinct by position. A coincidence involving an interior node is an
/// UNREPRESENTED JUNCTION (or a chain self-touch) and is rejected
/// (REVIEW_M1 blocker M1-N1: scenes C1/C4 and the self-touch case).
///
/// Consequence used downstream: after this check, position equality between
/// segment endpoints is IDENTITY of chain points, so the position-based
/// adjacency in the interference stage is exact, not heuristic.
fn check_chain_point_distinctness(g: &PlanarGraph) -> Result<(), GraphError> {
    let mut pts: Vec<(Pt, ChainPointRef)> =
        Vec::with_capacity(g.vertices.len() + g.boundaries.len());
    for (vi, v) in g.vertices.iter().enumerate() {
        pts.push((v.pos, ChainPointRef::Vertex(VertexId(vi as u32))));
    }
    for (bi, b) in g.boundaries.iter().enumerate() {
        for (ni, n) in b.curve.interior_nodes.iter().enumerate() {
            pts.push((
                n.pos,
                ChainPointRef::InteriorNode {
                    boundary: BoundaryId(bi as u32),
                    node: ni,
                },
            ));
        }
    }
    // Total order: position, then reference — the reported pair is
    // deterministic even with several collisions.
    pts.sort_by(|x, y| x.0.lex_cmp(y.0).then_with(|| x.1.cmp(&y.1)));
    for w in pts.windows(2) {
        if w[0].0 == w[1].0 {
            let (a, b) = (w[0].1, w[1].1);
            // Vertex-vertex duplicates are already rejected by
            // check_vertices; keep the same typed error for direct callers.
            if let (ChainPointRef::Vertex(va), ChainPointRef::Vertex(vb)) = (a, b) {
                return Err(GraphError::DuplicateVertexPosition(va, vb));
            }
            return Err(GraphError::UnrepresentedJunction { a, b });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Number rules (§5.5): no NaN/Inf, no -0.0 anywhere in the scene.
// ---------------------------------------------------------------------------

fn check_numbers(scene: &VectorScene) -> Result<(), SceneError> {
    let check = |v: f64, location: &dyn Fn() -> String| -> Result<(), SceneError> {
        if !v.is_finite() {
            return Err(SceneError::NonFinite {
                location: location(),
            });
        }
        if is_negative_zero(v) {
            return Err(SceneError::NegativeZero {
                location: location(),
            });
        }
        Ok(())
    };
    let check_pt = |p: Pt, loc: &dyn Fn() -> String| -> Result<(), SceneError> {
        check(p.x, &|| format!("{}.x", loc()))?;
        check(p.y, &|| format!("{}.y", loc()))
    };

    if let PixelFilter::Gaussian { sigma_px } = scene.formation.pixel_filter {
        check(sigma_px, &|| "formation.pixel_filter.sigma_px".to_string())?;
    }
    let g = &scene.graph;
    for (i, v) in g.vertices.iter().enumerate() {
        check_pt(v.pos, &|| format!("vertices[{i}].pos"))?;
    }
    for (bi, b) in g.boundaries.iter().enumerate() {
        if let Some(JoinKind::SmoothG1 { tangent_angle_rad }) = b.closure_join {
            check(tangent_angle_rad, &|| {
                format!("boundaries[{bi}].closure_join.tangent_angle_rad")
            })?;
        }
        for (ni, n) in b.curve.interior_nodes.iter().enumerate() {
            check_pt(n.pos, &|| {
                format!("boundaries[{bi}].curve.interior_nodes[{ni}].pos")
            })?;
            if let JoinKind::SmoothG1 { tangent_angle_rad } = n.join {
                check(tangent_angle_rad, &|| {
                    format!("boundaries[{bi}].curve.interior_nodes[{ni}].join.tangent_angle_rad")
                })?;
            }
        }
        for (si, s) in b.curve.segments.iter().enumerate() {
            let loc = |field: &str| format!("boundaries[{bi}].curve.segments[{si}].{field}");
            match *s {
                Segment::Line => {}
                Segment::CircularArc { radius_px, .. } => {
                    check(radius_px, &|| loc("radius_px"))?;
                }
                Segment::EllipticArc {
                    rx_px,
                    ry_px,
                    x_axis_rotation_rad,
                    ..
                } => {
                    check(rx_px, &|| loc("rx_px"))?;
                    check(ry_px, &|| loc("ry_px"))?;
                    check(x_axis_rotation_rad, &|| loc("x_axis_rotation_rad"))?;
                }
                Segment::Quad { ctrl } => {
                    check_pt(ctrl, &|| loc("ctrl"))?;
                }
                Segment::Cubic { ctrl1, ctrl2 } => {
                    check_pt(ctrl1, &|| loc("ctrl1"))?;
                    check_pt(ctrl2, &|| loc("ctrl2"))?;
                }
            }
        }
    }
    for (fi, f) in g.faces.iter().enumerate() {
        if let Paint::OpaqueSolid(c) = f.paint {
            check(c.r, &|| format!("faces[{fi}].paint.r"))?;
            check(c.g, &|| format!("faces[{fi}].paint.g"))?;
            check(c.b, &|| format!("faces[{fi}].paint.b"))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Structural invariants
// ---------------------------------------------------------------------------

fn check_ids(g: &PlanarGraph) -> Result<(), GraphError> {
    let oob = |what: &'static str, id: u32, len: usize| GraphError::IdOutOfRange { what, id, len };
    if g.exterior.index() >= g.faces.len() {
        return Err(oob("exterior face", g.exterior.0, g.faces.len()));
    }
    for b in &g.boundaries {
        for (what, id, len) in [
            ("left face", b.left_face.0, g.faces.len()),
            ("right face", b.right_face.0, g.faces.len()),
            ("start vertex", b.start_vertex.0, g.vertices.len()),
            ("end vertex", b.end_vertex.0, g.vertices.len()),
        ] {
            if id as usize >= len {
                return Err(oob(what, id, len));
            }
        }
    }
    for he in &g.half_edges {
        for (what, id, len) in [
            ("half-edge boundary", he.boundary.0, g.boundaries.len()),
            ("twin half-edge", he.twin.0, g.half_edges.len()),
            ("next half-edge", he.next.0, g.half_edges.len()),
            ("half-edge face", he.face.0, g.faces.len()),
        ] {
            if id as usize >= len {
                return Err(oob(what, id, len));
            }
        }
    }
    for f in &g.faces {
        for l in &f.loops {
            if l.index() >= g.half_edges.len() {
                return Err(oob("face loop half-edge", l.0, g.half_edges.len()));
            }
        }
    }
    Ok(())
}

fn check_faces(g: &PlanarGraph) -> Result<(), GraphError> {
    let empty_scene = g.boundaries.is_empty() && g.half_edges.is_empty() && g.vertices.is_empty();
    for (fi, f) in g.faces.iter().enumerate() {
        let fid = FaceId(fi as u32);
        let is_ext = fid == g.exterior;
        match &f.paint {
            // The exterior face is always transparent. Bounded interior
            // faces MAY also be TransparentExterior: transparent holes /
            // counters (e.g. the inside of a donut) are separate faces of
            // the partition that show the background through.
            Paint::OpaqueSolid(_) if is_ext => {
                return Err(GraphError::ExteriorNotTransparent(fid));
            }
            Paint::OpaqueSolid(c) => {
                let ok = c.components().iter().all(|v| (0.0..=1.0).contains(v));
                if !ok {
                    return Err(GraphError::PaintOutOfRange(fid));
                }
            }
            Paint::TransparentExterior => {}
        }
        if f.loops.is_empty() && !(is_ext && empty_scene) {
            return Err(GraphError::FaceWithoutLoops(fid));
        }
    }
    Ok(())
}

fn check_vertices(g: &PlanarGraph) -> Result<(), GraphError> {
    // Duplicate positions: total order sort, adjacent comparison.
    let mut order: Vec<usize> = (0..g.vertices.len()).collect();
    order.sort_by(|&a, &b| g.vertices[a].pos.lex_cmp(g.vertices[b].pos));
    for w in order.windows(2) {
        if g.vertices[w[0]].pos == g.vertices[w[1]].pos {
            let (a, b) = (w[0].min(w[1]), w[0].max(w[1]));
            return Err(GraphError::DuplicateVertexPosition(
                VertexId(a as u32),
                VertexId(b as u32),
            ));
        }
    }
    // No isolated vertices.
    let mut used = vec![false; g.vertices.len()];
    for b in &g.boundaries {
        used[b.start_vertex.index()] = true;
        used[b.end_vertex.index()] = true;
    }
    if let Some(i) = used.iter().position(|u| !u) {
        return Err(GraphError::IsolatedVertex(VertexId(i as u32)));
    }
    Ok(())
}

fn check_boundaries(g: &PlanarGraph) -> Result<(), GraphError> {
    for (bi, b) in g.boundaries.iter().enumerate() {
        let bid = BoundaryId(bi as u32);
        if b.left_face == b.right_face {
            return Err(GraphError::DanglingCrack(bid, b.left_face));
        }
        match (b.start_vertex == b.end_vertex, b.closure_join) {
            (false, Some(_)) => return Err(GraphError::ClosureJoinOnOpenBoundary(bid)),
            (true, None) => return Err(GraphError::MissingClosureJoin(bid)),
            (true, Some(JoinKind::SmoothG1 { tangent_angle_rad }))
                if !(tangent_angle_rad > -std::f64::consts::PI
                    && tangent_angle_rad <= std::f64::consts::PI) =>
            {
                return Err(GraphError::ClosureTangentAngleOutOfRange(bid));
            }
            _ => {}
        }
        let chain = &b.curve;
        if chain.segments.is_empty() {
            return Err(GraphError::EmptyChain(bid));
        }
        if chain.interior_nodes.len() + 1 != chain.segments.len() {
            return Err(GraphError::ChainArityMismatch {
                boundary: bid,
                nodes: chain.interior_nodes.len(),
                segments: chain.segments.len(),
            });
        }
        for (ni, n) in chain.interior_nodes.iter().enumerate() {
            if let JoinKind::SmoothG1 { tangent_angle_rad } = n.join {
                if !(tangent_angle_rad > -std::f64::consts::PI
                    && tangent_angle_rad <= std::f64::consts::PI)
                {
                    return Err(GraphError::TangentAngleOutOfRange {
                        boundary: bid,
                        node: ni,
                    });
                }
            }
        }
        let pts = chain.node_positions(
            g.vertices[b.start_vertex.index()].pos,
            g.vertices[b.end_vertex.index()].pos,
        );
        for (si, s) in chain.segments.iter().enumerate() {
            let (p0, p1) = (pts[si], pts[si + 1]);
            if p0 == p1 {
                return Err(GraphError::DegenerateSegment {
                    boundary: bid,
                    segment: si,
                });
            }
            check_segment_params(s, p0, p1).map_err(|reason| GraphError::InvalidSegmentParams {
                boundary: bid,
                segment: si,
                reason,
            })?;
        }
    }
    // Exact duplicates (same directed endpoints AND identical curve). Needed
    // both as an overlap reject and for the canonical total order (ADR-0006).
    for i in 0..g.boundaries.len() {
        for j in (i + 1)..g.boundaries.len() {
            let (a, b) = (&g.boundaries[i], &g.boundaries[j]);
            if a.start_vertex == b.start_vertex
                && a.end_vertex == b.end_vertex
                && a.curve == b.curve
            {
                return Err(GraphError::DuplicateBoundary(
                    BoundaryId(i as u32),
                    BoundaryId(j as u32),
                ));
            }
        }
    }
    Ok(())
}

fn check_segment_params(s: &Segment, p0: Pt, p1: Pt) -> Result<(), String> {
    match *s {
        Segment::Line | Segment::Quad { .. } | Segment::Cubic { .. } => Ok(()),
        Segment::CircularArc { radius_px, .. } => {
            if radius_px <= 0.0 {
                return Err(format!("radius_px must be > 0, got {radius_px}"));
            }
            let chord = p0.dist(p1);
            if 2.0 * radius_px < chord {
                return Err(format!(
                    "arc not representable: 2*radius_px = {} < chord = {chord}",
                    2.0 * radius_px
                ));
            }
            Ok(())
        }
        Segment::EllipticArc {
            rx_px,
            ry_px,
            x_axis_rotation_rad,
            ..
        } => {
            if rx_px <= 0.0 || ry_px <= 0.0 {
                return Err(format!("radii must be > 0, got rx={rx_px} ry={ry_px}"));
            }
            if !(0.0..std::f64::consts::PI).contains(&x_axis_rotation_rad) {
                return Err(format!(
                    "x_axis_rotation_rad must be in [0, pi), got {x_axis_rotation_rad}"
                ));
            }
            // Endpoint parameterization feasibility (SVG F.6.6): the
            // half-chord, rotated into the ellipse frame, must fit.
            let dx2 = (p0.x - p1.x) / 2.0;
            let dy2 = (p0.y - p1.y) / 2.0;
            let (sin_phi, cos_phi) = x_axis_rotation_rad.sin_cos();
            let x1p = cos_phi * dx2 + sin_phi * dy2;
            let y1p = -sin_phi * dx2 + cos_phi * dy2;
            let lambda = (x1p / rx_px).powi(2) + (y1p / ry_px).powi(2);
            if lambda > 1.0 {
                return Err(format!(
                    "arc not representable: endpoint lambda = {lambda} > 1"
                ));
            }
            Ok(())
        }
    }
}

fn check_half_edges(g: &PlanarGraph) -> Result<(), GraphError> {
    if g.half_edges.len() != 2 * g.boundaries.len() {
        return Err(GraphError::HalfEdgeCount {
            half_edges: g.half_edges.len(),
            boundaries: g.boundaries.len(),
        });
    }
    // Twin involution: every half-edge has a twin on the same boundary with
    // the opposite direction (§12: "каждый half-edge имеет twin").
    for (hi, he) in g.half_edges.iter().enumerate() {
        let hid = HalfEdgeId(hi as u32);
        let twin = &g.half_edges[he.twin.index()];
        if he.twin == hid {
            return Err(GraphError::TwinViolation(hid, "twin of itself".into()));
        }
        if twin.twin != hid {
            return Err(GraphError::TwinViolation(hid, "twin not involutive".into()));
        }
        if twin.boundary != he.boundary {
            return Err(GraphError::TwinViolation(
                hid,
                "twin on a different boundary".into(),
            ));
        }
        if twin.forward == he.forward {
            return Err(GraphError::TwinViolation(
                hid,
                "twin has the same direction".into(),
            ));
        }
    }
    // Exactly one forward and one reverse half-edge per boundary — this is
    // the "two owners" invariant in half-edge form.
    let mut fwd = vec![0usize; g.boundaries.len()];
    let mut rev = vec![0usize; g.boundaries.len()];
    for he in &g.half_edges {
        if he.forward {
            fwd[he.boundary.index()] += 1;
        } else {
            rev[he.boundary.index()] += 1;
        }
    }
    for bi in 0..g.boundaries.len() {
        if fwd[bi] != 1 || rev[bi] != 1 {
            return Err(GraphError::BoundaryHalfEdgeCover(BoundaryId(bi as u32)));
        }
    }
    // Face-side consistency: forward borders left_face, reverse right_face.
    for (hi, he) in g.half_edges.iter().enumerate() {
        let b = &g.boundaries[he.boundary.index()];
        let want = if he.forward {
            b.left_face
        } else {
            b.right_face
        };
        if he.face != want {
            return Err(GraphError::HalfEdgeFaceMismatch(HalfEdgeId(hi as u32)));
        }
    }
    Ok(())
}

fn check_cycles_and_loops(g: &PlanarGraph) -> Result<(), GraphError> {
    let n = g.half_edges.len();
    // `next` must be a permutation.
    let mut indegree = vec![0usize; n];
    for he in &g.half_edges {
        indegree[he.next.index()] += 1;
    }
    if let Some(i) = indegree.iter().position(|&d| d != 1) {
        return Err(GraphError::NextNotPermutation(
            HalfEdgeId(i as u32),
            indegree[i],
        ));
    }
    // Geometric continuity of the walk: target(h) == origin(next(h)).
    for (hi, he) in g.half_edges.iter().enumerate() {
        let h = HalfEdgeId(hi as u32);
        let t = g.he_target(h);
        let o = g.he_origin(he.next);
        if t != o {
            return Err(GraphError::NextEndpointMismatch {
                h,
                n: he.next,
                t,
                o,
            });
        }
    }
    // Decompose into cycles; each cycle must stay within one face.
    let mut cycle_of = vec![usize::MAX; n];
    let mut cycle_min: Vec<usize> = Vec::new();
    let mut cycle_face: Vec<FaceId> = Vec::new();
    for start in 0..n {
        if cycle_of[start] != usize::MAX {
            continue;
        }
        let cid = cycle_min.len();
        let mut min_id = start;
        let face = g.half_edges[start].face;
        let mut cur = start;
        loop {
            cycle_of[cur] = cid;
            if g.half_edges[cur].face != face {
                return Err(GraphError::CycleFaceMismatch(face, g.half_edges[cur].face));
            }
            min_id = min_id.min(cur);
            cur = g.half_edges[cur].next.index();
            if cur == start {
                break;
            }
        }
        cycle_min.push(min_id);
        cycle_face.push(face);
    }
    // Face loops must list exactly one representative per actual cycle.
    for (fi, f) in g.faces.iter().enumerate() {
        let fid = FaceId(fi as u32);
        let mut actual: Vec<usize> = (0..cycle_min.len())
            .filter(|&c| cycle_face[c] == fid)
            .collect();
        let mut listed: Vec<usize> = f.loops.iter().map(|l| cycle_of[l.index()]).collect();
        // A representative from a foreign face's cycle is caught here too,
        // because that cycle id will not appear in `actual`.
        actual.sort_unstable();
        listed.sort_unstable();
        if actual != listed {
            return Err(GraphError::FaceLoopsMismatch(
                fid,
                format!("expected cycles {actual:?}, loops reference {listed:?}"),
            ));
        }
    }
    Ok(())
}

fn check_euler(g: &PlanarGraph) -> Result<(), GraphError> {
    // Union-find over vertices, joined by boundaries.
    let mut parent: Vec<usize> = (0..g.vertices.len()).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for b in &g.boundaries {
        let (ra, rb) = (
            find(&mut parent, b.start_vertex.index()),
            find(&mut parent, b.end_vertex.index()),
        );
        if ra != rb {
            parent[ra] = rb;
        }
    }
    let mut roots: Vec<usize> = (0..g.vertices.len())
        .map(|i| find(&mut parent, i))
        .collect();
    roots.sort_unstable();
    roots.dedup();
    let c = roots.len();
    let (v, e, f) = (g.vertices.len(), g.boundaries.len(), g.faces.len());
    // Planar embedding: V - E + F = 1 + C (F includes the exterior face).
    if v + f != 1 + c + e {
        return Err(GraphError::EulerMismatch { v, e, f, c });
    }
    Ok(())
}
