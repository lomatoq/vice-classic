//! The evidence and M7 vectorization executable paths, exercised as a binary.
//!
//! §4.1: a milestone ends in a working CLI path, not in types. What these
//! tests check is that the path RUNS on a committed fixture and that the two
//! things a reader of the output must be able to trust are true of it: the
//! §1.4 outcomes are distinguishable by exit code, and an oracle override
//! marks the run non-production in the artifact rather than in the operator's
//! memory.

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn vicec() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vicec"))
}

fn file_sha256(path: &Path) -> String {
    hex::encode(Sha256::digest(
        std::fs::read(path).expect("read executable"),
    ))
}

fn write_png(path: &Path, width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&pixel(x, y));
        }
    }
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&rgba)
        .unwrap();
}

#[test]
fn the_evidence_path_runs_on_a_committed_fixture_and_writes_its_report() {
    let dir = tempfile::tempdir().unwrap();
    let out = vicec()
        .arg("evidence")
        .arg(repo_root().join("tests/fixtures/smoke/circle_64.png"))
        .arg("--out")
        .arg(dir.path())
        .output()
        .expect("vicec runs");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    println!("{text}");
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("outcome: SUPPORTED"));
    let json = std::fs::read_to_string(dir.path().join("evidence.json")).expect("report written");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["schema"], "vice-classic/m4-flat2-evidence/v1");
    assert_eq!(v["production"], true);
    assert_eq!(v["outcome"]["outcome"], "supported");
    // The report carries the decode facts §8.1 asks for.
    assert_eq!(v["image"]["width_px"], 64);
    assert!(v["image"]["source_sha256"].as_str().unwrap().len() == 64);
    assert_eq!(v["image"]["icc_assumption"], "no_profile_assumed_srgb");
    // And the corridor of the chosen hypothesis.
    assert!(v["boundary"]["median_halfwidth_px"].as_f64().unwrap() > 0.0);
}

/// §30: an oracle override marks the run NON-PRODUCTION, in the artifact and
/// on stderr. Both, because a warning only a file carries is a warning
/// nobody reads, and a warning only the terminal carries does not survive
/// being copied.
#[test]
fn an_oracle_override_marks_the_run_non_production() {
    let dir = tempfile::tempdir().unwrap();
    let out = vicec()
        .arg("evidence")
        .arg(repo_root().join("tests/fixtures/smoke/circle_64.png"))
        .arg("--out")
        .arg(dir.path())
        .arg("--fg")
        .arg("0,0,0")
        .arg("--bg")
        .arg("255,255,255")
        .output()
        .expect("vicec runs");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(stdout.contains("NON-PRODUCTION"), "{stdout}");
    assert!(stderr.contains("NOT a production result"), "{stderr}");
    let json = std::fs::read_to_string(dir.path().join("evidence.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["production"], false);
    assert_eq!(v["hypotheses"][0], "H0/oracle-override");
}

/// A malformed override is a typed refusal, not a guess at the other half of
/// the hypothesis (§9.2).
#[test]
fn half_an_override_is_refused_rather_than_completed() {
    for args in [
        vec!["--bg", "0,0,0"],
        vec!["--exterior", "opaque"],
        vec!["--fg", "0,0"],
        vec!["--fg", "0,0,999"],
    ] {
        let mut c = vicec();
        c.arg("evidence")
            .arg(repo_root().join("tests/fixtures/smoke/circle_64.png"));
        for a in &args {
            c.arg(a);
        }
        let out = c.output().expect("vicec runs");
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
}

/// An input that is not a PNG is a FAILURE (exit 2) and not a verdict about
/// the model: §1.4 keeps `Failed` apart from `Unsupported`.
#[test]
fn a_broken_input_is_a_failure_not_an_unsupported_verdict() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("not-a.png");
    std::fs::write(&path, b"this is not a png").unwrap();
    let out = vicec()
        .arg("evidence")
        .arg(&path)
        .output()
        .expect("vicec runs");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("decode"));
}

#[test]
fn vectorize_failure_writes_only_the_typed_report_and_no_svg() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("not-a.png");
    let output = dir.path().join("result");
    std::fs::write(&input, b"this is not a png").unwrap();
    let run = vicec()
        .arg("vectorize")
        .arg(&input)
        .args(["--mode", "flat2", "--intent", "clean", "--preset", "fast"])
        .arg("--out")
        .arg(&output)
        .output()
        .expect("vicec runs");
    assert_eq!(run.status.code(), Some(2));
    let files: Vec<_> = std::fs::read_dir(&output)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(files, [std::ffi::OsString::from("result.report.json")]);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("result.report.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "failed");
    assert_eq!(report["reason"]["reason"], "decode");
}

#[test]
fn vectorize_never_falls_back_when_the_production_config_is_untrusted() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("result");
    let missing = dir.path().join("missing-production-config.json");
    let run = vicec()
        .arg("vectorize")
        .arg(repo_root().join("tests/fixtures/smoke/circle_64.png"))
        .args(["--mode", "flat2", "--preset", "fast"])
        .arg("--production-config")
        .arg(&missing)
        .arg("--out")
        .arg(&output)
        .output()
        .expect("vicec runs");
    assert_eq!(run.status.code(), Some(2));
    let files: Vec<_> = std::fs::read_dir(&output)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(files, [std::ffi::OsString::from("result.report.json")]);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("result.report.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "failed");
    assert_eq!(report["production"], false);
    assert_eq!(report["reason"]["reason"], "internal");
    assert!(report["reason"]["detail"]
        .as_str()
        .unwrap()
        .contains("production configuration refused"));
}

#[test]
fn vectorize_refuses_to_mix_a_new_verdict_with_stale_output() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("result");
    std::fs::create_dir(&output).unwrap();
    std::fs::write(output.join("result.svg"), b"stale").unwrap();
    let run = vicec()
        .arg("vectorize")
        .arg(repo_root().join("tests/fixtures/smoke/circle_64.png"))
        .args(["--mode", "flat2", "--preset", "fast"])
        .arg("--out")
        .arg(&output)
        .output()
        .expect("vicec runs");
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("not empty"));
    assert_eq!(std::fs::read(output.join("result.svg")).unwrap(), b"stale");
}

