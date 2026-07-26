# STATUS_M1 — Robust conventions + canonical IR + seal skeleton

Дата: 2026-07-26.
Spec: `VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`
(SHA-256 `652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1`).
Автор: coding-агент (Claude Code), single-milestone run по §34.
Стартовая точка: HEAD `57c8ff4` (M0 принят независимым REVIEW_M0, verdict
ACCEPT). Коммиты milestone: C006–C010; post-review дельта: C012–C013.

> **Этот отчёт — author report. Он сам по себе НЕ делает M1 green.**
> Автор НЕ самосертифицирует milestone (spec §32 правило 29, §34): M1
> считается green только после независимого clean-checkout review с
> подписью в `docs/REVIEW_M1.md`. **M2 не начат и не разрешён.**

> **Статус review (2026-07-26):** независимый REVIEW_M1 (C011, по HEAD
> `ba619bf` = C010) вернул **REJECT** с единственным блокером **M1-N1**
> (непредставленный junction принимался молча; `docs/REVIEW_M1.md` §5).
> 9 из 10 содержательных строк gate подтверждены ревьюером, включая
> доказательство тотальности канонического порядка и Linux-совпадение
> golden digest. **Блокер снят коммитом C012** (инвариант chain-point
> distinctness, репро-сцены C1/C4 + самокасание как негативные тесты,
> golden bytes/digest не изменились, PORTING_MANIFEST — 0 units);
> **ожидает re-review по дельте** (REVIEW_M1 §9: полная репродукция не
> требуется). Замечания M1-N2/N4..N9 — due на gate M2, см. §5.

## 1. Что сделано

- **Конвенции (§5), C006/C007.** `docs/COORDINATE_CONVENTION.md` +
  код: единственный transform-модуль `vice-geom::coords` (x вправо, y
  вниз, pixel `[x,x+1]×[y,y+1]`, центр `+0.5`, canvas `[0,W]×[0,H]`, f64);
  цветовая конвенция `vice-ir::color` (linear straight RGBA канонически,
  premultiplied для observation, `BlendSpace{LinearLight,EncodedSrgb}`,
  RGB под `α=0` — не evidence, IEC sRGB transfer с точным u8-roundtrip);
  digital topology `vice-ir::connectivity` (complementary connectivity,
  одинаковая связность обеих сторон непредставима) и exterior как
  настоящий `FaceId`; правила §5.5 (`-0`/NaN/Inf запрещены, ordered
  structures, shared parameters один раз — структурно). Все конвенции
  покрыты unit/property тестами.
- **Robust predicates adapter (§5.4), C006.** Тонкий typed слой
  `vice-geom::predicates` над внешним OSS-crate `robust` 1.2.0 (Rust-порт
  Shewchuk predicates; лицензия MIT OR Apache-2.0 проверена по пакету,
  THIRD_PARTY_NOTICES). Typed `Orientation`/`CirclePosition` (+`None` для
  вырожденного треугольника), точные segment-предикаты. Near-degenerate
  unit-тесты + property-сверка с независимой exact-i128 реализацией на
  диадической решётке. Донорский код НЕ портирован (REVIEW_M0 усл. 6):
  PORTING_MANIFEST — по-прежнему 0 units, кандидаты записаны как
  reference-only.
- **Canonical IR (§6) в M1-объёме, C007.** Crates `vice-geom`, `vice-ir`
  (и только они). `Segment{Line,CircularArc,EllipticArc,Quad,Cubic}` с
  endpoint-параметризацией без дублирования endpoints,
  `JoinKind{Corner,SmoothG1{shared tangent}}`, `CurveChain`;
  `PlanarGraph{exterior,vertices,boundaries,half_edges,faces}`,
  `Boundary{left/right,start/end,curve}`, `HalfEdge{twin,next,face}`,
  `Face{loops,paint}`, `Paint{OpaqueSolid(LinearRgb),TransparentExterior}`
  (прозрачные hole-faces легальны); `GlobalFormationHypothesis` — ровно
  минимальная M4-family (§10.1/§16.2); `VectorScene`. Speculative API
  сознательно НЕ создан: `SceneHypothesis`/`EvidenceRef`/`SupportRef`/
  `SceneProvenance`/`DecisionInterval`/`ExportPlan` не имеют в M1 ни
  одного producer/call site (§32 п.7; ADR-0005).
