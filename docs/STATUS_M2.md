# STATUS_M2 — Certified partition renderer + serialized roundtrip

Дата: 2026-07-26.
Spec: `VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`
(SHA-256 `652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1`).
Автор: coding-агент (Claude Code), single-milestone run по §34.
Стартовая точка: HEAD `f24a413` (M1 принят: REVIEW_M1 addendum, VERDICT
ACCEPT). Коммиты milestone: C015–C029.

> **Этот отчёт — author report. Он сам по себе НЕ делает M2 green.**
> Автор НЕ самосертифицирует milestone (spec §32 правило 29, §34). Для M2
> §34 требует сверх обычного независимого clean-checkout review ещё и
> **отдельный numerical/topology red-team pass** — то есть два независимых
> прохода: cold review + red-team. Ни один из них не выполнен на момент
> этого отчёта; gate G11 открыт. **M3 не начат и не разрешён.**

## 1. Что сделано

- **Долги REVIEW_M1, due на этом gate (C015–C021, отдельными
  коммитами):**
  - **M1-N2** (C015): единственный конструктор детей
    `exec::sanitized_command`; git- и tool-version-дети получают политику;
    ADR-0007 переписан с фактическим охватом и историей.
  - **M1-N7** (C016): эталон segment-intersection переписан независимой
    параметрической формулировкой (i128-рациональный solve, без общей
    orientation-декомпозиции с библиотекой).
  - **M1-N8** (C017): rejection-free генератор `point_on_segment_is_exact`
    (`prop_flat_map`); PROPTEST_CASES=20000 без настройки харнесса.
  - **M1-N9** (C018): критерий §32 п.7 в ADR-0005 («семантика типа
    принадлежит милестоуну», а не «есть call site»).
  - **M1-N4** (C019): переносы отслеживаются явными строками —
    таблица «Перенесённые обязательства» (D-1 N3-split + env.json split,
    D-2 B1/B2, D-3 license-precondition) в REQUIREMENTS_TRACEABILITY.
  - **M1-N10** (C021): `ValidatedScene` newtype реализован (второй call
    site — renderer); предусловие валидации выражено типом; ADR-0005
    addendum. (C020 — подготовительный чистый перенос interference-этапа в
    свой модуль: validate.rs был 859 LOC > 800-правила §4.1.)
  - **M1-N5, M1-N6** закрыты в составе renderer-а — см. ниже.
- **Certified tessellation (C022–C023).** `vice-geom::flatten`:
  Quad/Cubic/CircularArc/EllipticArc → polyline с СЕРТИФИЦИРОВАННЫМ
  chord-error bound (chord-interpolation bound `h²/8·max|B''|`, точная
  сагитта, operator-norm отображение эллипса; f64 eval-guard; вывод —
  ADR-0008). Типизированный `ChordTolerancePx`. `vice-render::RenderMesh`:
  фиксированная tessellation — одна polyline на boundary (обе faces
  обходят ТЕ ЖЕ вычисленные точки, реверс на twin-стороне: cracks
  непредставимы), битовый pass-through shared endpoints, budget honesty
  (превышение бюджета на cap — typed `BudgetExceeded`, не молчаливая
  недопоставка).
- **Exact signed-area coverage (C024).** `vice-render::coverage`: точный
  ∫∫ winding по пикселю для замкнутых loops (точные трапеции по cell
  pieces; детерминированная right-to-left свёртка; §5.5 fixed orders).
  Gate «area/translation/half-pixel»: half/quarter-pixel прямоугольники —
  ТОЧНОЕ f64-равенство; произвольные оффсеты — аналитическая площадь;
  circle→πr² в пределах certified area budget; трансляционная
  непрерывность; битовая инвариантность целочисленного сдвига на
  диадической решётке + typed bound вне её (**F-0006** в FAILURE_LEDGER).
  **M1-N6 закрыт**: политика вне-canvas геометрии — documented clip
  (ADR-0009), с тестами всех сторон и straddling-случая.
