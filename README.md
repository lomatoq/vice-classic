# vice-classic

Классический (без обязательного AI) raster → SVG inverse rasterizer.
Единственный source of truth: `VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`
(SHA-256 `652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1`).

**Текущее состояние: M0 (bootstrap / provenance / baselines).**
См. [docs/STATUS_M0.md](docs/STATUS_M0.md). M1 не начат и не разрешён до
независимого clean-checkout review (`docs/REVIEW_M0.md`).

## Что здесь есть (M0)

- `crates/vice-bench` — детерминированный baseline runner для трёх pinned
  донорских систем (`SOURCE_PINS.toml`): свежий checkout pin-SHA из
  локального зеркала, сборка, прогон по фиксированному smoke corpus,
  SHA-256 всех артефактов, typed errors, сравнение повторов.
- `tests/fixtures/smoke/` — фиксированный smoke corpus (5 PNG, побайтово
  воспроизводится `gen-smoke`-ом, зафиксирован `SMOKE_MANIFEST.toml`).
- `configs/baselines.toml` — явные команды baseline-ов, лимиты ресурсов.
- Provenance: `SOURCE_PINS.toml`, `PORTING_MANIFEST.toml` (в M0 — ноль
  перенесённых блоков), `THIRD_PARTY_NOTICES.md`.
- Governance: `REQUIREMENTS_TRACEABILITY.md`, `FAILURE_LEDGER.md`,
  `docs/ADR/`.

Чего здесь осознанно НЕТ: canonical IR, renderer, evidence, topology,
fitter, optimizer, UI/WASM/AI — они появляются только в своих milestones.

## Быстрые команды

```bash
cargo test --workspace
```

```bash
cargo run --release --bin baseline-runner -- selftest --out runs/selftest --corpus tests/fixtures/smoke --manifest tests/fixtures/smoke/SMOKE_MANIFEST.toml
```

Полный baseline run (нужны локальные зеркала pinned-репозиториев):
см. [docs/REPRODUCIBILITY_M0.md](docs/REPRODUCIBILITY_M0.md).

## Лицензия

Не определена до отдельного license/IP review (донoры owner-controlled;
см. `THIRD_PARTY_NOTICES.md`). Не публиковать до этого review.
