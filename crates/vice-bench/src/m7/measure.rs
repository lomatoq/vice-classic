use super::*;

pub fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map_or(1, usize::from)
        // One Quality run may approach the 1 GiB per-process research
        // envelope. Two workers provide useful parallelism without making an
        // unsafe aggregate-memory promise.
        .min(2)
}

#[derive(Clone, Copy)]
struct MeasurementJob {
    group: usize,
    scene: usize,
    cell: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MeasurementJournalHeader {
    schema: String,
    scope: String,
    split: String,
    preset: Preset,
    procedural_generation: u32,
    population_policy: String,
    population_commitment_sha256: String,
    procedural_variants_per_family: usize,
    mandatory_sizes_px: Vec<u32>,
    rasterizers: Vec<String>,
    identity: vice_opt::ModelIdentity,
    delivery_policy_sha256: String,
    confidence_calibration: Option<vice_core::ConfidenceCalibration>,
    shard_index: u32,
    shard_count: u32,
    execution: Option<MeasurementExecutionContext>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
enum MeasurementJournalRecord {
    Header {
        header: Box<MeasurementJournalHeader>,
    },
    Row {
        row: Box<MeasurementRow>,
    },
}

pub fn measure(request: MeasurementRequest) -> Result<MeasurementReport, String> {
    let config = CoreConfig::development_for(request.preset);
    measure_with_config(request, &config)
}

pub fn measure_with_config(
    request: MeasurementRequest,
    config: &CoreConfig,
) -> Result<MeasurementReport, String> {
    measure_resuming(request, config, ResumeState::default(), |_| Ok(()))
}

pub fn measure_to_path(
    request: MeasurementRequest,
    out: &Path,
    resume: bool,
) -> Result<MeasurementReport, String> {
    let config = CoreConfig::development_for(request.preset);
    measure_to_path_with_config(request, &config, out, resume)
}

pub fn measure_to_path_with_config(
    request: MeasurementRequest,
    config: &CoreConfig,
    out: &Path,
    resume: bool,
) -> Result<MeasurementReport, String> {
    request.validate()?;
    let journal = journal_path(out);
    if !resume && (out.exists() || journal.exists()) {
        return Err(format!(
            "{} or its checkpoint journal already exists; pass --resume or choose a new output",
            out.display()
        ));
    }

    let expected_header = journal_header(&request, config)?;
    let mut rows = Vec::new();
    let mut previous_elapsed_ms = 0;
    let mut previous_runs = 0;
    let mut previous_max_workers = 0;
    let mut previous_peak_working_set_bytes = 0;
    if resume && out.exists() {
        let previous = read_report(out)?;
        validate_report_header(&previous, &request, &expected_header)?;
        previous_elapsed_ms = previous.elapsed_ms;
        previous_runs = previous.runs;
        previous_max_workers = previous.max_workers_per_shard;
        previous_peak_working_set_bytes = previous.peak_working_set_bytes;
        rows.extend(previous.rows);
    }
    if resume && journal.exists() {
        let (header, journal_rows) = read_journal(&journal)?;
        if header != expected_header {
            return Err(format!(
                "checkpoint {} belongs to a different M7 measurement identity or shard",
                journal.display()
            ));
        }
        rows.extend(journal_rows);
    }

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    let journal_exists = journal.exists();
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&journal)
        .map_err(|error| format!("open checkpoint {}: {error}", journal.display()))?;
    let mut writer = BufWriter::new(file);
    if !journal_exists {
        write_journal_record(
            &mut writer,
            &MeasurementJournalRecord::Header {
                header: Box::new(expected_header),
            },
        )?;
    }
    let mut report = measure_resuming(
        request.clone(),
        config,
        ResumeState {
            rows,
            previous_elapsed_ms,
            previous_runs,
            previous_max_workers,
            previous_peak_working_set_bytes,
        },
        |row| {
            write_journal_record(
                &mut writer,
                &MeasurementJournalRecord::Row {
                    row: Box::new(row.clone()),
                },
            )
        },
    )?;
    writer
        .flush()
        .map_err(|error| format!("flush checkpoint {}: {error}", journal.display()))?;
    if let Some(context) = request.execution.clone() {
        let bytes = std::fs::read(&journal)
            .map_err(|error| format!("read checkpoint {}: {error}", journal.display()))?;
        attach_execution_attestation(&mut report, context, hex::encode(Sha256::digest(bytes)))?;
    }
    write_report(out, &report)?;
    Ok(report)
}

#[derive(Default)]
struct ResumeState {
    rows: Vec<MeasurementRow>,
    previous_elapsed_ms: u64,
    previous_runs: u32,
    previous_max_workers: u32,
    previous_peak_working_set_bytes: u64,
}

fn measure_resuming(
    request: MeasurementRequest,
    config: &CoreConfig,
    resume: ResumeState,
    mut checkpoint: impl FnMut(&MeasurementRow) -> Result<(), String>,
) -> Result<MeasurementReport, String> {
    let ResumeState {
        rows: resume_rows,
        previous_elapsed_ms,
        previous_runs,
        previous_max_workers,
        previous_peak_working_set_bytes,
    } = resume;
    request.validate()?;
    let started = Instant::now();
    let peak_memory = PeakWorkingSetMonitor::start()?;
    let scope = request.scope;
    if (scope == MeasurementScope::SealedAudit) != config.is_sealed_production() {
        return Err(
            "sealed-audit measurement requires a digest-pinned production config, while \
             development/calibration measurement requires an unsealed config"
                .into(),
        );
    }
    // Sharding is a function of the stable group ID, so select the shard
    // before constructing/certifying procedural scenes. This preserves the
    // exact population while bounding each worker to its own groups.
    let groups = groups_with_variants_filtered_for_generation(
        scope.variants(),
        M7_PROCEDURAL_GENERATION,
        |group_id| {
            measurement_shard(group_id, request.shard_count) == request.shard_index
                && (scope != MeasurementScope::CalibrationSmoke || group_id == "proc/annulus/000")
        },
    )?;
    let cells = scope
        .cells()
        .into_iter()
        .filter(|cell| request.size_filter.is_none_or(|size| cell.size_px == size))
        .collect::<Vec<_>>();
    if cells.is_empty() {
        return Err("M7 measurement selected no degradation cells".into());
    }
    let split = scope.split();
    let preset = request.preset;
    let identity = config.identity();
    let mut jobs = Vec::new();
    let mut source_groups = BTreeSet::new();
    for (group_index, group) in groups.iter().enumerate() {
        if SPLIT_POLICY_V1.split_of_group(group) != split || !scope.admits_group(group)? {
            continue;
        }
        source_groups.insert(group.id.as_str());
        for scene_index in 0..group.scenes.len() {
            for cell_index in 0..cells.len() {
                jobs.push(MeasurementJob {
                    group: group_index,
                    scene: scene_index,
                    cell: cell_index,
                });
            }
        }
    }
    let expected_keys = jobs
        .iter()
        .map(|job| {
            row_key_parts(
                &groups[job.group].id,
                groups[job.group].scenes[job.scene].id(),
                &cells[job.cell].id(),
            )
        })
        .collect::<BTreeSet<_>>();
    let mut rows_by_key = BTreeMap::new();
    for row in resume_rows {
        let key = row_key(&row);
        if !expected_keys.contains(&key) {
            return Err(format!(
                "resume row {key:?} is outside requested M7 shard/cell population"
            ));
        }
        match rows_by_key.insert(key.clone(), row.clone()) {
            Some(previous) if previous != row => {
                return Err(format!("conflicting resume rows for {key:?}"))
            }
            _ => {}
        }
    }
    let resumed_rows = rows_by_key.len() as u64;
    let pending = jobs
        .iter()
        .copied()
        .filter(|job| {
            !rows_by_key.contains_key(&row_key_parts(
                &groups[job.group].id,
                groups[job.group].scenes[job.scene].id(),
                &cells[job.cell].id(),
            ))
        })
        .collect::<Vec<_>>();
    let worker_count = request.workers.min(pending.len().max(1));
    if !pending.is_empty() {
        let next = AtomicUsize::new(0);
        let (sender, receiver) = mpsc::channel();
        let mut checkpoint_error = None;
        std::thread::scope(|threads| {
            for _ in 0..worker_count {
                let sender = sender.clone();
                let next = &next;
                let pending = &pending;
                let groups = &groups;
                let cells = &cells;
                threads.spawn(move || loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = pending.get(index) else {
                        break;
                    };
                    let group = &groups[job.group];
                    let scene = &group.scenes[job.scene];
                    let equivalence_members = group
                        .equivalence_class
                        .as_ref()
                        .map_or(1, |class| class.members.len());
                    let row = measure_one(
                        group.id.as_str(),
                        group.shape_family.as_str(),
                        scene,
                        &cells[job.cell],
                        equivalence_members,
                        config,
                        MeasurementExecution {
                            preset,
                            capture_baseline: scope == MeasurementScope::SealedAudit,
                        },
                    );
                    if sender.send(row).is_err() {
                        break;
                    }
                });
            }
            drop(sender);
            for row in receiver {
                let key = row_key(&row);
                if checkpoint_error.is_none() {
                    if let Err(error) = checkpoint(&row) {
                        checkpoint_error = Some(error);
                    }
                }
                rows_by_key.insert(key, row);
            }
        });
        if let Some(error) = checkpoint_error {
            return Err(error);
        }
    }
    let mut rows = rows_by_key.into_values().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        (
            left.group_id.as_str(),
            left.scene_id.as_str(),
            left.cell_id.as_str(),
        )
            .cmp(&(
                right.group_id.as_str(),
                right.scene_id.as_str(),
                right.cell_id.as_str(),
            ))
    });
    let complete = rows.len() == jobs.len();
    let candidates_available = rows.iter().filter(|row| row.candidate_available).count() as u64;
    let truncated_renders = rows
        .iter()
        .filter(|row| row.search_truncated == Some(true))
        .count() as u64;
    let peak_working_set_bytes = previous_peak_working_set_bytes.max(peak_memory.finish());
    let mut report = MeasurementReport {
        schema: M7_MEASUREMENT_SCHEMA.to_string(),
        scope: scope.as_str().to_string(),
        split: split.as_str().to_string(),
        preset,
        procedural_generation: M7_PROCEDURAL_GENERATION,
        population_policy: scope.population_policy().to_string(),
        population_commitment_sha256: population_commitment(scope),
        procedural_variants_per_family: scope.variants(),
        mandatory_sizes_px: {
            let mut sizes = cells.iter().map(|cell| cell.size_px).collect::<Vec<_>>();
            sizes.sort_unstable();
            sizes.dedup();
            sizes
        },
        rasterizers: {
            let mut rasterizers = cells
                .iter()
                .map(|cell| cell.profile.as_str().to_string())
                .collect::<Vec<_>>();
            rasterizers.sort();
            rasterizers.dedup();
            rasterizers
        },
        identity,
        delivery_policy_sha256: config.delivery_policy_sha256(),
        confidence_calibration: config.confidence.clone(),
        included_shards: vec![request.shard_index],
        shard_count: request.shard_count,
        max_workers_per_shard: previous_max_workers
            .max(worker_count.try_into().unwrap_or(u32::MAX)),
        complete,
        expected_renders_included_shards: jobs.len().try_into().unwrap_or(u64::MAX),
        resumed_rows,
        runs: previous_runs.saturating_add(1),
        renders: rows.len() as u64,
        rows,
        source_groups: source_groups.len().try_into().unwrap_or(u64::MAX),
        candidates_available,
        truncated_renders,
        elapsed_ms: previous_elapsed_ms
            .saturating_add(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)),
        peak_working_set_bytes,
        execution_attestation: None,
    };
    if let Some(context) = request.execution {
        let evidence = rows_commitment_sha256(&report);
        attach_execution_attestation(&mut report, context, evidence)?;
    }
    Ok(report)
}

