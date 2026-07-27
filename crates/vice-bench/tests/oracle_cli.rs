//! Integration tests for the `gt-corpus oracle` commands (spec §28 M3.5).
//!
//! The same reason `gt_corpus_cli.rs` exists: a gate clause that is only
//! unit-tested is a claim about a function, not about the command a reviewer
//! and CI actually run. §28 M3.5 gates on two sentences, and both are
//! printed by this binary, so both are driven here through the binary.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_gt-corpus"))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn run(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(bin())
        .args(args)
        .current_dir(repo_root())
        .output()
        .expect("spawn gt-corpus");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn write_report(scope: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("oracle.json");
    let (code, stdout, _) = run(&["oracle", "--out", out.to_str().unwrap(), "--scope", scope]);
    assert_eq!(code, Some(0), "{stdout}");
    (dir, out)
}

/// The gate table is PRINTED by the command, both rows met, and the
/// inverse-crime warning is on stderr as well as in the file: a warning only
/// an artifact carries is a warning an operator never reads.
#[test]
fn the_command_prints_both_gate_rows_and_warns_on_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("oracle.json");
    let (code, stdout, stderr) =
        run(&["oracle", "--out", out.to_str().unwrap(), "--scope", "test"]);
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    assert!(
        stdout.contains("[MET] no causal deltas across incompatible runs"),
        "{stdout}"
    );
    assert!(
        stdout.contains("[MET] inverse-crime warning visible"),
        "{stdout}"
    );
    assert!(
        stdout.contains("[INVERSE CRIME]"),
        "the contaminated pairings must be visible in the table itself: {stdout}"
    );
    assert!(
        stderr.contains("INVERSE CRIME"),
        "the warning must reach stderr, not only the artifact: {stderr}"
    );
    assert!(out.exists());
}

/// The report reproduces from a second run of the same command.
#[test]
fn the_report_reproduces() {
    let (dir, path) = write_report("test");
    let (code, stdout, stderr) = run(&["oracle-check", "--report", path.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    assert!(stdout.contains("with every metric compared"), "{stdout}");
    drop(dir);
}

/// And the check is not vacuous: a single perturbed metric is caught, and a
/// report from another platform is a TYPED refusal rather than a silent
/// pass — the F-0020 rule applied to the new artifact.
#[test]
fn a_perturbed_metric_and_a_foreign_platform_are_both_caught() {
    let (dir, path) = write_report("test");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();

    let tampered = dir.path().join("tampered.json");
    v["ceiling_arms"][0]["metrics"]["max_abs_code"] = serde_json::json!(999.0);
    std::fs::write(&tampered, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    let (code, _, stderr) = run(&["oracle-check", "--report", tampered.to_str().unwrap()]);
    assert_eq!(code, Some(1), "{stderr}");
    assert!(stderr.contains("did NOT reproduce"), "{stderr}");

    // Foreign platform: refused with exit 2, naming both platforms.
    let mut w: serde_json::Value = serde_json::from_str(&text).unwrap();
    w["platform"]["os"] = serde_json::json!("elsewhere");
    let foreign = dir.path().join("foreign.json");
    std::fs::write(&foreign, serde_json::to_string_pretty(&w).unwrap()).unwrap();
    let (code, _, stderr) = run(&["oracle-check", "--report", foreign.to_str().unwrap()]);
    assert_eq!(code, Some(2), "{stderr}");
    assert!(
        stderr.contains("elsewhere") && stderr.contains("TIER A"),
        "the refusal must name both platforms and the tier: {stderr}"
    );

    // With --structural it compares the platform-independent projection and
    // says so - and the flag is INERT on the recording platform, so it
    // cannot be used to hide a real failure.
    let (code, stdout, stderr) = run(&[
        "oracle-check",
        "--report",
        foreign.to_str().unwrap(),
        "--structural",
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    assert!(stdout.contains("metrics NOT compared"), "{stdout}");

    let (code, stdout, stderr) = run(&[
        "oracle-check",
        "--report",
        tampered.to_str().unwrap(),
        "--structural",
    ]);
    assert_eq!(
        code,
        Some(1),
        "--structural must not excuse a same-platform failure: {stdout}{stderr}"
    );
}

/// The committed artifact is the FULL scope, and it verifies on the
/// platform that recorded it. On any other platform this is a typed
/// refusal, which is the honest outcome rather than a skipped assertion.
#[test]
fn the_committed_report_verifies_or_refuses_by_platform() {
    let path = repo_root().join("docs/gt/ORACLE_M3_5.json");
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        v["config"]["scope"], "full",
        "the committed report is full scope"
    );
    let same = v["platform"]["os"] == std::env::consts::OS
        && v["platform"]["arch"] == std::env::consts::ARCH;
    let (code, stdout, stderr) = run(&["oracle-check", "--report", path.to_str().unwrap()]);
    if same {
        assert_eq!(code, Some(0), "{stdout}{stderr}");
    } else {
        assert_eq!(
            code,
            Some(2),
            "a foreign platform must be refused: {stderr}"
        );
        let (code, stdout, stderr) = run(&[
            "oracle-check",
            "--report",
            path.to_str().unwrap(),
            "--structural",
        ]);
        assert_eq!(code, Some(0), "{stdout}{stderr}");
    }
}

/// F-0022, made reproducible on ONE platform.
///
/// The defect was that `--structural` compared values which are functions of
/// libm, so the projection was platform-independent in its doc comment only.
/// CI found it; nothing local did, because everything local ran on the
/// recording platform - which is precisely the blind spot F-0020 also lived
/// in.
///
/// So the other platform is SIMULATED here: a report whose platform differs
/// and whose scene-digest-derived hashes all differ, exactly as a different
/// libm would produce, must still reproduce structurally. And the control in
/// the other direction: a difference in COMPOSITION must still be caught, or
/// the projection would have been fixed by making it blind.
#[test]
fn a_report_from_a_simulated_other_platform_reproduces_structurally() {
    let (dir, path) = write_report("test");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&text).unwrap();

    v["platform"]["os"] = serde_json::json!("elsewhere");
    // Every hash that is a sha256 over a scene digest: a different libm
    // moves all of them at once, so the simulation moves all of them.
    v["fixture_set_hash"] = serde_json::json!("f".repeat(64));
    for arm in v["ceiling_arms"].as_array_mut().unwrap() {
        arm["key_fingerprint"] = serde_json::json!("e".repeat(64));
    }
    for f in v["factorial"].as_array_mut().unwrap() {
        f["key_fingerprint"] = serde_json::json!("d".repeat(64));
    }
    let foreign = dir.path().join("other-platform.json");
    std::fs::write(&foreign, serde_json::to_string_pretty(&v).unwrap()).unwrap();

    let (code, stdout, stderr) = run(&[
        "oracle-check",
        "--report",
        foreign.to_str().unwrap(),
        "--structural",
    ]);
    assert_eq!(
        code,
        Some(0),
        "a simulated foreign platform must reproduce structurally.\n{stdout}\n{stderr}"
    );

    // Control: composition still has to be compared. A renamed scene is not
    // float noise, and the structural mode must say so.
    v["ceiling_arms"][0]["scene_id"] = serde_json::json!("proc/not-a-scene/999#z");
    let broken = dir.path().join("other-platform-broken.json");
    std::fs::write(&broken, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    let (code, _, stderr) = run(&[
        "oracle-check",
        "--report",
        broken.to_str().unwrap(),
        "--structural",
    ]);
    assert_eq!(
        code,
        Some(1),
        "the structural mode must still catch composition: {stderr}"
    );
}
