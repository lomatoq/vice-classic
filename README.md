# vice-classic

Классический (без обязательного AI) raster → SVG inverse rasterizer.
Единственный source of truth: `VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`
(SHA-256 `652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1`).

**Текущее состояние: M2 (certified partition renderer), автор остановлен
на gate.** M0 и M1 приняты независимыми review
([REVIEW_M0](docs/REVIEW_M0.md), [REVIEW_M1](docs/REVIEW_M1.md)). M2
прошёл два cold review и red-team pass, получил замечания и блокеры
([REVIEW_M2_A](docs/REVIEW_M2_A.md), [REVIEW_M2_B](docs/REVIEW_M2_B.md),
[REDTEAM_M2](docs/REDTEAM_M2.md)); их закрытие и текущий статус —
[docs/STATUS_M2.md](docs/STATUS_M2.md). **M3 не начат и не разрешён.**

## Что здесь есть

- `crates/vice-bench` (M0) — детерминированный baseline runner для трёх
  pinned донорских систем (`SOURCE_PINS.toml`): свежий checkout pin-SHA из
  локального зеркала, сборка, прогон по фиксированному smoke corpus,
  SHA-256 всех артефактов, typed errors, сравнение повторов.
- `crates/vice-geom` (M1/M2) — координатные конвенции, `Vec2`, robust
  predicates (adaptive-precision), certified curve flattening с
  chord-error бюджетом.
- `crates/vice-ir` (M1) — canonical IR: typed curve grammar, shared
  planar graph с exterior как настоящим `FaceId`, типизированная
  валидация §12-инвариантов, canonical bytes + sha256 (seal skeleton).
- `crates/vice-render` (M2) — certified partition renderer: точное
  signed-area покрытие на фиксированной tessellation, сертификация
  вложения/ориентации loops, premultiplied compositing, ROI с dependency
  closure, типизированный числовой домен, seal revalidation skeleton,
  independent differential court.
- `tests/fixtures/smoke/` — фиксированный smoke corpus (5 PNG, побайтово
  воспроизводится `gen-smoke`-ом, зафиксирован `SMOKE_MANIFEST.toml`).
- `configs/baselines.toml` — явные команды baseline-ов, лимиты ресурсов.
- Provenance: `SOURCE_PINS.toml`, `PORTING_MANIFEST.toml` (ноль
  перенесённых блоков), `THIRD_PARTY_NOTICES.md`.
- Governance: `REQUIREMENTS_TRACEABILITY.md`, `FAILURE_LEDGER.md`,
  `docs/ADR/`.

Чего здесь осознанно НЕТ: evidence, topology envelope, fitter, optimizer,
GT-корпус/scorecard, SVG-экспорт, UI/WASM/AI — они появляются только в
своих milestones.

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
