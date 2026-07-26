//! Rasterizer profiles for the GT corpus (spec §27.1 "rasterization by
//! several independent engines/profiles", §27.2 degradation axes).
//!
//! Two rules shape this module.
//!
//! **No inverse crime.** The images a vectorizer will be judged on must not
//! be produced by the vectorizer's own forward model. `vice-render` is
//! therefore present as a profile but permanently flagged
//! `inverse_crime = true`, and the corpus keeps its renders out of the
//! reliability sample. The GT images come from independent implementations:
//! an exact per-pixel polygon-clip integrator written here (different
//! algorithm from the production signed-area accumulator), a supersampling
//! integrator that can carry a PSF, and two external engines.
//!
//! **The instrument is measured, not assumed** (FAILURE_LEDGER meta-rule
//! M-4). Two of these profiles are ours, so their accuracy is a claim we
//! would otherwise be making about ourselves. [`measure_profile_accuracy`]
//! measures it — the supersampler against the exact clip integrator on box
//! coverage, where the correct answer is a pure area and both must agree —
//! and the corpus records the number rather than asserting a quality.

use vice_geom::Pt;
use vice_render::CertifiedMesh;

/// The point-spread function of a degradation cell (§27.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Psf {
    /// Unit box = exact pixel-area coverage.
    Box,
    /// Tent filter of radius 1 px.
    Triangle,
    /// Isotropic gaussian, truncated at 3 sigma.
    Gaussian { sigma_px: f64 },
}

impl Psf {
    pub fn as_str(&self) -> &'static str {
        match self {
            Psf::Box => "box",
            Psf::Triangle => "triangle",
            Psf::Gaussian { .. } => "gaussian",
        }
    }

    /// Support radius in pixels.
    fn radius_px(&self) -> f64 {
        match self {
            Psf::Box => 0.5,
            Psf::Triangle => 1.0,
            Psf::Gaussian { sigma_px } => 3.0 * sigma_px,
        }
    }

    /// Unnormalized weight at an offset from the pixel centre.
    fn weight(&self, dx: f64, dy: f64) -> f64 {
        match self {
            Psf::Box => {
                if dx.abs() <= 0.5 && dy.abs() <= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            Psf::Triangle => {
                let wx = (1.0 - dx.abs()).max(0.0);
                let wy = (1.0 - dy.abs()).max(0.0);
                wx * wy
            }
            Psf::Gaussian { sigma_px } => {
                let s2 = sigma_px * sigma_px;
                (-(dx * dx + dy * dy) / (2.0 * s2)).exp()
            }
        }
    }
}

/// Which rasterizer produced an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RasterProfile {
    /// Exact box coverage by per-pixel polygon clipping + shoelace. Our
    /// own, and a DIFFERENT algorithm from the production accumulator.
    ExactClip,
    /// Supersampled integration with a PSF. Our own; the only profile that
    /// can carry a non-box PSF.
    Supersample,
    /// External scanline AA, Skia lineage.
    TinySkia,
    /// External scanline AA, Firefox lineage.
    Raqote,
    /// The production renderer. INVERSE CRIME: present so the gap can be
    /// measured, never counted as ground truth.
    ViceRender,
}

impl RasterProfile {
    pub fn as_str(&self) -> &'static str {
        match self {
            RasterProfile::ExactClip => "exact-clip",
            RasterProfile::Supersample => "supersample",
            RasterProfile::TinySkia => "tiny-skia",
            RasterProfile::Raqote => "raqote",
            RasterProfile::ViceRender => "vice-render",
        }
    }

    /// True when this profile shares an implementation with the system
    /// under test, so its images must not be used as ground truth (§27.1).
    pub fn is_inverse_crime(&self) -> bool {
        matches!(self, RasterProfile::ViceRender)
    }

    /// Independent of the production renderer AND of each other's lineage.
    pub fn is_independent_engine(&self) -> bool {
        matches!(self, RasterProfile::TinySkia | RasterProfile::Raqote)
    }

    /// Can this profile realize the requested PSF at all?
    pub fn supports(&self, psf: Psf) -> bool {
        match self {
            RasterProfile::Supersample => true,
            // Every other engine integrates a unit box and nothing else;
            // pretending otherwise would silently mislabel the formation.
            _ => psf == Psf::Box,
        }
    }

    pub const ALL: &'static [RasterProfile] = &[
        RasterProfile::ExactClip,
        RasterProfile::Supersample,
        RasterProfile::TinySkia,
        RasterProfile::Raqote,
        RasterProfile::ViceRender,
    ];
}