struct PeakWorkingSetMonitor {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}

impl PeakWorkingSetMonitor {
    fn start() -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU64::new(0));
        let pid = sysinfo::get_current_pid().map_err(|error| error.to_string())?;
        let worker_stop = Arc::clone(&stop);
        let worker_peak = Arc::clone(&peak);
        let worker = std::thread::spawn(move || {
            let mut system = sysinfo::System::new();
            while !worker_stop.load(Ordering::Relaxed) {
                system.refresh_processes_specifics(
                    sysinfo::ProcessesToUpdate::Some(&[pid]),
                    true,
                    sysinfo::ProcessRefreshKind::nothing().with_memory(),
                );
                if let Some(process) = system.process(pid) {
                    worker_peak.fetch_max(process.memory(), Ordering::Relaxed);
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        });
        Ok(Self {
            stop,
            peak,
            worker: Some(worker),
        })
    }

    fn finish(mut self) -> u64 {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.peak.load(Ordering::Relaxed)
    }
}

impl Drop for PeakWorkingSetMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn row_key(row: &MeasurementRow) -> String {
    row_key_parts(&row.group_id, &row.scene_id, &row.cell_id)
}

fn row_key_parts(group_id: &str, scene_id: &str, cell_id: &str) -> String {
    format!("{group_id}\0{scene_id}\0{cell_id}")
}

