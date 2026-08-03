use super::*;

fn court_population(
    scope: M8CourtScope,
    variants_per_family: usize,
) -> Result<Vec<GtSourceGroup>, String> {
    let mut groups = procedural_groups_filtered_for_generation(
        variants_per_family,
        M8_PROCEDURAL_GENERATION,
        |id| id.contains("/dot_cluster/") || id.contains("/triple_junction/"),
    )
    .into_iter()
    .filter(|group| parse_variant(&group.id).is_ok_and(|variant| scope.admits_variant(variant)))
    .collect::<Vec<_>>();
    if scope != M8CourtScope::Smoke {
        let mut independent =
            crate::gt::authored::authored_groups().map_err(|error| error.to_string())?;
        independent
            .extend(crate::gt::authored::m8_authored_groups().map_err(|error| error.to_string())?);
        independent.extend(crate::gt::adversarial::adversarial_groups());
        independent.extend(crate::gt::adversarial::m8_adversarial_groups());
        independent.retain(|group| {
            group.scenes.len() == 1
                && !group.intentionally_ambiguous
                && scope.admits_nonprocedural(&group.id)
                && is_observable_multiregion_source(&group.scenes[0])
        });
        groups.extend(independent);
    }
    groups.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(groups)
}

pub(super) fn eligible_court_population(
    scope: M8CourtScope,
    variants_per_family: usize,
) -> Result<Vec<GtSourceGroup>, String> {
    Ok(court_population(scope, variants_per_family)?
        .into_iter()
        .filter(|group| {
            let Some(scene) = group.scenes.first() else {
                return false;
            };
            let cell = court_cell(scope.profile(), &group.shape_family);
            let visible_scale_px = scene.min_salient_scale_px() * f64::from(cell.size_px)
                / f64::from(crate::gt::grammar::AUTHORING_CANVAS_PX);
            scene.partition_truth().visible_faces >= 3 && visible_scale_px >= 5.0
        })
        .collect())
}

fn is_observable_multiregion_source(scene: &crate::gt::GtScene) -> bool {
    if scene.partition_truth().visible_faces < 3 {
        return false;
    }
    let paints = scene
        .scene()
        .graph()
        .faces
        .iter()
        .filter_map(|face| match face.paint {
            vice_ir::Paint::OpaqueSolid(rgb) => {
                Some([rgb.r.to_bits(), rgb.g.to_bits(), rgb.b.to_bits()])
            }
            vice_ir::Paint::TransparentExterior => None,
        })
        .collect::<BTreeSet<_>>();
    paints.len() >= 3
}

pub(super) fn court_cluster_id(group: &GtSourceGroup) -> String {
    parse_variant(&group.id).map_or_else(
        |_| format!("{}/source/{}", group.origin.as_str(), group.id),
        |variant| {
            format!(
                "{}/cluster-{:03}",
                group.shape_family,
                variant / M8_CLUSTER_SIZE
            )
        },
    )
}

#[derive(Debug)]
struct ExpectedCourtIdentity {
    rows: BTreeMap<String, (String, String, String, String, String)>,
    merged_commitment_sha256: String,
}

fn expected_court_identity(
    scope: M8CourtScope,
    variants_per_family: usize,
    shard_count: u32,
) -> Result<ExpectedCourtIdentity, String> {
    let groups = eligible_court_population(scope, variants_per_family)?;
    let mut rows = BTreeMap::new();
    for group in &groups {
        let scene = group
            .scenes
            .first()
            .ok_or_else(|| format!("{} has no scene", group.id))?;
        let cell = court_cell(scope.profile(), &group.shape_family);
        let rendered = render_cell(scene, &cell, 1)?;
        let png = encode_png(rendered.width_px, rendered.height_px, &rendered.rgba8)?;
        rows.insert(
            group.id.clone(),
            (
                group.origin.as_str().into(),
                group.shape_family.clone(),
                court_cluster_id(group),
                rendered.cell_id,
                hex::encode(Sha256::digest(&png)),
            ),
        );
    }
    let mut merged = Sha256::new();
    merged.update(b"vice-classic/m8-merged-court/v1");
    for shard in 0..shard_count {
        let mut commitment = Sha256::new();
        commitment.update(M8_COURT_SCHEMA.as_bytes());
        commitment.update((variants_per_family as u64).to_le_bytes());
        for group in groups
            .iter()
            .filter(|group| shard_of(&group.id, shard_count) == shard)
        {
            commitment.update(group.id.as_bytes());
            commitment.update(
                vice_ir::canonical_scene_bytes(group.scenes[0].scene().scene())
                    .map_err(|error| error.to_string())?,
            );
        }
        merged.update(hex::encode(commitment.finalize()).as_bytes());
    }
    Ok(ExpectedCourtIdentity {
        rows,
        merged_commitment_sha256: hex::encode(merged.finalize()),
    })
}

