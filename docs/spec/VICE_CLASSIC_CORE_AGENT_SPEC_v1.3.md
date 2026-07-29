# VICE Classic Core
## Техническое задание на алгоритмический raster → SVG inverse rasterizer

**Версия:** 1.3 — Reliability-Hardened  
**Дата фиксации:** 2026-07-26  
**Статус:** каноническая версия; заменяет v1.0–v1.2.  
**Назначение:** основной implementation/research contract для coding-агента.  
**Цель:** собрать классический, проверяемый и коммерчески пригодный vectorization core для логотипов, иконок, клипарта и flat-color artwork без обязательной нейросети.

## Что исправлено в v1.3

v1.3 не обещает невозможную «99% идеальность на любых картинках». Вместо этого она вводит измеримый контракт: **не менее 99% надёжности среди результатов, которые core сам признал успешными**, при отдельно измеряемом coverage. Для неоднозначных или неидентифицируемых растров система обязана отказаться, а не уверенно выдать произвольную геометрию.

Главные исправления:

- введены identifiability classes, calibrated abstention и risk–coverage gate;
- `success` теперь требует статистически откалиброванной уверенности, а не только лучшего score;
- confidence привязан к versioned `SupportedModelUniverseV1`; heuristic bounds маркируются как empirical/unknown, а не выдаются за сертификаты;
- observation likelihood учитывает spatial correlation, чтобы один blur edge не создавал сотни ложных независимых доказательств;
- reliability считается по независимым source groups, использует sealed-audit burn policy и не раздувается фазами/размерами одного SVG;
- numerical `DecisionInterval` сохраняет физически разные near-tie hypotheses; deterministic tie-break не имеет права скрывать ambiguity;
- hard input classifier заменён soft model evidence; ранний ошибочный `unsupported` запрещён;
- минимальная image-formation family перенесена в Flat2 M4, а не отложена до M9;
- two-color evidence переписан в premultiplied linear RGBA и корректно поддерживает прозрачный exterior;
- частично прозрачные interior fills явно исключены из первого Flat2 core;
- max/min-tree дополнены cubical-complex topology, complementary connectivity и well-composedness rules;
- M5 больше не выбирает окончательную topology по слабому polyline proxy: он сохраняет hypothesis envelope до M6/M7;
- oracle suite превращён в factorial partition × formation experiment и отдельно делит candidate-generation, selector, parameter fit и optimizer;
- tangent tolerance больше не называется G1: точная G1 достигается только joint constrained chain refit;
- финальный selector больше не использует BIC как source of truth: все сравнения идут в общих единицах negative log-posterior / MDL bits;
- `E_render + E_evidence` как двойной учёт тех же пикселей запрещён; evidence служит proposal/trust-region механизмом;
- multiscale loss не имеет права повторно учитывать одни и те же пиксели без orthogonal-band likelihood;
- exact ROI acceptance исправлен: baseline пересчитывается после каждого принятого блока, ROI включает PSF halo и dependency closure;
- quantized scene повторно проходит topology/G1/render verification после seal;
- добавлен partition-aware renderer и отдельный export materialization plan для системного устранения SVG seams;
- добавлены inverse-crime protection, independent rasterizers, metamorphic tests, fuzz/property tests и statistical human court;
- connected script остаётся compound visible shape, не OCR/font lane;
- M0–M4 больше не выдаются агенту одним огромным заданием: один запуск — один milestone и обязательная остановка на gate report.

# 0. Директива агенту

Ты не продолжаешь хаотично ремонтировать `lomatoq/Vice-`, `lomatoq/v-ice` или `lomatoq/v-ize`.

Ты создаёшь новый Rust workspace `vice-classic`. Старые репозитории используются только как pinned baselines, algorithm donors, test donors и журнал тупиков.

Главная постановка:

> Найти не контур, наиболее похожий на пиксельную лестницу, а наиболее простой **идентифицируемый** visible vector scene и global image-formation model, которые правдоподобно породили наблюдаемый растр.

Система состоит из четырёх дисциплин:

1. **generation** — строит несколько palette/topology/grammar/formation hypotheses;
2. **inference** — сравнивает их единым posterior/MDL в физических единицах;
3. **verification** — запрещает topology, geometry и serialization failures;
4. **selective delivery** — отдаёт SVG только при откалиброванной уверенности, иначе возвращает `ambiguous`.

Никакой AI-модели в обязательном пути первого core нет. Будущий ML может лишь предложить hypothesis; классическая система обязана уметь полностью проверить, отклонить или принять его без доверия к модели.

Нельзя компенсировать неизвестность широкой uncertainty tube, hidden fallback, тысячами pixel rectangles или названием `success` для результата без confidence gate.

**Один agent run реализует только один milestone.** После milestone агент создаёт status/gate report и останавливается. Автоматический переход дальше без review запрещён даже при зелёных тестах.

# 1. Product и reliability contract

## 1.1. Поддерживаемый класс первой пригодной версии

Первая рабочая линия поддерживает:

- Flat2 и затем multiregion **visible planar** flat-color artwork;
- логотипы, иконки, клипарт, плоские маскоты и wordmarks;
- connected script только как compound visible shape с holes/counters;
- opaque interior fills;
- transparent exterior/background;
- antialiasing, 8-bit quantization и ограниченную global PSF family;
- умеренную деградацию только после отдельного M9.

Результат:

- compact SVG из `L/A/Q/C` и native primitives там, где они согласованы с shared graph;
- одна shared boundary между соседними visible faces;
- deterministic canonical scene и экспорт;
- zero accepted self-intersections/G1 breaks/topology corruption;
- JSON report, trace, posterior alternatives и confidence;
- zero hidden pixel fallback.

## 1.2. Не-задачи первого core

До соответствующих milestones не делать:

- фотографии и texture-heavy images;
- semantic object recognition;
- authored layer-stack recovery, amodal completion и скрытые части объектов;
- OCR, glyph identity, font reconstruction, editable TextRun;
- semi-transparent overlapping interior layers;
- arbitrary blend modes, shadows и effects;
- gradients до M11;
- stroke centerline lane до M10;
- GPU/WASM/UI до quality CPU gate;
- sample-specific patches;
- output, состоящий из pixel rectangles, как успешный SVG.

Термин `GT partition` во всём документе означает visible planar partition после painter compositing, не исходные authoring layers.

## 1.3. Intent и compute — ортогональные оси

```rust
enum Intent { Exact, Clean }
enum ComputePreset { Fast, Quality }
```

### `Intent::Exact`

- более слабый structural prior;
- больше допустимых anchors/segments;
- observed irregularity сохраняется, если она устойчива по formation hypotheses и превышает evidence/noise floor;
- primitive/relation promotion требует большого posterior margin;
- exact не означает staircase tracing или копирование codec noise.

### `Intent::Clean`

- более сильные code lengths для лишних anchors, false corners и free cubics;
- предпочтение true lines/arcs/circles, equal radii, symmetry/repetition;
- idealization разрешена только при сохранении hard topology и calibrated observed details.

### `ComputePreset`

Меняет только search/solver budget: beam width, k-best paths, continuation scales, iteration caps, formation candidates и render precision. Он не меняет intent prior/code tables. Fast и Quality оценивают кандидатов одним final posterior; Fast лишь исследует меньше пространства.

## 1.4. Typed outcomes

```rust
enum VectorizeOutcome {
    Success(SealedResult),
    Ambiguous(AmbiguityReport),
    Unsupported(UnsupportedReport),
    Failed(FailureReport),
}
```

- `Ambiguous`: несколько physically plausible scenes или formation models остаются близки;
- `Unsupported`: ни одна model family не объясняет input в supported likelihood envelope;
- `Failed`: numerical/internal failure;
- эти статусы не объединяются в один `AmbiguousOrFailed`.

Research core никогда не вызывает legacy fallback. Product wrapper позже может вызвать pinned legacy engine, но обязан показывать provenance и не считать его Classic success.

## 1.5. Идентифицируемость и честный «99%» контракт

Raster→vector ill-posed: разные scenes могут дать практически одинаковые pixels. Поэтому нельзя требовать восстановления authoring truth там, где он не наблюдаем.

Для GT fixtures вводится:

```rust
enum IdentifiabilityClass {
    Identifiable,
    EquivalentFamily,   // несколько scenes считаются корректной equivalence class
    InformationLost,    // detail/topology физически не восстановимы
}
```

Успех оценивается относительно **допустимого visible-scene equivalence class**, а не обязательно исходного числа authoring paths.

Каждый candidate report содержит:

```rust
enum BoundValue<T> {
    Certified(T),              // математически доказанный bound
    EmpiricallyCalibrated(T),  // оценка с frozen held-out calibration
    Unknown,                   // отсутствие доказательства не маскируется числом
}

struct ConfidenceReport {
    top1_equivalence_class_bits: f64,
    top2_class_margin_bits: f64,
    retained_mass_lower_bound: BoundValue<f64>,
    unexplored_mass_upper_bound: BoundValue<f64>,
    topology_entropy_upper_bound: BoundValue<f64>,
    formation_entropy_upper_bound: BoundValue<f64>,
    perturbation_stability: f64,
    posterior_predictive_mismatch: f64,
    calibration_bucket: String,
}
```

Нельзя сериализовать heuristic estimate как `Certified`. Если finite normalized posterior или admissible search bound ещё не реализованы, поле остаётся `EmpiricallyCalibrated` либо `Unknown`.

`Success` разрешён только если одновременно:

1. verifier green до и после quantization/export;
2. input explainability проходит supported model gate;
3. posterior сначала агрегирован по **delivery-equivalence classes**, чтобы численно разные, но одинаково полезные scenes не создавали ложную ambiguity;
4. top-1/top-2 class margin, retained-mass lower bound и unexplored-mass upper bound проходят frozen calibration;
5. topology/formation entropy upper bounds ниже bucket limits;
6. posterior-predictive residual не показывает systematic out-of-model structure;
7. winner устойчив к small phase, sample-step, render-tolerance и solver perturbations;
8. selective-risk calibration разрешает success для данного bucket.

Надёжность и coverage публикуются отдельно:

```text
selective catastrophic risk = catastrophic accepted outputs / all accepted outputs
coverage = accepted outputs / all supported inputs
```

Production-quality claim «≥99% надёжности» допустим только когда one-sided Clopper–Pearson upper bound для catastrophic accepted risk ниже 1% на frozen held-out наборе при заранее выбранном confidence level. Для 99% confidence это обычно требует сотен accepted случаев даже при нуле failures. Нельзя добиться gate, отказавшись почти от всего: minimum coverage фиксируется отдельно по bucket.

Стартовые coverage targets до M3 freeze:

```text
Flat2 clean-AA identifiable @128–512: >= 80%
Flat2 clean-AA identifiable @64:      >= 60%
```

