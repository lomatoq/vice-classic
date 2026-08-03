//! Canonical M7 release-claim manifest.
//!
//! The individual courts intentionally remain separate artifacts. This module
//! binds their exact bytes, common audit/governance identities, model
//! identities, export implementation, and renderer implementation into the
//! one replay entry point required by M7-39.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::baseline::M7_BASELINE_COURT_SCHEMA;
use super::determinism::M7_DETERMINISM_SCHEMA;
use super::oracle::M7_ORACLE_SCHEMA;
use super::release::M7_RELEASE_VERDICT_SCHEMA;

pub const M7_CANONICAL_ARTIFACT_SCHEMA: &str = "vice-classic/m7-canonical-artifact/v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComponentDigest {
    pub role: &'static str,
    pub schema: String,
    pub sha256: String,
    pub gate_met: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeliveryIdentity {
    pub preset: String,
    pub model: vice_opt::ModelIdentity,
    pub delivery_policy_sha256: String,
    pub production_config_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImplementationIdentity {
    pub export_plan_schema: &'static str,
    pub svg_parser_id: &'static str,
    pub svg_renderer_id: &'static str,
    pub render_digest_schema: &'static str,
    pub export_identity_sha256: String,
    pub renderer_identity_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CanonicalArtifact {
    pub schema: &'static str,
    pub release_commit_sha: String,
    pub audit_generation: u64,
    pub corpus_sha256: String,
    pub population_commitment_sha256: String,
    pub preregistration_sha256: String,
    pub gates_sha256: String,
    pub runner_attestation_sha256: String,
    pub gate_provenance_sha256: String,
    pub quality_calibration_measurement_sha256: String,
    pub fast_calibration_measurement_sha256: String,
    pub geometry_measurement_sha256: String,
    pub quality_report_sha256: String,
    pub fast_report_sha256: String,
    pub delivery_identities: Vec<DeliveryIdentity>,
    pub implementation: ImplementationIdentity,
    pub components: Vec<ComponentDigest>,
    pub gate_met: bool,
}

struct LoadedComponent {
    digest: ComponentDigest,
    value: Value,
}

pub fn assemble(
    release_path: &Path,
    baseline_path: &Path,
    oracle_path: &Path,
    determinism_path: &Path,
) -> Result<CanonicalArtifact, String> {
    let release = load_component("release", release_path, M7_RELEASE_VERDICT_SCHEMA)?;
    let baseline = load_component("baseline_blind", baseline_path, M7_BASELINE_COURT_SCHEMA)?;
    let oracle = load_component("oracle", oracle_path, M7_ORACLE_SCHEMA)?;
    let determinism = load_component("determinism", determinism_path, M7_DETERMINISM_SCHEMA)?;
    for component in [&release, &baseline, &oracle, &determinism] {
        if !component.digest.gate_met {
            return Err(format!("{} component is not green", component.digest.role));
        }
    }

    let release_commit_sha = string_field(&release.value, "release_commit_sha")?;
    validate_git_oid("release_commit_sha", &release_commit_sha)?;
    let audit_generation = u64_field(&release.value, "audit_generation")?;
    let corpus_sha256 = string_field(&release.value, "corpus_sha256")?;
    let population_commitment_sha256 =
        string_field(&release.value, "population_commitment_sha256")?;
    let preregistration_sha256 = string_field(&release.value, "preregistration_sha256")?;
    let gates_sha256 = string_field(&release.value, "gates_sha256")?;
    let runner_attestation_sha256 = string_field(&release.value, "runner_attestation_sha256")?;
    let gate_provenance_sha256 = string_field(&release.value, "gate_provenance_sha256")?;
    let quality_report_sha256 = string_field(&release.value, "quality_report_sha256")?;
    let fast_report_sha256 = string_field(&release.value, "fast_report_sha256")?;
    let quality_calibration_measurement_sha256 =
        string_field(&release.value, "quality_calibration_measurement_sha256")?;
    let fast_calibration_measurement_sha256 =
        string_field(&release.value, "fast_calibration_measurement_sha256")?;
    let geometry_measurement_sha256 = string_field(&release.value, "geometry_measurement_sha256")?;
    for (name, value) in [
        ("corpus_sha256", &corpus_sha256),
        (
            "population_commitment_sha256",
            &population_commitment_sha256,
        ),
        ("preregistration_sha256", &preregistration_sha256),
        ("gates_sha256", &gates_sha256),
        ("runner_attestation_sha256", &runner_attestation_sha256),
        ("gate_provenance_sha256", &gate_provenance_sha256),
        ("quality_report_sha256", &quality_report_sha256),
        ("fast_report_sha256", &fast_report_sha256),
        (
            "quality_calibration_measurement_sha256",
            &quality_calibration_measurement_sha256,
        ),
        (
            "fast_calibration_measurement_sha256",
            &fast_calibration_measurement_sha256,
        ),
        ("geometry_measurement_sha256", &geometry_measurement_sha256),
    ] {
        validate_sha256_like(name, value)?;
    }
    for component in [&baseline, &oracle, &determinism] {
        require_equal(component, "release_commit_sha", release_commit_sha.as_str())?;
        require_equal(component, "corpus_sha256", corpus_sha256.as_str())?;
        require_equal(
            component,
            "population_commitment_sha256",
            population_commitment_sha256.as_str(),
        )?;
        require_equal(
            component,
            "runner_attestation_sha256",
            runner_attestation_sha256.as_str(),
        )?;
    }
    for component in [&baseline, &oracle] {
        require_equal(component, "audit_generation", Value::from(audit_generation))?;
        require_equal(
            component,
            "preregistration_sha256",
            preregistration_sha256.as_str(),
        )?;
        require_equal(component, "gates_sha256", gates_sha256.as_str())?;
        require_equal(
            component,
            "gate_provenance_sha256",
            gate_provenance_sha256.as_str(),
        )?;
        require_equal(
            component,
            "quality_report_sha256",
            quality_report_sha256.as_str(),
        )?;
        require_equal(component, "fast_report_sha256", fast_report_sha256.as_str())?;
    }
    require_determinism_report(&determinism.value, "quality", &quality_report_sha256)?;
    require_determinism_report(&determinism.value, "fast", &fast_report_sha256)?;

    let delivery_identities = [
        (
            "quality",
            string_field(&release.value, "quality_production_config_sha256")?,
        ),
        (
            "fast",
            string_field(&release.value, "fast_production_config_sha256")?,
        ),
    ]
    .into_iter()
    .map(|(preset, config)| delivery_identity(&release.value, preset, config))
    .collect::<Result<Vec<_>, String>>()?;
    if delivery_identities[0].model.universe_sha256 != delivery_identities[1].model.universe_sha256
        || delivery_identities[0].model.pricing_sha256
            != delivery_identities[1].model.pricing_sha256
        || delivery_identities[0].model.backend_sha256
            != delivery_identities[1].model.backend_sha256
    {
        return Err(
            "Fast and Quality do not share the frozen universe, pricing, and backend".into(),
        );
    }

    let export_source = format!(
        "{}|{}|{}",
        vice_svg::EXPORT_PLAN_SCHEMA,
        vice_svg::SVG_PARSER_ID,
        env!("CARGO_PKG_VERSION")
    );
    let renderer_source = format!(
        "{}|{}|{}",
        vice_render::RENDER_DIGEST_SCHEMA,
        vice_svg::SVG_RENDERER_ID,
        env!("CARGO_PKG_VERSION")
    );
    let implementation = ImplementationIdentity {
        export_plan_schema: vice_svg::EXPORT_PLAN_SCHEMA,
        svg_parser_id: vice_svg::SVG_PARSER_ID,
        svg_renderer_id: vice_svg::SVG_RENDERER_ID,
        render_digest_schema: vice_render::RENDER_DIGEST_SCHEMA,
        export_identity_sha256: digest(export_source.as_bytes()),
        renderer_identity_sha256: digest(renderer_source.as_bytes()),
    };
    Ok(CanonicalArtifact {
        schema: M7_CANONICAL_ARTIFACT_SCHEMA,
        release_commit_sha,
        audit_generation,
        corpus_sha256,
        population_commitment_sha256,
        preregistration_sha256,
        gates_sha256,
        runner_attestation_sha256,
        gate_provenance_sha256,
        quality_calibration_measurement_sha256,
        fast_calibration_measurement_sha256,
        geometry_measurement_sha256,
        quality_report_sha256,
        fast_report_sha256,
        delivery_identities,
        implementation,
        components: vec![
            release.digest,
            baseline.digest,
            oracle.digest,
            determinism.digest,
        ],
        gate_met: true,
    })
}

pub fn write(path: &Path, artifact: &CanonicalArtifact) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(artifact).map_err(|error| error.to_string())?;
    std::fs::write(path, [bytes.as_slice(), b"\n"].concat())
        .map_err(|error| format!("write {}: {error}", path.display()))
}

fn load_component(
    role: &'static str,
    path: &Path,
    expected_schema: &str,
) -> Result<LoadedComponent, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
    let schema = string_field(&value, "schema")?;
    if schema != expected_schema {
        return Err(format!(
            "{role} has schema {schema:?}, expected {expected_schema:?}"
        ));
    }
    let gate_met = value
        .get("gate_met")
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{role}.gate_met is absent or not boolean"))?;
    Ok(LoadedComponent {
        digest: ComponentDigest {
            role,
            schema,
            sha256: digest(&bytes),
            gate_met,
        },
        value,
    })
}