#[test]
fn production_vectorize_delivers_a_supported_128px_input() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("result");
    let run = vicec()
        .arg("vectorize")
        .arg(repo_root().join("tests/fixtures/smoke/triangle_128.png"))
        .args(["--mode", "flat2", "--intent", "clean", "--preset", "fast"])
        .arg("--out")
        .arg(&output)
        .output()
        .expect("vicec runs");
    assert_eq!(
        run.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(output.join("result.svg").is_file());
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("result.report.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "success");
    assert_eq!(report["production"], true);
}

#[test]
fn auto_mode_chooses_flat2_for_a_supported_flat2_input() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("result");
    let run = vicec()
        .arg("vectorize")
        .arg(repo_root().join("tests/fixtures/smoke/triangle_128.png"))
        .args(["--mode", "auto", "--intent", "clean", "--preset", "fast"])
        .arg("--out")
        .arg(&output)
        .output()
        .expect("vicec runs");
    assert_eq!(run.status.code(), Some(0));
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("result.report.json")).unwrap()).unwrap();
    assert_eq!(report["selected_lane"], "flat2");
    assert_eq!(report["status"], "success");
    assert_eq!(report["production"], true);
    assert!(output.join("result.svg").is_file());
}

#[test]
fn auto_routes_multicolour_art_and_emits_labeled_experimental_svg() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("multicolour.png");
    let output = dir.path().join("result");
    write_png(&input, 24, 12, |x, _| match x / 8 {
        0 => [230, 20, 20, 255],
        1 => [20, 220, 30, 255],
        _ => [20, 40, 230, 255],
    });
    let run = vicec()
        .arg("vectorize")
        .arg(&input)
        .args(["--mode", "auto", "--preset", "fast", "--experimental"])
        .arg("--out")
        .arg(&output)
        .output()
        .expect("vicec runs");
    assert_eq!(run.status.code(), Some(3));
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("result.report.json")).unwrap()).unwrap();
    assert_eq!(report["selected_lane"], "multiregion");
    assert_eq!(report["production"], false);
    assert_eq!(report["experimental_artifacts"], true);
    assert!(output.join("result.experimental.svg").is_file());
    assert!(!output.join("result.svg").exists());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("result.manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["experimental"], true);
    assert_eq!(manifest["production"], false);
    assert_eq!(
        manifest["artifact_trust"],
        "non_production_manual_inspection_only"
    );
}