Остальные случаи могут честно возвращаться как `ambiguous`.

Confidence threshold калибруется только на отдельном calibration split. Для каждого bucket candidates сортируются по conservative confidence score; выбирается самый низкий threshold, при котором one-sided Clopper–Pearson bound проходит risk target и coverage остаётся выше frozen minimum. Threshold затем проверяется на untouched test split. Guarantee предполагает exchangeability внутри bucket; distribution shift обязан понижать confidence или переводить input в `unsupported/ambiguous`.

Неполный search не может повышать уверенность. Если generator/beam не даёт certified posterior-mass bound, вся неизвестная масса записывается в `unexplored_mass_upper_bound`; Fast preset на сложном input скорее откажется, чем станет увереннее из-за того, что не увидел альтернативы.

### Supported model universe и предел смысла confidence

Любая posterior mass и confidence относятся не к «всем мыслимым SVG», а к конечному, versioned **SupportedModelUniverseV1**. Он обязан перечислять допустимые:

- topology operators и лимиты complexity;
- formation families и диапазоны параметров;
- geometry families, relation families и quantization precision;
- paint/exterior models;
- search truncation rules и доказуемые bounds.

Universe сериализуется канонически и получает `model_universe_hash`. Изменение universe — отдельная model-version change с полной recalibration; нельзя молча расширить grammar и сохранить старый confidence threshold.

`Success` означает: «candidate надёжен внутри поддерживаемого model universe, а posterior-predictive checks не выявили, что input находится вне него». Это не утверждение об authored truth вне наблюдаемой equivalence class.

Search-completeness claims имеют два уровня:

- **R1 Empirical selective reliability** — цель M7: confidence policy проходит untouched clustered held-out risk–coverage test. Search-mass поля могут быть `EmpiricallyCalibrated`, но не называются теоретической posterior completeness.
- **R2 Search-certified reliability** — дополнительный более сильный tier: discrete search использует best-first/branch-and-bound с admissible lower bounds, а continuous families имеют certified lower envelopes в релевантном score window. Только этот tier может утверждать certified unexplored posterior-mass bound.

Отсутствие R2 не блокирует честный R1 result, но report и UI обязаны различать эти claims.

## 1.6. Alpha/transparency contract Flat2 v1

Flat2 v1 поддерживает:

- opaque foreground/interior face;
- opaque background face или transparent exterior face;
- partial edge alpha, возникающий из coverage/antialiasing.

Flat2 v1 **не** поддерживает interior fill с истинной постоянной alpha `0<α<1` поверх другого authored layer. Такой input помечается `unsupported` или остаётся в competing model, но не притворяется обычной two-color coverage задачей.

RGB под `alpha≈0` не является цветовым доказательством и игнорируется после premultiplication.

# 2. Зафиксированные источники

Использовать именно эти commit SHA как начальную точку аудита. Если HEAD позже изменится, не переключаться молча.

```toml
[[source]]
name = "v-ice"
repo = "lomatoq/v-ice"
sha = "9211b3213d9b47defdf19ae4d0842af1c3ade45f"
role = "algorithm donor + Rust baseline + GT/diagnostics donor"
license_status = "OWNER_CONTROLLED_VERIFY_BEFORE_PUBLIC_RELEASE"

[[source]]
name = "v-ize"
repo = "lomatoq/v-ize"
sha = "95a65194cf34e2d96b41eb299b4769eac624be80"
role = "typed curve fitting + WASM architecture + inverse-render experiments"
license_status = "MIT OR Apache-2.0 declared in workspace"

[[source]]
name = "Vice-"
repo = "lomatoq/Vice-"
sha = "200897ab3e888970e330deeb3bb9e157923cc0aa"
role = "contracts + verifier ideas + failed-system baseline"
license_status = "OWNER_CONTROLLED_VERIFY_BEFORE_PUBLIC_RELEASE"
```

## 2.1. Обязательный provenance workflow

Создать в новом репозитории:

```text
SOURCE_PINS.toml
PORTING_MANIFEST.toml
THIRD_PARTY_NOTICES.md
```

Для каждого перенесённого блока записывать:

```toml
[[unit]]
id = "coverage-rasterizer-v1"
source_repo = "lomatoq/v-ice"
source_sha = "9211b3213d9b47defdf19ae4d0842af1c3ade45f"
source_path = "src/core/raster.rs"
destination = "crates/vice-render/src/coverage.rs"
mode = "ported_and_refactored" # exact_port | ported_and_refactored | clean_room | reference_only
authorization = "owner-controlled"
tests_ported = [
  "half_pixel_offsets_are_exact",
  "coverage_is_continuous_in_translation",
  "circle_area_matches_pi_r_squared"
]
notes = "Exact coverage of flattened geometry; genericized for new IR"
```

Правила:

- никакого copy-paste без записи в manifest;
- никакого кода из закрытого Vector Magic;
- статьи дают алгоритм и формулы, но их реализация должна быть clean-room;
- внешний open-source код переносить только после проверки лицензии;
- перед публичным/коммерческим релизом добавить явный SPDX/license для owner-controlled репозиториев.

---

# 3. Что заимствовать, что переписать, что не трогать

## 3.1. `lomatoq/v-ice`

### Перенести и очистить

**`src/core/geom.rs`**

- базовые `Pt/Vec2` операции;
- distance, dot, cross, curve evaluation;
- тесты на геометрию.

**`src/core/raster.rs`**

- signed-area coverage accumulator;
- half-pixel tests;
- continuity tests;
- area tests.

Но переписать API под generic `RenderMesh`, а не под старый `Bezigon`.

**`src/pipeline/planar.rs`**

- crack extraction;
- junction detection;
- maximal shared-edge walks;
- сборка face loops из `(boundary_id, reversed)`.

Это должен стать фундаментом нового core, а не optional pass после per-region tracing.

**`src/diagnostics/*` и `examples/gt_battery.rs`**

- stage capture;
- reproducible corpus runner;
- geometry/topology metrics;
- baseline reports.

Перенести идеи и полезные тесты, но не старые aggregate-формулы как единственный quality gate.

**Части `src/pipeline/energy.rs`**

- two-color projection;
- APT/HPT/LPT/SPT формулы;
- normalization tests.

Использовать как reference для energy terms, не копировать текущую зависимость от уже жёстких region labels.

### Использовать только как reference

**`src/pipeline/fit.rs`, `solver.rs`, `facefit.rs`**

- брать primitive fitters и corner-aware идеи;
- не переносить целиком текущий orchestration.

**`src/pipeline/optimize.rs`**

- взять energy definitions и полезные unit tests;
- не переносить старую систему множества `FAIR`, `LEASH`, `CLEASH`, `CCLAMP` и прочих env-патчей как архитектуру;
- новый optimizer должен работать с единым objective и exact transactional acceptance.

**`src/regularize/*`**

- primitive/constraint solvers могут стать proposal generators;
- ни один snap не должен применяться без model comparison против unconstrained candidate.

### Не переносить в новый core

- текущий `segment.rs` как основной segmentation algorithm;
- `vectorize.rs` целиком;
- residual patch stack;
- image-specific exceptions;
- десятки env-флагов;
- любой код, который после неверной segmentation маскирует ошибку локальной заплаткой.

## 3.2. `lomatoq/v-ize`

### Перенести и очистить

**`crates/vize-core/src/geom.rs`**

Взять typed vocabulary и tangent API:

```text
Line
CircularArc
QuadraticBezier
CubicBezier
```

Расширить generic IR на `EllipticArc` и whole-loop native primitives.

**`crates/vize-core/src/fit.rs`**

Взять:

- Schneider cubic fitting;
- line fitter;
- arc fitter;
- quadratic reduction;
- endpoint tangent logic;
- fair-cubic tests.

Но заменить greedy farthest-reach selection на настоящий global DAG shortest-path/DP. Предположение «feasibility почти monotone» недостаточно для production model selection.

**`crates/vize-core/src/engine_grad.rs`**

Взять:

- flattening provenance;
- scatter gradient от polyline vertices к anchors/handles;
- finite-difference gradient tests;
- local ROI idea.

Soft-SDF renderer использовать как **proposal-gradient surrogate**, а не как окончательный суд. Каждое изменение принимает exact renderer.

**Workspace/CLI/WASM layout**

- чистое разделение core/cli/bench/wasm;
- `#![forbid(unsafe_code)]` в math/core crates;
- wasm-safe core после CPU-quality версии.

### Использовать только как reference

**`engine_b.rs`**

- полезны block-coordinate descent и full-energy guard;
- не переносить весь модуль и env-tuning;
- не оставлять arcs/typed primitives неподвижными навсегда.

**`render.rs`**

- local ROI и premultiplied compositing полезны;
- 4× supersampling не должен быть единственным exact judge, когда уже есть signed-area coverage.

**`vai_place` и hard-coded Vectorizer.AI anatomy rules**

- держать только в research feature;
- не делать фундаментом новой системы;
- anchor placement должен выходить из MDL, curvature evidence и render residual, а не из имитации одного black-box продукта.

## 3.3. `lomatoq/Vice-`

### Перенести как generic Rust contracts

**`vice_compiler/vector_program.py`**

Взять концепции:

- canonical program;
- line/arc/ellipse/cubic/biarc vocabulary;
- paint layers;
- одна quantization перед serialization;
- canonical JSON;
- program digest;
- exact SVG fragment digest;
- exporter не имеет права заново придумывать geometry.

Не переносить text-specific names (`TextVectorProgram`, `source_line_id`). Сделать общий `VectorSceneProgram`.

**`vice_compiler/coverage_evidence.py`**

Взять:

- robust two-color coverage;
- linear-RGB projection;
- residual-derived uncertainty;
- marching squares at `alpha=0.5`;
- `+0.5` pixel-center convention;
- arclength samples, normals, corridor halfwidth, physical `ds` weights.

Переписать на Rust и добавить randomized property tests.

**`vice_compiler/materialization_certificates.py`**

Взять как verifier:

- component correspondence;
- fusion/split detection;
- counter preservation;
- separation corridors;
- delivery identity.

Не использовать verifier как generator.

**`svg_fragment_renderer.py`**

Взять contract:

> сериализованный candidate, candidate в court и exported candidate — одни и те же bytes.

### Не переносить

- `geometry_vectorizer.py` monolith;
- PCDC proposal routing;
- pixel fallback materializer;
- text-only court как главный generator;
- `_try_*_calibration` stack;
- всё, что позволило отдать 10k path commands как пользовательский результат.

---

# 4. Rust workspace: целевая структура и milestone-scoped creation

Ниже — **целевая** структура зрелого репозитория, а не список crates, которые надо пустыми создать в C001.

