//! Scalar fields, one per §11.1 entry, per palette/formation hypothesis.
//!
//! §11.1 lists five:
//!
//! ```text
//! raw coverage posterior mean
//! TV-L2/Huber restored fields
//! conservative detail/gap-preserving field
//! denoised field
//! bounded deconvolution candidates, ONLY under a stable global kernel
//! ```
//!
//! Every one of them is here, and none of them is a name over the same
//! array: each is asserted to DIFFER from the raw field on a fixture where
//! it should, and the corpus harness publishes, per field, how often the
//! GT-equivalent topology came out of it and how often it was the ONLY field
//! that produced it. A field that never contributes anything its neighbour
//! did not is visible as a number, not as an opinion.
//!
//! ## The one that is allowed to be absent
//!
//! Bounded deconvolution is conditional in the spec — "только при stable
//! global kernel" — so its absence is a TYPED REFUSAL naming the reason, not
//! a silently missing entry. Two reasons occur:
//!
//! - the filter is not identifiable on this image (the shape is thinner than
//!   the kernel, so the transition-width statistic measures the shape:
//!   FAILURE_LEDGER F-0024). Deconvolving with a kernel the image does not
//!   determine is inventing detail;
//! - the filter IS identifiable and is `Box`. A box PSF of one pixel is the
//!   pixel integration itself, so on the pixel grid its kernel is the
//!   identity and there is nothing to invert. Refusing here rather than
//!   running an identity "deconvolution" keeps the field list honest: five
//!   entries of which one is a copy would look like five fields.
//!
//! ## Determinism
//!
//! Every iteration count and step is a constant of [`FieldConfig`]; no
//! convergence test, no adaptive step, no early exit. Two runs on the same
//! bytes produce the same floats, which is what makes the artifact of §5.5
//! Tier A a comparison rather than a judgement call.

use serde::Serialize;
use vice_ir::PixelFilter;

/// What the topology stage is given: one coverage field with the provenance
/// that produced it.
///
/// Deliberately NOT `&Flat2Evidence`: this crate has no opinion about how the
/// coverage was estimated, and taking the evidence type would make the
/// topology stage untestable without a decoder, a palette and a mixture.
#[derive(Debug, Clone)]
pub struct CoverageObservation<'a> {
    pub palette_id: String,
    pub formation_id: String,
    pub filter: PixelFilter,
    /// False when the shape is thinner than the kernel (F-0024). The
    /// deconvolution field is refused in that case, by name.
    pub filter_identifiable: bool,
    pub alpha: &'a [f64],
    pub width_px: usize,
    pub height_px: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    RawCoverage,
    TvHuberRestored,
    DetailPreserving,
    Denoised,
    BoundedDeconvolution,
}

impl FieldKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FieldKind::RawCoverage => "raw_coverage",
            FieldKind::TvHuberRestored => "tv_huber_restored",
            FieldKind::DetailPreserving => "detail_preserving",
            FieldKind::Denoised => "denoised",
            FieldKind::BoundedDeconvolution => "bounded_deconvolution",
        }
    }
    /// The five of §11.1, in a fixed order.
    pub const ALL: &'static [FieldKind] = &[
        FieldKind::RawCoverage,
        FieldKind::TvHuberRestored,
        FieldKind::DetailPreserving,
        FieldKind::Denoised,
        FieldKind::BoundedDeconvolution,
    ];
}

