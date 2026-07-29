//! Canonical M7 SVG materialization.
//!
//! An [`ExportPlan`] is built from canonicalized, already-quantized scene
//! geometry. Both profiles consume exactly that plan: PurePartition emits only
//! the partition fills; SeamSafe additionally emits lower-face underpaint on
//! eligible shared interior boundaries. [`parse_and_render_independently`]
//! returns an opaque witness that can only be constructed by an XML parse,
//! usvg parse, and resvg render of the serialized bytes.

#![forbid(unsafe_code)]

mod independent;
mod plan;
mod write;

pub use independent::{
    parse_and_render_independently, IndependentSvgError, IndependentlyRenderedSvg, SVG_PARSER_ID,
    SVG_RENDERER_ID,
};
pub use plan::{
    build_export_plan, canonical_export_plan_bytes, ApronPlan, ExportPlan, ExportPlanError,
    FacePlan, EXPORT_PLAN_SCHEMA,
};
pub use write::{materialize_svg, SvgMaterializationError, SvgProfile};

#[cfg(test)]
mod tests;