```text
vice-classic/
  Cargo.toml
  SOURCE_PINS.toml
  PORTING_MANIFEST.toml
  THIRD_PARTY_NOTICES.md

  crates/
    vice-geom/          # Vec2, robust predicates, curves, intersections
    vice-ir/            # canonical scene + planar graph + paint + provenance
    vice-image/         # decode, ICC/alpha conventions, linear/Oklab
    vice-evidence/      # palette, mixture, coverage, uncertainty, edge profiles
    vice-topology/      # event trees, hypotheses, RAG, planar graph, signatures
    vice-fit/           # line/arc/quad/cubic/primitive candidates + global DP/MDL
    vice-render/        # exact coverage + compositing + ROI + PSF
    vice-opt/           # continuous optimizer + discrete transactions
    vice-verify/        # topology, separation, intersections, digest identity
    vice-svg/           # canonical SVG writer/parser adapter
    vice-core/          # orchestration only
    vice-cli/           # command-line research product
    vice-bench/         # GT, oracles, baselines, reports, human A/B assets
    vice-wasm/          # only after CPU core passes quality gate

  configs/
    intent_exact.toml
    intent_clean.toml
    preset_fast.toml
    preset_quality.toml
    GATES_V1.toml

  tests/
    fixtures/
    golden/

  docs/
    ARCHITECTURE.md
    COORDINATE_CONVENTION.md
    ENERGY.md
    ORACLE_DECOMPOSITION.md
    PORTING_LOG.md
    KNOWN_FAILURES.md
    MILESTONES.md
```

## 4.1. Реально активные crates в M0–M4

Создать только:

```text
vice-geom
vice-ir
vice-image
vice-evidence
vice-render
vice-bench
vice-cli
```

`vice-topology`, `vice-fit`, `vice-opt`, `vice-verify`, `vice-svg`, `vice-core`, `vice-wasm` создаются только в milestone, где появляется их первая реальная executable responsibility.

Правила:

- не создавать пустые crates «на будущее»;
- не писать placeholder traits/APIs для M5–M12 во время M0–M4;
- не вводить abstraction, пока нет как минимум двух реальных implementations/call sites;
- target architecture можно описать в документации, но не превращать её в 70 speculative interfaces;
- каждый milestone заканчивается работающим CLI path, а не только типами.

Orchestrator `vice-core`, когда он появится, должен быть коротким. Никакого нового файла на 10k строк. Рекомендация: модуль больше 800–1000 LOC обязан быть разбит до merge.

---

# 5. Неподвижные математические и топологические конвенции

Эти решения принимаются до algorithm tuning и покрываются unit/property tests.

## 5.1. Координаты

```text
Frame: x вправо, y вниз.
Pixel (x,y) = [x,x+1] × [y,y+1].
Pixel center = (x+0.5,y+0.5).
Canvas = [0,W] × [0,H].
Internal geometry = f64.
```

Все transforms находятся в одном модуле; случайные `±0.5` в pipeline запрещены.

## 5.2. Цвет и observation space

Canonical scene colors хранятся в linear straight RGBA. Для compositing/raster likelihood используются premultiplied linear RGBA.

Observation formation может иметь hypothesis:

```rust
enum BlendSpace { LinearLight, EncodedSrgb }
```

Потому что реальные rasterizers иногда смешивают coverage до или после transfer function. Нельзя насильно считать любой AA линейным и затем объяснять mismatch геометрией.

Pipeline:

```text
decode bytes + ICC assumption
→ straight RGBA
→ canonical linear RGBA
→ premultiplied observation tensor
```

8-bit quantization является частью formation likelihood, а не случайным residual.

## 5.3. Digital topology

Binary topology использует одну заранее зафиксированную complementary-connectivity convention, например foreground 4-connected / background 8-connected. Для dual arm допустима обратная convention; близкие saddle cases порождают обе hypotheses.

Нельзя использовать одинаковую 4- или 8-connectivity для foreground/background и затем считать Euler signature доказанным.

Для multicolor строится cubical cell complex. Checkerboard/critical 2×2 configurations должны:

- либо быть преобразованы в explicit well-composed candidate;
- либо породить несколько saddle-resolution hypotheses;
- но никогда не разрешаться скрытым iteration-order tie-break.

Exterior — настоящий `FaceId`, а не отрицательный magic label.

## 5.4. Robust geometric predicates

Topology decisions не полагаются на `abs(cross)<1e-9` в обычном f64.

Использовать adaptive/exact predicates для:

- orientation;
- segment intersection;
- winding/inside tests около границы;
- ordering intersections на scanline;
- DCEL face assembly.

Metric computations могут оставаться f64, но combinatorial topology decisions используют robust predicates или integer/rational lattice representation. Все tolerances типизированы по units и назначению.

### Численные интервалы решения

Любой score/likelihood/render delta, который влияет на pruning, acceptance или delivery, хранит numerical error bound:

```rust
struct DecisionInterval { lo: f64, hi: f64 }
```

- `A` certifiably лучше `B` только если `A.hi < B.lo` для minimization;
- если интервалы физически различных hypotheses перекрываются, обе сохраняются либо result становится ambiguous;
- deterministic tie-break разрешён только внутри одной `DeliveryEquivalenceClass`;
- epsilon не имеет права молча выбрать topology, primitive family или success;
- bounds включают tessellation, convolution truncation, floating reduction и quantization errors.

Пересечения Bézier/arc paths для verifier доказываются analytic predicates либо certified recursive subdivision/interval bounding boxes. Простая проверка flattened polyline не является сертификатом отсутствия пересечения.

## 5.5. Quantization и determinism

- geometry остаётся f64 до seal;
- canonical parameters quantize один раз;
- shared endpoints/tangents quantize через shared parameter object, не независимо в каждом path;
- после quantization scene заново materialize/validate/render;
- `-0`, NaN, Inf запрещены;
- ordered maps/reductions и fixed seeds обязательны.

Determinism tiers:

```text
Tier A: same binary/platform → byte-identical digests (обязателен M1+)
Tier B: supported platforms → scene-equivalent within frozen tolerance (M12)
```

Не обещать cross-platform byte identity, пока libm/FMA/thread reductions не зафиксированы.

# 6. Canonical IR и hypothesis envelope

SVG DOM не является внутренней моделью.

## 6.1. Visible planar scene

```rust
struct VectorScene {
    canvas: Canvas,
    graph: PlanarGraph,
    formation: GlobalFormationHypothesis,
    provenance: SceneProvenance,
}

struct PlanarGraph {
    exterior: FaceId,
    vertices: Vec<GraphVertex>,
    boundaries: Vec<Boundary>,
    half_edges: Vec<HalfEdge>,
    faces: Vec<Face>,
}

struct Boundary {
    id: BoundaryId,
    left_face: FaceId,
    right_face: FaceId,
    start_vertex: VertexId,
    end_vertex: VertexId,
    curve: CurveChain,
    evidence: EvidenceRef,
}

struct Face {
    id: FaceId,
    loops: Vec<HalfEdgeId>,
    paint: Paint,
    support_provenance: SupportRef,
}
```

Interior shared boundary хранится один раз. Border boundary разделяет interior face и exterior face.

## 6.2. Geometry grammar

```rust
enum Segment {
    Line(LineSeg),
    CircularArc(CircularArcSeg),
    EllipticArc(EllipticArcSeg),
    Quad(QuadSeg),
    Cubic(CubicSeg),
}

enum JoinKind { Corner, SmoothG1 }
```

Smooth node хранит shared tangent parameter. Exact G1 возникает из parameterization/joint solve, а не из последующей проверки маленького угла.

## 6.3. Scene hypotheses

До final selection core хранит envelope:

```rust
struct SceneHypothesis {
    scene: VectorScene,
    posterior_bits: f64,
    lower_bound_bits: f64,
    delivery_class: DeliveryEquivalenceClassId,
    topology_id: TopologyHypothesisId,
    formation_id: FormationHypothesisId,
    grammar_id: GrammarHypothesisId,
    certificates: CertificateSet,
}
```

M5 не имеет права назвать topology окончательным winner по proxy geometry. Hypotheses сохраняются до typed fit/refinement, если они не **certifiably dominated**. Budget-driven pruning отдельно маркируется как approximation risk и сохраняет topology/formation diversity quotas.

Posterior confidence считается по суммарной массе `DeliveryEquivalenceClass`, а не по отдельным floating-point parameterizations. Класс объединяет scenes с одинаковой видимой topology и geometry/render differences ниже frozen user-visible tolerance.

## 6.4. Paint

Flat core:

```rust
enum Paint {
    OpaqueSolid(LinearRgb),
    TransparentExterior,
}
```

Gradients и true semi-transparent authored layers появляются только в своих milestones.

## 6.5. Export materialization plan

Canonical scene описывает visible partition. SVG rasterizers композят antialiased paths painter-style и могут показывать hairline seams даже при общей границе. Поэтому export имеет отдельный, проверяемый объект:

```rust
struct ExportPlan {
    visible_scene_digest: Digest,
    face_order: Vec<FaceId>,
    edge_aprons: Vec<EdgeApron>,
    svg_profile: SvgProfile,
}
```

`EdgeApron` — export-only underpaint вдоль shared edge нижнего face, полностью скрытый верхним face в идеальной геометрии. Он не меняет canonical visible boundary и принимается только после differential render court. Никаких ad-hoc strokes по конкретной картинке.

# 7. End-to-end inference pipeline

```text
Canonical decode
→ soft input/model analysis
→ palette + exterior hypotheses
→ minimal global formation hypotheses
→ premultiplied mixture evidence
→ event-driven topology envelope
→ robust shared DCEL
→ subpixel boundary observations
→ typed grammar candidates
→ k-best discrete grammar + joint G1 refit
→ primitive/relation candidates
→ exact full-resolution posterior/MDL
→ continuous trust-region refinement
→ compound discrete transactions
→ verifier
→ confidence/abstention
→ quantize + reverify + export materialization
→ SVG/JSON/trace
```

Правило: hard decision разрешён рано только если его альтернатива доказанно невозможна или certifiably dominated. Иначе решение остаётся в hypothesis envelope.

# 8. Stage A — canonical decode и soft model analysis

## 8.1. Decode

M0/M1 сначала поддерживают PNG. JPEG/WebP/AVIF добавляются только вместе с formation/noise tests.

Сохранять:

- dimensions, source hash, alpha;
- ICC/profile presence и применённое assumption;
- border/exterior statistics;
- original encoded bytes;
- resource-limit diagnostics.

Защита от decompression bombs и oversized dimensions обязательна до productization, но лимиты присутствуют с M0.

## 8.2. Soft input analysis

`InputClass` — не hard enum, управляющий ранним отказом. Это vector of model priors/evidence:

```rust
struct InputModelEvidence {
    flat_clean: f64,
    flat_aa: f64,
    flat_degraded: f64,
    pixel_art: f64,
    gradient_heavy: f64,
    photo_like: f64,
}
```

Измерения:

- unique-color density;
- local covariance/variance;
- edge profile widths;
- high-frequency energy;
- alpha behavior;
- interior-mode count;
- pairwise-mixture explainability;
- residual spatial structure.

Classifier задаёт priors и admissible model families, но не может один вернуть `Unsupported`. Отказ разрешён только после того, как все supported formation/scene hypotheses имеют плохой calibrated likelihood/explainability.

# 9. Stage B — palette и exterior hypotheses

## 9.1. Interior confidence

Высокий weight получают pixels с:

- low physical gradient;
- low local covariance;
- coherent same-color support;
- stable alpha;
- distance от mixed edge.

Pixels с `alpha≈0` не дают RGB evidence. Edge pixels не обучают palette полным весом.

## 9.2. Flat2 hypotheses

Строить несколько hypotheses, а не одну пару:

- transparent exterior + one opaque foreground;
- opaque border-supported background + foreground;
- full-bleed two-face scene без предположения, что border — background;
- label-swapped canonical equivalent.

Colors оцениваются robustly в linear space, но сравниваются через forward formation hypothesis.

Если у тонкой формы нет надёжного interior core, не выдумывать цвет из одного pixel: использовать bounded color hypothesis interval и позволить posterior/abstention решить.

Manual `--fg/--bg/--exterior` разрешены только как oracle/diagnostic overrides и помечают run как non-production.

## 9.3. Multicolor

M8 использует palette beam и alternation с partition/paint. Palette size выбирается через posterior code length, spatial coherence и exact rerender, не elbow rule.

# 10. Stage C — premultiplied mixture evidence

Для opaque visible faces и transparent/opaque exterior площадь покрытия линейна в premultiplied RGBA.

Пусть `P_f`, `P_b`, `P_i` — premultiplied linear RGBA vectors. Тогда:

\[
\hat a_p = \operatorname{clamp}_{[0,1]}
\frac{(P_i-P_b)\cdot(P_f-P_b)}{\|P_f-P_b\|^2}
\]

\[
r_p = \left\|P_i-[\hat a_pP_f+(1-\hat a_p)P_b]\right\|
\]

Это корректно и для transparent exterior `P_b=(0,0,0,0)`.

Дополнительно хранить:

- residual vector, не только norm;
- contrast/conditioning;
- local gradient;
- candidate blend-space likelihood;
- quantization interval;
- spatially correlated residual indicators.

Uncertainty выводится из calibrated formation/noise model, а не просто из hand-tuned формулы. Formula-based corridor допустим как initialization, но коэффициенты freeze на dev set и проверяются calibration gates.

## 10.1. Minimal formation family уже в M4

M4 поддерживает ограниченный global family:

```text
blend space: linear | encoded-sRGB
coverage filter: analytic box | triangle | small Gaussian family
8-bit quantization
transparent or opaque exterior
```

Kernel глобален для изображения. Per-edge blur запрещён. M9 расширяет family resize chains, codec artifacts и более сложные kernels; он не создаёт formation architecture с нуля.

## 10.2. Evidence не является вторым pixel likelihood

Mixture alpha, corridor и boundary samples вычислены из тех же pixels, что и final render likelihood. Поэтому их нельзя без вероятностной factorization ещё раз добавить как независимый `E_evidence`.

Evidence используется для:

- hypothesis generation;
- pruning невозможных candidates;
- trust-region sizes;
- proposal cost/DP surrogate;
- uncertainty/confidence diagnostics.

Final candidate selection использует один exact observation likelihood плюс priors/code lengths.

# 11. Stage D — event-driven topology envelope

## 11.1. Scalar fields

Для каждой palette/formation hypothesis строить:

- raw coverage posterior mean;
- TV-L2/Huber restored fields;
- conservative detail/gap-preserving field;
- denoised field;
- bounded deconvolution candidates только при stable global kernel.

## 11.2. Cubical topology и critical events

Основной generator:

- max-tree superlevel components;
- min-tree sublevel/background events с complementary connectivity;
- self-dual tree of shapes, когда robust implementation готов;
- component/hole birth/death;
- bridge/gap events;
- persistence plateaus;
- ambiguous saddle alternatives.

Fixed levels остаются cheap smoke probes.

Plateau/tie handling детерминирован: equal-valued pixels обрабатываются batch event, а не iteration order.

## 11.3. Candidate envelope, а не ранний winner

Для каждой topology хранить:

- cubical signature;
- components/holes/Euler;
- event persistence;
- formation/palette provenance;
- lower-bound data cost;
- topology/region code length;
- ambiguity flags.

Pruning tiers:

1. invalid topology — remove;
2. certifiably dominated lower bound — remove;
3. budget pruning — допускается только с diversity quotas и explicit approximation-risk record.

M4.5/M5 gate измеряет **candidate recall**: входит ли допустимая GT-equivalent topology в envelope. Он не требует, чтобы слабый proxy уже выбрал её winner.

## 11.4. Dual/primal continuation

Каждая topology transaction — compound operation:

```text
topology edit
→ rebuild affected DCEL
→ refit affected representation
→ refit paints
→ exact ROI posterior with halo
→ local certificates
→ accept into k-best envelope or rollback
```

M5 использует shared-polyline proxy только для preliminary bounds. M6 typed refit и M7 exact posterior имеют право изменить ranking. Proxy score не может необратимо удалить topology без certified bound.

## 11.5. Multicolor

M8 расширяет envelope на RAG merge/split and paint hypotheses. Возле triple junction pairwise color mixing недостаточен: использовать local area-fraction simplex или exact local forward rendering, где fractions нескольких faces суммируются в 1.

Morphology может предложить candidate, но не принять его без court.

# 12. Stage E — robust shared planar graph

Строить DCEL/cubical arrangement:

- explicit exterior face;
- robust junctions и border vertices;
- maximal shared boundary chains;
- oriented half-edges/twins;
- closed face loops;
- deterministic ambiguous-saddle branches.

Инварианты:

```text
каждый half-edge имеет twin;
каждая interior boundary имеет два owners;
border boundary имеет interior + exterior owner;
face cycles замкнуты и ориентированы;
no dangling cracks;
Euler/cubical signature сохранён;
non-adjacent boundaries не пересекаются;
```

Curve replacement topology-preserving только если:

- endpoints/junction incidence неизменны;
- fitted curve лежит в certified tubular neighborhood;
- tubes non-adjacent boundaries не пересекаются;
- robust intersection predicates green.

Если sufficient isotopy condition не доказана, операция требует full arrangement rebuild и verification; нельзя полагаться только на sampled self-intersection count.

# 13. Stage F — subpixel boundary observations и calibration

Для каждой shared boundary:

1. выбрать left/right face paint и formation hypothesis;
2. получить conditional coverage posterior;
3. extraction level `0.5` использовать только как initialization;
4. привязать chain к DCEL endpoints/junctions;
5. resample физическим ds;
6. оценить normal/tangent, confidence и correlation length;
7. сформировать calibrated normal interval.

```rust
struct BoundarySample {
    p: Vec2,
    normal: Vec2,
    halfwidth: f64,
    confidence: f64,
    weight_ds: f64,
    corr_length_px: f64,
}
```

Malformed chain не silent-repair: topology hypothesis invalid или ambiguous.

## 13.1. Corridor calibration

На independent GT rasterizers проверять:

- empirical coverage @50/@90/@95/@99;
- median/p95 width;
- conditional calibration по resolution, contrast, PSF, blend space, phase;
- bias along normals;
- invariance к sample step;
- calibration under held-out rasterizer.

Provisional clean-AA targets:

```text
coverage@95 >= 95%
median halfwidth <= 0.35 px
p95 halfwidth <= 0.75 px
```

Но gate freeze только после M3. Wide corridor не превращает failure в success. Model mismatch → `ambiguous/unsupported`.

Boundary evidence является trust region/proposal mechanism. Final posterior всё равно судит serialized scene через forward model.

# 14. Stage G — typed grammar, k-best DP и exact G1

## 14.1. Corner proposals

Corner saliency строится из multiscale signed turning, line-intersection support, curvature persistence и stability по topology/formation hypotheses. Это proposal confidence, не hard label.

## 14.2. Span candidate generation

Families:

- line;
- circular arc;
- quad;
- cubic;
- optional biarc;
- ellipse/clothoid только после targeted evidence.

Candidate endpoints берутся из sparse breakpoint set, persistence events и hierarchical interval schedule. Полный O(N²) all-pairs на dense samples запрещён без hard cap/profile.

Long line/arc support нельзя потерять из-за `max_candidate_support_px`: hierarchical candidates обязаны включать whole-run and multiscale intervals.

## 14.3. DP выбирает grammar, а не окончательные независимые curves

State содержит breakpoint, family, corner state и endpoint tangent interval/jet class. DP/k-shortest paths выбирает:

- breakpoints;
- families;
- corner vs smooth joins;
- coarse tangent compatibility.

Проверка `angle < tolerance` сама по себе **не является G1**.

Для каждого k-best discrete path запускается:

```text
joint constrained chain refit
```

с shared node positions и shared tangent variables. Если exact G1 + evidence feasibility недостижимы, path invalid и рассматривается следующий.

Closed loops решаются cyclic k-best search либо несколькими canonical cuts с доказанным cut-invariance test.

## 14.4. Proposal cost

Boundary integral:

\[
C_{proposal}=\int \rho(d_n(s)/h(s))\,ds
\]

используется для candidate ordering. Он invariant к resampling через `ds` и не добавляется второй раз в final pixel posterior.

## 14.5. Final MDL в физических bits

Raw-sample BIC не является final selector.

Final score:

```text
L_total = -log2 p(observed bytes | scene, formation, noise)
        + L_topology
        + L_geometry
        + L_paint
        + L_relations
        + L_formation
```

Минимальный explicit code:

- prefix code family;
- combinatorial code breakpoints/corners;
- parameter code `log2(range / calibrated precision)`;
- topology/face count code;
- formation family/parameter code;
- robust quantized residual likelihood.

`Intent::Exact/Clean` выбирает разные frozen prior/code tables. Нельзя просто подкрутить произвольные lambdas на test.

`BIC_eff` допускается только как diagnostic scaffold и не может promote feature, если exact likelihood + explicit code length disagree.

Обязательные invariance tests: 0.25/0.5/1.0px samples, duplicate samples, cyclic cut, uniform scale, translation, reflection.

# 15. Stage H — primitive и relation hypotheses

Whole-loop models:

- circle/ellipse;
- rect/rotated rect;
- rounded rect/capsule;
- regular polygon;
- free typed chain.

Relations:

- equal radii;
- concentricity;
- parallel/perpendicular lines;
- shared baseline;
- mirror symmetry;
- repeated transforms.

Каждый constrained model сравнивается с unconstrained sibling через тот же exact posterior/MDL. Relation prior не может компенсировать topology defect или salient residual.

Native SVG primitive разрешён только когда:

- canonical graph boundary точно соответствует primitive;
- shared neighboring faces используют ту же boundary object;
- post-quantization render/geometry verification green.

Нельзя заменить face на `<circle>` отдельно от соседней shared edge и тем самым разорвать partition identity.

# 16. Stage I — forward renderer и image formation

## 16.1. Certified partition renderer

Термин `exact` означает exact area coverage **для фиксированной polyline tessellation**. Bézier→polyline approximation имеет отдельный certified chord-error budget.

Renderer обязан:

- rasterize shared planar faces, включая exterior;
- использовать premultiplied compositing/area fractions;
- проверять per-pixel partition sum≈1 и отсутствие hidden gaps/overlaps;
- поддерживать ROI с dependency closure;
- быть deterministic.

At triple junction area fractions нескольких faces суммируются напрямую; pairwise sequential alpha compositing не считается ground-truth partition renderer.

## 16.2. Minimal formation V1 и M9 expansion

M4 уже имеет global discrete family:

```rust
struct GlobalFormationHypothesis {
    blend_space: BlendSpace,
    pixel_filter: PixelFilter,
    quantization: QuantizationModel,
    exterior: ExteriorModel,
}
```

M9 добавляет resize chain, broader PSF, JPEG/WebP residual model и kernel estimation.

Geometry, colors и blur частично неидентифицируемы. Formation parameter всегда global/strongly regularized; per-edge kernel запрещён.

## 16.3. Independent court

Внутренний renderer не тестируется только против самого себя. O0/differential suite использует independent high-resolution/reference rasterizers и реальный serialized SVG renderer (`resvg` плюс хотя бы один другой engine в product phase).

## 16.4. Optimization mesh

Inner solve фиксирует tessellation samples, чтобы objective не прыгал от adaptive branching. Mesh rebuild разрешён между outer iterations. Final render использует stricter tolerance и post-seal serialized bytes.

Soft-SDF/analytic surrogate может давать gradients, но exact/certified renderer принимает шаг.

# 17. Stage J — единый probabilistic/MDL objective

Final inference минимизирует negative log posterior в bits:

\[
L(S,F\mid I)= -\log_2 p(I\mid S,F) + L(S\mid intent)+L(F)
\]

Разложение scene prior:

```text
L(S|intent) = L_topology + L_geometry + L_paint + L_relations
```

Hard violations имеют infinite cost/reject.

## 17.1. Observation likelihood

- final judge использует full-resolution observed bytes/quantization model;
- clean bucket: calibrated quantized Gaussian/Student-t-like likelihood;
- degraded bucket: robust mixture likelihood с codec/outlier component;
- noise scales freeze по dev/formation bucket;
- low contrast учитывается conditioning, но не получает искусственную скидку, позволяющую удалить форму.

### Spatial correlation и запрет ложной posterior sharpness

Blur, resize, antialiasing и codec residuals пространственно коррелированы. Независимый per-pixel likelihood может умножить один физический edge residual сотни раз и сделать posterior опасно самоуверенным. Поэтому final likelihood обязан использовать один из audited вариантов:

1. whitening/precision model для residual field;
2. block likelihood с block size не меньше calibrated correlation support;
3. orthogonal frequency-band likelihood с отдельными frozen noise scales.

`iid pixel` разрешён только как diagnostic baseline. Он не может управлять production confidence, пока reliability calibration не докажет отсутствие overconfidence во всех frozen buckets. Report хранит `residual_model_id`, empirical correlation length, whitened-residual diagnostics и calibration error.

## 17.2. Запрет двойного учёта

Нельзя одновременно суммировать independent-looking `E_render` и `E_evidence`, если оба получены из одних pixels. Boundary corridors и multiscale maps — proposal/surrogate diagnostics.

Multiscale final likelihood разрешён только как orthogonal band decomposition с нормированной вероятностной моделью. Обычная сумма full-res + downsampled losses double-counts data и запрещена.

## 17.3. Priors/code lengths

- topology: components, holes, faces, explicit operators;
- geometry: families, breakpoints, parameter precision;
- paint: number/precision of colors;
- relations: code for activated constraints минус saved parameter bits;
- formation: filter/blend-space/parameters.

Intent меняет только prior tables. Compute preset не меняет objective.

## 17.4. Hard constraints

Reject при:

- invalid DCEL/Euler;
- unsupported fusion/split/lost required hole;
- boundary crossing/self-intersection;
- broken smooth G1;
- exposed export apron;
- post-quantization topology change;
- serialized render divergence выше gate.

# 18. Stage K — continuous refinement

## 18.1. Parameterization

- shared nodes хранятся один раз;
- smooth joins используют shared tangent angle + positive handle lengths;
- lines exact;
- arcs используют minimal non-overconstrained parameterization;
- radii/lengths positive через log/softplus parameter;
- junctions оптимизируются joint block.

## 18.2. Solver architecture

Exact objective non-smooth около topology/pixel events. Поэтому не обещать обычный L-BFGS как универсальное решение.

Использовать:

- surrogate gradients или finite differences;
- scaled trust region;
- projected/constrained block solve;
- exact backtracking acceptance;
- multiple deterministic initializations для hard cases;
- alternating color/geometry/formation blocks с joint escape block, если coordinate descent stalled.

Trust radius bounded calibrated corridor/feature scale. Optimizer не должен перескакивать topology event как continuous move.

## 18.3. Correct transactional acceptance

Для каждого block:

1. вычислить **current** parent objective после всех предыдущих accepted blocks;
2. snapshot dependency closure;
3. propose/project step;
4. построить ROI + PSF/tessellation/compositing halo;
5. exact rerender parent и child с одинаковым cache state;
6. local robust certificates;
7. accept при improvement больше numerical tolerance;
8. обновить caches/current objective;
9. периодически проверить full-scene objective и all certificates.

Старый pseudocode с одним stale `before` на весь outer loop запрещён.

ROI delta допустим только если dependency closure доказан. Изменение global formation/paint order требует full render.

## 18.4. Recovery и conditioning

O5 измеряет basin of attraction по geometry/color/formation perturbations. Report включает condition numbers, max moves, iterations и failure mode. Optimizer, который улучшает surrogate, но не exact posterior, не проходит gate.

# 19. Stage L — discrete/compound structure search

Operators:

- remove/insert anchor;
- split+joint-refit span;
- merge+joint-refit spans;
- family change;
- corner activate/deactivate;
- primitive/relation promote/demote;
- topology merge/split/bridge/hole transaction;
- paint/exterior change.

Одна логическая операция может временно требовать два шага, поэтому предложения создаются как **compound transactions** и оцениваются после complete local refit. Нельзя требовать, чтобы промежуточный half-operation уже улучшал score.

Quality beam:

- сохраняет k-best within posterior-bit margin;
- имеет topology/formation diversity quotas;
- memoizes canonical scene hashes;
- использует deterministic tie-break только внутри delivery-equivalent candidates; distinct hypotheses с overlapping score intervals сохраняются;
- имеет explicit candidate/time/memory budgets;
- отмечает budget-pruned posterior mass;
- ведёт conservative bound на unexplored alternatives; отсутствие bound уменьшает delivery confidence.

Fast mode может использовать greedy complete transactions, но final confidence учитывает, что search был усечён.

# 20. Stage M — verifier, seal и SVG materialization

## 20.1. Three-layer verification

**Combinatorial:** DCEL, Euler, owners, components/holes, separation, topology operators.  
**Geometric:** robust intersections, isotopy tube, finite coordinates, positive radii, exact G1.  
**Delivery:** serialized bytes, parser roundtrip, render digest, export apron coverage.

Verifier сравнивает output не с raw hard mask, а с selected evidence-supported topology hypothesis; GT correspondence используется только в benchmark.

## 20.2. Seal sequence

```text
f64 scene validate
→ quantize shared parameters once
→ reconstruct quantized scene
→ rerun DCEL/intersection/G1/isotopy verification
→ create ExportPlan
→ canonical JSON/SVG bytes
→ parse serialized SVG independently
→ render production bytes
→ compare internal/materialized/reference renders
→ seal scene/SVG/render digests
```

Если quantization ломает geometry, увеличить global precision policy отдельным reviewed change или reject; не делать локальную second quantization.

## 20.3. Seam-safe SVG системно

Чистые abutting SVG fills могут давать halos из-за painter alpha compositing. Решение — не случайный stroke, а validated export materialization:

- deterministic face z-order;
- lower-face underpaint/apron только вдоль shared interior edges;
- apron width derived from renderer differential calibration;
- apron полностью скрыт в ideal continuous scene;
- no exposed apron at gaps, exterior или junctions;
- PurePartition и SeamSafe profiles сравниваются на multiple renderers;
- exact intent может экспортировать оба файла, но user default выбирается frozen product gate.

Exporter не refit-ит geometry и не меняет visible scene. Он материализует sealed ExportPlan.

# 21. Главный orchestration pseudocode

```rust
fn vectorize(bytes: &[u8], req: &VectorizeRequest) -> VectorizeOutcome {
    let cfg = Config::resolve_and_seal(req)?;
    let source = match image::decode_canonical(bytes, &cfg) {
        Ok(v) => v,
        Err(e) => return VectorizeOutcome::Failed(e.into()),
    };

    let model_evidence = evidence::analyze_soft(&source, &cfg);
    let palettes = evidence::propose_palette_exterior_hypotheses(&source, &model_evidence, &cfg);
    let formations = formation::propose_minimal_global_hypotheses(&source, &model_evidence, &cfg);

    let mut envelope = HypothesisEnvelope::new(cfg.compute.total_budget);

    for palette in palettes {
        for formation in &formations {
            let mixture = match evidence::infer_premult_mixture(&source, &palette, formation, &cfg) {
                Ok(v) => v,
                Err(reason) => { trace_reject(reason); continue; }
            };

            for topo in topology::propose_envelope(&mixture, formation, &cfg) {
                let graph = match topology::build_robust_dcel(&topo, &cfg) {
                    Ok(g) => g,
                    Err(reason) => { trace_reject(reason); continue; }
                };

                let obs = match evidence::observe_boundaries(&source, &palette, formation, &graph, &cfg) {
                    Ok(v) => v,
                    Err(reason) => { trace_reject(reason); continue; }
                };

                // M5 may insert a proxy hypothesis; final success requires typed M6+ backend.
                let grammar_paths = fit::k_best_grammars(&graph, &obs, &cfg);
                for grammar in grammar_paths {
                    let mut scene = match fit::joint_refit_scene(graph.clone(), palette.clone(), formation.clone(), grammar, &obs, &cfg) {
                        Ok(v) => v,
                        Err(reason) => { trace_reject(reason); continue; }
                    };

                    scene = fit::primitive_relation_selection(scene, &source, &cfg);
                    scene = opt::refine_continuous(scene, &source, &cfg);
                    let children = opt::search_compound_transactions(scene, &source, &cfg);

                    for child in children {
                        let cert = verify::preseal_scene(&child, &source, &cfg);
                        if !cert.valid { trace_reject(cert.violations); continue; }

                        let bits = posterior::exact_bits(&child, &source, &cfg);
                        envelope.insert(SceneHypothesis::new(child, bits, cert), &cfg);
                    }
                }
            }
        }
    }

    if envelope.is_empty() {
        return classify_no_candidate(&source, &model_evidence, &cfg);
    }

    let confidence = confidence::evaluate(&envelope, &source, &cfg);
    if !confidence.allows_success() {
        return VectorizeOutcome::Ambiguous(envelope.into_ambiguity_report(confidence));
    }

    let winner = envelope.best_verified().unwrap();
    match svg::quantize_reverify_materialize_and_seal(winner, &source, &cfg) {
        Ok(sealed) => VectorizeOutcome::Success(sealed),
        Err(e) => VectorizeOutcome::Failed(e.into()),
    }
}
```