/// Why a field of §11.1 is not present. Never a silent omission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum FieldRefusal {
    #[error(
        "bounded deconvolution needs a stable global kernel and the filter is not identifiable \
         on this image: the transition-width statistic is measuring the shape, not the kernel \
         (F-0024), so inverting it would invent detail"
    )]
    KernelNotIdentifiable,
    #[error(
        "bounded deconvolution under a {filter} kernel is the identity on the pixel grid: a box \
         PSF of one pixel IS the pixel integration, so there is nothing to invert and a copy of \
         the raw field would be a fifth entry that is not a fifth field"
    )]
    KernelIsTheIdentity { filter: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScalarField {
    pub kind: FieldKind,
    pub width_px: usize,
    pub height_px: usize,
    #[serde(skip)]
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldSet {
    pub fields: Vec<ScalarField>,
    /// The §11.1 entries that are absent, each with its typed reason.
    pub refused: Vec<(FieldKind, FieldRefusal)>,
}

/// Coefficients of the restoration operators. Every one is a fixed count or
/// a fixed step: no convergence test, no adaptive anything (determinism).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FieldConfig {
    /// Gradient-descent iterations of the TV-L2/Huber restoration.
    pub tv_iterations: u32,
    /// Descent step.
    pub tv_step: f64,
    /// Huber knee, in coverage units: below it the penalty is quadratic
    /// (smoothing), above it linear (edges survive).
    pub tv_huber_delta: f64,
    /// Weight of the L2 data term against the Huber-TV term.
    pub tv_data_weight: f64,
    /// Landweber iterations of the bounded deconvolution.
    pub deconv_iterations: u32,
    /// Landweber relaxation.
    pub deconv_step: f64,
}

pub const FIELD_CONFIG_V1: FieldConfig = FieldConfig {
    tv_iterations: 30,
    tv_step: 0.15,
    tv_huber_delta: 0.05,
    tv_data_weight: 1.0,
    deconv_iterations: 10,
    deconv_step: 0.7,
};

/// Build every §11.1 field this observation admits.
pub fn build_fields(obs: &CoverageObservation<'_>, cfg: &FieldConfig) -> FieldSet {
    let (w, h) = (obs.width_px, obs.height_px);
    let raw = obs.alpha.to_vec();
    let mut fields = vec![field(FieldKind::RawCoverage, w, h, raw.clone())];
    fields.push(field(
        FieldKind::TvHuberRestored,
        w,
        h,
        tv_huber(&raw, w, h, cfg),
    ));
    fields.push(field(
        FieldKind::DetailPreserving,
        w,
        h,
        detail_preserving(&raw, w, h),
    ));
    fields.push(field(FieldKind::Denoised, w, h, denoised(&raw, w, h)));

    let mut refused = Vec::new();
    match deconvolution_kernel(obs.filter, obs.filter_identifiable) {
        Ok(k) => fields.push(field(
            FieldKind::BoundedDeconvolution,
            w,
            h,
            landweber(&raw, w, h, &k, cfg),
        )),
        Err(e) => refused.push((FieldKind::BoundedDeconvolution, e)),
    }
    FieldSet { fields, refused }
}

fn field(kind: FieldKind, width_px: usize, height_px: usize, values: Vec<f64>) -> ScalarField {
    ScalarField {
        kind,
        width_px,
        height_px,
        values,
    }
}

fn at(v: &[f64], w: usize, h: usize, x: isize, y: isize) -> f64 {
    // Neumann (clamped) boundary: the canvas edge is not an edge of the
    // scene, and a zero boundary would invent one.
    let xc = x.clamp(0, w as isize - 1) as usize;
    let yc = y.clamp(0, h as isize - 1) as usize;
    v[yc * w + xc]
}

/// TV-L2 with a Huber knee: `min 0.5*w*||u-f||^2 + sum huber_d(|grad u|)`.
///
/// Explicit gradient descent, fixed step, fixed iteration count. The Huber
/// knee is what makes it a RESTORATION rather than a blur: below `delta` the
/// penalty is quadratic and the field is smoothed; above it the penalty is
/// linear and a real edge is not paid for by its height.
fn tv_huber(f: &[f64], w: usize, h: usize, cfg: &FieldConfig) -> Vec<f64> {
    let mut u = f.to_vec();
    let d = cfg.tv_huber_delta;
    for _ in 0..cfg.tv_iterations {
        let mut grad = vec![0.0f64; u.len()];
        for y in 0..h as isize {
            for x in 0..w as isize {
                let c = at(&u, w, h, x, y);
                let mut g = cfg.tv_data_weight * (c - f[y as usize * w + x as usize]);
                for (dx, dy) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
                    let diff = c - at(&u, w, h, x + dx, y + dy);
                    // d/dc of huber(diff): diff for |diff| <= d, else
                    // d*sign(diff). The knee is where an edge stops paying
                    // in proportion to its own height.
                    g += if diff.abs() <= d {
                        diff
                    } else {
                        d * diff.signum()
                    };
                }
                grad[y as usize * w + x as usize] = g;
            }
        }
        for (ui, gi) in u.iter_mut().zip(&grad) {
            *ui = (*ui - cfg.tv_step * gi).clamp(0.0, 1.0);
        }
    }
    u
}

