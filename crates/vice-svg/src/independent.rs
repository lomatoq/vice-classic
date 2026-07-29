use resvg::{tiny_skia, usvg};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::SvgProfile;

pub const SVG_PARSER_ID: &str = "roxmltree+usvg-0.45";
pub const SVG_RENDERER_ID: &str = "resvg-0.45";
const MAX_RENDER_PIXELS: u64 = 1 << 28;

/// Opaque proof that serialized SVG bytes passed two independent parsers and
/// were rendered from those bytes. There is no public constructor.
#[derive(Debug, Clone, PartialEq)]
pub struct IndependentlyRenderedSvg {
    profile: SvgProfile,
    scene_digest_sha256: String,
    svg_digest_sha256: String,
    render_digest_sha256: String,
    width_px: u32,
    height_px: u32,
    apron_paths: usize,
    premultiplied_rgba8: Vec<u8>,
    png_bytes: Vec<u8>,
}

impl IndependentlyRenderedSvg {
    pub fn profile(&self) -> SvgProfile {
        self.profile
    }
    pub fn scene_digest_sha256(&self) -> &str {
        &self.scene_digest_sha256
    }
    pub fn svg_digest_sha256(&self) -> &str {
        &self.svg_digest_sha256
    }
    pub fn render_digest_sha256(&self) -> &str {
        &self.render_digest_sha256
    }
    pub fn width_px(&self) -> u32 {
        self.width_px
    }
    pub fn height_px(&self) -> u32 {
        self.height_px
    }
    pub fn apron_paths(&self) -> usize {
        self.apron_paths
    }
    pub fn premultiplied_rgba8(&self) -> &[u8] {
        &self.premultiplied_rgba8
    }
    pub fn png_bytes(&self) -> &[u8] {
        &self.png_bytes
    }
}

#[derive(Debug, Error)]
pub enum IndependentSvgError {
    #[error("serialized SVG is not UTF-8")]
    Utf8,
    #[error("independent XML parse failed: {0}")]
    Xml(String),
    #[error("SVG root or vice delivery attributes are invalid")]
    RootContract,
    #[error("declared SVG dimensions exceed the render budget")]
    RenderBudget,
    #[error("profile or apron count disagrees with serialized paths")]
    ProfileContract,
    #[error("independent SVG parse failed: {0}")]
    Svg(String),
    #[error("independent render allocation failed")]
    Allocation,
    #[error("independent PNG encoding failed: {0}")]
    Png(String),
}

pub fn parse_and_render_independently(
    svg_bytes: &[u8],
) -> Result<IndependentlyRenderedSvg, IndependentSvgError> {
    let text = std::str::from_utf8(svg_bytes).map_err(|_| IndependentSvgError::Utf8)?;
    let document =
        roxmltree::Document::parse(text).map_err(|e| IndependentSvgError::Xml(e.to_string()))?;
    let root = document.root_element();
    if root.tag_name().name() != "svg"
        || root.tag_name().namespace() != Some("http://www.w3.org/2000/svg")
    {
        return Err(IndependentSvgError::RootContract);
    }
    let width_px = root
        .attribute("width")
        .and_then(|v| v.parse::<u32>().ok())
        .ok_or(IndependentSvgError::RootContract)?;
    let height_px = root
        .attribute("height")
        .and_then(|v| v.parse::<u32>().ok())
        .ok_or(IndependentSvgError::RootContract)?;
    let scene_digest = root
        .attribute("data-vice-scene-sha256")
        .filter(|v| {
            v.len() == 64
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or(IndependentSvgError::RootContract)?
        .to_owned();
    let profile = match root.attribute("data-vice-profile") {
        Some("pure-partition") => SvgProfile::PurePartition,
        Some("seam-safe") => SvgProfile::SeamSafe,
        _ => return Err(IndependentSvgError::RootContract),
    };
    let declared_aprons = root
        .attribute("data-vice-aprons")
        .and_then(|v| v.parse::<usize>().ok())
        .ok_or(IndependentSvgError::RootContract)?;
    let apron_paths = root
        .descendants()
        .filter(|n| n.is_element() && n.attribute("class") == Some("vice-seam-apron"))
        .count();
    if declared_aprons != apron_paths || (profile == SvgProfile::PurePartition && apron_paths != 0)
    {
        return Err(IndependentSvgError::ProfileContract);
    }
    let pixels = u64::from(width_px) * u64::from(height_px);
    if width_px == 0 || height_px == 0 || pixels > MAX_RENDER_PIXELS {
        return Err(IndependentSvgError::RenderBudget);
    }

    let options = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg_bytes, &options)
        .map_err(|e| IndependentSvgError::Svg(e.to_string()))?;
    let mut pixmap =
        tiny_skia::Pixmap::new(width_px, height_px).ok_or(IndependentSvgError::Allocation)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    let premultiplied_rgba8 = pixmap.data().to_vec();
    let png_bytes = pixmap
        .encode_png()
        .map_err(|e| IndependentSvgError::Png(e.to_string()))?;
    Ok(IndependentlyRenderedSvg {
        profile,
        scene_digest_sha256: scene_digest,
        svg_digest_sha256: hex::encode(Sha256::digest(svg_bytes)),
        render_digest_sha256: hex::encode(Sha256::digest(&premultiplied_rgba8)),
        width_px,
        height_px,
        apron_paths,
        premultiplied_rgba8,
        png_bytes,
    })
}