pub(super) fn validate_formal_court(
    report: &M8CourtReport,
    scope: M8CourtScope,
) -> Result<(), String> {
    validate_formal_header(report, scope)?;
    let expected = expected_court_identity(scope, M8_VARIANTS_PER_FAMILY, M8_FORMAL_SHARDS)?;
    if report.corpus_commitment_sha256 != expected.merged_commitment_sha256
        || report.source_groups != expected.rows.len() as u64
        || report.rows.len() != expected.rows.len()
    {
        return Err("M8 formal court population commitment is invalid".into());
    }
    let actual_ids = report
        .rows
        .iter()
        .map(|row| row.group_id.as_str())
        .collect::<BTreeSet<_>>();
    let expected_ids = expected
        .rows
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if actual_ids != expected_ids || actual_ids.len() != report.rows.len() {
        return Err("M8 formal court has missing or duplicate row identities".into());
    }
    for row in &report.rows {
        let Some((origin, family, cluster, cell, source)) = expected.rows.get(&row.group_id) else {
            return Err(format!(
                "M8 formal court has unexpected row {}",
                row.group_id
            ));
        };
        if &row.fixture_origin != origin
            || &row.shape_family != family
            || &row.cluster_id != cluster
            || &row.cell_id != cell
            || &row.source_sha256 != source
        {
            return Err(format!(
                "M8 formal court row {} is not source-bound",
                row.group_id
            ));
        }
        validate_formal_row(row)?;
    }
    Ok(())
}

fn validate_formal_row(row: &M8CourtRow) -> Result<(), String> {
    if !row.accepted {
        if row.refusal.as_ref().is_none_or(String::is_empty)
            || row.intrinsic_catastrophic
            || row.exact_candidate_id.is_some()
            || row.topology.is_some()
            || row.boundary.is_some()
        {
            return Err(format!("M8 refused row {} is malformed", row.group_id));
        }
        return Ok(());
    }
    let topology = row
        .topology
        .as_ref()
        .ok_or_else(|| format!("M8 accepted row {} has no topology", row.group_id))?;
    let boundary = row
        .boundary
        .as_ref()
        .ok_or_else(|| format!("M8 accepted row {} has no boundary", row.group_id))?;
    let finite = |value: Option<f64>| value.is_some_and(|value| value.is_finite() && value >= 0.0);
    let topology_exact = topology.truth_visible_faces == topology.selected_visible_faces
        && topology.truth_components == topology.selected_components
        && topology.truth_holes == topology.selected_holes
        && topology.truth_exterior == topology.selected_exterior;
    let paint = row
        .paint_delta_codes
        .ok_or_else(|| format!("M8 accepted row {} has no paint delta", row.group_id))?;
    let catastrophic = !topology.exact
        || boundary.max_px > M8_CATASTROPHIC_BOUNDARY_MAX_PX
        || paint > M8_CATASTROPHIC_PAINT_DELTA_CODES;
    if row.refusal.is_some()
        || row.exact_candidate_id.as_ref().is_none_or(String::is_empty)
        || !finite(row.exact_total_bits)
        || !finite(row.exact_pixel_bits)
        || !finite(row.pixel_bits_per_block)
        || row.selected_palette_cardinality.is_none_or(|v| v < 3)
        || row.opaque_modes_seen.is_none_or(|v| v < 3)
        || row.selection_class.as_ref().is_none_or(String::is_empty)
        || row.exact_candidates_evaluated == 0
        || topology.exact != topology_exact
        || boundary.samples == 0
        || !boundary.p95_px.is_finite()
        || !boundary.p99_px.is_finite()
        || !boundary.max_px.is_finite()
        || boundary.p95_px < 0.0
        || boundary.p95_px > boundary.p99_px
        || boundary.p99_px > boundary.max_px
        || boundary.gate_counts.samples_at_or_below_p95_gate > boundary.samples
        || boundary.gate_counts.samples_at_or_below_p99_gate > boundary.samples
        || !finite(row.profile_mean_channel_delta)
        || !finite(row.internal_mean_channel_delta)
        || row.profile_max_channel_delta.is_none()
        || row.internal_max_channel_delta.is_none()
        || row.intrinsic_catastrophic != catastrophic
    {
        return Err(format!(
            "M8 accepted row {} has invalid decision fields",
            row.group_id
        ));
    }
    Ok(())
}