pub(super) fn measurement_shard(group_id: &str, shard_count: u32) -> u32 {
    let digest = Sha256::digest(
        [
            b"vice-classic/m7-source-shard/v1/".as_slice(),
            group_id.as_bytes(),
        ]
        .concat(),
    );
    u64::from_be_bytes(digest[..8].try_into().expect("sha256 has eight bytes"))
        .rem_euclid(u64::from(shard_count)) as u32
}

pub(super) fn preset_for_scope(scope: MeasurementScope) -> Preset {
    if scope == MeasurementScope::Smoke {
        Preset::Fast
    } else {
        Preset::Quality
    }
}

fn journal_header(
    request: &MeasurementRequest,
    config: &CoreConfig,
) -> Result<MeasurementJournalHeader, String> {
    let cells = request
        .scope
        .cells()
        .into_iter()
        .filter(|cell| request.size_filter.is_none_or(|size| cell.size_px == size))
        .collect::<Vec<_>>();
    if cells.is_empty() {
        return Err("M7 measurement selected no degradation cells".into());
    }
    let mut sizes = cells.iter().map(|cell| cell.size_px).collect::<Vec<_>>();
    sizes.sort_unstable();
    sizes.dedup();
    let mut rasterizers = cells
        .iter()
        .map(|cell| cell.profile.as_str().to_string())
        .collect::<Vec<_>>();
    rasterizers.sort();
    rasterizers.dedup();
    Ok(MeasurementJournalHeader {
        schema: M7_MEASUREMENT_SCHEMA.to_string(),
        scope: request.scope.as_str().to_string(),
        split: request.scope.split().as_str().to_string(),
        preset: request.preset,
        procedural_generation: M7_PROCEDURAL_GENERATION,
        population_policy: request.scope.population_policy().to_string(),
        population_commitment_sha256: population_commitment(request.scope),
        procedural_variants_per_family: request.scope.variants(),
        mandatory_sizes_px: sizes,
        rasterizers,
        identity: config.identity(),
        delivery_policy_sha256: config.delivery_policy_sha256(),
        confidence_calibration: config.confidence.clone(),
        shard_index: request.shard_index,
        shard_count: request.shard_count,
        execution: request.execution.clone(),
    })
}

