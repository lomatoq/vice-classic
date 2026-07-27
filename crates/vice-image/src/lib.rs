//! vice-image — canonical decode and the observation tensor (M4).
//!
//! Scope (spec v1.3 §8.1, §5.2, §1.6):
//!
//! - [`decode`]: bytes → dimensions, alpha presence, ICC/profile presence
//!   and the assumption actually applied, source hash, resource limits, and
//!   a canonical STRAIGHT sRGB8 RGBA buffer;
//! - [`observation`]: canonical bytes → the PREMULTIPLIED observation
//!   tensor of one [`vice_ir::BlendSpace`] hypothesis, plus the per-pixel
//!   8-bit quantization interval that belongs to it.
//!
//! Why the tensor is parameterised by the blend space rather than fixed.
//! §5.2 says the pipeline is `decode bytes + ICC assumption → straight RGBA
//! → canonical linear RGBA → premultiplied observation tensor`, and in the
//! same breath forbids assuming that every rasterizer blended coverage in
//! linear light. Both are satisfied by making the tensor a FUNCTION of the
//! hypothesis: the quantity that is linear in coverage lives in linear
//! light under `LinearLight` and in the encoded values under
//! `EncodedSrgb`, and picking one silently would be the assumption §5.2
//! prohibits.
//!
//! `alpha = 0` is not "black". Premultiplication maps it to the zero
//! vector, and this crate never un-premultiplies to recover a colour that
//! is not there (§1.6, and the same rule vice-ir's `unpremultiply` states
//! by returning `None`).
//!
//! Not here, deliberately: Oklab. §4.1 lists it among this crate's eventual
//! contents, but M4 has no consumer for a perceptual metric — the palette
//! separation that M4 measures is the conditioning `‖P_f − P_b‖` of the
//! mixture, in the observation space, and inventing a second colour metric
//! with no call site is the placeholder §32 rule 7 forbids.

#![forbid(unsafe_code)]

pub mod decode;
pub mod observation;

pub use decode::{CanonicalImage, DecodeLimits, IccAssumption, ImageError};
pub use observation::{
    dot, mix, norm, paint_observation_premul, sub, ObservationTensor, TensorSummary, CHANNELS,
    TRANSPARENT_EXTERIOR_PREMUL,
};