# 22. Premultiplied Flat2 evidence pseudocode

```rust
fn propose_flat2_models(img: &CanonicalImage, cfg: &Config) -> Vec<Flat2Evidence> {
    let interior = interior_confidence(img, cfg);
    let palette_pairs = robust_palette_exterior_hypotheses(img, &interior, cfg);
    let formations = minimal_formation_hypotheses(img, cfg);
    let mut out = Vec::new();

    for pair in palette_pairs {
        for f in &formations {
            let pf = f.to_observation_premul(pair.foreground);
            let pb = f.to_observation_premul(pair.background_or_exterior);
            let denom = norm2(pf - pb);
            if denom < cfg.evidence.min_conditioning { continue; }

            let mut alpha = Field::zeros(img.size());
            let mut residual = VectorField::zeros(img.size());

            for p in img.pixels() {
                let pi = f.decode_observation_premul(img.pixel(p));
                let a = clamp01(dot(pi - pb, pf - pb) / denom);
                alpha[p] = a;
                residual[p] = pi - (pb + (pf - pb) * a);
            }

            let likelihood = formation_likelihood(img, &alpha, &pair, f, cfg);
            let uncertainty = calibrate_conditional_uncertainty(
                &alpha, &residual, &pair, f, cfg
            );

            out.push(Flat2Evidence { pair, formation: f.clone(), alpha, residual, uncertainty, likelihood });
        }
    }

    retain_diverse_well_conditioned_models(out, cfg)
}
```

Color IRLS updates use only high-confidence interiors and rerun full formation likelihood. If no reliable core exists, color remains an interval/hypothesis and success confidence decreases.

# 23. Topology envelope pseudocode

```rust
fn propose_topology_envelope(ev: &Flat2Evidence, cfg: &Config) -> Vec<TopologyHypothesis> {
    let fields = build_restored_fields(ev, cfg);
    let mut candidates = Vec::new();

    for fh in fields {
        for connectivity in cfg.topology.complementary_connectivity_arms() {
            let complex = CubicalComplex::from_field(&fh.field, connectivity);
            let events = batch_critical_events(&complex, &fh.field, cfg);
            let levels = persistent_representative_levels(&events, cfg)
                .chain(levels_before_after_events(&events, cfg))
                .chain(cfg.topology.fixed_smoke_levels.iter().copied());

            for level in dedup_levels(levels) {
                for saddle in saddle_resolutions(&complex, level, ev, cfg) {
                    let labels = complex.threshold_with_resolution(level, saddle);
                    let sig = cubical_topology_signature(&labels, connectivity);
                    let candidate = TopologyHypothesis::from_events(
                        labels, sig, &events, &fh, connectivity, saddle, cfg
                    );
                    if candidate.minimum_evidence_feasible(ev, cfg) {
                        candidates.push(candidate);
                    }
                }
            }
        }
    }

    let valid = remove_invalid_and_exact_duplicates(candidates);
    let pareto = retain_pareto_topology_diversity(valid, cfg);
    prune_certified_dominated_then_budgeted(pareto, cfg)
}
```

Budget pruning report обязан указать потерянную estimated posterior mass и GT candidate recall на benchmark.

# 24. Typed grammar и joint-refit pseudocode

```rust
fn k_best_boundary_models(obs: &BoundaryObservation, cfg: &FitConfig) -> Vec<CurveChain> {
    let samples = physical_resample(obs, cfg.sample_step_px);
    let breaks = hierarchical_breakpoint_candidates(&samples, cfg);
    let corners = corner_proposals(&samples, cfg);
    let spans = generate_hierarchical_span_candidates(&samples, &breaks, cfg);

    let dag = build_jet_compatible_grammar_dag(spans, corners, cfg);
    let discrete_paths = k_shortest_paths(dag, cfg.k_discrete_paths);
    let mut chains = Vec::new();

    for path in discrete_paths {
        let init = materialize_discrete_grammar(path);
        match joint_constrained_refit(init, &samples, obs, cfg) {
            Ok(chain) if exact_g1_and_local_isotopy(&chain, obs, cfg) => chains.push(chain),
            _ => continue,
        }
    }

    rank_by_proposal_integral_and_code_length(chains, obs, cfg)
}
```

`joint_constrained_refit` optimизирует shared nodes/tangents всей chain одновременно. K-best нужен, потому что лучший coarse DP path может стать infeasible после exact constraints.

# 25. Continuous refinement pseudocode

```rust
fn refine_continuous(mut h: SceneHypothesis, target: &CanonicalImage, cfg: &Config) -> SceneHypothesis {
    for level in cfg.compute.coarse_to_fine_levels.iter().copied() {
        for _outer in 0..cfg.compute.max_outer_iters {
            let mut accepted = false;

            for block in optimization_blocks(&h.scene, level, cfg) {
                let parent_bits = posterior::exact_bits_scope(&h.scene, target, block.dependency_scope(), cfg);
                let snapshot = h.scene.snapshot(block.dependency_scope());
                let fixed_mesh = render::compile_fixed_mesh(&h.scene, block.dependency_scope(), level, cfg);
                let grad = surrogate_gradient(&h.scene, target, &fixed_mesh, &block, cfg);

                for step in trust_region_steps(&block, grad, &h.scene, cfg) {
                    h.scene.restore(snapshot.clone());
                    apply_projected_step(&mut h.scene, &block, step, cfg);

                    if !verify::local_combinatorial_and_geometric(&h.scene, block.dependency_scope(), cfg) {
                        continue;
                    }

                    let child_bits = posterior::exact_bits_scope_with_halo(
                        &h.scene, target, block.dependency_scope(), cfg
                    );
                    if child_bits + cfg.numeric.min_bits_improvement < parent_bits {
                        accepted = true;
                        update_render_and_posterior_caches(&mut h, block.dependency_scope(), cfg);
                        break;
                    }
                }

                if !accepted { h.scene.restore(snapshot); }
            }

            let full_bits = posterior::exact_bits(&h.scene, target, cfg);
            let cert = verify::preseal_scene(&h.scene, target, cfg);
            if !cert.valid { return h.rollback_last_verified(); }
            h.posterior_bits = full_bits;
            h.certificates = cert;

            if !accepted || converged(&h, cfg) { break; }
        }
    }
    h
}
```

# 26. Compound discrete search pseudocode

```rust
fn search_compound_transactions(
    seed: SceneHypothesis,
    target: &CanonicalImage,
    cfg: &Config,
) -> Vec<SceneHypothesis> {
    let mut beam = DiverseBeam::single(seed, cfg.compute.beam_width);
    let mut memo = SceneMemo::new();

    for _round in 0..cfg.compute.discrete_rounds {
        let mut proposals = Vec::new();
        for parent in beam.items() {
            for tx in propose_complete_transactions(parent, target, cfg) {
                let key = tx.canonical_key(parent.scene.digest());
                if memo.seen(&key) { continue; }

                let child = match apply_rebuild_joint_refit(parent, &tx, target, cfg) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if !child.certificates.valid { continue; }

                // Quality beam may retain near-best alternatives; Fast keeps strict improvements.
                if cfg.compute.retain_within_bits(parent.posterior_bits, child.posterior_bits) {
                    proposals.push(child.with_transaction(tx));
                }
                memo.insert(key);
            }
        }

        let next = beam.merge_and_select(proposals, cfg);
        if next.same_canonical_set(&beam) { break; }
        beam = next;
    }
    beam.into_items()
}
```

# 27. Benchmark, identifiability и oracle decomposition

Measurement строится до нового fitter и не меняется вместе с feature PR.

## 27.1. Corpus без inverse crime

Три независимых источника:

1. procedural GT scenes из grammar, написанной отдельно от production fit code;
2. hand-authored SVG corpus с legal provenance;
3. adversarial/metamorphic fixtures.

Rasterization выполняется несколькими independent engines/profiles. Splits держат целые source files/shape families/rasterizer profiles, а не соседние renders одного SVG.

Разделение данных трёхступенчатое:

1. `development` — разрешён для algorithm iteration;
2. `calibration` — только для frozen thresholds/noise/code tables/confidence policy;
3. `sealed_audit` — не открывается до release candidate. Любое изменение algorithm/config/gate после просмотра audit results сжигает этот audit split и требует нового untouched набора/version.

Нельзя многократно оптимизироваться по одному и тому же «held-out» leaderboard. Bucket boundaries, catastrophic taxonomy и pooling policy preregister до открытия sealed audit.

Каждый fixture содержит:

- visible partition truth;
- formation truth;
- допустимую scene equivalence class;
- identifiability label;
- salient/recoverable features;
- authored truth только как дополнительную диагностику.

Обязательно добавить intentionally ambiguous pairs: разные scenes, которые после degradation становятся indistinguishable. Правильное поведение — `ambiguous`, а не угадывание.

## 27.2. Degradation matrix

- sizes 16–512 с bucket-specific support contract;
- subpixel translations;
- multiple AA/rasterizer profiles;
- linear vs sRGB blending;
- box/triangle/Gaussian PSF;
- resize chains;
- JPEG/WebP позже;
- transparent/opaque exterior;
- contrast range;
- 8-bit quantization.

## 27.3. Baselines

Pinned internal baselines, VTracer/Potrace, manually captured commercial outputs where legally/operationally permissible. External tools — tournament opponents, не GT.

## 27.4. Metrics и selective reliability