fn validate_report_header(
    report: &MeasurementReport,
    request: &MeasurementRequest,
    expected: &MeasurementJournalHeader,
) -> Result<(), String> {
    let matches = report.schema == expected.schema
        && report.scope == expected.scope
        && report.split == expected.split
        && report.preset == expected.preset
        && report.procedural_generation == expected.procedural_generation
        && report.population_policy == expected.population_policy
        && report.population_commitment_sha256 == expected.population_commitment_sha256
        && report.procedural_variants_per_family == expected.procedural_variants_per_family
        && report.mandatory_sizes_px == expected.mandatory_sizes_px
        && report.rasterizers == expected.rasterizers
        && report.identity == expected.identity
        && report.delivery_policy_sha256 == expected.delivery_policy_sha256
        && report.confidence_calibration == expected.confidence_calibration
        && report.included_shards == [request.shard_index]
        && report.shard_count == request.shard_count
        && report
            .execution_attestation
            .as_ref()
            .map(|attestation| &attestation.context)
            == expected.execution.as_ref();
    if matches {
        Ok(())
    } else {
        Err("existing M7 report belongs to a different identity, scope, cell set, or shard".into())
    }
}

fn population_commitment(scope: MeasurementScope) -> String {
    if scope == MeasurementScope::SealedAudit {
        return M7_SUCCESSOR_POPULATION_COMMITMENT_SHA256.into();
    }
    let bytes = serde_json::to_vec(&(
        scope.as_str(),
        scope.population_policy(),
        M7_PROCEDURAL_GENERATION,
        scope.variants(),
        scope
            .cells()
            .iter()
            .map(DegradationCell::id)
            .collect::<Vec<_>>(),
    ))
    .expect("M7 development population serializes");
    hex::encode(Sha256::digest(bytes))
}