pub(super) fn validate_formal_header(
    report: &M8CourtReport,
    scope: M8CourtScope,
) -> Result<(), String> {
    let valid_hex = |value: &str, width| {
        value.len() == width
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    let expected_groups = eligible_court_population(scope, M8_VARIANTS_PER_FAMILY)?.len() as u64;
    if report.schema != M8_COURT_SCHEMA
        || report.scope != scope
        || report.procedural_generation != M8_PROCEDURAL_GENERATION
        || report.variants_per_family != M8_VARIANTS_PER_FAMILY
        || report.cluster_size != M8_CLUSTER_SIZE
        || report.profile != scope.profile().as_str()
        || report.shard_index != 0
        || report.shard_count != M8_FORMAL_SHARDS
        || report.included_shards != (0..M8_FORMAL_SHARDS).collect::<Vec<_>>()
        || report.model_universe_sha256
            != vice_opt::model_universe_hash(&vice_opt::SupportedModelUniverseV1::m8())
        || report.exact_config_sha256 != exact_config_digest(&court_exact_config())
        || !valid_hex(&report.candidate_sha, 40)
        || !valid_hex(&report.runner_sha256, 64)
        || report.execution_ids.len() != M8_FORMAL_SHARDS as usize
        || report
            .execution_ids
            .iter()
            .any(|id| id.starts_with("UNATTESTED"))
        || report.execution_ids.iter().collect::<BTreeSet<_>>().len() != report.execution_ids.len()
        || report.source_groups != expected_groups
        || report.accepted_groups != report.rows.iter().filter(|row| row.accepted).count() as u64
    {
        return Err(
            "M8 formal court header, population, or execution attestation is invalid".into(),
        );
    }
    Ok(())
}

pub(super) fn court_exact_config() -> M8ExactConfig {
    let mut cfg = M8ExactConfig::default();
    cfg.alternation.max_rounds = 2;
    cfg.max_vertex_trials_per_round = 8;
    cfg
}

pub(super) fn court_cell(profile: RasterProfile, family: &str) -> DegradationCell {
    DegradationCell {
        size_px: if family == "dot_cluster" { 256 } else { 128 },
        subpixel_dx: 0.25,
        subpixel_dy: 0.375,
        profile,
        psf: Psf::Box,
        blend: vice_ir::BlendSpace::LinearLight,
        resize: ResizeChain::None,
        contrast: 1.0,
    }
}

fn parse_variant(id: &str) -> Result<usize, String> {
    id.rsplit('/')
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| format!("M8 procedural id has no variant: {id}"))
}

pub(super) fn shard_of(id: &str, shard_count: u32) -> u32 {
    let digest = Sha256::digest(id.as_bytes());
    let value = u64::from_le_bytes(digest[..8].try_into().expect("eight digest bytes"));
    (value % u64::from(shard_count)) as u32
}

pub(super) fn encode_png(width: u32, height: u32, rgba8: &[u8]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(rgba8).map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}

#[derive(Serialize)]
struct ExactConfigIdentity {
    likelihood: vice_opt::BlockLikelihoodConfig,
    alternation: vice_opt::AlternationConfig,
    vertex_step_px: f64,
    max_vertex_trials_per_round: usize,
}

pub(super) fn exact_config_digest(cfg: &M8ExactConfig) -> String {
    digest_json(&ExactConfigIdentity {
        likelihood: cfg.likelihood,
        alternation: cfg.alternation,
        vertex_step_px: cfg.vertex_step_px,
        max_vertex_trials_per_round: cfg.max_vertex_trials_per_round,
    })
}
