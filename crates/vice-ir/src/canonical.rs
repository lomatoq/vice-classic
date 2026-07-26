//! Canonical serialization + digests: the M1 seal skeleton
//! (spec v1.3 §5.5, §28 M1; design in docs/ADR/ADR-0006).
//!
//! Guarantees:
//! - canonical bytes are a PURE function of scene CONTENT: entities are
//!   relabeled into a content-derived canonical order before serializing,
//!   so byte output is invariant under permutations of the construction
//!   order (property-tested);
//! - only ordered structures are serialized (Vecs in canonical order,
//!   fixed struct field order) — no hash maps anywhere on this path;
//! - shared parameters appear once: vertex positions only in the vertex
//!   table, chain endpoints never duplicated, shared tangents once per
//!   node — quantizing a shared parameter twice is unrepresentable;
//! - `-0.0`, NaN and ±Inf are rejected BEFORE serialization (typed error);
//!   float text is the shortest round-tripping decimal (serde_json/ryu),
//!   so parse → re-serialize is byte-identical;
//! - the digest is sha256 over the canonical bytes (Tier A determinism:
//!   same binary/platform ⇒ byte-identical digest, spec §5.5).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::graph::{
    Boundary, BoundaryId, Face, FaceId, HalfEdge, HalfEdgeId, PlanarGraph, VertexId,
};
use crate::scene::VectorScene;
use crate::validate::{validate_scene, SceneError};

/// Version tag of the canonical byte format. Changing the format or this
/// tag invalidates every recorded digest: reviewed change only.
pub const SCENE_SCHEMA: &str = "vice-classic/scene/v1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: String,
    scene: VectorScene,
}

/// Typed parse rejection for canonical scene bytes.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("json: {0}")]
    Json(String),
    #[error("schema mismatch: found {found:?}, expected {expected:?}")]
    SchemaMismatch { found: String, expected: String },
    #[error("invalid scene: {0}")]
    Invalid(#[from] SceneError),
}

/// Canonical bytes of a scene: validate → canonicalize → serialize.
pub fn canonical_scene_bytes(scene: &VectorScene) -> Result<Vec<u8>, SceneError> {
    validate_scene(scene)?;
    let canonical = VectorScene {
        canvas: scene.canvas,
        graph: canonicalize_graph(&scene.graph),
        formation: scene.formation,
    };
    let envelope = Envelope {
        schema: SCENE_SCHEMA.to_string(),
        scene: canonical,
    };
    Ok(serde_json::to_vec(&envelope).expect("validated scene serializes"))
}

/// Lower-case hex sha256 of the canonical bytes.
pub fn scene_digest_sha256(scene: &VectorScene) -> Result<String, SceneError> {
    let bytes = canonical_scene_bytes(scene)?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}

/// Parse scene bytes: strict JSON (unknown/duplicate fields rejected by
/// serde), schema tag check, then full validation. Accepts any valid
/// labeling; canonical bytes are re-obtained via [`canonical_scene_bytes`].
pub fn parse_scene(bytes: &[u8]) -> Result<VectorScene, ParseError> {
    let envelope: Envelope =
        serde_json::from_slice(bytes).map_err(|e| ParseError::Json(e.to_string()))?;
    if envelope.schema != SCENE_SCHEMA {
        return Err(ParseError::SchemaMismatch {
            found: envelope.schema,
            expected: SCENE_SCHEMA.to_string(),
        });
    }
    validate_scene(&envelope.scene)?;
    Ok(envelope.scene)
}

// ---------------------------------------------------------------------------
// Canonical relabeling
// ---------------------------------------------------------------------------