fn delivery_identity(
    value: &Value,
    preset: &str,
    production_config_sha256: String,
) -> Result<DeliveryIdentity, String> {
    let verdict = value
        .get(preset)
        .ok_or_else(|| format!("release.{preset} is absent"))?;
    let model: vice_opt::ModelIdentity = serde_json::from_value(
        verdict
            .get("identity")
            .cloned()
            .ok_or_else(|| format!("release.{preset}.identity is absent"))?,
    )
    .map_err(|error| format!("release.{preset}.identity: {error}"))?;
    for (name, digest) in [
        ("universe", &model.universe_sha256),
        ("pricing", &model.pricing_sha256),
        ("backend", &model.backend_sha256),
        ("config", &model.config_sha256),
    ] {
        validate_sha256_like(&format!("{preset}.{name}"), digest)?;
    }
    let delivery_policy_sha256 = string_field(verdict, "delivery_policy_sha256")?;
    validate_sha256_like(
        &format!("{preset}.delivery_policy_sha256"),
        &delivery_policy_sha256,
    )?;
    validate_sha256_like(
        &format!("{preset}.production_config_sha256"),
        &production_config_sha256,
    )?;
    Ok(DeliveryIdentity {
        preset: preset.into(),
        model,
        delivery_policy_sha256,
        production_config_sha256,
    })
}