- **Валидация invalid graphs (§12), C007.** Typed reject
  (`SceneError`/`GraphError`), не panic: id-диапазоны; twin-инволюция;
  два владельца каждой interior boundary / interior+exterior у border;
  face-side consistency; `next`-перестановка с геометрической
  непрерывностью; замкнутые циклы = face loops; no dangling cracks;
  isolated vertices/дубликаты запрещены; Euler `V−E+F=1+C` (отклоняет
  непланарные rotation systems — тест «тор из трёх петель»); сегментная
  геометрия (радиусы, представимость дуг, канонические диапазоны углов);
  пересечения non-adjacent boundaries: ТОЧНО для line-line пар (robust
  predicates, включая collinear overlap за общей вершиной), для кривых —
  bbox-сертификат непересечения либо ЯВНЫЙ undetermined-worklist
  `uncertified_interference_pairs()` (граница M1 честно задокументирована,
  ADR-0005).
- **Canonical serialization + digests (seal skeleton, §5.5), C008.**
  Canonical bytes = чистая функция контента: canonical relabeling
  (вершины по позиции, boundaries по (start,end,curve), faces
  exterior-first по loop-keys, half-edges boundary-major), компактный
  canonical JSON с schema-тегом `vice-classic/scene/v1`, sha256 digest.
  Строгий парсер (deny_unknown_fields, дубликаты полей, schema check,
  полная валидация, `1e999` → reject). Property-тесты: roundtrip
  побайтово, determinism повторной сериализации, инвариантность к
  пермутации порядка построения всех четырёх entity-векторов, rejects.
  Golden-артефакт + замороженный digest. Реальный сбой поймали gate-тестом
  (serde_json lossy float parsing) — **F-0005** в FAILURE_LEDGER, общее
  правило выведено.
- **REVIEW_M0 условие 5, C009 (отдельный коммит в vice-bench).**
  (a) `verify-corpus --config` берёт limits из конфига (N4);
  (b) фиксированная санация env дочерних процессов + `command_env`-срез в
  env.json (N6). ADR-0007. Записанные `docs/baselines/M0/**` не тронуты;
  все существующие тесты vice-bench зелёные.
- Governance: ADR-0004..0007; REQUIREMENTS_TRACEABILITY блок M1
  (M1-1…M1-10); THIRD_PARTY_NOTICES (robust, proptest); FAILURE_LEDGER
  F-0005; PORTING_MANIFEST — 0 units.

## 2. Итоги проверок (author-side, эта машина)

```text
cargo fmt --all --check                                  OK
cargo clippy --workspace --all-targets -- -D warnings    OK (0 warnings)
cargo test --workspace                                   115 passed / 0 failed
```

Разбивка тестов (после C012): vice-geom 26 (20 unit + 6 property);
vice-ir 70 (18 unit + 42 в `validate_rejects.rs` — **39 негативных +
3 позитивных контроля** (счётчик уточнён по REVIEW_M1 M1-N3: до C012 было
36 негативных + 2 контроля, а не «38 негативных») + 7 property + 3 golden);
vice-bench 19 (12 unit + 6 cli + 1 child-env). `#![forbid(unsafe_code)]`
во всех crates (плюс workspace-level forbid); модулей >800 LOC нет.

## 3. Gate table (author-side)

