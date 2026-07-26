//! The corpus's independent external engines, and the flagged
//! inverse-crime arm.
//!
//! Protocol (the same one ADR-0012 fixed for the M2 court, reused
//! deliberately so the two agree on what "coverage of a face" means): every
//! BOUNDED face is drawn on its own as a single nonzero-winding path
//! containing all of that face's loops, white on transparent, and the alpha
//! channel is read back as coverage. The exterior is not drawn — it is the
//! complement, bound by the partition sum.
//!
//! These engines are 8-bit. That is not a defect to hide: it is why they
//! are the INDEPENDENCE arm and not the accuracy arm (ADR-0012 §2), and the
//! corpus records which profile produced each image so a later analysis can
//! never confuse the two roles.

use vice_geom::Pt;
use vice_ir::FaceId;
use vice_render::{render_mesh_partition, CertifiedMesh};

use super::raster::{RasterProfile, ViewTransform};

fn build_tiny_skia_path(loops: &[Vec<Pt>]) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    for lp in loops {
        if lp.len() < 3 {
            continue;
        }
        pb.move_to(lp[0].x as f32, lp[0].y as f32);
        for p in &lp[1..] {
            pb.line_to(p.x as f32, p.y as f32);
        }
        pb.close();
    }
    pb.finish()
}

fn tiny_skia_face(loops: &[Vec<Pt>], w: u32, h: u32) -> Result<Vec<f64>, String> {
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("tiny-skia pixmap")?;
    if let Some(path) = build_tiny_skia_path(loops) {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(tiny_skia::Color::WHITE);
        paint.anti_alias = true;
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }
    Ok(pixmap
        .pixels()
        .iter()
        .map(|p| f64::from(p.alpha()) / 255.0)
        .collect())
}

fn raqote_face(loops: &[Vec<Pt>], w: u32, h: u32) -> Result<Vec<f64>, String> {
    let mut dt = raqote::DrawTarget::new(w as i32, h as i32);
    let mut pb = raqote::PathBuilder::new();
    let mut any = false;
    for lp in loops {
        if lp.len() < 3 {
            continue;
        }
        any = true;
        pb.move_to(lp[0].x as f32, lp[0].y as f32);
        for p in &lp[1..] {
            pb.line_to(p.x as f32, p.y as f32);
        }
        pb.close();
    }
    if any {
        let path = pb.finish();
        dt.fill(
            &path,
            &raqote::Source::Solid(raqote::SolidSource {
                r: 0xff,
                g: 0xff,
                b: 0xff,
                a: 0xff,
            }),
            &raqote::DrawOptions {
                antialias: raqote::AntialiasMode::Gray,
                blend_mode: raqote::BlendMode::Src,
                alpha: 1.0,
            },
        );
    }
    Ok(dt
        .get_data()
        .iter()
        .map(|argb| f64::from((argb >> 24) as u8) / 255.0)
        .collect())
}

/// Rasterize every face with an external engine.
///
/// The exterior is derived as `1 - Σ bounded`, clamped: an 8-bit engine
/// cannot draw the unbounded region, and inventing a path for it would put
/// a construction of ours into an arm whose whole value is that it is not
/// ours.
pub fn rasterize_external(
    loops: &[Vec<Vec<Pt>>],
    exterior: usize,
    w: u32,
    h: u32,
    profile: RasterProfile,
) -> Result<Vec<Vec<f64>>, String> {
    let n = (w as usize) * (h as usize);
    let mut per_face = vec![vec![0.0f64; n]; loops.len()];
    for (fi, lps) in loops.iter().enumerate() {
        if fi == exterior {
            continue;
        }
        per_face[fi] = match profile {
            RasterProfile::TinySkia => tiny_skia_face(lps, w, h)?,
            RasterProfile::Raqote => raqote_face(lps, w, h)?,
            other => return Err(format!("{} is not an external engine", other.as_str())),
        };
    }
    for i in 0..n {
        let bounded: f64 = per_face
            .iter()
            .enumerate()
            .filter(|(fi, _)| *fi != exterior)
            .map(|(_, c)| c[i])
            .sum();
        per_face[exterior][i] = (1.0 - bounded).clamp(0.0, 1.0);
    }
    Ok(per_face)
}

/// The inverse-crime arm: the production renderer, over a scene whose
/// geometry has been transformed into render space.
///
/// Implemented by rendering the certified mesh directly after scaling its
/// polylines, so the arm measures the renderer and not a re-tessellation.
pub fn rasterize_vice_render(
    certified: &CertifiedMesh,
    t: &ViewTransform,
) -> Result<Vec<Vec<f64>>, String> {
    let mut mesh = certified.mesh().clone();
    mesh.width_px = t.width_px;
    mesh.height_px = t.height_px;
    for bp in &mut mesh.boundary_polylines {
        for p in &mut bp.points {
            *p = t.apply(*p);
        }
        bp.max_deviation_px *= t.scale;
        bp.area_error_bound_px2 *= t.scale * t.scale;
    }
    for loops in &mut mesh.face_loops {
        for lp in loops {
            for p in &mut lp.points {
                *p = t.apply(*p);
            }
            lp.area_error_bound_px2 *= t.scale * t.scale;
        }
    }
    let _ = FaceId(0);
    let recertified = CertifiedMesh::certify(mesh, *certified.options())
        .map_err(|e| format!("transformed mesh does not certify: {e}"))?;
    let render = render_mesh_partition(&recertified).map_err(|e| e.to_string())?;
    Ok(render.face_coverage)
}
