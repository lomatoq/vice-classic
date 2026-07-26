//! Integration tests for the `gt-corpus` binary.
//!
//! These exist because their absence was a real defect, not an omission of
//! polish (REVIEW_M3 M3-N1). `AuditSeal::check` was unit-tested with the
//! literals `"c"/"p"/"g"` — that verifies the comparison LOGIC and says
//! nothing about where the operands come from. The operands came from two
//! different hash functions, so the first faithful `open` reported BURNED on
//! an untouched corpus, and neither the unit tests nor fifteen CI runs
//! noticed: while the seal is `sealed`, `check` returns `StillSealed` BEFORE
//! any comparison, so the comparison branch had never executed.
//!
//! That is meta-rule M-2 exactly — green because the state belonged to the
//! subclass where the check does not run — inside the milestone that wrote
//! M-2 down. So the tests below drive the BINARY, through the same commands
//! CI runs, with the seal in the state where the comparison actually
//! happens.

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

fn run(args: &[&std::ffi::OsStr]) -> (Option<i32>, String, String) {
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

fn osv(s: &str) -> std::ffi::OsString {
    std::ffi::OsString::from(s)
}

/// Build a small manifest and the seal/gates paths that go with it.
struct Fixture {
    _dir: tempfile::TempDir,
    manifest: PathBuf,
    gates: PathBuf,
    seal: PathBuf,
    corpus_hash: String,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("manifest.json");
        let (code, stdout, stderr) = run(&[
            osv("build").as_os_str(),
            osv("--out").as_os_str(),
            manifest.as_os_str(),
            osv("--scope").as_os_str(),
            osv("test").as_os_str(),
        ]);
        assert_eq!(code, Some(0), "build failed: {stderr}");
        let corpus_hash = stdout
            .lines()
            .find_map(|l| l.strip_prefix("corpus_hash: "))
            .expect("build prints the corpus hash")
            .trim()
            .to_string();
        assert_eq!(corpus_hash.len(), 64);

        // A private copy of the gates file, so a test can perturb it.
        let gates = dir.path().join("GATES.toml");
        std::fs::copy(repo_root().join("configs/GATES_V1.toml"), &gates).unwrap();

        Fixture {
            manifest,
            gates,
            seal: dir.path().join("seal.json"),
            corpus_hash,
            _dir: dir,
        }
    }

    fn write_seal(&self, status: &str, corpus: &str, prereg: &str, gates: &str) {
        let doc = serde_json::json!({
            "schema": "vice-classic/gt-audit-seal/v1",
            "generation": 1,
            "status": status,
            "policy_version": "vice-classic/gt-split/v1",
            "corpus_hash": corpus,
            "prereg_hash": prereg,
            "gates_hash": gates,
            "opened_note": "integration test",
            "burn_reason": ""
        });
        std::fs::write(&self.seal, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    }

    fn audit_status(&self) -> (Option<i32>, String, String) {
        run(&[
            osv("audit-status").as_os_str(),
            osv("--seal").as_os_str(),
            self.seal.as_os_str(),
            osv("--manifest").as_os_str(),
            self.manifest.as_os_str(),
            osv("--gates").as_os_str(),
            self.gates.as_os_str(),
        ])
    }

    fn gates_hash(&self) -> String {
        use sha2::Digest;
        hex::encode(sha2::Sha256::digest(std::fs::read(&self.gates).unwrap()))
    }

    fn prereg_hash(&self) -> String {
        vice_bench::prereg::Preregistration::v1().hash()
    }
}

/// The state M3 records: sealed, never opened. Scoring against it is
/// refused, and that refusal is not an error.
#[test]
fn a_sealed_audit_reports_sealed_and_refuses_scoring() {
    let f = Fixture::new();
    f.write_seal("sealed", "", "", "");
    let (code, stdout, _) = f.audit_status();
    assert_eq!(code, Some(0), "a sealed audit is the expected state");
    assert!(stdout.contains("sealed and never opened"), "{stdout}");
}

/// The branch that had never executed: opened at the hashes the system
/// itself produces, on an untouched corpus, must PASS.
///
/// This is the exact scenario the reviewer ran and it reported BURNED.
#[test]
fn opening_at_the_systems_own_hashes_passes_on_an_untouched_corpus() {
    let f = Fixture::new();
    f.write_seal("opened", &f.corpus_hash, &f.prereg_hash(), &f.gates_hash());
    let (code, stdout, stderr) = f.audit_status();
    assert_eq!(
        code,
        Some(0),
        "an untouched corpus must not burn.\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("open and untouched"), "{stdout}");
}

/// And each of the three recorded hashes must be able to burn it, not just
/// whichever one someone happened to try (M-1: the class, not the
/// enumeration).
#[test]
fn changing_any_one_of_the_three_recorded_hashes_burns_the_generation() {
    let wrong = "0".repeat(64);

    // 1. corpus
    let f = Fixture::new();
    f.write_seal("opened", &wrong, &f.prereg_hash(), &f.gates_hash());
    let (code, _, stderr) = f.audit_status();
    assert_eq!(code, Some(1), "corpus mismatch must burn");
    assert!(stderr.contains("opened at corpus"), "{stderr}");

    // 2. preregistration
    let f = Fixture::new();
    f.write_seal("opened", &f.corpus_hash, &wrong, &f.gates_hash());
    let (code, _, stderr) = f.audit_status();
    assert_eq!(code, Some(1), "preregistration mismatch must burn");
    assert!(stderr.contains("opened at preregistration"), "{stderr}");

    // 3. gates - perturbed by editing the FILE, so the test exercises the
    // real path from bytes to hash rather than a substituted string.
    let f = Fixture::new();
    f.write_seal("opened", &f.corpus_hash, &f.prereg_hash(), &f.gates_hash());
    assert_eq!(f.audit_status().0, Some(0), "control: unperturbed passes");
    let mut gates = std::fs::read_to_string(&f.gates).unwrap();
    gates.push_str("\n# a comment added after the audit was opened\n");
    std::fs::write(&f.gates, gates).unwrap();
    let (code, _, stderr) = f.audit_status();
    assert_eq!(code, Some(1), "gates mismatch must burn");
    assert!(stderr.contains("opened at gates"), "{stderr}");
}

/// Perturbing the CORPUS itself - not the recorded hash - must burn it too.
/// This is the case the mechanism exists for, and it is the one that
/// requires the two sides to compute the hash the same way.
#[test]
fn a_corpus_that_changed_after_opening_burns_the_generation() {
    let f = Fixture::new();
    f.write_seal("opened", &f.corpus_hash, &f.prereg_hash(), &f.gates_hash());
    assert_eq!(f.audit_status().0, Some(0), "control: untouched passes");

    // Drop one cell from the manifest: `audit-status` rebuilds at the
    // manifest's own scope, so a different cell list is a different corpus.
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&f.manifest).unwrap()).unwrap();
    doc["cells"].as_array_mut().unwrap().pop();
    std::fs::write(&f.manifest, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

    let (code, _, stderr) = f.audit_status();
    assert_eq!(code, Some(1), "a changed corpus must burn");
    assert!(stderr.contains("opened at corpus"), "{stderr}");
}

/// A burned generation stays burned, and an "opened" record with empty
/// hashes cannot pass by being empty.
#[test]
fn a_burned_or_incomplete_record_cannot_pass() {
    let f = Fixture::new();
    f.write_seal("burned", &f.corpus_hash, &f.prereg_hash(), &f.gates_hash());
    let (code, _, stderr) = f.audit_status();
    assert_eq!(code, Some(1));
    assert!(stderr.contains("already burned"), "{stderr}");

    let f = Fixture::new();
    f.write_seal("opened", "", "", "");
    let (code, _, stderr) = f.audit_status();
    assert_eq!(code, Some(1));
    assert!(stderr.contains("records no"), "{stderr}");
}

/// The hash `build` prints, the hash `report` puts in the scorecard and the
/// hash `audit-status` compares must be ONE value. Two of them being equal
/// by accident is what made this defect survive.
#[test]
fn build_report_and_audit_status_agree_on_the_corpus_hash() {
    let f = Fixture::new();
    let out = f._dir.path().join("scorecard.json");
    f.write_seal("sealed", "", "", "");
    let (code, _, stderr) = run(&[
        osv("report").as_os_str(),
        osv("--manifest").as_os_str(),
        f.manifest.as_os_str(),
        osv("--gates").as_os_str(),
        f.gates.as_os_str(),
        osv("--seal").as_os_str(),
        f.seal.as_os_str(),
        osv("--out").as_os_str(),
        out.as_os_str(),
    ]);
    assert_eq!(code, Some(0), "{stderr}");
    let card: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
    assert_eq!(
        card["hashes"]["corpus"].as_str().unwrap(),
        f.corpus_hash,
        "the scorecard and `build` must print the same corpus hash"
    );

    // And audit-status accepts exactly that value.
    f.write_seal(
        "opened",
        card["hashes"]["corpus"].as_str().unwrap(),
        card["hashes"]["preregistration"].as_str().unwrap(),
        card["hashes"]["gates_sha256"].as_str().unwrap(),
    );
    let (code, stdout, stderr) = f.audit_status();
    assert_eq!(
        code,
        Some(0),
        "the scorecard's own hashes must satisfy the burn check.\n{stdout}\n{stderr}"
    );
}

/// `verify` refuses a manifest whose render digests were tampered with, and
/// names the fixture. Driven through the binary, since that is the form a
/// reviewer uses.
#[test]
fn verify_catches_a_tampered_render_digest_and_names_it() {
    let f = Fixture::new();
    let (code, _, _) = run(&[
        osv("verify").as_os_str(),
        osv("--manifest").as_os_str(),
        f.manifest.as_os_str(),
    ]);
    assert_eq!(code, Some(0), "control: the untouched manifest verifies");

    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&f.manifest).unwrap()).unwrap();
    doc["renders"][0]["sha256"] = serde_json::json!("0".repeat(64));
    std::fs::write(&f.manifest, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

    let (code, stdout, _) = run(&[
        osv("verify").as_os_str(),
        osv("--manifest").as_os_str(),
        f.manifest.as_os_str(),
    ]);
    assert_eq!(code, Some(1));
    assert!(stdout.contains("differs:"), "{stdout}");
}

