//! M7 three-layer verifier and delivery seal.
//!
//! The verifier first binds every observed chain identity to one canonical
//! boundary, then certifies combinatorial structure, curve separation, G1 and
//! the full partition render. Shared parameters are quantized exactly once and
//! the same checks plus an isotopy-tube check are rerun. The delivery seal
//! finally reconstructs the expected ExportPlan/SVG bytes and accepts only
//! opaque witnesses produced by `vice-svg`'s independent parse/render path.

#![forbid(unsafe_code)]

mod delivery;
mod quantize;
mod scene;

pub use delivery::{
    seal_delivery, DeliveryComparison, DeliverySeal, DeliverySealConfig, DeliverySealError,
};
pub use quantize::{
    quantize_and_verify, quantize_and_verify_with_workspace, PostQuantizationCertificate,
    QuantizationError, QuantizationPolicy, QuantizedVerificationWorkspace, QuantizedVerifiedScene,
};
pub use scene::{
    canvas_closure_sha256, preseal_scene, rebind_scene_bindings, topology_signature_sha256,
    BoundaryBinding, BoundaryBindingOrigin, PresealCertificate, PresealedScene, VerificationConfig,
    VerificationError,
};

#[cfg(test)]
mod tests;
