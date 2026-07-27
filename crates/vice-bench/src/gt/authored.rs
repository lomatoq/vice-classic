//! Hand-authored SVG corpus (spec §27.1 source 2).
//!
//! Provenance, first, because it is the reason this source exists at all.
//! Every file under `tests/fixtures/gt/authored/` was authored in this
//! repository for this purpose. Nothing is taken from the donor pins or
//! from any third party: REVIEW_M0 condition 6 and debt D-3 make an
//! unreviewed donor asset a licensing problem, and a GT corpus is exactly
//! where that temptation is strongest.
//!
//! Why hand-authored files at all when a grammar exists: the grammar's
//! scenes share one author's idea of what a shape is. Committing readable
//! SVG that a human wrote by hand — asymmetric, off-grid, with corner
//! angles nobody would have parameterized — is a genuinely different
//! source, and it can be inspected and extended by someone who does not
//! read Rust.
//!
//! The accepted SUBSET is deliberately small and strictly enforced: a
//! `viewBox` at the authoring canvas, `<path>` elements with absolute
//! `M/L/C/Q/Z` data and a solid `fill`, each path being one island whose
//! first subpath is its outline and whose later subpaths are its holes.
//! Anything else is a typed refusal rather than a best-effort
//! interpretation — a loader that guesses would make the ground truth a
//! guess. Islands must be disjoint, and that is not taken on trust either:
//! the scene goes through `CertifiedMesh`, whose per-pixel range check is
//! what actually catches an overlap.

use std::collections::BTreeMap;
use std::path::Path as FsPath;

use vice_geom::Pt;
use vice_ir::{ExteriorModel, LinearRgb, Paint, Segment};

use super::build::{ring_signed_area, SceneBuilder};
use super::grammar::{flat2_formation, AUTHORING_CANVAS_PX};
use super::{AuthoredTruth, FixtureOrigin, GtScene, GtSourceGroup, SalientFeature};

/// Why an authored file is not in the accepted subset.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AuthoredError {
    #[error("{file}: {what}")]
    Rejected { file: String, what: String },
}

fn reject(file: &str, what: impl Into<String>) -> AuthoredError {
    AuthoredError::Rejected {
        file: file.to_string(),
        what: what.into(),
    }
}

/// One `<path>` of the accepted subset.
#[derive(Debug, Clone)]
struct AuthoredPath {
    fill: LinearRgb,
    /// Subpaths: `[0]` is the outline, the rest are holes.
    subpaths: Vec<Vec<(Pt, Segment)>>,
}

/// sRGB 8-bit hex to linear RGB (the inverse of `colour::srgb_encode`).
fn parse_fill(file: &str, s: &str) -> Result<LinearRgb, AuthoredError> {
    let h = s.trim();
    if h.len() != 7 || !h.starts_with('#') || !h[1..].bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(reject(file, format!("fill {h:?} must be #rrggbb")));
    }
    let ch = |i: usize| {
        let v = u8::from_str_radix(&h[1 + 2 * i..3 + 2 * i], 16).unwrap_or(0);
        let c = f64::from(v) / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    Ok(LinearRgb {
        r: ch(0),
        g: ch(1),
        b: ch(2),
    })
}

/// Extract the value of `attr` from a `<path .../>` element.
fn attr<'a>(el: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let i = el.find(&key)? + key.len();
    let j = el[i..].find('"')? + i;
    Some(&el[i..j])
}

