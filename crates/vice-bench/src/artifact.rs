//! Comparing a committed Tier A artifact with a fresh rebuild (spec §5.5,
//! F-0020, F-0022).
//!
//! Two artifacts now carry libm-derived floats — the oracle report and the
//! corridor report — and both are checked the same way: refuse across
//! platforms unless the caller asked for the platform-INDEPENDENT
//! projection, and say which of the two comparisons was made. Written once
//! here rather than twice in the CLI, because the second copy is where the
//! two would drift.

use serde::Serialize;

/// The Tier A platform an artifact belongs to, as the artifact records it.
pub fn platform_here() -> serde_json::Value {
    serde_json::json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    })
}

/// Outcome of comparing a recorded artifact with a rebuild.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ArtifactCheck {
    /// Same platform, every value compared, equal.
    Reproduced,
    /// Cross-platform, the projection compared, equal.
    ReproducedStructurally,
    Differs,
    /// Cross-platform without `--structural`: a typed refusal, never a
    /// silent pass.
    WrongPlatform,
}

impl ArtifactCheck {
    pub fn exit_code(&self) -> i32 {
        match self {
            ArtifactCheck::Reproduced | ArtifactCheck::ReproducedStructurally => 0,
            ArtifactCheck::Differs => 1,
            ArtifactCheck::WrongPlatform => 2,
        }
    }
}

/// Compare, and print the sentence that says WHICH comparison was made.
pub fn check(
    kind: &str,
    recorded: &serde_json::Value,
    rebuilt: &serde_json::Value,
    structural: bool,
    projection: fn(&serde_json::Value) -> serde_json::Value,
) -> ArtifactCheck {
    let here = platform_here();
    let recorded_platform = recorded.get("platform").cloned().unwrap_or_default();
    let same_platform = recorded_platform == here;
    if !same_platform && !structural {
        eprintln!(
            "error: this {kind} records metrics for platform {recorded_platform}, and this is \
             {here}. The metrics are libm-derived floats and therefore TIER A (spec 5.5, \
             ADR-0008). Re-run on the recording platform, or pass --structural to compare the \
             platform-independent projection."
        );
        return ArtifactCheck::WrongPlatform;
    }
    let (a, b, mode, ok) = if structural && !same_platform {
        (
            projection(recorded),
            projection(rebuilt),
            "STRUCTURALLY across platforms (metrics NOT compared - they are Tier A)",
            ArtifactCheck::ReproducedStructurally,
        )
    } else {
        (
            recorded.clone(),
            rebuilt.clone(),
            "with every metric compared",
            ArtifactCheck::Reproduced,
        )
    };
    if a == b {
        println!("{kind} reproduced {mode}");
        ok
    } else {
        eprintln!("{kind} did NOT reproduce {mode}");
        ArtifactCheck::Differs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(v: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "shape": v["shape"] })
    }

    /// The three outcomes, and the one that matters: a foreign platform
    /// without the flag is a REFUSAL with its own exit code, not a pass.
    #[test]
    fn a_foreign_platform_is_refused_and_the_projection_is_what_survives() {
        let here = serde_json::json!({"platform": platform_here(), "shape": 1, "metric": 0.5});
        let same = here.clone();
        assert_eq!(
            check("probe", &here, &same, false, strip),
            ArtifactCheck::Reproduced
        );
        let mut elsewhere = here.clone();
        elsewhere["platform"] = serde_json::json!({"os": "elsewhere", "arch": "other"});
        assert_eq!(
            check("probe", &elsewhere, &same, false, strip),
            ArtifactCheck::WrongPlatform
        );
        assert_eq!(ArtifactCheck::WrongPlatform.exit_code(), 2);
        // With the flag, the projection is compared - and it survives a
        // different METRIC while still catching a different SHAPE.
        let mut moved_metric = elsewhere.clone();
        moved_metric["metric"] = serde_json::json!(0.6);
        assert_eq!(
            check("probe", &moved_metric, &same, true, strip),
            ArtifactCheck::ReproducedStructurally
        );
        let mut moved_shape = elsewhere.clone();
        moved_shape["shape"] = serde_json::json!(2);
        assert_eq!(
            check("probe", &moved_shape, &same, true, strip),
            ArtifactCheck::Differs
        );
        assert_eq!(ArtifactCheck::Differs.exit_code(), 1);
    }

    /// On the recording platform the flag is INERT: it does not weaken a
    /// same-platform comparison into a structural one.
    #[test]
    fn the_structural_flag_is_inert_on_the_recording_platform() {
        let here = serde_json::json!({"platform": platform_here(), "shape": 1, "metric": 0.5});
        let mut moved = here.clone();
        moved["metric"] = serde_json::json!(0.6);
        assert_eq!(
            check("probe", &here, &moved, true, strip),
            ArtifactCheck::Differs
        );
    }
}
