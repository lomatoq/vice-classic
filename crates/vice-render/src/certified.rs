//! [`CertifiedMesh`] — the embedding certificate as a property of a VALUE
//! (debt D-4; REVIEW_M2_A M2-A-N8, REVIEW_M2_B M2-B-N5, ADR-0010).
//!
//! What M2 left open. `ValidatedScene` means "combinatorially valid": every
//! §12 invariant holds, but nothing about the geometric embedding. The
//! embedding certificate — loop orientation, the numeric domain the
//! tolerances were proven on, the resource bound — was a property of
//! CALLING `render_*`, not of any value. In M2 that was safe because every
//! executable path to pixels certified for itself. Both cold reviews said
//! the same thing about M3: the moment a consumer of `&ValidatedScene`
//! appears that does NOT render — a GT loader, identifiability metadata, a
//! scorecard — a B2-class scene passes it silently again.
//!
//! M3 introduces exactly those consumers, so the certificate becomes a
//! type. A `CertifiedMesh` cannot be constructed except by passing the
//! checks, it carries the [`RenderOptions`] it was certified UNDER (a
//! certificate for another numeric domain is not a certificate), and it is
//! now the only door to the mesh-level renderer.
//!
//! What it does and does not claim, precisely:
//!
//! - CLAIMS: this mesh has the loop-orientation structure of a planar
//!   embedding, and its geometry and canvas lie inside the numeric domain
//!   whose tolerances the renderer will use.
//! - DOES NOT CLAIM: that the faces tile the window. Nesting errors
//!   (reviewer scene B2) have perfectly oriented loops and are caught by
//!   the per-pixel range check, which is per-PIXEL work and belongs to the
//!   render. `CertifiedMesh` is the geometric half of the pair described in
//!   ADR-0010, and saying so is the point — a witness that overclaims is
//!   worse than none.

use vice_ir::{PixelFilter, ValidatedScene};

use crate::embedding::verify_embedding;
use crate::mesh::RenderMesh;
use crate::partition::{check_numeric_domain, RenderOptions, MAX_COVERAGE_ELEMENTS};
use crate::render_error::RenderError;

/// A [`RenderMesh`] together with a witness that it passed the geometric
/// certification of [`RenderOptions`]-bound checks. Private field: the only
/// constructors are the two below.
#[derive(Debug, Clone, PartialEq)]
pub struct CertifiedMesh {
    mesh: RenderMesh,
    options: RenderOptions,
}

impl CertifiedMesh {
    /// Certify an already-built mesh under `options`.
    ///
    /// The checks run in the order the M2 renderer ran them — resource
    /// bound, then numeric domain, then loop orientation — so the typed
    /// error a caller sees for a given input is unchanged by this
    /// refactoring. That ordering is part of the observable contract: the
    /// gate tests name specific errors for specific scenes.
    pub fn certify(mesh: RenderMesh, options: RenderOptions) -> Result<Self, RenderError> {
        let elements =
            u64::from(mesh.width_px) * u64::from(mesh.height_px) * mesh.face_loops.len() as u64;
        if elements > MAX_COVERAGE_ELEMENTS {
            return Err(RenderError::CanvasTooLarge {
                width_px: mesh.width_px,
                height_px: mesh.height_px,
                faces: mesh.face_loops.len(),
                limit_elements: MAX_COVERAGE_ELEMENTS,
            });
        }
        check_numeric_domain(&mesh, options.domain())?;
        verify_embedding(&mesh)?;
        Ok(CertifiedMesh { mesh, options })
    }

    /// Tessellate a validated scene and certify the result.
    pub fn from_scene(scene: &ValidatedScene, options: RenderOptions) -> Result<Self, RenderError> {
        let filter = scene.scene().formation.pixel_filter;
        if filter != PixelFilter::Box {
            return Err(RenderError::UnsupportedPixelFilter { got: filter });
        }
        let mesh = RenderMesh::build(scene, options.budget)?;
        Self::certify(mesh, options)
    }

    pub fn mesh(&self) -> &RenderMesh {
        &self.mesh
    }

    /// The options this mesh was certified under. Rendering with different
    /// tolerances would be rendering under an uncertified domain, which is
    /// why the renderer reads them from here rather than from an argument.
    pub fn options(&self) -> &RenderOptions {
        &self.options
    }
}