fn parse_path_data(file: &str, d: &str) -> Result<Vec<Vec<(Pt, Segment)>>, AuthoredError> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() {
            if !cur.trim().is_empty() {
                tokens.push(cur.trim().to_string());
            }
            cur.clear();
            tokens.push(ch.to_string());
        } else if ch == ',' || ch.is_whitespace() {
            if !cur.trim().is_empty() {
                tokens.push(cur.trim().to_string());
            }
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    if !cur.trim().is_empty() {
        tokens.push(cur.trim().to_string());
    }

    let mut subpaths: Vec<Vec<(Pt, Segment)>> = Vec::new();
    let mut current: Vec<(Pt, Segment)> = Vec::new();
    let mut i = 0usize;
    let num = |i: &mut usize| -> Result<f64, AuthoredError> {
        let t = tokens
            .get(*i)
            .ok_or_else(|| reject(file, "path data ends mid-command"))?;
        let v: f64 = t
            .parse()
            .map_err(|_| reject(file, format!("{t:?} is not a number")))?;
        if !v.is_finite() {
            return Err(reject(file, "non-finite coordinate"));
        }
        *i += 1;
        Ok(v)
    };
    while i < tokens.len() {
        let cmd = tokens[i].clone();
        i += 1;
        match cmd.as_str() {
            "M" => {
                if !current.is_empty() {
                    return Err(reject(file, "M inside an unclosed subpath; use Z"));
                }
                let (x, y) = (num(&mut i)?, num(&mut i)?);
                current.push((Pt::new(x, y), Segment::Line));
            }
            "L" => {
                let (x, y) = (num(&mut i)?, num(&mut i)?);
                current.push((Pt::new(x, y), Segment::Line));
            }
            "Q" => {
                let (cx, cy) = (num(&mut i)?, num(&mut i)?);
                let (x, y) = (num(&mut i)?, num(&mut i)?);
                if let Some(last) = current.last_mut() {
                    last.1 = Segment::Quad {
                        ctrl: Pt::new(cx, cy),
                    };
                }
                current.push((Pt::new(x, y), Segment::Line));
            }
            "C" => {
                let (c1x, c1y) = (num(&mut i)?, num(&mut i)?);
                let (c2x, c2y) = (num(&mut i)?, num(&mut i)?);
                let (x, y) = (num(&mut i)?, num(&mut i)?);
                if let Some(last) = current.last_mut() {
                    last.1 = Segment::Cubic {
                        ctrl1: Pt::new(c1x, c1y),
                        ctrl2: Pt::new(c2x, c2y),
                    };
                }
                current.push((Pt::new(x, y), Segment::Line));
            }
            "Z" => {
                // An explicit closing point (last == first) is how a hand
                // author usually writes "back to the start"; the ring
                // representation stores each vertex once, so drop it. The
                // segment that reaches it is already attached to the
                // previous vertex and becomes the closing segment.
                if current.len() >= 2 && current[current.len() - 1].0 == current[0].0 {
                    current.pop();
                }
                if current.len() < 3 {
                    return Err(reject(
                        file,
                        "a closed subpath needs at least 3 distinct anchor points",
                    ));
                }
                subpaths.push(std::mem::take(&mut current));
            }
            other => {
                return Err(reject(
                    file,
                    format!("command {other:?} is outside the accepted subset (M/L/Q/C/Z)"),
                ))
            }
        }
    }
    if !current.is_empty() {
        return Err(reject(file, "path data does not end with Z"));
    }
    if subpaths.is_empty() {
        return Err(reject(file, "no closed subpath"));
    }
    Ok(subpaths)
}

fn parse_svg(file: &str, text: &str) -> Result<Vec<AuthoredPath>, AuthoredError> {
    let expected = format!("viewBox=\"0 0 {AUTHORING_CANVAS_PX} {AUTHORING_CANVAS_PX}\"");
    if !text.contains(&expected) {
        return Err(reject(
            file,
            format!("must declare {expected} so scene units are unambiguous"),
        ));
    }
    let mut out = Vec::new();
    for chunk in text.split("<path").skip(1) {
        let el = chunk
            .split('>')
            .next()
            .ok_or_else(|| reject(file, "unterminated <path"))?;
        let d = attr(el, "d").ok_or_else(|| reject(file, "<path> without d"))?;
        let fill = attr(el, "fill").ok_or_else(|| reject(file, "<path> without fill"))?;
        out.push(AuthoredPath {
            fill: parse_fill(file, fill)?,
            subpaths: parse_path_data(file, d)?,
        });
    }
    if out.is_empty() {
        return Err(reject(file, "no <path> elements"));
    }
    Ok(out)
}

