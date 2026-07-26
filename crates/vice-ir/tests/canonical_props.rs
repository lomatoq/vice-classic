//! Property tests for the canonical serializer / digest (spec §28 M1 gate:
//! property roundtrips, same-platform byte determinism, invalid rejected).
//!
//! Scene strategy: 1..=4 islands placed in disjoint grid cells (guaranteed
//! line-line separation), mixing squares, holed squares, mixed-segment
//! islands (all five segment kinds, smooth joins) and single-vertex loop
//! islands, with generated colors and formation hypotheses.

mod common;

use common::*;
use proptest::prelude::*;
use vice_geom::Pt;
use vice_ir::{
    canonical_scene_bytes, parse_scene, scene_digest_sha256, validate_scene, BlendSpace, LinearRgb,
    PixelFilter, VectorScene,
};

#[derive(Debug, Clone, Copy)]
enum IslandKind {
    Square,
    HoledSquare,
    Mixed,
    Loop,
}

fn island_kind() -> impl Strategy<Value = IslandKind> {
    prop_oneof![
        Just(IslandKind::Square),
        Just(IslandKind::HoledSquare),
        Just(IslandKind::Mixed),
        Just(IslandKind::Loop),
    ]
}

fn color() -> impl Strategy<Value = LinearRgb> {
    (0u8..=255, 0u8..=255, 0u8..=255).prop_map(|(r, g, b)| {
        LinearRgb::new(
            f64::from(r) / 255.0,
            f64::from(g) / 255.0,
            f64::from(b) / 255.0,
        )
    })
}

fn pixel_filter() -> impl Strategy<Value = PixelFilter> {
    prop_oneof![
        Just(PixelFilter::Box),
        Just(PixelFilter::Triangle),
        (1u32..=40).prop_map(|s| PixelFilter::Gaussian {
            sigma_px: f64::from(s) / 16.0,
        }),
    ]
}

fn blend_space() -> impl Strategy<Value = BlendSpace> {
    prop_oneof![Just(BlendSpace::LinearLight), Just(BlendSpace::EncodedSrgb)]
}

/// Islands live in 40x40 grid cells with a wide margin; mixed islands span
/// ~10x10 px and loop islands ~4r, so islands never touch across cells
/// (their LINES never intersect; curve boxes may still overlap, which is
/// exactly the honest uncertified case).
fn scene_strategy() -> impl Strategy<Value = VectorScene> {
    (
        prop::collection::vec(island_kind(), 1..=4),
        prop::collection::vec(color(), 4),
        pixel_filter(),
        blend_space(),
    )
        .prop_map(|(kinds, colors, pf, bs)| {
            let islands: Vec<Island> = kinds
                .iter()
                .enumerate()
                .map(|(i, kind)| {
                    let cell_x = 8.0 + 40.0 * ((i % 2) as f64);
                    let cell_y = 8.0 + 40.0 * ((i / 2) as f64);
                    let origin = Pt::new(cell_x, cell_y);
                    let c = colors[i];
                    match kind {
                        IslandKind::Square => square_island(origin, 14.0, c),
                        IslandKind::HoledSquare => square_with_hole(origin, 16.0, 4.0, c),
                        IslandKind::Mixed => mixed_island(origin, c),
                        IslandKind::Loop => {
                            loop_island(Pt::new(cell_x + 8.0, cell_y + 8.0), 4.0, c)
                        }
                    }
                })
                .collect();
            let mut scene = build_scene(96, 96, &islands);
            scene.formation.pixel_filter = pf;
            scene.formation.blend_space = bs;
            scene
        })
}

/// Deterministic pseudo-permutation of 0..n from a seed (Fisher-Yates with
/// a tiny LCG; test-only).
fn permutation(n: usize, seed: u64) -> Vec<usize> {
    let mut p: Vec<usize> = (0..n).collect();
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    for i in (1..n).rev() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = (state >> 33) as usize % (i + 1);
        p.swap(i, j);
    }
    p
}

