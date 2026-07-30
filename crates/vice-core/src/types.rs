use serde::Serialize;

pub const CORE_REPORT_SCHEMA: &str = "vice-classic/m7-vectorize-report/v6";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    Success,
    Ambiguous,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum FailureReason {
    Evidence { detail: String },
    FormationOutsideUniverse { detail: String },
    BoundaryOutsideSelectiveCore { detail: String },
    Topology { detail: String },
    Fitting { detail: String },
    SearchTruncated { detail: String },
    NoVerifiedCandidate { detail: String },
    Confidence { detail: String },
    Decode { detail: String },
    Internal { detail: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RuntimeSummary {
    pub elapsed_ms: u64,
    pub candidates_scored: u64,
    pub candidate_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandidateSummary {
    pub hypothesis_id: String,
    pub topology_arm: String,
    pub topology_class: String,
    pub formation_class: String,
    pub scene_digest_sha256: String,
    pub delivery_digest: String,
    pub score: vice_opt::ScoreBreakdown,
    pub pre_quantization: vice_verify::PresealCertificate,
    pub post_quantization: vice_verify::PostQuantizationCertificate,
    pub delivery_seal: vice_verify::DeliverySeal,
    pub optimizer: vice_opt::OptimizationResult,
    pub transactions: Vec<vice_opt::TransactionApplication>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateFailureStage {
    SceneConstruction,
    Preseal,
    PaintOptimization,
    Quantization,
    ExportPlan,
    SvgMaterialization,
    IndependentRender,
    SerializedLikelihood,
    DeliverySeal,
    CanonicalArtifact,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandidateRefusal {
    pub hypothesis_id: String,
    pub stage: CandidateFailureStage,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransactionInventoryRow {
    pub kind: vice_opt::TransactionKind,
    pub proposed: u64,
    pub atomic_applied: u64,
    pub verified_and_exact_scored: u64,
    pub refused_before_score: u64,
    pub not_applicable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TransactionInventory {
    pub complete_kind_enumeration: bool,
    pub rows: Vec<TransactionInventoryRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopologyArmTrace {
    pub class: String,
    pub topology_class: String,
    pub signature_sha256: String,
    pub components: u32,
    pub holes: u32,
    pub foreground_connectivity: String,
    pub field: vice_topology::FieldKind,
    pub saddle: vice_topology::SaddleResolution,
    pub extraction_level: f64,
    /// Boundary level used by the typed geometry fit. Event levels propose
    /// topology; the canonical 0.5 contour owns geometry whenever it binds to
    /// that topology.
    pub fit_observation_level: f64,
    pub observed_chains: usize,
    pub fit_models_per_chain: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopologyArmRefusal {
    pub topology_class: String,
    pub signature_sha256: String,
    pub foreground_connectivity: String,
    pub field: vice_topology::FieldKind,
    pub saddle: vice_topology::SaddleResolution,
    pub extraction_level: f64,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TopologyEnvelopeTrace {
    pub proposal: vice_topology::Proposal,
    pub materialized_arms: Vec<TopologyArmTrace>,
    pub materialization_refusals: Vec<TopologyArmRefusal>,
    pub prefit_budget_pruned_arms: Vec<TopologyArmTrace>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VectorizeReport {
    pub schema: &'static str,
    pub status: DecisionStatus,
    pub reason: Option<FailureReason>,
    pub request: crate::VectorizeRequest,
    pub production: bool,
    pub source_sha256: Option<String>,
    pub binary_version: &'static str,
    pub toolchain: &'static str,
    pub environment: &'static str,
    pub identity: vice_opt::ModelIdentity,
    pub delivery_policy_sha256: String,
    pub calibration: Option<crate::ConfidenceCalibration>,
    pub evidence: Option<vice_evidence::Flat2Analysis>,
    pub topology: Option<TopologyEnvelopeTrace>,
    pub fits: Vec<vice_fit::ModelRun>,
    pub beam: Option<vice_opt::BudgetLedger>,
    pub search_mass: Option<vice_opt::SearchMassCertificate>,
    pub confidence_metrics: Option<crate::ConfidenceMetrics>,
    pub candidates: Vec<CandidateSummary>,
    pub candidate_refusals: Vec<CandidateRefusal>,
    pub transaction_inventory: Option<TransactionInventory>,
    pub selected_hypothesis_id: Option<String>,
    pub selected_boundary_bindings: Vec<vice_verify::BoundaryBinding>,
    pub runtime: RuntimeSummary,
}

impl VectorizeReport {
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("report serializes")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SuccessArtifacts {
    pub result_svg: Vec<u8>,
    pub pure_partition_svg: Vec<u8>,
    pub scene_json: Vec<u8>,
    pub export_plan_json: Vec<u8>,
    pub report_json: Vec<u8>,
    pub render_png: Vec<u8>,
    pub seal_json: Vec<u8>,
    pub trace_json: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VectorizeSuccess {
    pub report: VectorizeReport,
    pub artifacts: SuccessArtifacts,
}

/// Non-production witness for the held-out calibration court.
///
/// This is deliberately not a `SuccessArtifacts`: the accompanying outcome
/// remains non-success until a trusted production calibration is installed.
/// It lets `vice-bench` judge the selected canonical scene rather than infer
/// correctness from the candidate's own score.
#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationWitness {
    pub candidate: CandidateSummary,
    pub scene_json: Vec<u8>,
    pub export_plan_json: Vec<u8>,
    pub pure_partition_svg: Vec<u8>,
    pub seam_safe_svg: Vec<u8>,
    pub rendered_png: Vec<u8>,
    pub seal_json: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationRun {
    pub outcome: VectorizeOutcome,
    pub selected: Option<CalibrationWitness>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VectorizeOutcome {
    Success(VectorizeSuccess),
    Ambiguous(VectorizeReport),
    Unsupported(VectorizeReport),
    Failed(VectorizeReport),
}

impl VectorizeOutcome {
    pub fn report(&self) -> &VectorizeReport {
        match self {
            Self::Success(value) => &value.report,
            Self::Ambiguous(value) | Self::Unsupported(value) | Self::Failed(value) => value,
        }
    }
}
