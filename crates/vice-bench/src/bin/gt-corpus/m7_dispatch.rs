use super::*;

pub(super) fn run(command: Cmd) -> Result<i32, Box<Cmd>> {
    let code = match command {
        Cmd::M7Measure {
            out,
            scope,
            preset,
            size,
            workers,
            shard_index,
            shard_count,
            resume,
        } => {
            let mut request = m7::MeasurementRequest::new(scope.into());
            if let Some(preset) = preset {
                request.preset = preset.into();
            }
            request.size_filter = size;
            request.workers = workers.unwrap_or_else(m7::default_worker_count);
            request.shard_index = shard_index;
            request.shard_count = shard_count;
            let report = match m7::measure_to_path(request, &out, resume) {
                Ok(report) => report,
                Err(error) => {
                    eprintln!("error: {error}");
                    return Ok(2);
                }
            };
            println!(
                "M7 {} shards {:?}/{}: {} groups, {}/{} renders, {} selected candidates, {} \
                 truncated",
                report.scope,
                report.included_shards,
                report.shard_count,
                report.source_groups,
                report.renders,
                report.expected_renders_included_shards,
                report.candidates_available,
                report.truncated_renders
            );
            println!("M7 raw measurement: {}", out.display());
            0
        }
        Cmd::M7Merge { inputs, out } => {
            let mut reports = Vec::with_capacity(inputs.len());
            for input in &inputs {
                match m7::read_report(input) {
                    Ok(report) => reports.push(report),
                    Err(error) => {
                        eprintln!("error: {error}");
                        return Ok(2);
                    }
                }
            }
            let report = match m7::merge_reports(reports) {
                Ok(report) => report,
                Err(error) => {
                    eprintln!("error: {error}");
                    return Ok(2);
                }
            };
            if let Some(parent) = out.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    eprintln!("error: create {}: {error}", parent.display());
                    return Ok(2);
                }
            }
            if let Err(error) = m7::write_report(&out, &report) {
                eprintln!("error: {error}");
                return Ok(2);
            }
            println!(
                "M7 merged shards {:?}/{}: {}/{} renders; complete={}",
                report.included_shards,
                report.shard_count,
                report.renders,
                report.expected_renders_included_shards,
                report.complete
            );
            0
        }
        Cmd::M7Calibrate {
            report,
            audit_seal,
            out,
            production_config_out,
        } => {
            let report = match m7::read_report(&report) {
                Ok(report) => report,
                Err(error) => {
                    eprintln!("error: {error}");
                    return Ok(2);
                }
            };
            let audit: AuditSeal = match std::fs::read_to_string(&audit_seal)
                .map_err(|error| error.to_string())
                .and_then(|text| serde_json::from_str(&text).map_err(|error| error.to_string()))
            {
                Ok(audit) => audit,
                Err(error) => {
                    eprintln!("error: read {}: {error}", audit_seal.display());
                    return Ok(2);
                }
            };
            let analysis = match m7::analysis::analyze_calibration(&report, &audit) {
                Ok(analysis) => analysis,
                Err(error) => {
                    eprintln!("error: {error}");
                    return Ok(2);
                }
            };
            if let Some(parent) = out.parent() {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    eprintln!("error: create {}: {error}", parent.display());
                    return Ok(2);
                }
            }
            let text = serde_json::to_string_pretty(&analysis).expect("analysis serializes");
            if let Err(error) = std::fs::write(&out, format!("{text}\n")) {
                eprintln!("error: write {}: {error}", out.display());
                return Ok(2);
            }
            if let Some(config_out) = production_config_out {
                let Some(config) = &analysis.production_config else {
                    eprintln!(
                        "error: calibration did not produce a release-eligible production config"
                    );
                    return Ok(1);
                };
                if let Some(parent) = config_out.parent() {
                    if let Err(error) = std::fs::create_dir_all(parent) {
                        eprintln!("error: create {}: {error}", parent.display());
                        return Ok(2);
                    }
                }
                let bytes = serde_json::to_vec(config).expect("production config serializes");
                if let Err(error) = std::fs::write(&config_out, &bytes) {
                    eprintln!("error: write {}: {error}", config_out.display());
                    return Ok(2);
                }
                println!("M7 production config proposal: {}", config_out.display());
                println!(
                    "M7 production config sha256: {}",
                    vice_bench::hashing::sha256_hex(&bytes)
                );
            }
            println!(
                "M7 calibration: gate_met={}, threshold={:?}, unexplored_upper={}, runtime_p95={}ms",
                analysis.gate_met,
                analysis.selected_threshold,
                analysis.empirical_unexplored_relative_mass_upper_bound,
                analysis.runtime_p95_ms
            );
            for refusal in &analysis.refusals {
                eprintln!("M7 calibration refusal: {refusal}");
            }
            if analysis.gate_met {
                0
            } else {
                1
            }
        }
        Cmd::M7RunnerAttest {
            anchor_source,
            event_commit,
            repository_root,
            git_executable,
            vicec_executable,
            gates,
            gate_provenance,
            out,
        } => match m7_cmd::runner_attest(
            &anchor_source,
            &event_commit,
            &repository_root,
            &git_executable,
            &vicec_executable,
            &gates,
            &gate_provenance,
            &out,
        ) {
            Ok(attestation) => {
                println!(
                    "M7 runner attestation: commit={}, sha256={}, out={}",
                    attestation.event_commit_sha,
                    attestation.sha256().expect("attestation serializes"),
                    out.display()
                );
                0
            }
            Err(error) => {
                eprintln!("error: {error}");
                2
            }
        },
        Cmd::M7AuditOpen {
            governance,
            audit_seal,
            manifest,
            gates,
            note,
        } => match m7_cmd::open(
            &audit_seal,
            &manifest,
            &gates,
            &governance.runner_attestation,
            &governance.gate_provenance,
            &note,
        ) {
            Ok(seal) => {
                println!(
                    "M7 sealed audit generation {} opened and hash-bound: {}",
                    seal.generation,
                    audit_seal.display()
                );
                0
            }
            Err(error) => {
                eprintln!("error: {error}");
                2
            }
        },
        Cmd::M7AuditMeasure {
            governance,
            audit_seal,
            manifest,
            gates,
            production_config,
            preset,
            role,
            run_id,
            out,
            workers,
            shard_index,
            shard_count,
            resume,
        } => match m7_cmd::measure(
            &audit_seal,
            &manifest,
            &gates,
            &governance.runner_attestation,
            &governance.gate_provenance,
            &production_config,
            preset.into(),
            role.into(),
            &run_id,
            &out,
            workers,
            shard_index,
            shard_count,
            resume,
        ) {
            Ok(report) => {
                println!(
                    "M7 sealed audit shards {:?}/{}: {}/{} renders, {} production successes",
                    report.included_shards,
                    report.shard_count,
                    report.renders,
                    report.expected_renders_included_shards,
                    report
                        .rows
                        .iter()
                        .filter(|row| row.production_accepted)
                        .count()
                );
                println!("M7 sealed-audit measurement: {}", out.display());
                0
            }
            Err(error) => {
                eprintln!("error: {error}");
                2
            }
        },
        Cmd::M7AuditAnalyze {
            governance,
            audit_seal,
            manifest,
            gates,
            quality_report,
            fast_report,
            out,
        } => match m7_cmd::analyze(
            m7_cmd::GovernancePaths {
                seal: &audit_seal,
                manifest: &manifest,
                gates: &gates,
                runner_attestation: &governance.runner_attestation,
                gate_provenance: &governance.gate_provenance,
            },
            &quality_report,
            &fast_report,
            &out,
        ) {
            Ok(verdict) => {
                println!(
                    "M7 sealed verdict: gate_met={}, Quality coverage={:.3}/{:.3}, Fast \
                     coverage={:.3}/{:.3}",
                    verdict.gate_met,
                    verdict.quality.reliability.coverage_per_source,
                    verdict.quality.reliability.coverage_per_render,
                    verdict.fast.reliability.coverage_per_source,
                    verdict.fast.reliability.coverage_per_render,
                );
                println!("M7 release verdict: {}", out.display());
                if verdict.gate_met {
                    0
                } else {
                    1
                }
            }
            Err(error) => {
                eprintln!("error: {error}");
                2
            }
        },
        Cmd::M7Determinism {
            governance,
            audit_seal,
            manifest,
            gates,
            fast_parallel,
            fast_primary,
            fast_repeat,
            quality_parallel,
            quality_primary,
            quality_repeat,
            out,
        } => match m7_cmd::determinism(
            m7_cmd::GovernancePaths {
                seal: &audit_seal,
                manifest: &manifest,
                gates: &gates,
                runner_attestation: &governance.runner_attestation,
                gate_provenance: &governance.gate_provenance,
            },
            &[
                (m7::M7RunRole::FastParallel, fast_parallel),
                (m7::M7RunRole::FastPrimary, fast_primary),
                (m7::M7RunRole::FastRepeat, fast_repeat),
                (m7::M7RunRole::QualityParallel, quality_parallel),
                (m7::M7RunRole::QualityPrimary, quality_primary),
                (m7::M7RunRole::QualityRepeat, quality_repeat),
            ],
            &out,
        ) {
            Ok(verdict) => {
                for preset in &verdict.presets {
                    println!(
                        "M7 {:?} determinism: isolated={}, parallel={}, equal={}",
                        preset.preset,
                        preset.isolated_repeats,
                        preset.parallel_runs,
                        preset.all_normalized_bytes_equal
                    );
                }
                println!("M7 determinism artifact: {}", out.display());
                if verdict.gate_met {
                    0
                } else {
                    1
                }
            }
            Err(error) => {
                eprintln!("error: {error}");
                2
            }
        },
        Cmd::M7BaselineCourt {
            governance,
            audit_seal,
            manifest,
            gates,
            quality_report,
            fast_report,
            out,
        } => match m7_cmd::baseline_court(
            m7_cmd::GovernancePaths {
                seal: &audit_seal,
                manifest: &manifest,
                gates: &gates,
                runner_attestation: &governance.runner_attestation,
                gate_provenance: &governance.gate_provenance,
            },
            &quality_report,
            &fast_report,
            &out,
        ) {
            Ok(verdict) => {
                println!(
                    "M7 baseline/blind court: gate_met={}, Quality wins={}/{}, Fast wins={}/{}",
                    verdict.gate_met,
                    verdict.quality.blind.selected_wins,
                    verdict.quality.blind.non_tied_trials,
                    verdict.fast.blind.selected_wins,
                    verdict.fast.blind.non_tied_trials,
                );
                println!("M7 baseline/blind artifact: {}", out.display());
                if verdict.gate_met {
                    0
                } else {
                    1
                }
            }
            Err(error) => {
                eprintln!("error: {error}");
                2
            }
        },
        Cmd::M7Oracle {
            governance,
            audit_seal,
            manifest,
            gates,
            quality_report,
            fast_report,
            out,
        } => match m7_cmd::oracle(
            m7_cmd::GovernancePaths {
                seal: &audit_seal,
                manifest: &manifest,
                gates: &gates,
                runner_attestation: &governance.runner_attestation,
                gate_provenance: &governance.gate_provenance,
            },
            &quality_report,
            &fast_report,
            &out,
        ) {
            Ok(verdict) => {
                println!(
                    "M7 oracle: gate_met={}, PF(Q/F)={}/{}, geometry={}, G20 recovery={:.3}, \
                     G30 recovery={:.3}",
                    verdict.gate_met,
                    verdict.quality_pf.complete_rows,
                    verdict.fast_pf.complete_rows,
                    verdict.geometry.complete_six_arm_rows,
                    verdict.geometry.g20_recovery.recovery_rate,
                    verdict.geometry.g30_recovery.recovery_rate,
                );
                println!("M7 complete oracle artifact: {}", out.display());
                if verdict.gate_met {
                    0
                } else {
                    1
                }
            }
            Err(error) => {
                eprintln!("error: {error}");
                2
            }
        },
        Cmd::M7OracleGeometryCalibrate { out } => match m7_cmd::geometry_calibrate(&out) {
            Ok(measurements) => {
                let g20 = measurements
                    .recovery
                    .iter()
                    .filter(|row| row.mode == "G20" && row.normal_objective_recovered)
                    .count();
                let g30 = measurements
                    .recovery
                    .iter()
                    .filter(|row| row.mode == "G30" && row.normal_objective_recovered)
                    .count();
                println!(
                    "M7 geometry calibration: six-arm rows={}, G20 recovered={}, G30 recovered={}",
                    measurements.complete_six_arm_rows, g20, g30
                );
                println!("M7 geometry calibration artifact: {}", out.display());
                0
            }
            Err(error) => {
                eprintln!("error: {error}");
                2
            }
        },
        Cmd::M7CanonicalArtifact {
            release,
            baseline,
            oracle,
            determinism,
            out,
        } => match m7_cmd::canonical_artifact(&release, &baseline, &oracle, &determinism, &out) {
            Ok(artifact) => {
                println!(
                    "M7 canonical artifact: commit={}, components={}, out={}",
                    artifact.release_commit_sha,
                    artifact.components.len(),
                    out.display()
                );
                0
            }
            Err(error) => {
                eprintln!("error: {error}");
                2
            }
        },
        other => return Err(Box::new(other)),
    };
    Ok(code)
}