fn journal_path(out: &Path) -> PathBuf {
    let mut name = out.as_os_str().to_os_string();
    name.push(".rows.jsonl");
    PathBuf::from(name)
}

fn write_journal_record(
    writer: &mut BufWriter<File>,
    record: &MeasurementJournalRecord,
) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, record)
        .map_err(|error| format!("serialize M7 checkpoint: {error}"))?;
    writer
        .write_all(b"\n")
        .and_then(|_| writer.flush())
        .map_err(|error| format!("write M7 checkpoint: {error}"))
}

fn read_journal(path: &Path) -> Result<(MeasurementJournalHeader, Vec<MeasurementRow>), String> {
    let file = File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let mut header = None;
    let mut rows = Vec::new();
    for (line_index, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|error| format!("read {}: {error}", path.display()))?;
        let record: MeasurementJournalRecord = serde_json::from_str(&line).map_err(|error| {
            format!("parse {} line {}: {error}", path.display(), line_index + 1)
        })?;
        match record {
            MeasurementJournalRecord::Header { header: found } if line_index == 0 => {
                header = Some(*found)
            }
            MeasurementJournalRecord::Header { .. } => {
                return Err(format!("{} contains more than one header", path.display()))
            }
            MeasurementJournalRecord::Row { row } => rows.push(*row),
        }
    }
    header
        .map(|header| (header, rows))
        .ok_or_else(|| format!("{} has no M7 checkpoint header", path.display()))
}

pub fn read_report(path: &Path) -> Result<MeasurementReport, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse {}: {error}", path.display()))
}

pub fn write_report(path: &Path, report: &MeasurementReport) -> Result<(), String> {
    let text = serde_json::to_string_pretty(report)
        .map_err(|error| format!("serialize report: {error}"))?;
    std::fs::write(path, format!("{text}\n"))
        .map_err(|error| format!("write {}: {error}", path.display()))
}

pub fn report_content_sha256(report: &MeasurementReport) -> String {
    let bytes = serde_json::to_vec(report).expect("M7 report serializes");
    hex::encode(Sha256::digest(bytes))
}