- **Partition renderer + M1-N5 (C025).** `vice-render::partition` +
  `embedding`: coverage КАЖДОЙ face (включая exterior = окно + её
  негативные loops) считается независимо, потому что сумма signed
  windings — алгебраическое тождество и «проверкой» не является
  (ADR-0010). Hard-проверки на каждом рендере: (a) certified
  loop-orientation (shoelace ± (tessellation budget + f64 bound); ровно
  один положительный loop у bounded face; exterior — только
  отрицательные; неуверенность = typed `UncertifiableLoopOrientation`,
  не угадывание) — ловит B4-класс REVIEW_M1; (b) per-pixel range-check
  независимых coverage — ловит B2-класс (остров-в-острове на exterior:
  exterior уходит в −1); (c) per-pixel sum-check (1±1e-9). **Вместе (a)+(b)
  — закрытие M1-N5 как hard gate на единственном исполняемом
  render/seal-пути M2**; `validate_scene` (замороженный M1-контракт)
  сознательно не расширялся — обоснование в ADR-0010. Композитинг —
  ПРЯМАЯ premultiplied-сумма `Σ coverage·premul(paint)`; на triple
  junction fractions суммируются напрямую; тест фиксирует аналитические
  fractions 0.375/0.375/0.25 и доказывает, что painter-style pairwise
  compositing дал бы другой результат. Typed resource limit до аллокаций.
- **ROI с dependency closure (C026).** Замыкание = row band (зависимость
  пикселя — только от edge pieces его строки и колонок правее);
  реализация считает band тем же кодом, что строки полного рендера →
  «ROI == full render в окне» ПОБИТОВО (тест `to_bits`-равенства, окна
  через shared edge / junction / дыру / 1 пиксель / band без далёкого
  острова). Область действия сертификатов явная: ориентация — глобально,
  range/sum — в окне (ADR-0011).
- **Independent differential court (C027, §16.3).** Два независимых
  внешних растеризатора: `tiny-skia` 0.11.4 и `raqote` 0.8.5
  (dev-dependencies; лицензии BSD-3-Clause проверены по фактическим
  пакетам — THIRD_PARTY_NOTICES §2b) + чисто математический эталон
  (64×64 binary sampling аналитического диска). Протокол: каждая bounded
  face рендерится отдельно как nonzero path из ТОЙ ЖЕ фиксированной
  tessellation; native-curve arm (tiny-skia сам флэттенит quad/cubic)
  накрывает и наш tessellation budget. Метрики типизированы
  (max/mean |Δcoverage|); пороги ЗАМОРОЖЕНЫ из измеренных значений с
  задокументированным запасом (полная таблица измерений — ADR-0012).
  Ключевые исходы: pixel-aligned сцены совпали ТОЧНО (0.0 — пиксельные
  конвенции трёх рендереров тождественны), half-pixel — ровно 8-битное
  округление альфы, наклонные ребра — AA-модели эталонов (tiny-skia
  ≤0.1275, raqote ≤0.2318). Превышение порога = находка §36, не повод
  расширить порог.
- **Seal revalidation skeleton (C028, §20.2 в M2-объёме).**
  `seal_render_cycle`: f64 validate → canonical bytes (+scene digest) →
  parse → re-validate → render → render digest → re-serialize (байты
  самовоспроизводятся) → re-parse → re-render (render digest байт-в-байт).
  Детерминизм цикла и label-инвариантность sealed digests — тестами;
  golden scene+render digests заморожены для lines-only и Bézier-сцены
  (рендер-путь этих сцен — только IEEE-операции и sqrt, так что
  Linux-CI — реальная кросс-платформенная проба; расхождение = находка).
  Квантование/ExportPlan/SVG — НЕ в M2 (M7+), как и предписывает §28.