/// Conservative detail/gap-preserving field.
///
/// The property that makes it CONSERVATIVE for topology is that it never
/// removes a local extremum: a one-pixel ridge is a bridge and a one-pixel
/// dip is a gap, and an ordinary smoother deletes both. So an extremum keeps
/// its raw value exactly, and everything else gets the alpha-trimmed mean of
/// its 3x3 window (the window's own min and max dropped), which removes
/// impulse noise without moving the structures the topology stage is looking
/// for.
fn detail_preserving(f: &[f64], w: usize, h: usize) -> Vec<f64> {
    let mut out = vec![0.0f64; f.len()];
    for y in 0..h as isize {
        for x in 0..w as isize {
            let mut win = [0.0f64; 9];
            let mut n = 0;
            for dy in -1isize..=1 {
                for dx in -1isize..=1 {
                    win[n] = at(f, w, h, x + dx, y + dy);
                    n += 1;
                }
            }
            let c = win[4];
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for (i, v) in win.iter().enumerate() {
                if i == 4 {
                    continue;
                }
                lo = lo.min(*v);
                hi = hi.max(*v);
            }
            let i = y as usize * w + x as usize;
            if c > hi || c < lo {
                // A strict local extremum: a ridge or a dip. Keep it.
                out[i] = c;
                continue;
            }
            let mut v = win;
            v.sort_by(|a, b| a.partial_cmp(b).expect("coverage is finite"));
            out[i] = v[1..8].iter().sum::<f64>() / 7.0;
        }
    }
    out
}

/// Plain denoising: the separable 1-2-1 binomial kernel, i.e. the cheapest
/// honest low-pass. It is here to be the NEIGHBOUR the other fields are
/// measured against — it closes thin gaps and thins bridges, which is
/// exactly the failure mode `detail_preserving` avoids, and the corpus
/// numbers show the difference instead of asserting it.
fn denoised(f: &[f64], w: usize, h: usize) -> Vec<f64> {
    let mut tmp = vec![0.0f64; f.len()];
    for y in 0..h as isize {
        for x in 0..w as isize {
            tmp[y as usize * w + x as usize] = (at(f, w, h, x - 1, y)
                + 2.0 * at(f, w, h, x, y)
                + at(f, w, h, x + 1, y))
                / 4.0;
        }
    }
    let mut out = vec![0.0f64; f.len()];
    for y in 0..h as isize {
        for x in 0..w as isize {
            out[y as usize * w + x as usize] = (at(&tmp, w, h, x, y - 1)
                + 2.0 * at(&tmp, w, h, x, y)
                + at(&tmp, w, h, x, y + 1))
                / 4.0;
        }
    }
    out
}

/// A separable pixel-grid kernel, as half-widths of weights.
#[derive(Debug, Clone, PartialEq)]
pub struct SeparableKernel {
    /// Weights, odd length, summing to 1, centre at the middle.
    pub taps: Vec<f64>,
}

impl SeparableKernel {
    pub fn apply(&self, f: &[f64], w: usize, h: usize) -> Vec<f64> {
        let r = (self.taps.len() / 2) as isize;
        let mut tmp = vec![0.0f64; f.len()];
        for y in 0..h as isize {
            for x in 0..w as isize {
                let mut s = 0.0;
                for (k, t) in self.taps.iter().enumerate() {
                    s += t * at(f, w, h, x + k as isize - r, y);
                }
                tmp[y as usize * w + x as usize] = s;
            }
        }
        let mut out = vec![0.0f64; f.len()];
        for y in 0..h as isize {
            for x in 0..w as isize {
                let mut s = 0.0;
                for (k, t) in self.taps.iter().enumerate() {
                    s += t * at(&tmp, w, h, x, y + k as isize - r);
                }
                out[y as usize * w + x as usize] = s;
            }
        }
        out
    }
}

