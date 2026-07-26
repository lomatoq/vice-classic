# REQUIREMENTS_TRACEABILITY — vice-classic

Формат (spec v1.3 §32 правило 23): invariant → implementation → tests →
milestone gate. Пока покрыт только M0.

| # | Требование (M0, spec §28 + bootstrap prompt) | Реализация | Тесты / проверка | Gate |
|---|---|---|---|---|
| M0-1 | Workspace только из реально нужных M0 crates, без placeholder-ов | `Cargo.toml` (единственный member `crates/vice-bench`), ADR-0001 | `cargo build --workspace`; ревью структуры | STATUS_M0 T1 |
| M0-2 | `SOURCE_PINS.toml`, `PORTING_MANIFEST.toml` (0 units), `THIRD_PARTY_NOTICES.md` | одноимённые файлы в корне | ревью; манифест пуст ⇔ ни одной копии донорского кода | STATUS_M0 T2 |
| M0-3 | `REQUIREMENTS_TRACEABILITY.md`, `FAILURE_LEDGER.md`, `docs/ADR/` | этот файл; `FAILURE_LEDGER.md` (3 записи); ADR-0001..0003 | ревью | STATUS_M0 T3 |
| M0-4 | Пин toolchain, committed `Cargo.lock`, версии инструментов, env manifest | `rust-toolchain.toml` (1.96.0), `Cargo.lock`, `envinfo.rs` → `env.json` + `environment_sha256` в каждом отчёте | `selftest_pipeline_is_deterministic` (env hash в отчёте) | STATUS_M0 T4 |
| M0-5 | Deterministic baseline runner на фиксированном smoke corpus | `runner.rs` (fresh clone+detach пина, build, run, repeats), `configs/baselines.toml`, corpus `tests/fixtures/smoke/` | 12 unit + 5 integration тестов; selftest; двойной прогон | STATUS_M0 T5 |
| M0-6 | Hashes binary/source/config/input/toolchain/env + runtime + exit status + artifacts | `report.rs`, `hashing.rs`; `report.json`/`hashes.json` | `selftest_pipeline_is_deterministic`; ручная сверка hashes.json двух прогонов | STATUS_M0 T6 |
| M0-7 | CI: format, clippy, tests, clean-checkout smoke | `.github/workflows/ci.yml` | локальный эквивалент прогнан (fmt/clippy/test/selftest/verify-corpus/gen-smoke --check) | STATUS_M0 T7 |
| M0-8 | Input resource limits + typed baseline errors | `limits.rs` (size/IHDR/timeout/output-cap), `error.rs` | unit-тесты limits (bomb-заголовок 1e9×1e9, oversize, bad sig); `missing_mirror_is_isolated_typed_failure`; `corrupted_corpus_aborts_run` | STATUS_M0 T8 |
| M0-9 | Изоляция сбоя одного baseline от отчётов остальных | `BaselineCtx::execute` (typed error → status=failed, цикл продолжается) | `missing_mirror_is_isolated_typed_failure` (оба baseline в отчёте) | STATUS_M0 T9 |
| M0-10 | Corpus/commands/outputs зафиксированы; честная фиксация nondeterminism | `SMOKE_MANIFEST.toml`; commands в `baselines.toml` + `report.json`; determinism-record по повторам; `docs/baselines/M0/` | `generated_corpus_verifies`; сравнение повторов в записанном прогоне | STATUS_M0 T10 |
| M0-11 | `docs/STATUS_M0.md` с gate table + STOP; независимый review обязателен | `docs/STATUS_M0.md` | ревью | STATUS_M0 |
