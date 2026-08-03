//! M10 line-art evidence: adaptive foreground, skeleton and stroke graph.

mod graph;
mod mask;
mod skeleton;

use std::collections::BTreeSet;

use serde::Serialize;
use vice_image::CanonicalImage;
use vice_ir::{StrokeCap, StrokeJoin, ValidatedStrokeScene};

pub const LINE_ART_EVIDENCE_SCHEMA: &str = "vice-classic/line-art-evidence/v1";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LineArtEvidenceReport {
    pub schema: &'static str,
    pub source_sha256: String,
    pub width_px: u32,
    pub height_px: u32,
    pub adaptive_threshold_codes: u8,
    pub foreground_pixels: u64,
    pub skeleton_pixels: u64,
    pub graph_vertices: u64,
    pub graph_edges: u64,
    pub branch_junctions: u64,
    pub median_width_px: f64,
    pub candidate_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineArtProposal {
    pub report: LineArtEvidenceReport,
    /// Complete load-bearing global style inventory. Irrelevant dimensions are
    /// not duplicated; style is selected by the final common pixel objective.
    pub candidates: Vec<ValidatedStrokeScene>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LineArtRefusal {
    #[error("line-art source has no measurable foreground/background contrast")]
    NoContrast,
    #[error("line-art foreground is empty or fills the complete canvas")]
    DegenerateForeground,
    #[error("line-art mask contains an isolated dot with no centerline edge")]
    IsolatedDot,
    #[error("line-art skeleton graph exceeds the bounded edge/vertex inventory")]
    GraphBudget,
    #[error("line-art graph construction failed: {detail}")]
    Graph { detail: String },
}

pub fn propose_line_art_strokes(image: &CanonicalImage) -> Result<LineArtProposal, LineArtRefusal> {
    let measured = mask::measure(image)?;
    let skeleton = skeleton::thin(&measured.foreground, measured.width, measured.height);
    let skeleton_pixels = skeleton.iter().filter(|pixel| **pixel).count();
    if skeleton_pixels == 0 {
        return Err(LineArtRefusal::IsolatedDot);
    }
    let topology = graph::trace(
        &skeleton,
        &measured.foreground,
        &measured.distance_squared,
        measured.width,
        measured.height,
    )?;
    let cap_inventory = [StrokeCap::Butt, StrokeCap::Round, StrokeCap::Square];
    let join_inventory = [
        StrokeJoin::Miter { limit: 4.0 },
        StrokeJoin::Round,
        StrokeJoin::Bevel,
    ];
    let has_caps = topology.vertices.iter().any(|vertex| vertex.degree == 1);
    let has_joins = topology.vertices.iter().any(|vertex| vertex.degree == 2);
    let caps: &[StrokeCap] = if has_caps {
        &cap_inventory
    } else {
        &cap_inventory[..1]
    };
    let joins: &[StrokeJoin] = if has_joins {
        &join_inventory
    } else {
        &join_inventory[1..2]
    };
    let mut identities = BTreeSet::new();
    let mut candidates = Vec::with_capacity(caps.len() * joins.len());
    for &cap in caps {
        for &join in joins {
            let candidate = graph::materialize(
                &topology,
                measured.width as u32,
                measured.height as u32,
                measured.foreground_paint,
                measured.background_paint,
                cap,
                join,
            )?;
            let identity = vice_ir::stroke_scene_digest_sha256(&candidate).map_err(|error| {
                LineArtRefusal::Graph {
                    detail: error.to_string(),
                }
            })?;
            if identities.insert(identity) {
                candidates.push(candidate);
            }
        }
    }
    let median_width_px = graph::median_edge_width(&topology);
    Ok(LineArtProposal {
        report: LineArtEvidenceReport {
            schema: LINE_ART_EVIDENCE_SCHEMA,
            source_sha256: image.source_sha256().into(),
            width_px: measured.width as u32,
            height_px: measured.height as u32,
            adaptive_threshold_codes: measured.threshold,
            foreground_pixels: measured.foreground.iter().filter(|pixel| **pixel).count() as u64,
            skeleton_pixels: skeleton_pixels as u64,
            graph_vertices: topology.vertices.len() as u64,
            graph_edges: topology.edges.len() as u64,
            branch_junctions: topology
                .vertices
                .iter()
                .filter(|vertex| vertex.degree >= 3)
                .count() as u64,
            median_width_px,
            candidate_count: candidates.len() as u64,
        },
        candidates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_image::IccAssumption;

    fn bar_image() -> CanonicalImage {
        let mut rgba = vec![255u8; 32 * 16 * 4];
        for y in 7..10 {
            for x in 4..28 {
                let offset = (y * 32 + x) * 4;
                rgba[offset..offset + 3].fill(0);
            }
        }
        CanonicalImage::from_straight_srgb8(32, 16, rgba, true, IccAssumption::NoProfileAssumedSrgb)
            .unwrap()
    }

    #[test]
    fn an_observed_bar_only_enumerates_load_bearing_caps() {
        let proposal = propose_line_art_strokes(&bar_image()).unwrap();
        assert_eq!(proposal.candidates.len(), 3);
        assert_eq!(proposal.report.graph_edges, 1);
        assert_eq!(proposal.report.graph_vertices, 2);
        assert!((1.0..=4.0).contains(&proposal.report.median_width_px));
        for candidate in proposal.candidates {
            assert_eq!(candidate.scene().edges.len(), 1);
        }
    }
}