#[test]
fn explicit_gradient_mode_exposes_scene_and_render_without_production_claim() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("gradient.png");
    let output = dir.path().join("result");
    write_png(&input, 32, 16, |x, _| {
        let red = ((x as f64 / 31.0) * 255.0).round() as u8;
        [red, 20, 255 - red, 255]
    });
    let run = vicec()
        .arg("vectorize")
        .arg(&input)
        .args(["--mode", "gradient", "--preset", "fast", "--experimental"])
        .arg("--out")
        .arg(&output)
        .output()
        .expect("vicec runs");
    assert_eq!(run.status.code(), Some(3));
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("result.report.json")).unwrap()).unwrap();
    assert_eq!(report["selected_lane"], "gradient");
    assert_eq!(report["production"], false);
    assert!(output.join("result.experimental.scene.json").is_file());
    assert!(output.join("result.experimental.render.png").is_file());
}

#[test]
fn explicit_line_art_mode_is_public_and_never_silently_becomes_flat2() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("line-art.png");
    let output = dir.path().join("result");
    write_png(&input, 48, 24, |x, y| {
        if (5..43).contains(&x) && (10..14).contains(&y) {
            [0, 0, 0, 255]
        } else {
            [255, 255, 255, 255]
        }
    });
    let run = vicec()
        .arg("vectorize")
        .arg(&input)
        .args(["--mode", "line-art", "--preset", "fast", "--experimental"])
        .arg("--out")
        .arg(&output)
        .output()
        .expect("vicec runs");
    assert!(matches!(run.status.code(), Some(3 | 4)));
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("result.report.json")).unwrap()).unwrap();
    assert_eq!(report["selected_lane"], "line_art");
    assert_eq!(report["production"], false);
    assert_ne!(report["route_reason"], "supported Flat2 evidence");

    let auto_output = dir.path().join("auto-result");
    let auto = vicec()
        .arg("vectorize")
        .arg(&input)
        .args(["--mode", "auto", "--preset", "fast", "--experimental"])
        .arg("--out")
        .arg(&auto_output)
        .output()
        .expect("vicec auto runs");
    assert!(matches!(auto.status.code(), Some(3 | 4)));
    let auto_report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(auto_output.join("result.report.json")).unwrap())
            .unwrap();
    assert_eq!(auto_report["selected_lane"], "line_art");
    assert!(auto_report["route_reason"]
        .as_str()
        .unwrap()
        .contains("stroke-first"));
}

#[test]
fn legacy_wrapper_refuses_an_unpinned_engine_before_execution() {
    let dir = tempfile::tempdir().unwrap();
    let engine = std::env::current_exe().unwrap();
    let output = dir.path().join("legacy");
    let run = vicec()
        .arg("legacy-vectorize")
        .arg(repo_root().join("tests/fixtures/smoke/circle_64.png"))
        .arg("--engine")
        .arg(engine)
        .arg("--engine-sha256")
        .arg("0".repeat(64))
        .args(["--arg", "{input}", "--arg", "{output}"])
        .arg("--out")
        .arg(&output)
        .output()
        .expect("vicec runs");
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("digest does not match"));
    assert!(
        !output.exists(),
        "refusal must happen before output creation"
    );
}

#[test]
fn explicit_legacy_wrapper_is_provenanced_and_never_a_classic_success() {
    let dir = tempfile::tempdir().unwrap();
    let input = repo_root().join("tests/fixtures/smoke/circle_64.png");
    let output = dir.path().join("legacy");

    #[cfg(unix)]
    let (engine, argv): (PathBuf, Vec<&str>) =
        (PathBuf::from("/bin/cp"), vec!["{input}", "{output}"]);
    #[cfg(windows)]
    let (engine, argv): (PathBuf, Vec<&str>) = (
        PathBuf::from(std::env::var_os("COMSPEC").expect("COMSPEC")),
        vec!["/C", "copy", "/Y", "{input}", "{output}"],
    );

    let mut command = vicec();
    command
        .arg("legacy-vectorize")
        .arg(&input)
        .arg("--engine")
        .arg(&engine)
        .arg("--engine-sha256")
        .arg(file_sha256(&engine));
    for arg in argv {
        command.args(["--arg", arg]);
    }
    let run = command
        .arg("--out")
        .arg(&output)
        .output()
        .expect("vicec runs");
    assert_eq!(
        run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        std::fs::read(output.join("legacy-result.svg")).unwrap(),
        std::fs::read(&input).unwrap()
    );
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(output.join("legacy.report.json")).expect("legacy report"),
    )
    .unwrap();
    assert_eq!(report["schema"], "vice-classic/legacy-wrapper-report/v1");
    assert_eq!(report["status"], "legacy_success");
    assert_eq!(report["classic_success"], false);
    assert_eq!(report["engine_sha256_before"], file_sha256(&engine));
    assert_eq!(report["engine_sha256_after"], file_sha256(&engine));
}