| # | Gate (spec §28 M1 + REVIEW_M0 §6 усл. 5) | Статус | Evidence |
|---|---|---|---|
| G1 | Координатная/цветовая/топологическая конвенции зафиксированы, в одном модуле, покрыты тестами | PASS | `vice-geom/src/coords.rs`, `vice-ir/src/{color,connectivity}.rs`, docs/COORDINATE_CONVENTION.md; unit-тесты |
| G2 | Robust predicates adapter, проверенная реализация, near-degenerate тесты | PASS | `vice-geom/src/predicates.rs` (robust 1.2.0, лицензия verified), `tests/predicate_props.rs` (exact-i128 reference), ADR-0004 |
| G3 | Canonical IR типы M1-объёма, ноль placeholder-ов/speculative API | PASS | `vice-ir/src/{curve,graph,scene}.rs`, ADR-0005; ревью: нет типов без call sites |
| G4 | **Invalid graphs rejected** (typed, §12) | PASS (после C012; на C010 опровергнут REVIEW_M1 M1-N1, ожидает re-review по дельте) | `vice-ir/src/validate.rs` вкл. chain-point distinctness (`UnrepresentedJunction`); 39 негативных тестов + 3 позитивных контроля в `validate_rejects.rs` (по одному сломанному инварианту, вкл. Euler/torus и репро-сцены C1/C4 ревьюера) |
| G5 | **Property roundtrips**: parse→re-serialize побайтово, digest стабилен, битые числа/графы/байты отклоняются | PASS | `tests/canonical_props.rs` (proptest), unit canonical.rs |
| G6 | **Same-platform byte determinism** (Tier A): повторная сериализация и пермутация порядка построения → те же байты/digest; golden bytes+digest заморожены | PASS | property permutation-invariance; `tests/golden_digest.rs` + `tests/golden/scene_v1.json` |
| G7 | REVIEW_M0 усл. 5a (N4): `verify-corpus --config` | PASS | `cli.rs::verify_corpus_takes_limits_from_config` (0/1/2 exit-сценарии) |
| G8 | REVIEW_M0 усл. 5b (N6): санация child-env + срез в env.json | PASS | `exec.rs`/`envinfo.rs`, `tests/child_env.rs`; M0-артефакты не тронуты |
| G9 | fmt / clippy -D warnings / все тесты workspace | PASS | §2 |
| G10 | Provenance: 0 ported units; новые внешние crates с проверенными лицензиями | PASS | PORTING_MANIFEST.toml (0 units + candidates note), THIRD_PARTY_NOTICES.md |
| G11 | Независимый REVIEW_M1 подписан | **ОТКРЫТ — блокирует M2**: REVIEW_M1 (C011) = REJECT по блокеру M1-N1; блокер снят C012; ожидает re-review по дельте C012–C013 (REVIEW_M1 §9) | `docs/REVIEW_M1.md` (9/10 строк gate подтверждены) |

## 4. Known limitations (честная граница M1)

Независимая проверка этой границы — `docs/REVIEW_M1.md` §5/§8: ревьюер
подтвердил пп. 1–7 как честно задокументированные, а сверх них нашёл
блокер M1-N1 (непредставленный junction), **закрытый коммитом C012**
(инвариант chain-point distinctness: любой стык обязан быть общей
graph-вершиной; статус — REJECT снят C012, ожидает re-review по дельте).

1. **Curve–curve intersection не сертифицируется.** Для пар сегментов с
   кривыми M1 даёт либо bbox-сертификат непересечения, либо ЯВНЫЙ
   undetermined-worklist (`uncertified_interference_pairs`). Полная
   certified-машинерия (analytic predicates / certified subdivision) — M2+.
   Самопересечение смежных кривых сегментов (общий узел) вдали от узла —
   вне §12-инварианта, проверяется геометрической verification M2+.
2. **G1-согласованность не валидируется.** `SmoothG1` хранит shared
   tangent parameter, но соответствие tangent ↔ геометрия сегментов не
   проверяется: exact G1 по spec возникает из joint constrained refit
   (M6), а не из проверки угла. Bezier handles хранятся абсолютными
   точками; polar-параметризация от shared tangent — решение M6 (§18.1).