/// Relabel a VALID graph into its canonical order. Pure function of the
/// graph content; idempotent. Total order is guaranteed by validation:
/// duplicate vertex positions and duplicate boundaries are rejected, and
/// face loop-keys are disjoint half-edge sets.
///
/// Order:
/// - vertices: lexicographic by position (x, then y);
/// - boundaries: by (new start id, new end id, curve content);
/// - faces: exterior first, then by sorted rotated loop keys;
/// - half-edges: boundary-major, forward before reverse (ids 2b, 2b+1);
/// - face loops: minimal half-edge id per cycle, ascending.
pub fn canonicalize_graph(g: &PlanarGraph) -> PlanarGraph {
    // --- vertices ---------------------------------------------------------
    let mut v_order: Vec<usize> = (0..g.vertices.len()).collect();
    v_order.sort_by(|&a, &b| g.vertices[a].pos.lex_cmp(g.vertices[b].pos));
    let mut perm_v = vec![0usize; g.vertices.len()]; // old -> new
    for (new, &old) in v_order.iter().enumerate() {
        perm_v[old] = new;
    }

    // --- boundaries -------------------------------------------------------
    let b_key = |b: &Boundary| -> (usize, usize, String) {
        (
            perm_v[b.start_vertex.index()],
            perm_v[b.end_vertex.index()],
            serde_json::to_string(&b.curve).expect("curve serializes"),
        )
    };
    let keys: Vec<(usize, usize, String)> = g.boundaries.iter().map(b_key).collect();
    let mut b_order: Vec<usize> = (0..g.boundaries.len()).collect();
    b_order.sort_by(|&a, &b| keys[a].cmp(&keys[b]));
    let mut perm_b = vec![0usize; g.boundaries.len()];
    for (new, &old) in b_order.iter().enumerate() {
        perm_b[old] = new;
    }

    // --- half-edge target ids (boundary-major, forward first) -------------
    // Old half-edge -> new id, via its (boundary, forward) identity.
    let mut perm_h = vec![0usize; g.half_edges.len()];
    for (old, he) in g.half_edges.iter().enumerate() {
        let nb = perm_b[he.boundary.index()];
        perm_h[old] = 2 * nb + usize::from(!he.forward);
    }

    // --- cycles of `next` ------------------------------------------------
    let n = g.half_edges.len();
    let mut cycle_of = vec![usize::MAX; n];
    let mut cycles: Vec<Vec<usize>> = Vec::new(); // members as OLD ids
    for start in 0..n {
        if cycle_of[start] != usize::MAX {
            continue;
        }
        let cid = cycles.len();
        let mut members = Vec::new();
        let mut cur = start;
        loop {
            cycle_of[cur] = cid;
            members.push(cur);
            cur = g.half_edges[cur].next.index();
            if cur == start {
                break;
            }
        }
        cycles.push(members);
    }

    // --- faces -------------------------------------------------------------
    // Loop key: the cycle as (new boundary id, forward) pairs, rotated so
    // the minimal element comes first. Face key: sorted loop keys.
    let loop_key = |members: &[usize]| -> Vec<(usize, bool)> {
        let elems: Vec<(usize, bool)> = members
            .iter()
            .map(|&old| {
                let he = &g.half_edges[old];
                (perm_b[he.boundary.index()], he.forward)
            })
            .collect();
        let min_pos = elems
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.cmp(b.1))
            .map(|(i, _)| i)
            .unwrap_or(0);
        let mut rotated = Vec::with_capacity(elems.len());
        rotated.extend_from_slice(&elems[min_pos..]);
        rotated.extend_from_slice(&elems[..min_pos]);
        rotated
    };
    let mut face_cycles: Vec<Vec<usize>> = vec![Vec::new(); g.faces.len()]; // face -> cycle ids
    for (cid, members) in cycles.iter().enumerate() {
        let face = g.half_edges[members[0]].face.index();
        face_cycles[face].push(cid);
    }
    let face_key = |fi: usize| -> Vec<Vec<(usize, bool)>> {
        let mut ks: Vec<Vec<(usize, bool)>> = face_cycles[fi]
            .iter()
            .map(|&cid| loop_key(&cycles[cid]))
            .collect();
        ks.sort();
        ks
    };
    let ext = g.exterior.index();
    let mut f_order: Vec<usize> = (0..g.faces.len()).filter(|&f| f != ext).collect();
    let fkeys: Vec<Vec<Vec<(usize, bool)>>> = (0..g.faces.len()).map(face_key).collect();
    f_order.sort_by(|&a, &b| fkeys[a].cmp(&fkeys[b]));
    let mut perm_f = vec![0usize; g.faces.len()];
    perm_f[ext] = 0;
    for (i, &old) in f_order.iter().enumerate() {
        perm_f[old] = i + 1;
    }

    // --- rebuild ------------------------------------------------------------
    let mut vertices = Vec::with_capacity(g.vertices.len());
    for &old in &v_order {
        vertices.push(g.vertices[old]);
    }

    let mut boundaries = Vec::with_capacity(g.boundaries.len());
    for &old in &b_order {
        let b = &g.boundaries[old];
        boundaries.push(Boundary {
            left_face: FaceId(perm_f[b.left_face.index()] as u32),
            right_face: FaceId(perm_f[b.right_face.index()] as u32),
            start_vertex: VertexId(perm_v[b.start_vertex.index()] as u32),
            end_vertex: VertexId(perm_v[b.end_vertex.index()] as u32),
            curve: b.curve.clone(),
        });
    }

    let mut half_edges = vec![
        HalfEdge {
            boundary: BoundaryId(0),
            forward: true,
            twin: HalfEdgeId(0),
            next: HalfEdgeId(0),
            face: FaceId(0),
        };
        n
    ];
    for (old, he) in g.half_edges.iter().enumerate() {
        let new = perm_h[old];
        half_edges[new] = HalfEdge {
            boundary: BoundaryId(perm_b[he.boundary.index()] as u32),
            forward: he.forward,
            twin: HalfEdgeId(perm_h[he.twin.index()] as u32),
            next: HalfEdgeId(perm_h[he.next.index()] as u32),
            face: FaceId(perm_f[he.face.index()] as u32),
        };
    }

    let mut faces: Vec<Face> = vec![
        Face {
            loops: Vec::new(),
            paint: crate::graph::Paint::TransparentExterior,
        };
        g.faces.len()
    ];
    for (old, f) in g.faces.iter().enumerate() {
        // Canonical loop representative: minimal NEW half-edge id per cycle.
        let mut loops: Vec<HalfEdgeId> = face_cycles[old]
            .iter()
            .map(|&cid| {
                let min_new = cycles[cid].iter().map(|&m| perm_h[m]).min().expect("cycle");
                HalfEdgeId(min_new as u32)
            })
            .collect();
        loops.sort();
        faces[perm_f[old]] = Face {
            loops,
            paint: f.paint,
        };
    }

    PlanarGraph {
        exterior: FaceId(0),
        vertices,
        boundaries,
        half_edges,
        faces,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::PlanarGraph;
    use crate::scene::{
        Canvas, ExteriorModel, GlobalFormationHypothesis, PixelFilter, QuantizationModel,
        VectorScene,
    };

    fn empty() -> VectorScene {
        VectorScene {
            canvas: Canvas {
                width_px: 8,
                height_px: 8,
            },
            graph: PlanarGraph::empty(),
            formation: GlobalFormationHypothesis {
                blend_space: crate::color::BlendSpace::LinearLight,
                pixel_filter: PixelFilter::Box,
                quantization: QuantizationModel::Uint8,
                exterior: ExteriorModel::Transparent,
            },
        }
    }

    #[test]
    fn empty_scene_roundtrips_byte_identically() {
        let s = empty();
        let bytes = canonical_scene_bytes(&s).unwrap();
        let parsed = parse_scene(&bytes).unwrap();
        assert_eq!(canonical_scene_bytes(&parsed).unwrap(), bytes);
        assert_eq!(
            scene_digest_sha256(&s).unwrap(),
            scene_digest_sha256(&parsed).unwrap()
        );
    }

    #[test]
    fn schema_mismatch_is_rejected() {
        let s = empty();
        let bytes = canonical_scene_bytes(&s).unwrap();
        let tampered = String::from_utf8(bytes)
            .unwrap()
            .replace("vice-classic/scene/v1", "vice-classic/scene/v999");
        match parse_scene(tampered.as_bytes()) {
            Err(ParseError::SchemaMismatch { .. }) => {}
            other => panic!("expected SchemaMismatch, got {other:?}"),
        }
    }

    #[test]
    fn unknown_and_duplicate_fields_are_rejected() {
        let s = empty();
        let text = String::from_utf8(canonical_scene_bytes(&s).unwrap()).unwrap();
        let unknown = text.replacen("\"schema\"", "\"bogus\":1,\"schema\"", 1);
        assert!(matches!(
            parse_scene(unknown.as_bytes()),
            Err(ParseError::Json(_))
        ));
        let duplicate = text.replacen(
            "\"schema\":\"vice-classic/scene/v1\"",
            "\"schema\":\"vice-classic/scene/v1\",\"schema\":\"vice-classic/scene/v1\"",
            1,
        );
        assert!(matches!(
            parse_scene(duplicate.as_bytes()),
            Err(ParseError::Json(_))
        ));
    }

    #[test]
    fn nonfinite_number_literals_are_rejected() {
        let s = empty();
        let text = String::from_utf8(canonical_scene_bytes(&s).unwrap()).unwrap();
        // 1e999 overflows to +inf during parsing; the scene validator must
        // reject it even when serde_json accepts the literal.
        let with_inf = text.replacen(
            "\"pixel_filter\":\"box\"",
            "\"pixel_filter\":{\"gaussian\":{\"sigma_px\":1e999}}",
            1,
        );
        match parse_scene(with_inf.as_bytes()) {
            Err(ParseError::Json(_)) | Err(ParseError::Invalid(_)) => {}
            other => panic!("expected reject, got {other:?}"),
        }
    }
}