pub fn merge_reports(reports: Vec<MeasurementReport>) -> Result<MeasurementReport, String> {
    let Some(first) = reports.first().cloned() else {
        return Err("M7 merge requires at least one report".into());
    };
    let compatible = |report: &MeasurementReport| {
        report.schema == first.schema
            && report.scope == first.scope
            && report.split == first.split
            && report.preset == first.preset
            && report.procedural_generation == first.procedural_generation
            && report.population_policy == first.population_policy
            && report.population_commitment_sha256 == first.population_commitment_sha256
            && report.procedural_variants_per_family == first.procedural_variants_per_family
            && report.mandatory_sizes_px == first.mandatory_sizes_px
            && report.rasterizers == first.rasterizers
            && report.identity == first.identity
            && report.delivery_policy_sha256 == first.delivery_policy_sha256
            && report.confidence_calibration == first.confidence_calibration
            && report.shard_count == first.shard_count
            && report
                .execution_attestation
                .as_ref()
                .map(|attestation| &attestation.context)
                == first
                    .execution_attestation
                    .as_ref()
                    .map(|attestation| &attestation.context)
    };
    if reports.iter().any(|report| !compatible(report)) {
        return Err(
            "M7 merge inputs disagree on schema, identity, population, cell set, or shard count"
                .into(),
        );
    }

    let mut included_shards = BTreeSet::new();
    let mut rows = BTreeMap::new();
    let mut source_groups = 0u64;
    let mut expected_renders = 0u64;
    let mut resumed_rows = 0u64;
    let mut runs = 0u32;
    let mut max_workers = 0u32;
    let mut elapsed_ms = 0u64;
    let mut peak_working_set_bytes = 0u64;
    let mut inputs_complete = true;
    let mut input_report_hashes = Vec::new();
    for report in reports {
        if let Some(attestation) = &report.execution_attestation {
            validate_execution_attestation(&report)?;
            input_report_hashes.push(attestation.report_sha256.clone());
        }
        for shard in report.included_shards {
            if !included_shards.insert(shard) {
                return Err(format!("M7 merge includes shard {shard} more than once"));
            }
        }
        for row in report.rows {
            let key = row_key(&row);
            match rows.insert(key.clone(), row.clone()) {
                Some(previous) if previous != row => {
                    return Err(format!("M7 merge has conflicting rows for {key:?}"))
                }
                Some(_) => return Err(format!("M7 merge duplicates row {key:?}")),
                None => {}
            }
        }
        source_groups = source_groups.saturating_add(report.source_groups);
        expected_renders = expected_renders.saturating_add(report.expected_renders_included_shards);
        resumed_rows = resumed_rows.saturating_add(report.resumed_rows);
        runs = runs.saturating_add(report.runs);
        max_workers = max_workers.max(report.max_workers_per_shard);
        elapsed_ms = elapsed_ms.saturating_add(report.elapsed_ms);
        peak_working_set_bytes = peak_working_set_bytes.max(report.peak_working_set_bytes);
        inputs_complete &= report.complete;
    }
    let rows = rows.into_values().collect::<Vec<_>>();
    let candidates_available = rows.iter().filter(|row| row.candidate_available).count() as u64;
    let truncated_renders = rows
        .iter()
        .filter(|row| row.search_truncated == Some(true))
        .count() as u64;
    let all_shards_present = included_shards.len() == first.shard_count as usize
        && included_shards.iter().copied().eq(0..first.shard_count);
    let execution_context = first
        .execution_attestation
        .as_ref()
        .map(|attestation| attestation.context.clone());
    let mut merged = MeasurementReport {
        schema: first.schema.clone(),
        scope: first.scope.clone(),
        split: first.split.clone(),
        preset: first.preset,
        procedural_generation: first.procedural_generation,
        population_policy: first.population_policy.clone(),
        population_commitment_sha256: first.population_commitment_sha256.clone(),
        procedural_variants_per_family: first.procedural_variants_per_family,
        mandatory_sizes_px: first.mandatory_sizes_px.clone(),
        rasterizers: first.rasterizers.clone(),
        identity: first.identity.clone(),
        delivery_policy_sha256: first.delivery_policy_sha256.clone(),
        confidence_calibration: first.confidence_calibration.clone(),
        included_shards: included_shards.into_iter().collect(),
        shard_count: first.shard_count,
        max_workers_per_shard: max_workers,
        complete: inputs_complete && all_shards_present && rows.len() as u64 == expected_renders,
        expected_renders_included_shards: expected_renders,
        resumed_rows,
        runs,
        renders: rows.len() as u64,
        rows,
        source_groups,
        candidates_available,
        truncated_renders,
        elapsed_ms,
        peak_working_set_bytes,
        execution_attestation: None,
    };
    if let Some(context) = execution_context {
        input_report_hashes.sort();
        let evidence = hex::encode(Sha256::digest(
            serde_json::to_vec(&input_report_hashes).expect("input commitments serialize"),
        ));
        attach_execution_attestation(&mut merged, context, evidence)?;
    }
    Ok(merged)
}
