use super::*;

pub fn merge_courts(mut reports: Vec<M8CourtReport>) -> Result<M8CourtReport, String> {
    if reports.is_empty() {
        return Err("M8 merge has no inputs".into());
    }
    reports.sort_by_key(|report| report.shard_index);
    let first = reports.first().expect("nonempty").clone();
    let shard_count = first.shard_count;
    if reports.len() != shard_count as usize
        || reports.iter().enumerate().any(|(index, report)| {
            report.shard_index != index as u32
                || report.shard_count != shard_count
                || report.scope != first.scope
                || report.procedural_generation != first.procedural_generation
                || report.variants_per_family != first.variants_per_family
                || report.cluster_size != first.cluster_size
                || report.profile != first.profile
                || report.candidate_sha != first.candidate_sha
                || report.runner_sha256 != first.runner_sha256
                || report.model_universe_sha256 != first.model_universe_sha256
                || report.exact_config_sha256 != first.exact_config_sha256
        })
    {
        return Err("M8 shards are incomplete or identity-incompatible".into());
    }
    let mut rows = reports
        .iter_mut()
        .flat_map(|report| std::mem::take(&mut report.rows))
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| a.group_id.cmp(&b.group_id));
    if rows
        .windows(2)
        .any(|pair| pair[0].group_id == pair[1].group_id)
    {
        return Err("M8 shards contain duplicate source groups".into());
    }
    let execution_ids = reports
        .iter()
        .flat_map(|report| report.execution_ids.iter().cloned())
        .collect::<Vec<_>>();
    if execution_ids.len() != shard_count as usize
        || execution_ids.iter().collect::<BTreeSet<_>>().len() != execution_ids.len()
    {
        return Err("M8 shards do not carry distinct execution attestations".into());
    }
    let mut commitment = Sha256::new();
    commitment.update(b"vice-classic/m8-merged-court/v1");
    for report in &reports {
        commitment.update(report.corpus_commitment_sha256.as_bytes());
    }
    let source_groups = rows.len() as u64;
    let accepted_groups = rows.iter().filter(|row| row.accepted).count() as u64;
    Ok(M8CourtReport {
        schema: first.schema,
        scope: first.scope,
        procedural_generation: first.procedural_generation,
        variants_per_family: first.variants_per_family,
        cluster_size: first.cluster_size,
        profile: first.profile,
        shard_index: 0,
        shard_count,
        included_shards: (0..shard_count).collect(),
        candidate_sha: first.candidate_sha,
        runner_sha256: first.runner_sha256,
        execution_ids,
        model_universe_sha256: first.model_universe_sha256,
        exact_config_sha256: first.exact_config_sha256,
        corpus_commitment_sha256: hex::encode(commitment.finalize()),
        source_groups,
        accepted_groups,
        rows,
    })
}
