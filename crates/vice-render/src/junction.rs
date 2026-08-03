//! Explicit area-fraction simplex certificates for multi-face junctions (M8).

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};
use vice_ir::FaceId;

use crate::partition::PartitionRender;

pub const JUNCTION_FRACTION_SCHEMA: &str = "vice-classic/junction-fractions/v1";

#[derive(Debug, Clone, PartialEq)]
pub struct JunctionFractionSample {
    pub x: u32,
    pub y: u32,
    pub fractions: Vec<(FaceId, f64)>,
    pub sum: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JunctionFractionCertificate {
    pub schema: &'static str,
    pub incident_faces: Vec<FaceId>,
    pub samples: Vec<JunctionFractionSample>,
    pub max_partition_sum_error: f64,
    pub min_fraction: f64,
    pub max_fraction: f64,
    pub digest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum JunctionCertificateError {
    #[error("a junction certificate requires at least three distinct incident faces")]
    TooFewIncidentFaces,
    #[error("junction support floor or tolerance is malformed")]
    InvalidTolerance,
    #[error("incident face {face:?} does not exist in the render")]
    UnknownFace { face: FaceId },
    #[error("render coverage dimensions are malformed")]
    MalformedRender,
    #[error("face {face:?} has non-simplex coverage {value} at ({x},{y})")]
    FractionOutOfRange {
        face: FaceId,
        x: u32,
        y: u32,
        value: f64,
    },
    #[error("partition fractions sum to {sum} at ({x},{y}), outside tolerance {tolerance}")]
    PartitionSum {
        x: u32,
        y: u32,
        sum: f64,
        tolerance: f64,
    },
    #[error("no pixel contains positive support from at least three incident faces")]
    NoMultiFaceSample,
}

/// Revalidate the complete partition and extract only pixels at which at
/// least three declared incident faces have positive area.  No pairwise
/// mixture is computed: each sample is one non-negative simplex whose sum is
/// checked against the complete face partition.
pub fn certify_junction_fractions(
    render: &PartitionRender,
    incident_faces: &[FaceId],
    support_floor: f64,
    tolerance: f64,
) -> Result<JunctionFractionCertificate, JunctionCertificateError> {
    if !support_floor.is_finite()
        || !tolerance.is_finite()
        || !(0.0..1.0).contains(&support_floor)
        || !(0.0..1.0).contains(&tolerance)
    {
        return Err(JunctionCertificateError::InvalidTolerance);
    }
    let incident = incident_faces.iter().copied().collect::<BTreeSet<_>>();
    if incident.len() < 3 {
        return Err(JunctionCertificateError::TooFewIncidentFaces);
    }
    for face in &incident {
        if face.0 as usize >= render.face_coverage.len() {
            return Err(JunctionCertificateError::UnknownFace { face: *face });
        }
    }
    let n = (render.width_px as usize)
        .checked_mul(render.height_px as usize)
        .ok_or(JunctionCertificateError::MalformedRender)?;
    if render.face_coverage.is_empty()
        || render
            .face_coverage
            .iter()
            .any(|coverage| coverage.len() != n)
        || render.composite.len() != n
    {
        return Err(JunctionCertificateError::MalformedRender);
    }

    let mut samples = Vec::new();
    let mut max_partition_sum_error = 0.0f64;
    let mut min_fraction = 1.0f64;
    let mut max_fraction = 0.0f64;
    for i in 0..n {
        let x = (i % render.width_px as usize) as u32;
        let y = (i / render.width_px as usize) as u32;
        let mut sum = 0.0;
        for (face, coverage) in render.face_coverage.iter().enumerate() {
            let value = coverage[i];
            if !value.is_finite() || value < -tolerance || value > 1.0 + tolerance {
                return Err(JunctionCertificateError::FractionOutOfRange {
                    face: FaceId(face as u32),
                    x,
                    y,
                    value,
                });
            }
            sum += value;
        }
        let error = (sum - 1.0).abs();
        max_partition_sum_error = max_partition_sum_error.max(error);
        if error > tolerance {
            return Err(JunctionCertificateError::PartitionSum {
                x,
                y,
                sum,
                tolerance,
            });
        }
        let fractions = incident
            .iter()
            .map(|face| (*face, render.face_coverage[face.0 as usize][i]))
            .filter(|(_, value)| *value > support_floor)
            .collect::<Vec<_>>();
        if fractions.len() >= 3 {
            for (_, value) in &fractions {
                min_fraction = min_fraction.min(*value);
                max_fraction = max_fraction.max(*value);
            }
            samples.push(JunctionFractionSample {
                x,
                y,
                fractions,
                sum,
            });
        }
    }
    if samples.is_empty() {
        return Err(JunctionCertificateError::NoMultiFaceSample);
    }
    let incident_faces = incident.into_iter().collect::<Vec<_>>();
    let digest_sha256 = digest(render, &incident_faces, &samples);
    Ok(JunctionFractionCertificate {
        schema: JUNCTION_FRACTION_SCHEMA,
        incident_faces,
        samples,
        max_partition_sum_error,
        min_fraction,
        max_fraction,
        digest_sha256,
    })
}

fn digest(
    render: &PartitionRender,
    incident: &[FaceId],
    samples: &[JunctionFractionSample],
) -> String {
    let mut h = Sha256::new();
    h.update(JUNCTION_FRACTION_SCHEMA.as_bytes());
    h.update(render.width_px.to_le_bytes());
    h.update(render.height_px.to_le_bytes());
    for face in incident {
        h.update(face.0.to_le_bytes());
    }
    for sample in samples {
        h.update(sample.x.to_le_bytes());
        h.update(sample.y.to_le_bytes());
        h.update(sample.sum.to_bits().to_le_bytes());
        for (face, value) in &sample.fractions {
            h.update(face.0.to_le_bytes());
            h.update(value.to_bits().to_le_bytes());
        }
    }
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vice_ir::color::PremulRgba;

    fn analytic() -> PartitionRender {
        PartitionRender {
            width_px: 2,
            height_px: 1,
            face_coverage: vec![
                vec![0.0, 1.0],
                vec![0.375, 0.0],
                vec![0.375, 0.0],
                vec![0.25, 0.0],
            ],
            composite: vec![
                PremulRgba {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
                PremulRgba {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            ],
        }
    }

    #[test]
    fn analytic_triple_junction_is_one_simplex_not_three_pairwise_mixtures() {
        let c =
            certify_junction_fractions(&analytic(), &[FaceId(1), FaceId(2), FaceId(3)], 0.0, 1e-12)
                .unwrap();
        assert_eq!(c.samples.len(), 1);
        assert_eq!(
            c.samples[0].fractions,
            vec![(FaceId(1), 0.375), (FaceId(2), 0.375), (FaceId(3), 0.25)]
        );
        assert_eq!(c.samples[0].sum, 1.0);
    }

    #[test]
    fn incident_face_permutation_cannot_change_the_certificate() {
        let a =
            certify_junction_fractions(&analytic(), &[FaceId(3), FaceId(1), FaceId(2)], 0.0, 1e-12)
                .unwrap();
        let b =
            certify_junction_fractions(&analytic(), &[FaceId(2), FaceId(3), FaceId(1)], 0.0, 1e-12)
                .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn gap_overlap_and_pairwise_only_inputs_are_refused() {
        let mut gap = analytic();
        gap.face_coverage[3][0] = 0.20;
        assert!(matches!(
            certify_junction_fractions(&gap, &[FaceId(1), FaceId(2), FaceId(3)], 0.0, 1e-12),
            Err(JunctionCertificateError::PartitionSum { .. })
        ));
        assert!(matches!(
            certify_junction_fractions(&analytic(), &[FaceId(1), FaceId(2)], 0.0, 1e-12),
            Err(JunctionCertificateError::TooFewIncidentFaces)
        ));
    }
}
