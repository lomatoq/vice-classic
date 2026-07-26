//! Seal revalidation SKELETON (spec §20.2, in the M2 scope of §28 M2:
//! quantization policy, ExportPlan and SVG materialization belong to
//! later milestones and do NOT exist here).
//!
//! The M2 cycle:
//!
//! ```text
//! f64 scene validate
//! → canonical bytes (+ scene digest)
//! → parse → re-validate → render → render digest
//! → re-serialize (bytes must be identical)
//! → re-parse → re-validate → re-render (render digest must be identical)
//! ```
//!
//! The claim tested by the double cycle is the delivery-relevant one: the
//! serialized bytes fully determine the render — parsing the canonical
//! bytes again yields a BYTE-identical render (Tier A). The render of a
//! non-canonical labeling of the same scene is intentionally NOT claimed
//! byte-identical: canonical relabeling rotates loop representatives, and
//! f64 addition is commutative but not associative, so per-pixel sums may
//! legally differ by ulps (they stay within the partition tolerances —
//! asserted in the gate tests; §5.5 bounds include floating reduction).

use vice_ir::{canonical_scene_bytes, parse_scene, ValidatedScene, VectorScene};

use crate::partition::{render_digest_sha256, render_partition, RenderOptions};
use crate::render_error::RenderError;

/// Typed seal-cycle failure.
#[derive(Debug, thiserror::Error)]
pub enum SealError {
    #[error(transparent)]
    Invalid(#[from] vice_ir::SceneError),
    #[error("canonical parse failed: {0}")]
    Parse(String),
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error("canonical bytes diverged after roundtrip ({first_len} vs {second_len} bytes)")]
    BytesDiverged { first_len: usize, second_len: usize },
    #[error("render digest diverged after re-parse: {first} vs {second}")]
    RenderDiverged { first: String, second: String },
}

/// The digests sealed by one cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealReport {
    /// sha256 of the canonical scene bytes (vice-ir seal skeleton).
    pub scene_digest_sha256: String,
    /// sha256 of the composite render bytes of the canonical scene.
    pub render_digest_sha256: String,
    pub canonical_bytes_len: usize,
}

/// Run the M2 seal revalidation cycle.
pub fn seal_render_cycle(
    scene: &VectorScene,
    options: &RenderOptions,
) -> Result<SealReport, SealError> {
    // 1. f64 scene validation, typed.
    let _validated = ValidatedScene::new(scene.clone())?;

    // 2. Canonical bytes + scene digest.
    let bytes = canonical_scene_bytes(scene)?;
    let scene_digest = vice_ir::scene_digest_sha256(scene)?;

    // 3. Parse → re-validate → render → digest.
    let parsed = parse_scene(&bytes).map_err(|e| SealError::Parse(e.to_string()))?;
    let revalidated = ValidatedScene::new(parsed)?;
    let render1 = render_partition(&revalidated, options)?;
    let digest1 = render_digest_sha256(&render1);

    // 4. Re-serialize: the canonical bytes must reproduce themselves.
    let bytes2 = canonical_scene_bytes(revalidated.scene())?;
    if bytes2 != bytes {
        return Err(SealError::BytesDiverged {
            first_len: bytes.len(),
            second_len: bytes2.len(),
        });
    }

    // 5. Re-parse → re-validate → re-render: the serialized bytes fully
    //    determine the render, byte for byte.
    let parsed2 = parse_scene(&bytes2).map_err(|e| SealError::Parse(e.to_string()))?;
    let revalidated2 = ValidatedScene::new(parsed2)?;
    let render2 = render_partition(&revalidated2, options)?;
    let digest2 = render_digest_sha256(&render2);
    if digest2 != digest1 {
        return Err(SealError::RenderDiverged {
            first: digest1,
            second: digest2,
        });
    }

    Ok(SealReport {
        scene_digest_sha256: scene_digest,
        render_digest_sha256: digest1,
        canonical_bytes_len: bytes.len(),
    })
}
