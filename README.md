# vice-classic

Классический (без обязательного AI) raster → SVG inverse rasterizer.
Единственный source of truth: `VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`
(SHA-256 `652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1`).

**Текущее состояние: M4.5 (cubical event topology envelope), автор
остановлен на gate; ждёт независимого cold review.**

Приняты независимыми review: M0, M1 ([REVIEW_M0](docs/REVIEW_M0.md),
[REVIEW_M1](docs/REVIEW_M1.md)); M2 — двумя review плюс отдельный
red-team pass: A дал REJECT и затем ACCEPT в двух addendum-ах, B дал
ACCEPT ([REVIEW_M2_A](docs/REVIEW_M2_A.md),
[REVIEW_M2_B](docs/REVIEW_M2_B.md), [REDTEAM_M2](docs/REDTEAM_M2.md)); M3 — ACCEPT в addendum-е после REJECT
([REVIEW_M3](docs/REVIEW_M3.md)); M3.5 — ACCEPT
([REVIEW_M3_5](docs/REVIEW_M3_5.md)).

M4 получил от независимого cold review **REJECT** с тремя блокерами и
**ACCEPT** в addendum-е после дельты ([docs/REVIEW_M4.md](docs/REVIEW_M4.md)),
с условиями D1 и D2 к следующему гейту. Оба закрыты в M4.5.

M4.5 остановлен на своём гейте: `docs/STATUS_M4_5.md`. Автор НЕ
самосертифицирует (§32 п. 29, §34), поэтому **M5 не начат и не разрешён**.

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
- `crates/vice-image` (M4) — canonical decode и premultiplied observation
  tensor как ФУНКЦИЯ гипотезы blend space; квантовый интервал едет вместе
  с тензором как граница.
- `crates/vice-evidence` (M4) — interior confidence, несколько
  Flat2-гипотез палитры/exterior, минимальное global formation family,
  premultiplied mixture, §1.6-детектор, boundary observations и corridor.
  Ничто из этого не является вторым pixel likelihood, и это выражено
  типом, а не комментарием.
- `crates/vice-topology` (M4.5) — скалярные поля §11.1, cubical complex с
  комплементарной связностью, критические события max/min-деревьев как
  ПАРТИИ по равным значениям, saddle-альтернативы, кандидатный конверт с
  тремя уровнями pruning и dual/primal continuation, честная относительно
  того, чего без DCEL сделать нельзя. Ничто здесь не выбирает победителя:
  §11.3 требует конверта, и гейт мерит RECALL.
- `crates/vice-cli` (M4) — `vicec evidence`: исполняемый путь милестоуна.
  `vicec vectorize` НЕ объявлен, потому что за ним пока нет ни топологии,
  ни фиттера.
- `tests/fixtures/smoke/` — фиксированный smoke corpus (5 PNG, побайтово
  воспроизводится `gen-smoke`-ом, зафиксирован `SMOKE_MANIFEST.toml`).
- `configs/baselines.toml` — явные команды baseline-ов, лимиты ресурсов.
- Provenance: `SOURCE_PINS.toml`, `PORTING_MANIFEST.toml` (ноль
  перенесённых блоков), `THIRD_PARTY_NOTICES.md`.
- `docs/gt/` (M3, M3.5, M4, M4.5) — GT-корпус, его manifest и seal, oracle-,
  corridor- и topology-артефакты. Все три отчётных артефакта — Tier A: несут
  свою платформу и отказываются сравниваться с чужой.
- Governance: `REQUIREMENTS_TRACEABILITY.md`, `FAILURE_LEDGER.md`,
  `docs/ADR/`.

Чего здесь осознанно НЕТ: topology envelope, fitter, optimizer,
SVG-экспорт, UI/WASM/AI — они появляются только в своих milestones. M4
производит ГИПОТЕЗЫ и наблюдения, а не доставку: ни одной сцены он не
доставляет и ни одного confidence-числа не производит.

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