Отдельные axes:

- topology/component/hole/fusion/split;
- boundary p50/p95/p99/max;
- primitive/family/breakpoint accuracy;
- G1/curvature/self-intersections;
- paint/palette;
- exact observation likelihood;
- scene code bits, paths/segments/anchors/bytes;
- runtime/memory;
- deterministic digest;
- posterior calibration;
- residual whiteness/block calibration and spatial overconfidence tests;
- supported-universe/search-completeness calibration;
- clustered per-source and per-render risk–coverage;
- sealed-audit generation / test-burn status;
- risk–coverage;
- ambiguity detection;
- posterior-predictive model mismatch;
- unexplored posterior-mass bound;
- human preference.

Promotion reports stratified by resolution, formation, contrast, topology complexity и identifiability. Среднее без tails не принимается.

### Statistical reliability gate

`Catastrophic accepted failure` фиксируется до теста и включает минимум:

- wrong visible component/hole/fusion/split вне допустимой equivalence class;
- exposed gap/seam/apron, который меняет читаемую форму;
- accepted self-intersection/boundary crossing;
- gross boundary outlier выше frozen p99/max gate;
- broken smooth G1, создающий видимый kink;
- wrong color/face omission выше salient-detail gate;
- serialized SVG, не соответствующий judged scene.

Не-catastrophic quality regressions публикуются отдельно и всё равно могут блокировать human gate.

Для accepted outputs считать one-sided Clopper–Pearson upper bound catastrophic risk. Gate и confidence level freeze заранее. Coverage gate отдельный, чтобы система не проходила через тотальный abstention.

### Независимая единица испытания и sample-size contract

Несколько resolutions/degradations одного source SVG коррелированы и не считаются независимыми Bernoulli trials. Primary reliability unit — уникальная source-scene family/group. Provisional conservative rule: source group считается catastrophic, если catastrophic defect возник хотя бы на одном accepted обязательном variant этого group. Альтернативный cluster model допустим только если preregistered и validated.

При нуле catastrophic failures для one-sided 99% Clopper–Pearson upper bound `<1%` нужно минимум **459 независимых accepted source groups**. При failures sample-size считается exact до открытия audit. Нельзя раздуть `n`, добавив сотни phase shifts одного логотипа.

Claims публикуются по bucket. Общий claim по нескольким buckets требует заранее определённой family-wise correction либо честной hierarchical/cluster model; post-hoc pooling запрещён. Coverage публикуется и per-source, и per-render.

### Human court

- digest-bound randomized A/B;
- left/right randomization;
- ties отдельно;
- minimum query count и analysis plan freeze до просмотра результатов;
- point estimate + confidence interval;
- single-user court маркируется user-preference gate, multi-rater — population gate.

## 27.5. Metamorphic/property/differential tests

Обязательные transformations:

- translation/padding;
- rotation/reflection;
- uniform scale;
- color permutation;
- foreground/background label swap;
- sample-density duplication;
- cyclic path cut;
- repeated run/platform tier.

Property/fuzz:

- DCEL assembly;
- robust intersections;
- serializer/parser;
- near-degenerate curves;
- malformed/oversized inputs;
- random quantization.

Differential:

- internal renderer vs independent references;
- canonical SVG parse/render roundtrip;
- PurePartition vs SeamSafe export profiles.

## 27.6. Factorial oracle suite

Simple sequential `O2-O1`, `O3-O2` зависит от intervention order. Поэтому partition и formation сначала исследуются полным 2×2:

```text
PF00: auto partition + estimated formation
PF10: GT partition   + estimated formation
PF01: auto partition + GT formation
PF11: GT partition   + GT formation
```

Для каждой metric публиковать partition main effect, formation main effect и interaction. Все arms используют один downstream backend/config/budget/fixtures.

Далее geometry decomposition при `GT partition + GT formation`:

```text
G00 auto candidates + auto selector
G10 oracle GT-compatible candidate injected + auto selector
G01 auto candidates + oracle selector over available set
G11 oracle candidate set + oracle selector
G20 forced GT-equivalent families/breakpoints + auto parameter fit
G30 GT parameters / no optimizer (renderer/serialization ceiling)
```

Интерпретация:

- `G10-G00`: candidate-generation ceiling;
- `G01-G00`: selector ceiling внутри auto set;
- `G11` показывает combined discrete ceiling;
- `G20` изолирует parameter fitting;
- controlled perturbation recovery из `G20/G30` изолирует optimizer.

Paint oracle добавляется в M8, formation expansion oracle — M9.

Compatibility key:

```text
backend_id + config_hash + candidate_budget + fixture_hash + intervention_schema_version
```

Несовместимые arms нельзя вычитать. Early reference-backend results provisional. Fake placeholder arms запрещены.

## 27.7. Frozen gates

Gate/config/noise/code tables меняются отдельным reviewed commit. Feature PR не может одновременно ослабить собственный gate.

# 28. Milestones и жёсткие гейты

## M0 — Bootstrap/provenance/baselines

- только нужные crates;
- source pins/manifest/licenses;
- `rust-toolchain.toml`, committed `Cargo.lock`, renderer/tool version manifest и reproducible command environment;
- CI, deterministic smoke runner;
- resource limits;
- baseline outputs.

**Stop после `STATUS_M0.md`.** Никакого algorithm port сверх минимального smoke. Builder report сам по себе не делает milestone green: отдельный reviewer/human запускает clean-checkout reproduction и подписывает gate review.

## M1 — Robust conventions + canonical IR + seal skeleton

- coordinates/color/topology conventions;
- exterior face/DCEL types;
- robust predicates adapter;
- geometry/paint/formation/hypothesis types;
- canonical shared-parameter serialization/digests.

Gate: property roundtrips, same-platform byte determinism, invalid graphs rejected.

## M2 — Certified partition renderer + serialized roundtrip

- signed-area polygon coverage port;
- bounded curve tessellation;
- per-face/exterior partition coverage;
- premult compositing;
- ROI/dependency tests;
- independent renderer differential;
- seal revalidation skeleton.

Gate: area/translation/half-pixel, partition sum, multiple rasterizers, no self-reference-only tests.

## M3 — GT/identifiability/scorecard

- heterogeneous corpus;
- equivalence/ambiguity fixtures;
- degradation matrix;
- baselines;
- risk–coverage and statistical court framework;
- `SupportedModelUniverseV1` schema/hash;
- residual-correlation benchmark and likelihood calibration protocol;
- frozen gates/code-table placeholders.

Gate: reports reproduce from clean checkout; no test leakage; sealed-audit burn policy active; source-group independence defined; supported universe is finite/versioned; correlation-aware likelihood protocol exists before any confidence claim.

## M3.5 — Factorial oracle harness

- O0/G30 renderer ceiling;
- PF 2×2 where reference backend supports honest injection;
- intervention schemas/compatibility keys;
- later arms `not_yet_applicable`.

Gate: no causal deltas across incompatible runs; inverse-crime warning visible.

## M4 — Flat2 palette + minimal formation + premult evidence

- exterior/full-bleed hypotheses;
- global minimal formation family;
- premult RGBA mixture;
- uncertainty/corridor calibration;
- boundary observations.

Gate: corridor coverage+sharpness on held-out rasterizer; formation factorial updated; transparent exterior correct; semi-transparent interiors rejected.

## M4.5 — Cubical event topology envelope

- complementary connectivity;
- max/min tree events;
- saddle alternatives/well-composed candidates;
- persistence plateaus;
- candidate envelope and recall metrics.

Gate: GT-equivalent topology **present in envelope** on identifiable supported fixtures; ambiguous fixtures retain alternatives; no magic-threshold-only architecture.

## M5 — Shared DCEL + safe dual/primal transactions

- robust DCEL;
- topology envelopes;
- polyline proxy bounds;
- local compound topology transactions;
- exact ROI with halo;
- topology/isotopy certificates.

Gate: no final-topology claim from proxy; candidate recall maintained after budget pruning; no unrelated graph mutation; no dangling/invalid faces.

## M6 — Typed grammar + k-best DP + joint G1 + explicit MDL

- hierarchical span candidates;
- candidate-generation budgets;
- k-best jet-compatible grammar paths;
- joint constrained chain refit;
- explicit code lengths;
- primitive/relation hypotheses.

Gate: exact G1 after joint solve; sample/cut/transform invariance; oracle G00–G20 decomposition; no BIC-only promotion.

## M7 — Exact posterior refinement + selective delivery + export materialization

- full-resolution correlation-aware likelihood;
- supported-universe/search-mass accounting;
- trust-region optimizer;
- compound discrete search;
- pre/post-quantization verifier;
- confidence/abstention;
- PurePartition/SeamSafe materialization;
- complete PF/G/O oracle rerun.

Первая user alpha только после:

- clustered source-level selective catastrophic-risk upper bound и per-source/per-render coverage gates green на untouched sealed Flat2 audit bucket;
- no accepted self-intersection/G1/topology corruption;
- better than best internal baseline on boundary tails and catastrophic defects without complexity explosion;
- statistical blind court green;
- no hidden fallback.

## M8 — Multiregion visible flat-color

- palette/partition alternation;
- RAG transactions;
- multi-face junction area fractions;
- paint oracle;
- shared multicolor DCEL.

## P1 — Partition correction API/editor

После M8: deterministic merge/split/assign/protect/restore edit script; affected graph reruns core. Не Bézier editor.

## M9 — Extended degradation/formation

- resize chain;
- broader PSF/kernel estimation;
- JPEG/WebP likelihood;
- formation calibration.

Clean gates must remain green.

## M10 — Stroke/line-art lane

Centerline/width/caps/joins/junctions с fill-vs-stroke model selection.

## M11 — Gradients

Solid/linear/radial classification, discontinuity-aware segmentation, stops, compact geometry. Flat path regression forbidden.

## M12 — Productization

WASM/UI/editor polish, performance, cross-platform determinism tier, explicit legacy wrapper, security, licensing/patent/FTO review before commercial release.

# 29. Reliability, quality и performance SLO

После M3 значения freeze отдельным commit. Provisional targets:

```text
Flat2 clean-AA identifiable @128–512:
  accepted catastrophic risk CP upper bound < 1%
  confidence level for bound: 99%
  selective coverage >= 80%
  accepted self-intersections = 0
  accepted G1 violations = 0
  accepted topology corruption = 0
  boundary p95 target <= 0.35 px
  primitive recovery target >= 95% where primitive is identifiable

Flat2 clean-AA identifiable @64:
  same catastrophic-risk target
  selective coverage >= 60%
  boundary p95 target <= 0.5 px
```

Ambiguous/information-lost cases оцениваются по correct abstention, а не как forced reconstruction failures.

Complexity:

- report Pareto curve likelihood vs bits;
- extra anchors разрешены только когда exact posterior их окупает;
- no uncontrolled O(N²/N³) candidate explosion;
- per-stage candidate/memory/time caps и budget-pruning diagnostics.

Performance provisional M7 research target на named reference CPU:

```text
512×512 Flat2 Quality p95 <= 10 s
512×512 Flat2 Fast    p95 <= 1 s
peak memory <= 1 GiB
```

Correctness важнее early speed, но algorithm с unbounded growth не проходит milestone.

# 30. CLI contract

Один milestone — один executable path. M7 CLI:

```bash
vicec vectorize input.png \
  --mode flat2 \
  --intent clean \
  --preset quality \
  --out out/sample
```

Outputs:

```text
result.svg
result.pure-partition.svg
result.scene.json
result.export-plan.json
result.report.json
result.render.png
trace/
```

Flags:

```text
--intent exact|clean
--preset fast|quality
--trace
--dump-candidates N
--strict
--milestone-debug <feature>      # marks run non-production
--fg/--bg/--exterior             # oracle/diagnostic only
```

Arbitrary `--config`/research override делает result `research_unsealed`; он не может считаться production `success` или участвовать в promotion gate.

# 31. Report contract

Report содержит:

```text
source/binary/config/source-pin/toolchain/environment hashes
sealed-audit generation + split/burn status
intent + compute preset
research override status
soft input model evidence
palette/exterior hypotheses
formation hypotheses and posterior
retained/pruned topology hypotheses
budget-pruned posterior mass estimate
selected grammar and k-best alternatives
posterior bits breakdown
model_universe_hash + search truncation/bound certificate
residual_model_id + empirical correlation length + whitening/block diagnostics
confidence by delivery-equivalence class
top2 class margin, retained-mass lower bound, unexplored-mass upper bound + BoundValue status
score DecisionIntervals and overlapping non-equivalent alternatives
entropy upper bounds, perturbation stability, posterior-predictive mismatch
identifiability bucket if GT
verifier pre/post quantization
export profile/aprons
internal and serialized renderer metrics
runtime/memory/candidate counts
all digests
status + typed reason
```

Нельзя писать `success`, если confidence, post-seal verification или serialized render court не прошли.

# 32. Coding и research rules

1. Один conceptual change на commit.
2. Measurement/test раньше algorithm change.
3. Port только через provenance manifest; donor code считается untrusted до собственных tests.
4. Никаких sample names/size hacks/hidden env vars.
5. Units в type/config names.
6. Robust predicates для topology.
7. No placeholder crates/APIs.
8. No UI/AI/gradients до M7.
9. No gate change в feature PR.
10. No single aggregate quality score.
11. No raw sample-count BIC final selection.
12. No double-counted pixel evidence.
13. No final G1 claim по angle tolerance.
14. No topology winner from M5 proxy.
15. No early hard unsupported from rule classifier.
16. No per-edge formation kernel.
17. No post-export geometry invention.
18. Verify after quantization and serialized roundtrip.
19. Report tails, risk–coverage и ambiguity.
20. New failure → general model/operator/test, не patch.
21. One agent run → one milestone → stop report.
22. Any research override marks output non-production.
23. Maintain `REQUIREMENTS_TRACEABILITY.md`: invariant → implementation → tests → milestone gate.
24. Maintain `FAILURE_LEDGER.md` and ADRs for architecture decisions.
25. Fuzz/property/metamorphic tests are required, not optional polish.
26. Distinct hypotheses with overlapping numerical score intervals are retained; tie-break cannot erase uncertainty.
27. Test/audit split cannot be reused after result-driven changes.
28. Correlated renders of one source do not inflate reliability sample size.
29. Milestone green requires independent clean-checkout review; author agent cannot self-certify.

# 33. Recommended commit sequence

```text
C001 M0 workspace, pins, manifests, CI, resource limits
C002 M0 baseline adapters + smoke report
STOP STATUS_M0

C003 M1 coordinate/color/topology conventions + robust predicates
C004 M1 canonical IR/hypothesis types
C005 M1 serializer/digests/property tests
STOP STATUS_M1

C006 M2 coverage renderer port
C007 M2 partition/exterior renderer + differential suite
C008 M2 quantized roundtrip skeleton
STOP STATUS_M2

C009 M3 heterogeneous GT + identifiability metadata
C010 M3 degradation/multiple rasterizers/baselines
C011 M3 scorecard/risk-coverage/human court/frozen gates
STOP STATUS_M3

C012 M3.5 factorial oracle framework + O0/PF provisional report
STOP STATUS_M3_5

C013 M4 palette/exterior hypotheses
C014 M4 minimal formation family
C015 M4 premult mixture/uncertainty
C016 M4 boundary observations/corridor calibration
STOP STATUS_M4

C017 M4.5 cubical max/min event trees + saddle hypotheses
C018 M4.5 topology envelope/recall/pruning report
STOP STATUS_M4_5

C019–C022 M5 robust DCEL + safe transactions
C023–C027 M6 typed grammar/joint G1/MDL
C028–C033 M7 posterior optimizer/verifier/confidence/export
```

Номера после C018 уточняются после gate review; не планировать speculative interfaces заранее.

# 34. Operational protocol для coding-агента

Bootstrap prompt содержит параметр:

```text
CURRENT_MILESTONE=M0
```

Агент читает весь spec, но реализует только `CURRENT_MILESTONE`. В конце он обязан:

- выполнить format/test/bench для milestone;
- создать `docs/STATUS_<M>.md`;
- обновить provenance/traceability/failure ledger;
- перечислить exact ports vs rewrites;
- показать generated artifacts и gate table;
- остановиться.

Следующий milestone запускается новым prompt только после отдельного review. Review выполняется human или независимым agent context, который:

- делает clean checkout;
- запускает documented commands без author caches;
- проверяет gate artifacts и negative tests;
- пытается воспроизвести минимум один failure/adversarial case;
- подписывает `docs/REVIEW_<M>.md` либо возвращает blockers.

Даже зелёный author gate не даёт агенту право самостоятельно продолжать. M2, M5 и M7 требуют отдельного numerical/topology red-team pass.

Первая задача сейчас: **только M0**. Никакого IR/renderer/evidence code сверх интерфейса, реально необходимого baseline runner.

# 35. Definition of first usable classical core

M7 считается первой пригодной Flat2 базой, когда core:

- восстанавливает или честно не принимает identifiable visible flat scenes;
- корректно моделирует opaque/transparent exterior и minimal formation;
- сохраняет topology в shared robust DCEL;
- выбирает typed grammar joint G1 solve и explicit MDL;
- улучшает exact serialized-render posterior;
- проходит pre/post-quantization verifier;
- выдаёт compact deterministic SVG и systematic seam-safe materialization;
- имеет calibrated success/ambiguous decision;
- выполняет selective-risk и coverage SLO;
- выигрывает best internal baseline на frozen GT tails и blind court.

До этого ML, semantic layers и gradient product work считаются distraction.

# 36. Stop/blocker conditions

Остановить ветку и написать blocker report, если:

- independent renderer court не подтверждает internal renderer;
- posterior objective не выражен в совместимых units;
- evidence повторно учитывает те же pixels;
- topology conventions/connectivity дают противоречивый Euler;
- GT-equivalent topology выпадает из envelope из-за proxy/budget pruning;
- G1 достигается только post-hoc weld с ухудшением evidence;
- quantization/serialization ломают topology или render;
- ROI delta расходится с full-image delta;
- optimizer exact posterior систематически ухудшается;
- success confidence некалиброван или проходит через abstention почти на всём corpus;
- quality зависит от sample-specific threshold;
- runtime/candidate count имеет неконтролируемый рост;
- commercial implementation требует код/данные с несовместимой лицензией или непроверенным IP status;
- human court опровергает machine improvement.

Blocker report лучше нового fallback layer.

# 37. Литературный и инженерный фундамент

Clean-room research basis:

1. J. R. Diebel, *Bayesian Image Vectorization: The Probabilistic Inversion of Vector Image Rasterization*, Stanford, 2008.
2. M. Yang et al., *Effective Clipart Image Vectorization through Direct Optimization of Bezigons*, IEEE TVCG 22(2), 2016, DOI `10.1109/TVCG.2015.2440273`.
3. M. Yang, H. Chao, *ECISER: Efficient Clip-art Image Segmentation by Re-rasterization*, CAD 58, 2015, DOI `10.1016/j.cad.2014.08.011`.
4. S. Hoshyari et al., *Perception-Driven Semi-Structured Boundary Vectorization*, ACM TOG 37(4), 2018, DOI `10.1145/3197517.3201312`.
5. E. Dominici et al., *PolyFit: Perception-Aligned Vectorization of Raster Clip-Art via Intermediate Polygonal Fitting*, ACM TOG 39(4), 2020.
6. J. Yang et al., *Subpixel Deblurring of Anti-Aliased Raster Clip-Art*, CGF 42(2), 2023, DOI `10.1111/cgf.14744`. Paper/data/code license проверять отдельно; использовать как research reference, не копировать restricted implementation.
7. R. Y. He, S. H. Kang, J.-M. Morel, *A Formalization of Image Vectorization by Region Merging*, SIAM JIS 18(3), 2025, DOI `10.1137/24M1696469`.
8. S. Chakraborty et al., *Image Vectorization via Gradient Reconstruction*, CGF 44(2), 2025, DOI `10.1111/cgf.70055` — gradients milestone.
9. C. Ballester, V. Caselles, P. Monasse, *The Tree of Shapes of an Image*, ESAIM COCV 9, 2003.
10. L. Latecki, *Multicolor Well-Composed Pictures*, Pattern Recognition Letters 16(4), 1995, DOI `10.1016/0167-8655(94)00104-B`.
11. J. R. Shewchuk, *Adaptive Precision Floating-Point Arithmetic and Fast Robust Geometric Predicates*, DCG 18(3), 1997, DOI `10.1007/PL00009321`.

Перед commercial release отдельно провести license, patent и freedom-to-operate review; технический spec не является юридическим заключением.

# 38. Финальная инструкция агенту

Собирай не trace-and-smooth demo, а **selective, causally diagnosable inverse rasterizer**.

Порядок истины:

```text
robust conventions
→ independent forward renderer court
→ identifiability-aware GT
→ factorial oracles
→ minimal formation + calibrated evidence
→ topology envelope, не ранний winner
→ shared robust DCEL
→ typed k-best grammar + joint G1
→ exact posterior/MDL
→ trusted refinement and compound transactions
→ post-quantization delivery verification
→ calibrated success/ambiguous decision
```

Надёжная система не обязана всегда отвечать. Она обязана почти никогда не выдавать плохой SVG под видом уверенного успеха — и отдельно доказывать, на какой доле supported inputs она способна ответить.