/// A refusal that costs nothing must not be paid for with a rebuild
/// (REVIEW_M3 M3-D2).
///
/// The platform check is a string comparison, and it used to run AFTER the
/// corpus was regenerated: the reviewer measured 292 seconds before `exit 2`.
/// Measured here rather than asserted in a comment, against the COMMITTED
/// manifest, whose rebuild is minutes of work - so the bound below has a
/// margin of two orders of magnitude and is not a flaky timing test.
#[test]
fn a_foreign_platform_is_refused_before_the_corpus_is_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    let committed = repo_root().join("docs/gt/CORPUS_MANIFEST.json");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&committed).unwrap()).unwrap();
    doc["platform"]["os"] = serde_json::json!("elsewhere");
    let foreign = dir.path().join("foreign.json");
    std::fs::write(&foreign, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

    let started = std::time::Instant::now();
    let (code, _, stderr) = run(&[
        osv("verify").as_os_str(),
        osv("--manifest").as_os_str(),
        foreign.as_os_str(),
    ]);
    let elapsed = started.elapsed();
    assert_eq!(code, Some(2), "{stderr}");
    assert!(stderr.contains("elsewhere"), "{stderr}");
    assert!(
        elapsed.as_secs() < 60,
        "the platform refusal took {elapsed:?}; it is a string comparison and must not \
         wait for a corpus rebuild"
    );
}