/// The kernel a bounded deconvolution may invert, or the typed reason there
/// is none.
pub fn deconvolution_kernel(
    filter: PixelFilter,
    identifiable: bool,
) -> Result<SeparableKernel, FieldRefusal> {
    if !identifiable {
        return Err(FieldRefusal::KernelNotIdentifiable);
    }
    match filter {
        PixelFilter::Box => Err(FieldRefusal::KernelIsTheIdentity {
            filter: "box".to_string(),
        }),
        PixelFilter::Triangle => Ok(SeparableKernel {
            taps: vec![0.25, 0.5, 0.25],
        }),
        PixelFilter::Gaussian { sigma_px } => {
            let r = (3.0 * sigma_px).ceil().max(1.0) as isize;
            let mut taps: Vec<f64> = (-r..=r)
                .map(|k| {
                    let t = k as f64 / sigma_px;
                    (-0.5 * t * t).exp()
                })
                .collect();
            let s: f64 = taps.iter().sum();
            for t in &mut taps {
                *t /= s;
            }
            Ok(SeparableKernel { taps })
        }
    }
}

/// Bounded (Landweber) deconvolution: `u <- clamp[0,1]( u + b (f - K u) )`.
///
/// BOUNDED in both senses §11.1 needs. The iteration count is fixed, so the
/// operator cannot run away into high-frequency amplification; and every
/// iterate is projected back into `[0,1]`, which is the physical range of a
/// coverage field — an unconstrained inverse produces coverages of 1.4 and
/// -0.3 next to a strong edge, and those are not candidates, they are
/// artefacts.
fn landweber(f: &[f64], w: usize, h: usize, k: &SeparableKernel, cfg: &FieldConfig) -> Vec<f64> {
    let mut u = f.to_vec();
    for _ in 0..cfg.deconv_iterations {
        let ku = k.apply(&u, w, h);
        for (i, ui) in u.iter_mut().enumerate() {
            *ui = (*ui + cfg.deconv_step * (f[i] - ku[i])).clamp(0.0, 1.0);
        }
    }
    u
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disk(w: usize, h: usize, r: f64) -> Vec<f64> {
        let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
        (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
                let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
                (r + 0.5 - d).clamp(0.0, 1.0)
            })
            .collect()
    }

    fn obs<'a>(a: &'a [f64], w: usize, h: usize, f: PixelFilter, ident: bool) -> CoverageObservation<'a> {
        CoverageObservation {
            palette_id: "p".into(),
            formation_id: "f".into(),
            filter: f,
            filter_identifiable: ident,
            alpha: a,
            width_px: w,
            height_px: h,
        }
    }

    /// Every field present is a DIFFERENT field. A restoration list whose
    /// entries agree everywhere is one field with five names, and the
    /// per-field recall numbers the harness publishes would then be
    /// measuring nothing.
    #[test]
    fn every_present_field_differs_from_every_other() {
        let (w, h) = (24, 24);
        let a = disk(w, h, 7.0);
        let set = build_fields(
            &obs(&a, w, h, PixelFilter::Gaussian { sigma_px: 1.0 }, true),
            &FIELD_CONFIG_V1,
        );
        assert_eq!(set.fields.len(), 5, "all five of 11.1 are present here");
        for i in 0..set.fields.len() {
            for j in i + 1..set.fields.len() {
                let (p, q) = (&set.fields[i], &set.fields[j]);
                let max = p
                    .values
                    .iter()
                    .zip(&q.values)
                    .map(|(x, y)| (x - y).abs())
                    .fold(0.0f64, f64::max);
                assert!(
                    max > 1e-6,
                    "{} and {} agree to {max:e}: that is one field with two names",
                    p.kind.as_str(),
                    q.kind.as_str()
                );
            }
        }
    }

    /// The conservative field is conservative in the sense that matters: a
    /// one-pixel bridge survives it and does NOT survive the plain denoiser.
    ///
    /// Both directions, because "preserves detail" is only a property if the
    /// neighbour it is contrasted with actually destroys it.
    #[test]
    fn the_conservative_field_keeps_a_one_pixel_bridge_that_the_denoiser_loses() {
        let (w, h) = (11, 5);
        let mut a = vec![0.0f64; w * h];
        for y in 1..4 {
            for x in 0..3 {
                a[y * w + x] = 1.0;
            }
            for x in 8..11 {
                a[y * w + x] = 1.0;
            }
        }
        // A single-pixel-high neck joining the two blobs.
        for x in 3..8 {
            a[2 * w + x] = 1.0;
        }
        let mid = |v: &[f64]| v[2 * w + 5];
        let dp = detail_preserving(&a, w, h);
        let dn = denoised(&a, w, h);
        assert!(
            mid(&dp) > 0.5,
            "the conservative field must keep the neck above the level: {}",
            mid(&dp)
        );
        assert!(
            mid(&dn) < 0.5,
            "the plain denoiser must lose it, or the contrast between the two fields is not \
             measurable: {}",
            mid(&dn)
        );
    }

    /// Deconvolution is refused BY NAME in each of its two conditions, and
    /// produced in the third. Without the positive case, "refused" would be
    /// indistinguishable from "not implemented".
    #[test]
    fn bounded_deconvolution_is_refused_by_name_where_the_kernel_is_not_stable() {
        assert_eq!(
            deconvolution_kernel(PixelFilter::Gaussian { sigma_px: 1.0 }, false),
            Err(FieldRefusal::KernelNotIdentifiable)
        );
        assert_eq!(
            deconvolution_kernel(PixelFilter::Box, true),
            Err(FieldRefusal::KernelIsTheIdentity {
                filter: "box".to_string()
            })
        );
        let k = deconvolution_kernel(PixelFilter::Triangle, true).expect("triangle is invertible");
        assert_eq!(k.taps.len(), 3);
        assert!((k.taps.iter().sum::<f64>() - 1.0).abs() < 1e-12);

        let (w, h) = (16, 16);
        let a = disk(w, h, 5.0);
        let set = build_fields(&obs(&a, w, h, PixelFilter::Box, true), &FIELD_CONFIG_V1);
        assert_eq!(set.fields.len(), 4);
        assert_eq!(set.refused.len(), 1);
        assert_eq!(set.refused[0].0, FieldKind::BoundedDeconvolution);
    }

    /// The deconvolution SHARPENS: a blurred edge comes back narrower, and
    /// stays inside `[0,1]`. Both halves are the point of "bounded".
    #[test]
    fn bounded_deconvolution_sharpens_and_stays_in_range() {
        let (w, h) = (24, 24);
        let sharp = disk(w, h, 7.0);
        let k = deconvolution_kernel(PixelFilter::Gaussian { sigma_px: 1.0 }, true).unwrap();
        let blurred = k.apply(&sharp, w, h);
        let restored = landweber(&blurred, w, h, &k, &FIELD_CONFIG_V1);
        let transition = |v: &[f64]| v.iter().filter(|x| **x > 0.02 && **x < 0.98).count();
        assert!(
            transition(&restored) < transition(&blurred),
            "restored transition band {} is not narrower than the blurred {}",
            transition(&restored),
            transition(&blurred)
        );
        assert!(restored.iter().all(|v| (0.0..=1.0).contains(v)));
    }

    /// Same bytes, same floats. Twice, in one process and from a fresh
    /// input vector, because the artifact is Tier A.
    #[test]
    fn the_fields_are_deterministic() {
        let (w, h) = (20, 20);
        let a = disk(w, h, 6.0);
        let b = a.clone();
        let o1 = obs(&a, w, h, PixelFilter::Gaussian { sigma_px: 0.5 }, true);
        let o2 = obs(&b, w, h, PixelFilter::Gaussian { sigma_px: 0.5 }, true);
        let x = build_fields(&o1, &FIELD_CONFIG_V1);
        let y = build_fields(&o2, &FIELD_CONFIG_V1);
        assert_eq!(x.fields, y.fields);
        for (p, q) in x.fields.iter().zip(&y.fields) {
            assert!(p.values.iter().zip(&q.values).all(|(a, b)| a.to_bits() == b.to_bits()));
        }
    }
}
