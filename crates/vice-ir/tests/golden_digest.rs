//! Golden canonical-bytes / digest test (Tier A determinism, spec §5.5).
//!
//! The golden scene exercises every segment kind, both join kinds, a
//! transparent hole face and a single-vertex loop boundary. Its canonical
//! bytes are committed at `tests/golden/scene_v1.json` and the sha256 is
//! frozen below.
//!
//! If this test fails, the canonical byte format changed. That is a
//! REVIEWED change (spec §27.7 spirit): bump `SCENE_SCHEMA`, regenerate
//! the golden file in the same commit, and record why. Never "fix" the
//! digest silently.

mod common;

use common::*;
use vice_geom::Pt;
use vice_ir::{canonical_scene_bytes, parse_scene, scene_digest_sha256};

const GOLDEN_DIGEST: &str = "112d0478f86b57ca94bb8d1805042894ce2e1da640297f119853fca61cc2f1f9";
const GOLDEN_BYTES: &[u8] = include_bytes!("golden/scene_v1.json");

fn golden_scene() -> vice_ir::VectorScene {
    build_scene(
        128,
        96,
        &[
            square_with_hole(Pt::new(4.0, 4.0), 24.0, 6.0, red()),
            mixed_island(Pt::new(60.0, 30.0), blue()),
            loop_island(Pt::new(100.0, 70.0), 5.0, red()),
        ],
    )
}

#[test]
fn golden_bytes_are_stable() {
    let bytes = canonical_scene_bytes(&golden_scene()).unwrap();
    assert_eq!(
        bytes, GOLDEN_BYTES,
        "canonical byte format drifted from the committed golden file"
    );
}

#[test]
fn golden_digest_is_stable() {
    assert_eq!(scene_digest_sha256(&golden_scene()).unwrap(), GOLDEN_DIGEST);
}

#[test]
fn golden_file_parses_and_roundtrips() {
    let parsed = parse_scene(GOLDEN_BYTES).unwrap();
    assert_eq!(canonical_scene_bytes(&parsed).unwrap(), GOLDEN_BYTES);
    assert_eq!(scene_digest_sha256(&parsed).unwrap(), GOLDEN_DIGEST);
}