fn require_determinism_report(
    determinism: &Value,
    preset: &str,
    report_sha256: &str,
) -> Result<(), String> {
    let presets = determinism
        .get("presets")
        .and_then(Value::as_array)
        .ok_or_else(|| "determinism.presets is absent or not an array".to_string())?;
    let matching = presets
        .iter()
        .filter(|verdict| verdict.get("preset").and_then(Value::as_str) == Some(preset))
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return Err(format!("determinism has no {preset} verdict"));
    }
    if matching.iter().any(|verdict| {
        verdict
            .get("runs")
            .and_then(Value::as_array)
            .is_some_and(|runs| {
                runs.iter().any(|run| {
                    run.get("canonical_report_sha256").and_then(Value::as_str)
                        == Some(report_sha256)
                })
            })
    }) {
        Ok(())
    } else {
        Err(format!(
            "determinism does not include the {preset} report used by the release verdict"
        ))
    }
}

fn require_equal(
    component: &LoadedComponent,
    field: &str,
    expected: impl Into<Value>,
) -> Result<(), String> {
    let expected = expected.into();
    if component.value.get(field) != Some(&expected) {
        return Err(format!(
            "{}.{field} is not bound to the release verdict",
            component.digest.role
        ));
    }
    Ok(())
}

fn string_field(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("{field} is absent or not a string"))
}

fn u64_field(value: &Value, field: &str) -> Result<u64, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{field} is absent or not an unsigned integer"))
}

fn validate_sha256_like(name: &str, value: &str) -> Result<(), String> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(format!("{name} is not a lowercase sha256"))
    }
}