proptest! {
    /// Every generated scene is valid — the generator itself is honest.
    #[test]
    fn generated_scenes_validate(scene in scene_strategy()) {
        prop_assert!(validate_scene(&scene).is_ok());
    }

    /// parse(serialize(x)) re-serializes to byte-identical output, and the
    /// digest survives the roundtrip.
    #[test]
    fn roundtrip_is_byte_identical(scene in scene_strategy()) {
        let bytes = canonical_scene_bytes(&scene).unwrap();
        let parsed = parse_scene(&bytes).unwrap();
        let bytes2 = canonical_scene_bytes(&parsed).unwrap();
        prop_assert_eq!(&bytes, &bytes2);
        prop_assert_eq!(
            scene_digest_sha256(&scene).unwrap(),
            scene_digest_sha256(&parsed).unwrap()
        );
    }

    /// Serializing twice yields identical bytes (same-platform determinism).
    #[test]
    fn repeated_serialization_is_deterministic(scene in scene_strategy()) {
        let a = canonical_scene_bytes(&scene).unwrap();
        let b = canonical_scene_bytes(&scene).unwrap();
        prop_assert_eq!(a, b);
    }

    /// Permuting the CONSTRUCTION ORDER of every entity vector (with all
    /// references remapped) must not change the canonical bytes: canonical
    /// form is a function of content, not of insertion order.
    #[test]
    fn construction_order_permutation_keeps_bytes(
        scene in scene_strategy(),
        seed in any::<u64>(),
    ) {
        let g = &scene.graph;
        let perm_v = permutation(g.vertices.len(), seed ^ 0x5eed_0001);
        let perm_b = permutation(g.boundaries.len(), seed ^ 0x5eed_0002);
        let perm_h = permutation(g.half_edges.len(), seed ^ 0x5eed_0003);
        let perm_f = permutation(g.faces.len(), seed ^ 0x5eed_0004);
        let shuffled = apply_permutation(&scene, &perm_v, &perm_b, &perm_h, &perm_f);

        // The shuffled scene is the same scene: still valid, same bytes.
        prop_assert!(validate_scene(&shuffled).is_ok());
        prop_assert_eq!(
            canonical_scene_bytes(&scene).unwrap(),
            canonical_scene_bytes(&shuffled).unwrap()
        );
        prop_assert_eq!(
            scene_digest_sha256(&scene).unwrap(),
            scene_digest_sha256(&shuffled).unwrap()
        );
    }

    /// Corrupted numbers are rejected at the serialization boundary.
    #[test]
    fn broken_numbers_are_rejected(
        scene in scene_strategy(),
        which in 0usize..3,
    ) {
        let mut s = scene;
        if s.graph.vertices.is_empty() {
            return Ok(());
        }
        let v = &mut s.graph.vertices[0].pos;
        match which {
            0 => v.x = f64::NAN,
            1 => v.y = f64::INFINITY,
            _ => v.x = -0.0,
        }
        prop_assert!(canonical_scene_bytes(&s).is_err());
        prop_assert!(scene_digest_sha256(&s).is_err());
    }

    /// Structurally broken graphs are rejected by the serializer.
    #[test]
    fn broken_graphs_are_rejected(
        scene in scene_strategy(),
        which in 0usize..4,
    ) {
        let mut s = scene;
        match which {
            0 => {
                let h = &mut s.graph.half_edges[0];
                h.twin = vice_ir::HalfEdgeId(0);
            }
            1 => {
                s.graph.half_edges.pop();
            }
            2 => {
                let b = &mut s.graph.boundaries[0];
                b.right_face = b.left_face;
            }
            _ => {
                s.graph.faces[0].paint =
                    vice_ir::Paint::OpaqueSolid(LinearRgb::new(0.5, 0.5, 0.5));
            }
        }
        prop_assert!(canonical_scene_bytes(&s).is_err());
    }

    /// Truncated canonical bytes never parse.
    #[test]
    fn truncated_bytes_are_rejected(scene in scene_strategy(), cut in 1usize..64) {
        let bytes = canonical_scene_bytes(&scene).unwrap();
        let cut = cut.min(bytes.len() - 1);
        prop_assert!(parse_scene(&bytes[..bytes.len() - cut]).is_err());
    }
}
