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
fn verify_corpus_takes_limits_from_config() {
    let t = tempfile::tempdir().unwrap();
    let corpus = t.path().join("corpus");
    gen_corpus(&corpus);

    // Without --config: built-in default limits, corpus verifies.
    let ok = Command::new(runner_bin())
        .arg("verify-corpus")
        .arg("--corpus")
        .arg(&corpus)
        .arg("--manifest")
        .arg(corpus.join("SMOKE_MANIFEST.toml"))
        .status()
        .unwrap();
    assert!(ok.success());

    // With --config whose limits are absurdly tight, the same corpus must
    // FAIL verification: the limits demonstrably come from the config.
    let config = t.path().join("tight.toml");
    std::fs::write(
        &config,
        r#"
schema = "vice-classic/baselines/v1"

[limits]
max_input_bytes = 10
max_png_dimension = 4096
run_timeout_secs = 300
build_timeout_secs = 3600
max_output_bytes = 33554432
"#,
    )
    .unwrap();
    let output = Command::new(runner_bin())
        .arg("verify-corpus")
        .arg("--corpus")
        .arg(&corpus)
        .arg("--manifest")
        .arg(corpus.join("SMOKE_MANIFEST.toml"))
        .arg("--config")
        .arg(&config)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("too large"), "stdout: {stdout}");

    // An unparsable/unknown-schema config is a hard error (exit 2), not a
    // silent fallback to defaults.
    let bad = t.path().join("bad.toml");
    std::fs::write(&bad, "schema = \"wrong/schema\"\n").unwrap();
    let output = Command::new(runner_bin())
        .arg("verify-corpus")
        .arg("--corpus")
        .arg(&corpus)
        .arg("--manifest")
        .arg(corpus.join("SMOKE_MANIFEST.toml"))
        .arg("--config")
        .arg(&bad)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

/// End-to-end proof that the declared asset pin (M0 blocker B2) does what
/// the config says: a real mirror, a real pinned checkout, and an asset that
/// is NOT in the commit.
///
/// Unit tests cover `assets::stage` in isolation; this covers the wiring —
/// that staging happens after the pin is verified, into the checkout the
/// baseline actually runs against, and that a wrong asset stops the baseline
/// instead of changing its result.
#[test]
fn declared_assets_are_staged_into_the_pinned_checkout_or_typed_refused() {
    let t = tempfile::tempdir().unwrap();
    let corpus = t.path().join("corpus");
    gen_corpus(&corpus);

    // A minimal mirror: one commit that deliberately does NOT contain the
    // asset, exactly like the Vice- pin and its gitignored model.
    let mirror = t.path().join("mirrors").join("donor");
    std::fs::create_dir_all(&mirror).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args([
                "-c",
                "user.email=t@example.invalid",
                "-c",
                "user.name=t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(&mirror)
            .output()
            .expect("git available");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    git(&["init", "-q"]);
    std::fs::write(mirror.join("tool.py"), "import sys, pathlib, shutil\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "pin"]);
    let pin = git(&["rev-parse", "HEAD"]);

    // The asset root, laid out as <asset-root>/<mirror_hint>/<source>.
    let asset_root = t.path().join("assets");
    std::fs::create_dir_all(asset_root.join("donor").join("models")).unwrap();
    let body = b"pinned-model-bytes";
    std::fs::write(asset_root.join("donor/models/m.bin"), body).unwrap();
    let digest = {
        use sha2::Digest;
        hex::encode(sha2::Sha256::digest(body))
    };

    let write_config = |sha: &str| {
        let cfg = format!(
            r#"
schema = "vice-classic/baselines/v1"

[[baseline]]
name = "donor"
repo = "nobody/donor"
pin_sha = "{pin}"
mirror_hint = "donor"
kind = "python"
run = ["python", "-c", "import pathlib,sys; p=pathlib.Path(sys.argv[1]); p.parent.mkdir(parents=True, exist_ok=True); p.write_bytes((pathlib.Path(sys.argv[2])/'models/m.bin').read_bytes())", "{{out_dir}}/{{stem}}.out", "{{checkout}}"]
outputs = ["{{stem}}.out"]

[[baseline.asset]]
path = "models/m.bin"
sha256 = "{sha}"
bytes = {len}
"#,
            len = body.len()
        );
        let p = t.path().join(format!("cfg-{}.toml", &sha[..8]));
        std::fs::write(&p, cfg).unwrap();
        p
    };

    let run = |config: &Path, out: &Path, with_root: bool| -> serde_json::Value {
        let mut cmd = Command::new(runner_bin());
        cmd.arg("run")
            .arg("--config")
            .arg(config)
            .arg("--corpus")
            .arg(&corpus)
            .arg("--manifest")
            .arg(corpus.join("SMOKE_MANIFEST.toml"))
            .arg("--mirror-root")
            .arg(t.path().join("mirrors"))
            .arg("--out")
            .arg(out)
            .arg("--repeats")
            .arg("1");
        if with_root {
            cmd.arg("--asset-root").arg(&asset_root);
        }
        let output = cmd.output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_str(&std::fs::read_to_string(out.join("report.json")).unwrap()).unwrap()
    };

    // (1) Correct asset: staged, recorded, and visibly used by the run —
    // the declared output is a copy of the asset the checkout did not have.
    let good = write_config(&digest);
    let out_ok = t.path().join("out-ok");
    let report = run(&good, &out_ok, true);
    let b = &report["baselines"][0];
    assert_eq!(b["status"], "completed", "{b:#}");
    assert_eq!(b["assets"][0]["path"], "models/m.bin");
    assert_eq!(b["assets"][0]["sha256"], digest);
    assert!(b["runs"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["status"] == "ok"));
    let produced = out_ok.join("work/donor/rep0/rect_32/out/rect_32.out");
    assert_eq!(std::fs::read(&produced).unwrap(), body);
    // Normative provenance: the staged hash is in hashes.json too.
    let hashes: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out_ok.join("hashes.json")).unwrap())
            .unwrap();
    assert_eq!(
        hashes["baselines"]["donor"]["assets"]["models/m.bin"],
        digest
    );

    // (2) Same asset root, WRONG declared hash: the baseline fails typed and
    // never runs, instead of quietly producing results from another file.
    let bad = write_config(&"0".repeat(64));
    let out_bad = t.path().join("out-bad");
    let report = run(&bad, &out_bad, true);
    let b = &report["baselines"][0];
    assert_eq!(b["status"], "failed");
    assert_eq!(b["error"]["kind"], "asset_mismatch");
    assert!(b["runs"].as_array().unwrap().is_empty());

    // (3) Correct config, no --asset-root: typed refusal, NOT a silent run
    // without the asset (which is what M0 recorded as output_missing).
    let out_noroot = t.path().join("out-noroot");
    let report = run(&good, &out_noroot, false);
    assert_eq!(
        report["baselines"][0]["error"]["kind"],
        "asset_root_missing"
    );
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