fn validate_git_oid(name: &str, value: &str) -> Result<(), String> {
    if matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(format!("{name} is not a lowercase Git object id"))
    }
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn common(schema: &str) -> Value {
        json!({
            "schema": schema,
            "audit_generation": 7,
            "corpus_sha256": "1".repeat(64),
            "population_commitment_sha256": "0".repeat(64),
            "preregistration_sha256": "2".repeat(64),
            "gates_sha256": "3".repeat(64),
            "release_commit_sha": "4".repeat(40),
            "runner_attestation_sha256": "5".repeat(64),
            "gate_provenance_sha256": "6".repeat(64),
            "quality_report_sha256": "e".repeat(64),
            "fast_report_sha256": "f".repeat(64),
            "quality_calibration_measurement_sha256": "a".repeat(64),
            "fast_calibration_measurement_sha256": "b".repeat(64),
            "geometry_measurement_sha256": "c".repeat(64),
            "quality_production_config_sha256": "d".repeat(64),
            "fast_production_config_sha256": "1".repeat(64),
            "gate_met": true
        })
    }

    fn write_json(path: &Path, value: &Value) {
        std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }

    #[test]
    fn canonical_artifact_binds_every_green_component_and_identity() {
        let temp = tempfile::tempdir().unwrap();
        let mut release = common(M7_RELEASE_VERDICT_SCHEMA);
        for preset in ["quality", "fast"] {
            release[preset] = json!({
                "identity": {
                    "universe_sha256": "7".repeat(64),
                    "pricing_sha256": "8".repeat(64),
                    "backend_sha256": "9".repeat(64),
                    "config_sha256": if preset == "quality" { "a".repeat(64) } else { "b".repeat(64) }
                },
                "delivery_policy_sha256": if preset == "quality" { "c".repeat(64) } else { "d".repeat(64) }
            });
        }
        let baseline = common(M7_BASELINE_COURT_SCHEMA);
        let oracle = common(M7_ORACLE_SCHEMA);
        let determinism = json!({
            "schema": M7_DETERMINISM_SCHEMA,
            "release_commit_sha": "4".repeat(40),
            "runner_attestation_sha256": "5".repeat(64),
            "corpus_sha256": "1".repeat(64),
            "population_commitment_sha256": "0".repeat(64),
            "presets": [
                {"preset": "quality", "runs": [{"canonical_report_sha256": "e".repeat(64)}]},
                {"preset": "fast", "runs": [{"canonical_report_sha256": "f".repeat(64)}]}
            ],
            "gate_met": true
        });
        let paths = [
            temp.path().join("release.json"),
            temp.path().join("baseline.json"),
            temp.path().join("oracle.json"),
            temp.path().join("determinism.json"),
        ];
        for (path, value) in paths
            .iter()
            .zip([&release, &baseline, &oracle, &determinism])
        {
            write_json(path, value);
        }
        let artifact = assemble(&paths[0], &paths[1], &paths[2], &paths[3]).unwrap();
        assert!(artifact.gate_met);
        assert_eq!(artifact.components.len(), 4);
        assert_eq!(artifact.delivery_identities.len(), 2);
        assert_eq!(artifact.release_commit_sha.len(), 40);
    }

    #[test]
    fn a_component_from_another_gate_freeze_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let mut release = common(M7_RELEASE_VERDICT_SCHEMA);
        for preset in ["quality", "fast"] {
            release[preset] = json!({
                "identity": {
                    "universe_sha256": "7".repeat(64),
                    "pricing_sha256": "8".repeat(64),
                    "backend_sha256": "9".repeat(64),
                    "config_sha256": "a".repeat(64)
                },
                "delivery_policy_sha256": "b".repeat(64)
            });
        }
        let baseline = common(M7_BASELINE_COURT_SCHEMA);
        let mut oracle = common(M7_ORACLE_SCHEMA);
        oracle["gates_sha256"] = Value::String("f".repeat(64));
        let determinism = json!({
            "schema": M7_DETERMINISM_SCHEMA,
            "release_commit_sha": "4".repeat(40),
            "runner_attestation_sha256": "5".repeat(64),
            "corpus_sha256": "1".repeat(64),
            "population_commitment_sha256": "0".repeat(64),
            "presets": [
                {"preset": "quality", "runs": [{"canonical_report_sha256": "e".repeat(64)}]},
                {"preset": "fast", "runs": [{"canonical_report_sha256": "f".repeat(64)}]}
            ],
            "gate_met": true
        });
        let paths = [
            temp.path().join("release.json"),
            temp.path().join("baseline.json"),
            temp.path().join("oracle.json"),
            temp.path().join("determinism.json"),
        ];
        for (path, value) in paths
            .iter()
            .zip([&release, &baseline, &oracle, &determinism])
        {
            write_json(path, value);
        }
        assert!(assemble(&paths[0], &paths[1], &paths[2], &paths[3])
            .unwrap_err()
            .contains("oracle.gates_sha256"));
    }
}