- Governance: ADR-0008..0012 + addendum-ы ADR-0005/0007;
  REQUIREMENTS_TRACEABILITY блок M2 (M2-1…M2-13) + таблица переносов;
  THIRD_PARTY_NOTICES §2b; FAILURE_LEDGER F-0006; PORTING_MANIFEST —
  **по-прежнему 0 units** (clean-room вместо порта: у v-ice нет license
  grant — REVIEW_M0 усл. 6; спековский «coverage port» выполнен как
  clean-room реализация, кандидаты остаются reference-only);
  COORDINATE_CONVENTION §1a (конвенция ориентации loops).

## 2. Итоги проверок (author-side, эта машина)

```text
cargo fmt --all --check                                  OK
cargo clippy --workspace --all-targets -- -D warnings    OK (0 warnings)
cargo test --workspace                                   173 passed / 0 failed
```

Разбивка: vice-bench 20 (13 unit + 6 cli + 1 child-env); vice-geom 37
(31 unit вкл. flatten + 6 property); vice-ir 71 (18 unit + 7 property +
3 golden + 43 validate_rejects); **vice-render 45** (15 coverage unit +
4 mesh_gate + 4 coverage_gate + 11 partition_gate + 4 roi_gate +
3 differential_court + 4 seal_gate). `#![forbid(unsafe_code)]` во всех
четырёх crates (court-растеризаторы — внешние dev-deps, их код не наш);
модулей >800 LOC нет (максимум — тестовый validate_rejects.rs 863;
продакшн-максимум runner.rs 786; validate.rs разгружен до 742 в C020).
Никаких env-флагов поведения; все пороги — константы кода с ADR.

## 3. Gate table (author-side; spec §28 M2 + REVIEW_M1 замечания)

| # | Gate | Статус | Evidence |
|---|---|---|---|
| G1 | **Area / translation / half-pixel** | PASS | `coverage.rs` unit + `coverage_gate.rs`: half/quarter-pixel — точные f64-равенства; πr² в пределах certified budget (три tolerance); аналитические площади при произвольных фазах; continuity; диадический сдвиг побитово (недиадический — typed bound, F-0006) |
| G2 | **Partition sum** (per-pixel ≈1, no hidden gaps/overlaps, triple junction прямой суммой, premult compositing) | PASS | `partition_gate.rs`: scene zoo (empty/rect/shared-edge/donut/junction/arcs/off-canvas); range+sum на каждом рендере с типизированными порогами (ADR-0010); аналитические junction-fractions и доказательство «не painter» |
| G3 | Bounded curve tessellation с certified budget | PASS | `vice-geom::flatten` (bounds выведены в ADR-0008, сверены независимой брутфорс-метрикой), `mesh_gate.rs` budget honesty |
| G4 | ROI с dependency closure; ROI == full render в окне | PASS | `roi_gate.rs` — ПОБИТОВОЕ равенство; ADR-0011 |
| G5 | **Multiple rasterizers** (§16.3 independent court) | PASS | tiny-skia + raqote + аналитический sampling-эталон; замороженные пороги из измерений; ADR-0012 |
| G6 | **M1-N5 (hard):** геометрия вложения/ориентации loops | PASS (в M2-архитектуре: hard на render/seal-пути) | `embedding.rs` + range-check; B4 → `ExteriorPositiveLoop`, B2 → `PartitionRangeViolation`, сомнение → `UncertifiableLoopOrientation`; ADR-0010 фиксирует, почему НЕ в `validate_scene` (M1-контракт и golden заморожены; §32 п.9) |
| G7 | **M1-N6:** политика вне-canvas — выбрана и записана | PASS | ADR-0009 (documented clip) + тесты clip/straddling/partition |
| G8 | Seal skeleton §20.2: цикл детерминирован, после parse рендер побайтово тот же | PASS | `seal_gate.rs` (двойной цикл, golden digests, relabeling-инвариантность, typed rejects) |
| G9 | Долги M1-N2, N4, N7, N8, N9, N10 | PASS | C015–C021; построчно в traceability M2-9…M2-12 |
| G10 | **No self-reference-only tests**; clean-room; 0 ported units | PASS | Аналитические эталоны (πr², точные площади, junction-fractions), независимая брутфорс-метрика расстояний, i128-парам. solve, два внешних растеризатора, sampling-эталон; PORTING_MANIFEST — 0 units |
| G11 | Независимый REVIEW_M2 (clean checkout) + numerical/topology red-team pass (§34) | **ОТКРЫТ — блокирует M3** | автор не самосертифицирует; оба прохода не выполнены |

