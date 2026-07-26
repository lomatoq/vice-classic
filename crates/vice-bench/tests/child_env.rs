//! Child environment policy tests (REVIEW_M0 condition 5 / N6).
//!
//! This file is a SEPARATE integration-test binary on purpose: it mutates
//! this process's environment with `set_var`, and a dedicated process
//! keeps that mutation away from every other test. All probes run inside
//! the single test function, sequentially.

use std::path::PathBuf;
use std::time::Duration;

use vice_bench::envinfo;
use vice_bench::exec::run_with_timeout;

fn runner_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_baseline-runner"))
}

fn probe(dir: &std::path::Path, var: &str) -> String {
    let out = dir.join(format!("{var}.txt"));
    let log = dir.join(format!("{var}.log"));
    let argv = vec![
        runner_bin().display().to_string(),
        "env-probe".to_string(),
        var.to_string(),
        out.display().to_string(),
    ];
    let outcome = run_with_timeout(&argv, dir, Duration::from_secs(60), &log).unwrap();
    assert_eq!(outcome.exit_code, Some(0), "probe {var} failed");
    std::fs::read_to_string(&out).unwrap()
}

#[test]
fn children_get_sanitized_pinned_environment() {
    // Pollute the ambient environment of THIS process; run_with_timeout
    // children inherit it and the policy must sanitize it.
    std::env::set_var("RUSTFLAGS", "--cfg bogus_ambient_flag");
    std::env::set_var("CARGO_TARGET_DIR", "C:/definitely/bogus/target");
    std::env::set_var("RUSTC_WRAPPER", "bogus-wrapper");
    std::env::set_var("PYTHONHASHSEED", "999");

    let t = tempfile::tempdir().unwrap();

    // Influence-carrying variables are removed for children.
    assert_eq!(probe(t.path(), "RUSTFLAGS"), "RUSTFLAGS=<unset>");
    assert_eq!(
        probe(t.path(), "CARGO_TARGET_DIR"),
        "CARGO_TARGET_DIR=<unset>"
    );
    assert_eq!(probe(t.path(), "RUSTC_WRAPPER"), "RUSTC_WRAPPER=<unset>");

    // PYTHONHASHSEED is pinned to 0, overriding the ambient 999.
    assert_eq!(probe(t.path(), "PYTHONHASHSEED"), "PYTHONHASHSEED=0");

    // Sanity: the policy is removal of a FIXED list, not env_clear — PATH
    // survives (children could not even spawn tools otherwise).
    let path_probe = probe(t.path(), "PATH");
    assert_ne!(path_probe, "PATH=<unset>");

    // The env manifest records the policy and the ambient presence of the
    // overridden variables (names only, no values).
    let m = envinfo::collect();
    assert_eq!(m.command_env.policy, envinfo::CHILD_ENV_POLICY);
    assert!(m.command_env.removed.iter().any(|n| n == "RUSTFLAGS"));
    assert_eq!(
        m.command_env.set.get("PYTHONHASHSEED").map(String::as_str),
        Some("0")
    );
    for name in [
        "RUSTFLAGS",
        "CARGO_TARGET_DIR",
        "RUSTC_WRAPPER",
        "PYTHONHASHSEED",
    ] {
        assert!(
            m.command_env
                .ambient_overrides_present
                .iter()
                .any(|n| n == name),
            "{name} should be recorded as ambient-present"
        );
    }
    let json = envinfo::canonical_json(&m);
    assert!(json.contains("\"command_env\""));
    assert!(!json.contains("bogus_ambient_flag"), "values must not leak");
}