/// Turn one accepted file into a source group.
pub(crate) fn load_authored(file_name: &str, text: &str) -> Result<GtSourceGroup, AuthoredError> {
    let paths = parse_svg(file_name, text)?;
    let mut b = SceneBuilder::new(
        AUTHORING_CANVAS_PX,
        AUTHORING_CANVAS_PX,
        flat2_formation(ExteriorModel::Transparent),
    );
    let mut salient = Vec::new();
    let mut params = BTreeMap::new();
    let mut min_outline = f64::INFINITY;

    for (pi, p) in paths.iter().enumerate() {
        let face = b.add_face(Paint::OpaqueSolid(p.fill));
        for (si, sub) in p.subpaths.iter().enumerate() {
            let mut pts: Vec<Pt> = sub.iter().map(|(p, _)| *p).collect();
            let mut segs: Vec<Segment> = sub.iter().map(|(_, s)| s.clone()).collect();
            let area = ring_signed_area(&pts);
            if area.abs() < 1e-9 {
                return Err(reject(file_name, "degenerate subpath"));
            }
            // The subset fixes the MEANING of the winding, so authoring
            // order does not have to be memorized: an outline encloses its
            // face, a hole excludes it. Orientation is normalized here and
            // the certification still has the last word.
            let want_positive = si == 0;
            if (area > 0.0) != want_positive {
                pts.reverse();
                segs.reverse();
                segs.rotate_right(1);
            }
            if si == 0 {
                min_outline = min_outline.min(area.abs().sqrt());
                salient.push(SalientFeature::Component {
                    area_px2: area.abs(),
                });
                b.add_ring(&pts, &segs, face, SceneBuilder::EXTERIOR)
                    .map_err(|e| reject(file_name, e.to_string()))?;
            } else {
                let hole = b.add_face(Paint::TransparentExterior);
                salient.push(SalientFeature::Hole {
                    face: hole as u32,
                    area_px2: area.abs(),
                });
                let mut hp = pts.clone();
                let mut hs = segs.clone();
                hp.reverse();
                hs.reverse();
                hs.rotate_right(1);
                b.add_ring(&hp, &hs, hole, face)
                    .map_err(|e| reject(file_name, e.to_string()))?;
            }
        }
        params.insert(format!("path{pi}_subpaths"), p.subpaths.len() as f64);
    }
    salient.push(SalientFeature::PaintPair { separation: 1.0 });
    params.insert("paths".to_string(), paths.len() as f64);

    let scene = b
        .build()
        .map_err(|e| reject(file_name, format!("not a valid scene: {e}")))?;
    let stem = FsPath::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file_name.to_string());
    let id = format!("authored/{stem}");
    let gt = GtScene::new(
        format!("{id}#a"),
        &id,
        scene,
        AuthoredTruth {
            construction: format!("hand-authored SVG {file_name}"),
            parameters: params,
        },
        salient,
    )
    .map_err(|e| reject(file_name, format!("not a certified embedding: {e}")))?;

    Ok(GtSourceGroup {
        id,
        origin: FixtureOrigin::Authored,
        shape_family: format!("authored/{stem}"),
        provenance: format!(
            "hand-authored in this repository: tests/fixtures/gt/authored/{file_name}"
        ),
        scenes: vec![gt],
        equivalence_class: None,
        intentionally_ambiguous: false,
    })
}

/// The committed authored corpus, embedded so a clean checkout needs no
/// path resolution and the files cannot silently drift from the code.
pub const AUTHORED_FILES: &[(&str, &str)] = &[
    (
        "pennant.svg",
        include_str!("../../../../tests/fixtures/gt/authored/pennant.svg"),
    ),
    (
        "keyhole.svg",
        include_str!("../../../../tests/fixtures/gt/authored/keyhole.svg"),
    ),
    (
        "leaf.svg",
        include_str!("../../../../tests/fixtures/gt/authored/leaf.svg"),
    ),
    (
        "bracket.svg",
        include_str!("../../../../tests/fixtures/gt/authored/bracket.svg"),
    ),
    (
        "lobed.svg",
        include_str!("../../../../tests/fixtures/gt/authored/lobed.svg"),
    ),
    (
        "twotone.svg",
        include_str!("../../../../tests/fixtures/gt/authored/twotone.svg"),
    ),
];