## 4. Известные ограничения (честная граница M2)

1. **«Exact» = exact для ФИКСИРОВАННОЙ tessellation** (§16.1): кривые
   несут certified chord/area budget, не нулевую ошибку. Бюджеты
   консервативны (могут быть в разы больше фактической ошибки) — это
   честная сторона неравенства.
2. **Формация не рендерится**: только Box (exact area coverage);
   Triangle/Gaussian → typed `UnsupportedPixelFilter` (M4 formation).
   PSF/квантование/ExportPlan/SVG-writer — M4/M7+; canonical SVG-экспорта
   в M2 нет (суд получает полигоны напрямую).
3. **Embedding-проверки живут на render/seal-пути**, не в
   `validate_scene`: parse валидной-но-невложимой сцены (B2/B4-класс)
   по-прежнему успешен как ПАРСИНГ; любой render/seal её отвергает.
   Reviewer M2 должен явно оценить это архитектурное решение (ADR-0010).
4. **Render digest байт-в-байт — для канонической формы**; рендеры
   разных labelings совпадают в пределах partition tolerances (f64
   ассоциативность; задокументировано в `seal.rs`). Tier A determinism;
   для сцен с дугами (libm sin/cos) кросс-платформенная байтовая
   стабильность НЕ обещается, замороженные golden-digests дуг не содержат.
5. **Certified curve-curve intersection** по-прежнему нет (uncertified
   worklist M1 остаётся честным UNDETERMINED); сцена с реально
   пересекающимися кривыми валидируется, но render отвергает её
   range/sum-проверками только если пересечение создаёт measurable
   overlap. Полная машинерия — M5+ (DCEL rebuild).
6. **ROI-сертификаты локальны окну** (range/sum) — документировано и
   протестировано; полная сертификация = полный рендер.
7. Разрешение court-эталонов ограничено их 8-битной альфой и
   AA-моделями (пороги — ADR-0012); суд ловит грубые ошибки (~0.5), а
   суб-1e-3 точность доказывается аналитическими gate-тестами.

## 5. Blockers перед M3 (и наследуемые)

1. **G11**: независимый clean-checkout REVIEW_M2 + отдельный
   numerical/topology red-team pass (§34). До их подписи M3 не разрешён.
2. **D-1 (M1-N4/N3-split)**: разведение hashes.json/env.json на
   нормативную/информационную части — одной правкой с pre-M3 перезаписью
   baseline-ов (см. таблицу переносов в REQUIREMENTS_TRACEABILITY).
3. **D-2 (B1/B2 из M0)**: v-ice build (dav1d либо reviewed-смена пина) и
   явный asset-pin для Vice- — reviewed-решения ДО M3 (baseline-ы входят
   в scorecard §27.3, т.е. прямо в объём M3).
4. **D-3**: первый `[[unit]]` в PORTING_MANIFEST — только после
   license/IP review донора (REVIEW_M0 усл. 6; в M2 соблюдено: 0 units).
5. M3 потребует GT-корпус и rasterizer-профили; court-инфраструктура M2
   (tiny-skia/raqote + метрики) переиспользуема, но corpus/splits/burn
   policy — отдельная работа §27, не начатая здесь.

## 6. Явное заявление об остановке

Автор НЕ самосертифицирует M2. Gate G11 открыт до подписи независимого
REVIEW_M2 и red-team pass (spec §32 правило 29, §34). Никакой код M3
(GT-корпус, identifiability-метаданные, scorecard, degradation matrix)
не начат: в workspace ровно четыре crates (vice-bench, vice-geom,
vice-ir, vice-render).

**STOPPED AFTER M2 — M3 NOT STARTED.**
