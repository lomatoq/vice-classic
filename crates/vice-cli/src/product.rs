use serde::Serialize;
use sha2::{Digest, Sha256};

pub const RELEASE_STATUS_SCHEMA: &str = "vice-classic/m12-release-status/v1";

#[derive(Debug, Serialize)]
pub struct ReleaseStatus {
    pub schema: &'static str,
    pub binary_version: &'static str,
    pub cross_platform_tier: &'static str,
    pub fast_config_sha256: &'static str,
    pub quality_config_sha256: &'static str,
    pub wasm_adapter_schema: &'static str,
    pub classic_fallback_policy: &'static str,
    pub technical_release_candidate: bool,
    pub public_release_authorized: bool,
    pub commercial_release_authorized: bool,
    pub legal_review_status: &'static str,
    pub legal_blockers: [&'static str; 3],
    pub structural_contract_sha256: String,
}

pub fn release_status() -> ReleaseStatus {
    let structural = [
        RELEASE_STATUS_SCHEMA,
        env!("CARGO_PKG_VERSION"),
        "tier_b_structural",
        vice_core::M7_FAST_PRODUCTION_CONFIG_SHA256,
        vice_core::M7_QUALITY_PRODUCTION_CONFIG_SHA256,
        "vice-classic/wasm-result/v1",
        "explicit_only_never_classic_fallback",
    ]
    .join("\n");
    ReleaseStatus {
        schema: RELEASE_STATUS_SCHEMA,
        binary_version: env!("CARGO_PKG_VERSION"),
        cross_platform_tier: "tier_b_structural",
        fast_config_sha256: vice_core::M7_FAST_PRODUCTION_CONFIG_SHA256,
        quality_config_sha256: vice_core::M7_QUALITY_PRODUCTION_CONFIG_SHA256,
        wasm_adapter_schema: "vice-classic/wasm-result/v1",
        classic_fallback_policy: "explicit_only_never_classic_fallback",
        technical_release_candidate: true,
        public_release_authorized: false,
        commercial_release_authorized: false,
        legal_review_status: "human_review_required",
        legal_blockers: [
            "repository license grant not selected",
            "owner-controlled donor pins require explicit non-use/license attestation",
            "patent and freedom-to-operate opinion requires qualified human counsel",
        ],
        structural_contract_sha256: hex::encode(Sha256::digest(structural.as_bytes())),
    }
}

pub fn canonical_json() -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec_pretty(&release_status())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn technical_readiness_never_mints_legal_authorization() {
        let status = release_status();
        assert!(status.technical_release_candidate);
        assert!(!status.public_release_authorized);
        assert!(!status.commercial_release_authorized);
        assert_eq!(status.structural_contract_sha256.len(), 64);
    }
}