/// Load every committed authored file.
pub(crate) fn authored_groups() -> Result<Vec<GtSourceGroup>, AuthoredError> {
    AUTHORED_FILES
        .iter()
        .map(|(name, text)| load_authored(name, text))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_committed_authored_file_loads_and_certifies() {
        let groups = authored_groups().expect("the committed corpus must load");
        assert_eq!(groups.len(), AUTHORED_FILES.len());
        let ids: BTreeSet<&str> = groups.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids.len(), groups.len(), "group ids must be unique");
        for g in &groups {
            assert_eq!(g.origin, FixtureOrigin::Authored);
            assert!(
                g.provenance.contains("hand-authored in this repository"),
                "provenance must be explicit: {}",
                g.provenance
            );
            let t = g.scenes[0].partition_truth();
            assert!(t.visible_faces >= 1);
            assert!(t.total_ink_px2 > 100.0, "{}: empty scene", g.id);
        }
    }

    /// The authored corpus must genuinely widen the corpus, not repeat the
    /// grammar: it has to contribute holes, curves, multiple paints and
    /// non-convex outlines.
    #[test]
    fn the_authored_corpus_covers_what_it_was_added_for() {
        let groups = authored_groups().unwrap();
        let mut holes = 0;
        let mut curved = 0;
        let mut multi_paint = 0;
        let mut multi_component = 0;
        for g in &groups {
            let t = g.scenes[0].partition_truth();
            if t.holes > 0 {
                holes += 1;
            }
            if t.palette.len() > 1 {
                multi_paint += 1;
            }
            if t.components > 1 {
                multi_component += 1;
            }
            if g.scenes[0]
                .scene()
                .graph()
                .boundaries
                .iter()
                .any(|b| b.curve.segments.iter().any(|s| *s != Segment::Line))
            {
                curved += 1;
            }
        }
        assert!(holes >= 2, "holes: {holes}");
        assert!(curved >= 2, "curved: {curved}");
        assert!(multi_paint >= 1, "multi-paint: {multi_paint}");
        assert!(multi_component >= 1, "multi-component: {multi_component}");
    }

    /// Out-of-subset input is a TYPED REFUSAL, never a best-effort guess:
    /// a loader that interprets what it does not support turns the ground
    /// truth into a guess.
    #[test]
    fn out_of_subset_input_is_refused_rather_than_guessed() {
        let ok = r##"<svg viewBox="0 0 256 256"><path fill="#112233" d="M 20 20 L 200 30 L 180 200 Z"/></svg>"##;
        assert!(load_authored("ok.svg", ok).is_ok());

        let cases: &[(&str, &str)] = &[
            (
                "relative commands",
                r##"<svg viewBox="0 0 256 256"><path fill="#112233" d="m 20 20 l 180 10 l -20 170 z"/></svg>"##,
            ),
            (
                "arc command",
                r##"<svg viewBox="0 0 256 256"><path fill="#112233" d="M 20 20 A 30 30 0 0 1 200 30 L 180 200 Z"/></svg>"##,
            ),
            (
                "no viewBox",
                r##"<svg width="256"><path fill="#112233" d="M 20 20 L 200 30 L 180 200 Z"/></svg>"##,
            ),
            (
                "wrong viewBox size",
                r##"<svg viewBox="0 0 100 100"><path fill="#112233" d="M 20 20 L 90 30 L 80 90 Z"/></svg>"##,
            ),
            (
                "no fill",
                r##"<svg viewBox="0 0 256 256"><path d="M 20 20 L 200 30 L 180 200 Z"/></svg>"##,
            ),
            (
                "named colour",
                r##"<svg viewBox="0 0 256 256"><path fill="red" d="M 20 20 L 200 30 L 180 200 Z"/></svg>"##,
            ),
            (
                "unclosed subpath",
                r##"<svg viewBox="0 0 256 256"><path fill="#112233" d="M 20 20 L 200 30 L 180 200"/></svg>"##,
            ),
            (
                "two-point subpath",
                r##"<svg viewBox="0 0 256 256"><path fill="#112233" d="M 20 20 L 200 30 Z"/></svg>"##,
            ),
            ("no paths", r##"<svg viewBox="0 0 256 256"></svg>"##),
            (
                "non-numeric coordinate",
                r##"<svg viewBox="0 0 256 256"><path fill="#112233" d="M 20 x L 200 30 L 180 200 Z"/></svg>"##,
            ),
        ];
        for (what, svg) in cases {
            assert!(
                load_authored("bad.svg", svg).is_err(),
                "{what} must be refused, not interpreted"
            );
        }
    }

    /// Overlap is not taken on trust. An authored file whose islands
    /// intersect is refused by the CERTIFICATION, not by a hand-written
    /// geometric check that could be wrong in the same way the loader is.
    #[test]
    fn overlapping_islands_are_caught_by_certification() {
        let overlapping = r##"<svg viewBox="0 0 256 256">
          <path fill="#112233" d="M 40 40 L 160 40 L 160 160 L 40 160 Z"/>
          <path fill="#445566" d="M 100 100 L 220 100 L 220 220 L 100 220 Z"/>
        </svg>"##;
        let err = load_authored("overlap.svg", overlapping)
            .expect_err("overlapping islands are not a planar partition");
        let msg = err.to_string();
        assert!(
            msg.contains("certified") || msg.contains("valid scene"),
            "the refusal must come from the certification path: {msg}"
        );
    }

    /// Authoring order must not be something to memorize: a hole written
    /// in either winding produces the same scene, because the subset fixes
    /// the MEANING (first subpath outlines, later ones subtract) and the
    /// loader normalizes to it.
    #[test]
    fn hole_winding_is_normalized_by_meaning_not_by_authoring_order() {
        let cw = r##"<svg viewBox="0 0 256 256"><path fill="#112233" d="M 40 40 L 200 40 L 200 200 L 40 200 Z M 90 90 L 150 90 L 150 150 L 90 150 Z"/></svg>"##;
        let ccw = r##"<svg viewBox="0 0 256 256"><path fill="#112233" d="M 40 40 L 200 40 L 200 200 L 40 200 Z M 90 90 L 90 150 L 150 150 L 150 90 Z"/></svg>"##;
        let a = load_authored("a.svg", cw).unwrap();
        let b = load_authored("b.svg", ccw).unwrap();
        assert_eq!(
            a.scenes[0].partition_truth().face_area_px2,
            b.scenes[0].partition_truth().face_area_px2
        );
        assert_eq!(a.scenes[0].partition_truth().holes, 1);
    }
}