/// A geometric mapping from scene coordinates to render pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewTransform {
    pub scale: f64,
    pub dx: f64,
    pub dy: f64,
    pub width_px: u32,
    pub height_px: u32,
}

impl ViewTransform {
    pub fn apply(&self, p: Pt) -> Pt {
        Pt::new(p.x * self.scale + self.dx, p.y * self.scale + self.dy)
    }
}

/// Per-face coverage over a render, row-major, indexed by face.
#[derive(Debug, Clone, PartialEq)]
pub struct CoverageStack {
    pub width_px: u32,
    pub height_px: u32,
    pub per_face: Vec<Vec<f64>>,
}

/// Transform every face loop of a certified mesh into render space.
pub fn transformed_loops(certified: &CertifiedMesh, t: &ViewTransform) -> Vec<Vec<Vec<Pt>>> {
    certified
        .mesh()
        .face_loops
        .iter()
        .map(|loops| {
            loops
                .iter()
                .map(|lp| lp.points.iter().map(|p| t.apply(*p)).collect())
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Profile 1: exact per-pixel clip + shoelace
// ---------------------------------------------------------------------------

/// Exact box coverage of one closed loop over one pixel: clip the loop to
/// the pixel box (Sutherland-Hodgman) and take the shoelace of the result.
///
/// For a closed loop, `signed_area(L ∩ box)` IS the integral of its winding
/// over the box, so summing over a face's loops gives that face's exact
/// area coverage. No scanline, no accumulator, no ray convention — the
/// point is that it shares no machinery with the renderer.
fn clip_polygon_to_box(poly: &[Pt], x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Pt> {
    let mut out: Vec<Pt> = poly.to_vec();
    // Half-planes: x >= x0, x <= x1, y >= y0, y <= y1.
    for stage in 0..4 {
        if out.is_empty() {
            return out;
        }
        let inside = |p: &Pt| match stage {
            0 => p.x >= x0,
            1 => p.x <= x1,
            2 => p.y >= y0,
            _ => p.y <= y1,
        };
        let intersect = |a: Pt, b: Pt| -> Pt {
            match stage {
                0 | 1 => {
                    let xc = if stage == 0 { x0 } else { x1 };
                    let t = if (b.x - a.x).abs() < f64::MIN_POSITIVE {
                        0.0
                    } else {
                        (xc - a.x) / (b.x - a.x)
                    };
                    Pt::new(xc, a.y + (b.y - a.y) * t)
                }
                _ => {
                    let yc = if stage == 2 { y0 } else { y1 };
                    let t = if (b.y - a.y).abs() < f64::MIN_POSITIVE {
                        0.0
                    } else {
                        (yc - a.y) / (b.y - a.y)
                    };
                    Pt::new(a.x + (b.x - a.x) * t, yc)
                }
            }
        };
        let mut next: Vec<Pt> = Vec::with_capacity(out.len() + 4);
        for i in 0..out.len() {
            let a = out[i];
            let b = out[(i + 1) % out.len()];
            let (ain, bin) = (inside(&a), inside(&b));
            if ain {
                next.push(a);
            }
            if ain != bin {
                next.push(intersect(a, b));
            }
        }
        out = next;
    }
    out
}

fn shoelace(poly: &[Pt]) -> f64 {
    if poly.len() < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        // Translate to the first vertex before multiplying: the operands
        // stay small, which is the conditioning fix ADR-0012 §8 records for
        // exactly this shape of computation.
        let (ax, ay) = (a.x - poly[0].x, a.y - poly[0].y);
        let (bx, by) = (b.x - poly[0].x, b.y - poly[0].y);
        s += ax * by - bx * ay;
    }
    0.5 * s
}

/// The exact integrator as a pure function of loops.
///
/// Public because two consumers need coverage WITHOUT a scene: the
/// metamorphic properties, which construct geometry directly, and
/// `PartitionTruth`, which must not obtain a truth field from the
/// production renderer (REVIEW_M3 M3-N5).
pub fn exact_clip_loops(loops: &[Vec<Pt>], w: u32, h: u32) -> Vec<f64> {
    exact_clip_face(loops, w, h)
}

fn exact_clip_face(loops: &[Vec<Pt>], w: u32, h: u32) -> Vec<f64> {
    let mut out = vec![0.0f64; (w as usize) * (h as usize)];
    for lp in loops {
        // Bounding box of the loop limits the pixels it can touch.
        let (mut lo_x, mut hi_x, mut lo_y, mut hi_y) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for p in lp {
            lo_x = lo_x.min(p.x);
            hi_x = hi_x.max(p.x);
            lo_y = lo_y.min(p.y);
            hi_y = hi_y.max(p.y);
        }
        if !lo_x.is_finite() {
            continue;
        }
        let px0 = lo_x.floor().max(0.0) as i64;
        let px1 = (hi_x.ceil() as i64).min(i64::from(w));
        let py0 = lo_y.floor().max(0.0) as i64;
        let py1 = (hi_y.ceil() as i64).min(i64::from(h));
        // Closed polylines repeat the first point; drop it for clipping.
        let ring: Vec<Pt> = if lp.len() >= 2 && lp[0] == lp[lp.len() - 1] {
            lp[..lp.len() - 1].to_vec()
        } else {
            lp.clone()
        };
        for y in py0.max(0)..py1 {
            for x in px0.max(0)..px1 {
                let clipped =
                    clip_polygon_to_box(&ring, x as f64, y as f64, (x + 1) as f64, (y + 1) as f64);
                if clipped.len() >= 3 {
                    out[(y as usize) * (w as usize) + x as usize] += shoelace(&clipped);
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Profile 2: supersampled integration with a PSF
// ---------------------------------------------------------------------------

/// Winding number of a point with respect to a set of closed loops, by ray
/// crossing. Independent of both the accumulator and the clip integrator.
fn winding_at(loops: &[Vec<Pt>], p: Pt) -> i32 {
    let mut w = 0;
    for lp in loops {
        let n = lp.len();
        for i in 0..n.saturating_sub(1) {
            let (a, b) = (lp[i], lp[i + 1]);
            if a.y <= p.y {
                if b.y > p.y {
                    // Upward crossing: left turn means the point is inside.
                    let side = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
                    if side > 0.0 {
                        w += 1;
                    }
                }
            } else if b.y <= p.y {
                let side = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
                if side < 0.0 {
                    w -= 1;
                }
            }
        }
    }
    w
}

/// Supersampling rate per pixel and axis. 24 gives 576 samples per pixel;
/// the measured accuracy against the exact integrator is recorded rather
/// than assumed (see `measure_profile_accuracy`).
pub const SUPERSAMPLE_RATE: u32 = 24;

// The accuracy gates of the corpus instruments (spec §27.7 frozen values,
// mirrored in `configs/GATES_V1.toml` `[corpus_instruments]`).
//
// These are CONSTANTS rather than literals inside the tests because
// REVIEW_M3 M3-N3 showed what a literal costs: the frozen file said 0.06
// and the test said 0.06, but nothing connected them, so relaxing the test
// to 0.2 would have moved no hash and tripped no rule. Now the gate file
// and the assertion read the same name, and a test walks every frozen key
// to check they still agree.

/// Max per-pixel disagreement allowed between our supersampler and the
/// exact integrator (measured: 0.0186 at k = 24).
pub const SUPERSAMPLE_MAX_ABS_GATE: f64 = 0.06;
/// Edge-mean of the same (measured: 0.0118).
pub const SUPERSAMPLE_EDGE_MEAN_ABS_GATE: f64 = 0.02;
/// The inverse-crime arm must keep agreeing with the exact integrator to
/// f64 noise; if it ever does not, one of the two is broken.
pub const VICE_RENDER_MAX_ABS_GATE: f64 = 1e-9;
/// Upper bound on the 8-bit AA noise of the external engines on the
/// committed corpus (measured: tiny-skia 0.0793, raqote 0.2387). They carry
/// the INDEPENDENCE role, not the accuracy role (ADR-0012 §2).
pub const EXTERNAL_ENGINE_MAX_ABS_GATE: f64 = 0.35;

fn supersample_face(loops: &[Vec<Pt>], w: u32, h: u32, psf: Psf) -> Vec<f64> {
    let k = SUPERSAMPLE_RATE;
    let r = psf.radius_px();
    let reach = r.ceil() as i64;
    let mut out = vec![0.0f64; (w as usize) * (h as usize)];
    for y in 0..h {
        for x in 0..w {
            let (mut acc, mut wsum) = (0.0f64, 0.0f64);
            for sy in -reach..=reach {
                for sx in -reach..=reach {
                    for jy in 0..k {
                        for jx in 0..k {
                            let ox = sx as f64 + (f64::from(jx) + 0.5) / f64::from(k) - 0.5;
                            let oy = sy as f64 + (f64::from(jy) + 0.5) / f64::from(k) - 0.5;
                            let wgt = psf.weight(ox, oy);
                            if wgt == 0.0 {
                                continue;
                            }
                            wsum += wgt;
                            let p = Pt::new(f64::from(x) + 0.5 + ox, f64::from(y) + 0.5 + oy);
                            if winding_at(loops, p) != 0 {
                                acc += wgt;
                            }
                        }
                    }
                }
            }
            out[(y as usize) * (w as usize) + x as usize] =
                if wsum > 0.0 { acc / wsum } else { 0.0 };
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Rasterize every face of a certified mesh under one profile.
pub fn rasterize(
    certified: &CertifiedMesh,
    t: &ViewTransform,
    profile: RasterProfile,
    psf: Psf,
) -> Result<CoverageStack, String> {
    if !profile.supports(psf) {
        return Err(format!(
            "profile {} cannot realize psf {}",
            profile.as_str(),
            psf.as_str()
        ));
    }
    let loops = transformed_loops(certified, t);
    let (w, h) = (t.width_px, t.height_px);
    let exterior = certified.mesh().exterior.index();
    let per_face: Vec<Vec<f64>> = match profile {
        RasterProfile::ExactClip => loops
            .iter()
            .enumerate()
            .map(|(fi, lps)| {
                let mut cov = exact_clip_face(lps, w, h);
                if fi == exterior {
                    for v in &mut cov {
                        *v += 1.0;
                    }
                }
                cov
            })
            .collect(),
        RasterProfile::Supersample => loops
            .iter()
            .enumerate()
            .map(|(fi, lps)| {
                let mut cov = supersample_face(lps, w, h, psf);
                if fi == exterior {
                    // The exterior's winding is negative inside the shapes;
                    // its coverage is the complement of everything else.
                    for v in &mut cov {
                        *v = 1.0 - *v;
                    }
                }
                cov
            })
            .collect(),
        RasterProfile::TinySkia | RasterProfile::Raqote => {
            super::raster_external::rasterize_external(&loops, exterior, w, h, profile)?
        }
        RasterProfile::ViceRender => {
            // The inverse-crime arm: the production renderer over the same
            // transformed geometry. Reached only through an explicitly
            // flagged profile.
            super::raster_external::rasterize_vice_render(certified, t)?
        }
    };
    Ok(CoverageStack {
        width_px: w,
        height_px: h,
        per_face,
    })
}

/// Measured accuracy of one of OUR profiles against the exact integrator,
/// on box coverage where the correct answer is a pure area.
///
/// Meta-rule M-4: the tool that judges must be measured, not assumed
/// better. The supersampler's error is a property of the sampling rate and
/// of the geometry, and the corpus manifest publishes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProfileAccuracy {
    pub max_abs: f64,
    pub edge_mean_abs: f64,
    pub pixels: usize,
}

pub fn measure_profile_accuracy(
    certified: &CertifiedMesh,
    t: &ViewTransform,
    profile: RasterProfile,
) -> Result<ProfileAccuracy, String> {
    let truth = rasterize(certified, t, RasterProfile::ExactClip, Psf::Box)?;
    let got = rasterize(certified, t, profile, Psf::Box)?;
    let (mut max_abs, mut sum, mut edges) = (0.0f64, 0.0f64, 0usize);
    for (a, b) in truth.per_face.iter().zip(&got.per_face) {
        for (x, y) in a.iter().zip(b) {
            let d = (x - y).abs();
            max_abs = max_abs.max(d);
            // Edge pixels only: interior and empty pixels agree by
            // construction and would dilute the mean into a statement about
            // canvas size instead of accuracy (F-0010).
            if *x > 0.0 && *x < 1.0 {
                sum += d;
                edges += 1;
            }
        }
    }
    Ok(ProfileAccuracy {
        max_abs,
        edge_mean_abs: if edges > 0 { sum / edges as f64 } else { 0.0 },
        pixels: edges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gt::colour::composite_rgba8;
    use crate::gt::grammar::procedural_groups;
    use vice_ir::{BlendSpace, Paint};

    fn view(scene_px: u32, out_px: u32, dx: f64, dy: f64) -> ViewTransform {
        ViewTransform {
            scale: f64::from(out_px) / f64::from(scene_px),
            dx,
            dy,
            width_px: out_px,
            height_px: out_px,
        }
    }

    /// The exact integrator must be exact where the answer is known in
    /// closed form, on pixels of all three kinds.
    #[test]
    fn the_exact_integrator_is_exact_on_analytic_geometry() {
        let square = vec![vec![
            Pt::new(1.5, 1.5),
            Pt::new(4.5, 1.5),
            Pt::new(4.5, 4.5),
            Pt::new(1.5, 4.5),
            Pt::new(1.5, 1.5),
        ]];
        let cov = exact_clip_face(&square, 8, 8);
        let at = |x: usize, y: usize| cov[y * 8 + x];
        assert_eq!(at(1, 1), 0.25, "corner pixel");
        assert_eq!(at(2, 1), 0.5, "edge pixel");
        assert_eq!(at(2, 2), 1.0, "interior pixel");
        assert_eq!(at(6, 6), 0.0, "outside pixel");
        let total: f64 = cov.iter().sum();
        assert!((total - 9.0).abs() < 1e-12, "total {total}");
    }

    /// M-4: the instrument is measured, not assumed. The supersampler is
    /// OURS, so its error against the exact integrator is a number the
    /// corpus must know - and this test both prints it and bounds it.
    #[test]
    fn our_own_profiles_are_measured_against_the_exact_integrator() {
        let groups = procedural_groups(1);
        let mut worst_super = 0.0f64;
        let mut worst_super_edge = 0.0f64;
        let mut worst_vice = 0.0f64;
        // Every third family, at a small size: the supersampler is O(k^2)
        // per pixel and this runs in the default `cargo test` profile.
        for g in groups.iter().step_by(2) {
            let s = &g.scenes[0];
            let t = view(crate::gt::grammar::AUTHORING_CANVAS_PX, 40, 0.0, 0.0);
            let acc = measure_profile_accuracy(s.certified(), &t, RasterProfile::Supersample)
                .expect("supersample measurable");
            worst_super = worst_super.max(acc.max_abs);
            worst_super_edge = worst_super_edge.max(acc.edge_mean_abs);
            let vice = measure_profile_accuracy(s.certified(), &t, RasterProfile::ViceRender)
                .expect("vice-render measurable");
            worst_vice = worst_vice.max(vice.max_abs);
        }
        println!(
            "measured: supersample max {worst_super:.6} edge-mean {worst_super_edge:.6}; vice-render max {worst_vice:.3e}"
        );
        // The supersampler is a point-sampling estimator: its per-pixel
        // error is O(1/k) on an edge, k = 24. The bound is stated from the
        // measurement, with margin, not chosen to be comfortable.
        assert!(
            worst_super < SUPERSAMPLE_MAX_ABS_GATE,
            "supersampler worse than its documented rate: {worst_super}"
        );
        assert!(
            worst_super_edge < SUPERSAMPLE_EDGE_MEAN_ABS_GATE,
            "edge mean {worst_super_edge}"
        );
        // And the inverse-crime arm agrees with the exact integrator to
        // near machine precision. That is exactly WHY it may not be used as
        // ground truth: agreement this tight measures shared assumptions,
        // not correctness.
        assert!(
            worst_vice < VICE_RENDER_MAX_ABS_GATE,
            "the production renderer should agree to f64 noise: {worst_vice}"
        );
        assert!(RasterProfile::ViceRender.is_inverse_crime());
        assert!(!RasterProfile::ExactClip.is_inverse_crime());
    }

    /// Independence has a price and the corpus must know it: the external
    /// engines are 8-bit, so their disagreement is far larger than the
    /// exact arms'. Measuring it here is what keeps the two roles apart
    /// (ADR-0012 section 2).
    #[test]
    fn external_engines_are_independent_and_visibly_coarser() {
        let groups = procedural_groups(1);
        let s = &groups[0].scenes[0];
        let t = view(crate::gt::grammar::AUTHORING_CANVAS_PX, 64, 0.0, 0.0);
        let mut worst = 0.0f64;
        for p in [RasterProfile::TinySkia, RasterProfile::Raqote] {
            let acc = measure_profile_accuracy(s.certified(), &t, p).expect("measurable");
            println!(
                "{}: max {:.4} edge-mean {:.4}",
                p.as_str(),
                acc.max_abs,
                acc.edge_mean_abs
            );
            worst = worst.max(acc.max_abs);
            assert!(p.is_independent_engine());
        }
        assert!(
            worst > 1e-3,
            "an external engine agreeing to 1e-3 would mean it is not independent"
        );
        assert!(
            worst < EXTERNAL_ENGINE_MAX_ABS_GATE,
            "but it must still be the same shape: {worst}"
        );
    }

    /// Every profile must produce a partition: coverage sums to one per
    /// pixel, within that profile's own measured noise.
    #[test]
    fn every_profile_produces_a_partition() {
        let groups = procedural_groups(1);
        let s = &groups[1].scenes[0];
        let t = view(crate::gt::grammar::AUTHORING_CANVAS_PX, 48, 0.25, -0.5);
        for (p, tol) in [
            (RasterProfile::ExactClip, 1e-9),
            (RasterProfile::Supersample, 1e-9),
            (RasterProfile::TinySkia, 0.02),
            (RasterProfile::Raqote, 0.02),
            (RasterProfile::ViceRender, 1e-9),
        ] {
            let st = rasterize(s.certified(), &t, p, Psf::Box).expect("rasterizes");
            let n = (st.width_px as usize) * (st.height_px as usize);
            let mut worst = 0.0f64;
            for i in 0..n {
                let sum: f64 = st.per_face.iter().map(|c| c[i]).sum();
                worst = worst.max((sum - 1.0).abs());
            }
            assert!(worst <= tol, "{}: worst |sum-1| = {worst}", p.as_str());
        }
    }

    #[test]
    fn a_non_box_psf_is_refused_by_the_engines_that_cannot_realize_it() {
        let groups = procedural_groups(1);
        let s = &groups[0].scenes[0];
        let t = view(crate::gt::grammar::AUTHORING_CANVAS_PX, 32, 0.0, 0.0);
        for p in [
            RasterProfile::ExactClip,
            RasterProfile::TinySkia,
            RasterProfile::Raqote,
            RasterProfile::ViceRender,
        ] {
            assert!(
                rasterize(s.certified(), &t, p, Psf::Triangle).is_err(),
                "{} must refuse a PSF it cannot realize rather than render a box",
                p.as_str()
            );
        }
        assert!(rasterize(s.certified(), &t, RasterProfile::Supersample, Psf::Triangle).is_ok());
        assert!(rasterize(
            s.certified(),
            &t,
            RasterProfile::Supersample,
            Psf::Gaussian { sigma_px: 0.6 }
        )
        .is_ok());
    }

    /// A blur must actually blur: a wider PSF moves coverage off the shape
    /// and into its neighbourhood. Without this the PSF axis could be
    /// present in the manifest and absent from the pixels.
    #[test]
    fn the_psf_axis_changes_the_image_monotonically() {
        let groups = procedural_groups(1);
        let s = &groups[0].scenes[0];
        let t = view(crate::gt::grammar::AUTHORING_CANVAS_PX, 48, 0.0, 0.0);
        let face = 1;
        let edge_count = |psf: Psf| {
            let st = rasterize(s.certified(), &t, RasterProfile::Supersample, psf).unwrap();
            st.per_face[face]
                .iter()
                .filter(|v| **v > 0.01 && **v < 0.99)
                .count()
        };
        let b = edge_count(Psf::Box);
        let tri = edge_count(Psf::Triangle);
        let g = edge_count(Psf::Gaussian { sigma_px: 1.2 });
        println!("edge pixels: box {b}, triangle {tri}, gaussian {g}");
        assert!(tri > b, "a tent filter must widen the transition");
        assert!(g > tri, "a 1.2 px gaussian must widen it further");
    }

    /// The blend-space and contrast axes must change the BYTES, not only
    /// the manifest.
    ///
    /// Found while writing this: the blend space is observable ONLY where
    /// two OPAQUE paints mix. With a single ink on a transparent exterior
    /// the mixing happens in alpha, which is linear either way, and the two
    /// blend spaces produce identical bytes. That is a fact about the axis,
    /// so the fixture is chosen to exercise it (a shared edge between two
    /// opaque faces) instead of the assertion being weakened.
    #[test]
    fn blending_in_linear_light_and_in_srgb_give_different_bytes() {
        let groups = procedural_groups(1);
        let shared = groups
            .iter()
            .find(|g| g.shape_family == "shared_edge")
            .expect("the grammar has a two-opaque-face family");
        let s = &shared.scenes[0];
        let t = view(crate::gt::grammar::AUTHORING_CANVAS_PX, 32, 0.3, 0.1);
        let st = rasterize(s.certified(), &t, RasterProfile::ExactClip, Psf::Box).unwrap();
        let paints: Vec<Paint> = s.scene().graph().faces.iter().map(|f| f.paint).collect();

        let lin = composite_rgba8(&st, &paints, BlendSpace::LinearLight, 1.0);
        let srgb = composite_rgba8(&st, &paints, BlendSpace::EncodedSrgb, 1.0);
        assert_ne!(
            lin, srgb,
            "the blend space must be observable in the pixels"
        );

        // And it must be observable exactly where two opaque paints mix:
        // fully-covered pixels of a single face agree between blend spaces.
        let differing_are_mixed = lin
            .chunks(4)
            .zip(srgb.chunks(4))
            .filter(|(a, b)| a != b)
            .all(|(a, _)| a[3] > 0);
        assert!(differing_are_mixed, "only covered pixels may differ");

        let low = composite_rgba8(&st, &paints, BlendSpace::LinearLight, 0.25);
        assert_ne!(lin, low, "the contrast axis must be observable too");
        // Reducing contrast must shrink the SEPARATION between the paints,
        // which is the property the axis exists to vary - and unlike
        // "gets lighter" it does not depend on which colours were drawn.
        let spread = |v: &[u8]| {
            let ch: Vec<f64> = v
                .chunks(4)
                .filter(|p| p[3] > 250)
                .map(|p| f64::from(p[0]))
                .collect();
            let (lo, hi) = ch
                .iter()
                .fold((f64::MAX, f64::MIN), |(l, h), c| (l.min(*c), h.max(*c)));
            hi - lo
        };
        assert!(
            spread(&low) < spread(&lin),
            "lower contrast must reduce paint separation: {} vs {}",
            spread(&low),
            spread(&lin)
        );
    }
}
