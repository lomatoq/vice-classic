//! Integration tests exercising the built binaries end-to-end.
//! These run in CI from a clean checkout: no mirrors, no network.

use std::path::{Path, PathBuf};
use std::process::Command;

fn runner_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_baseline-runner"))
}

fn gensmoke_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_gen-smoke"))
}

fn gen_corpus(dir: &Path) {
    let status = Command::new(gensmoke_bin())
        .arg("--out")
        .arg(dir)
        .arg("--write-manifest")
        .status()
        .expect("spawn gen-smoke");
    assert!(status.success(), "gen-smoke failed");
}

#[test]
fn copy_adapter_copies_bytes() {
    let t = tempfile::tempdir().unwrap();
    let src = t.path().join("a.bin");
    std::fs::write(&src, b"payload").unwrap();
    let dst = t.path().join("sub").join("b.bin");
    let status = Command::new(runner_bin())
        .arg("copy-adapter")
        .arg(&src)
        .arg(&dst)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(std::fs::read(&dst).unwrap(), b"payload");
}

#[test]
fn generated_corpus_verifies() {
    let t = tempfile::tempdir().unwrap();
    let corpus = t.path().join("corpus");
    gen_corpus(&corpus);
    let status = Command::new(runner_bin())
        .arg("verify-corpus")
        .arg("--corpus")
        .arg(&corpus)
        .arg("--manifest")
        .arg(corpus.join("SMOKE_MANIFEST.toml"))
        .status()
        .unwrap();
    assert!(status.success(), "verify-corpus reported drift");
}

#[test]
fn selftest_pipeline_is_deterministic() {
    let t = tempfile::tempdir().unwrap();
    let corpus = t.path().join("corpus");
    gen_corpus(&corpus);
    let out = t.path().join("out");
    let status = Command::new(runner_bin())
        .arg("selftest")
        .arg("--out")
        .arg(&out)
        .arg("--corpus")
        .arg(&corpus)
        .arg("--manifest")
        .arg(corpus.join("SMOKE_MANIFEST.toml"))
        .status()
        .unwrap();
    assert!(status.success(), "selftest failed");

    let hashes: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("hashes.json")).unwrap()).unwrap();
    assert_eq!(
        hashes["baselines"]["selftest-copy"]["primary_deterministic"],
        serde_json::Value::Bool(true)
    );
    assert_eq!(
        hashes["baselines"]["selftest-copy"]["status"],
        serde_json::Value::String("completed".into())
    );
    // report.json exists and carries the environment hash.
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("report.json")).unwrap()).unwrap();
    assert!(report["environment_sha256"].as_str().unwrap().len() == 64);
}

const TEST_CONFIG: &str = r#"
schema = "vice-classic/baselines/v1"

[[baseline]]
name = "bogus-a"
repo = "nobody/bogus-a"
pin_sha = "0123456789abcdef0123456789abcdef01234567"
mirror_hint = "no-such-dir-a"
kind = "rust"
run = ["{binary}", "{input}"]

[[baseline]]
name = "bogus-b"
repo = "nobody/bogus-b"
pin_sha = "0123456789abcdef0123456789abcdef01234567"
mirror_hint = "no-such-dir-b"
kind = "python"
run = ["python", "{input}"]
"#;

#[test]
fn missing_mirror_is_isolated_typed_failure() {
    let t = tempfile::tempdir().unwrap();
    let corpus = t.path().join("corpus");
    gen_corpus(&corpus);
    let config = t.path().join("baselines.toml");
    std::fs::write(&config, TEST_CONFIG).unwrap();
    let out_dir = t.path().join("out");

    let output = Command::new(runner_bin())
        .arg("run")
        .arg("--config")
        .arg(&config)
        .arg("--corpus")
        .arg(&corpus)
        .arg("--manifest")
        .arg(corpus.join("SMOKE_MANIFEST.toml"))
        .arg("--mirror-root")
        .arg(t.path().join("mirrors"))
        .arg("--out")
        .arg(&out_dir)
        .output()
        .unwrap();
    // The runner itself succeeds: baseline failures are typed data in the
    // report, not process failures.
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("report.json")).unwrap())
            .unwrap();
    let baselines = report["baselines"].as_array().unwrap();
    assert_eq!(baselines.len(), 2, "both baselines must be reported");
    for b in baselines {
        assert_eq!(b["status"], "failed");
        assert_eq!(b["error"]["kind"], "mirror_missing");
    }
    assert!(out_dir.join("hashes.json").is_file());
}

#[test]
fn corrupted_corpus_aborts_run() {
    let t = tempfile::tempdir().unwrap();
    let corpus = t.path().join("corpus");
    gen_corpus(&corpus);
    // Corrupt one corpus file after manifest generation.
    let victim = corpus.join("rect_32.png");
    let mut bytes = std::fs::read(&victim).unwrap();
    bytes.push(0xFF);
    std::fs::write(&victim, bytes).unwrap();

    let config = t.path().join("baselines.toml");
    std::fs::write(&config, TEST_CONFIG).unwrap();

    let output = Command::new(runner_bin())
        .arg("run")
        .arg("--config")
        .arg(&config)
        .arg("--corpus")
        .arg(&corpus)
        .arg("--manifest")
        .arg(corpus.join("SMOKE_MANIFEST.toml"))
        .arg("--mirror-root")
        .arg(t.path().join("mirrors"))
        .arg("--out")
        .arg(t.path().join("out"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("corpus integrity"), "stderr: {stderr}");
}
