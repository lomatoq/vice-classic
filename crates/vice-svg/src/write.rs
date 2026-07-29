use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::plan::ExportPlan;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SvgProfile {
    PurePartition,
    SeamSafe,
}

impl SvgProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            SvgProfile::PurePartition => "pure-partition",
            SvgProfile::SeamSafe => "seam-safe",
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum SvgMaterializationError {
    #[error("export plan schema is unsupported")]
    UnsupportedPlan,
    #[error("export plan contains an invalid SVG token")]
    InvalidToken,
}

fn token_is_safe(value: &str) -> bool {
    !value.contains(['<', '>', '"', '&'])
}

pub fn materialize_svg(
    plan: &ExportPlan,
    profile: SvgProfile,
) -> Result<Vec<u8>, SvgMaterializationError> {
    if plan.schema() != crate::EXPORT_PLAN_SCHEMA {
        return Err(SvgMaterializationError::UnsupportedPlan);
    }
    if plan.faces().iter().any(|f| {
        !token_is_safe(f.path_d())
            || !token_is_safe(f.fill_srgb8())
            || !f.fill_srgb8().starts_with('#')
    }) || plan.aprons().iter().any(|a| {
        !token_is_safe(a.path_d())
            || !token_is_safe(a.stroke_srgb8())
            || !a.stroke_srgb8().starts_with('#')
    }) {
        return Err(SvgMaterializationError::InvalidToken);
    }
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" data-vice-scene-sha256=\"{}\" data-vice-profile=\"{}\" data-vice-aprons=\"{}\">\n",
        plan.width_px(),
        plan.height_px(),
        plan.width_px(),
        plan.height_px(),
        plan.scene_digest_sha256(),
        profile.as_str(),
        if profile == SvgProfile::SeamSafe {
            plan.aprons().len()
        } else {
            0
        }
    );
    for face in plan.faces() {
        svg.push_str(&format!(
            "  <path id=\"face-{}\" data-z=\"{}\" d=\"{}\" fill=\"{}\" fill-rule=\"nonzero\"/>\n",
            face.face_id(),
            face.z_index(),
            face.path_d(),
            face.fill_srgb8()
        ));
        if profile == SvgProfile::SeamSafe {
            for apron in plan
                .aprons()
                .iter()
                .filter(|a| a.lower_face() == face.face_id())
            {
                svg.push_str(&format!(
                    "  <path id=\"apron-{}\" class=\"vice-seam-apron\" data-lower-face=\"{}\" data-upper-face=\"{}\" d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"butt\" stroke-linejoin=\"round\"/>\n",
                    apron.boundary_id(),
                    apron.lower_face(),
                    apron.upper_face(),
                    apron.path_d(),
                    apron.stroke_srgb8(),
                    apron.width_px()
                ));
            }
        }
    }
    svg.push_str("</svg>\n");
    Ok(svg.into_bytes())
}