3. **Геометрическая ориентация loops** (outer vs holes по signed area)
   не проверяется — нужна curve-машинерия M2+; M1 проверяет только
   комбинаторную согласованность сторон + Euler-планарность.
4. **Canonical form не факторизует visible-equivalence:** два разных
   графа, описывающих одну видимую сцену (например, соседние faces
   одинаковой краски), имеют разные canonical bytes. Delivery-equivalence
   — M7-концепция; M1 гарантирует детерминизм для ДАННОГО графа.
5. **Tier A determinism only:** байтовая идентичность заявлена для одной
   платформы/бинаря; cross-platform Tier B — M12. (Фактически REVIEW_M1 §2
   зафиксировал совпадение golden digest на Linux CI — результат сильнее
   заявленного, но Tier B по-прежнему НЕ обещается.)
6. Квантование параметров перед seal (M7) в M1 отсутствует — скелет
   сериализации/digest готов, политика precision не выбрана.
7. `ambient_overrides_present` в env.json фиксирует присутствие, но не
   значения ambient-переменных (сознательно; ADR-0007).

## 5. Blockers перед M2 (и наследуемые)

1. **Re-review дельты C012–C013** — единственный открытый блокер самого
   milestone: REVIEW_M1 (C011) вернул REJECT по M1-N1; блокер снят C012,
   по §9 review требуется повторный review только по дельте (полная
   репродукция не требуется). До подписи обновлённого REVIEW_M1 M2 не
   разрешён.
2. **Замечания REVIEW_M1, due на gate M2** (не блокируют M1;
   REVIEW_M1 §9 пп. 4–11 — здесь фиксируются, НЕ исполняются сейчас):
   - **M1-N2:** политика child-env не применяется к git-/tool-version-детям
     — применить либо переформулировать «без исключений» в ADR-0007;
   - **M1-N4:** N3-split hashes.json + разведение env.json на
     нормативную/информационную части — при pre-M3 перезаписи baseline-ов
     (перенос срока отслеживается здесь явной строкой);
   - **M1-N5:** геометрическая проверка вложения/ориентации loops —
     hard-требование gate M2, не бессрочное «известное ограничение»;
   - **M1-N6:** выбрать и записать политику по геометрии вне canvas до
     появления renderer-а;
   - **M1-N7:** заменить эталон segment-intersection на независимую
     формулировку (§28 M2 «no self-reference-only tests»);
   - **M1-N8:** перегенерировать `point_on_segment_is_exact` без
     `prop_assume`, чтобы property-тест масштабировался;
   - **M1-N9:** уточнить в ADR-0005 критерий применения §32 п.7
     («семантика типа принадлежит милестоуну», а не «есть call site»).
3. **B4 — ЗАКРЫТ** (REVIEW_M1 §2): CI исполнился на обоих push-ах
   (`d08f7d6`, `ba619bf`), оба run-а success на `ubuntu-latest`, включая
   первую кросс-платформенную пробу golden digest — совпал.
4. **B1/B2 (M0, дедлайн до M3):** v-ice build_failed baseline и
   несамодостаточный Vice- pin — reviewed-решения до M3; вместе с этой
   перезаписью выполняется N3-split hashes.json (нормативная/
   информационная секции) — см. REPRODUCIBILITY_M0.md и M1-N4 выше.
5. Для M2 (renderer) потребуется решение о порте coverage-кода доноров:
   без license grant для v-ice порт запрещён (REVIEW_M0 усл. 6, повторено
   REVIEW_M1 §9 п.12) — планировать clean-room либо внешние crates заранее.

## 6. Явное заявление об остановке

Автор НЕ самосертифицирует M1. Gate G11 открыт до подписи обновлённого
REVIEW_M1 по дельте C012–C013 (spec §32 правило 29, §34). Никакой код M2
(renderer/coverage/tessellation и т.д.) не начат: в workspace ровно три
crates (vice-bench, vice-geom, vice-ir).

**STOPPED AFTER M1 — M2 NOT STARTED.**
