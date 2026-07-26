# REVIEW_M3 — независимый clean-checkout review milestone M3

## 0. Контекст ревьюера

Холодный независимый контекст (Opus). Автором milestone-а не являюсь, его вердикт мне не сообщался; всё ниже — мои собственные прогоны и измерения. Прочитаны: spec v1.3 §27 целиком, §28 (M0–M3.5), §1.5, §17.1, §29, §31, §32, §34, §36; предыдущие gate-документы (REVIEW_M0/M1/M2_A/M2_B, REDTEAM_M2 с четырьмя addendum-ами, FAILURE_LEDGER включая мета-правила M-1…M-4) — как методология и как перечень повторяющихся классов ошибок, но не как источник выводов. `docs/STATUS_M3.md` сверялся ПОСЛЕ того, как собственные прогоны были закончены.

### Проверенный объект

```text
spec        VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md
            sha256 652FD0B6E17C96C38AF0173DDCC93A3921EAFD60A9AFF34C8D848829228D9BB1
            == ожидаемому 652fd0b6…9bb1                                        OK

clone       git clone C:\Users\nirrt\Toolset\vice-classic %TEMP%\m3rev
HEAD        be3c40d4a8ade2dfa32e7e0c24bc160514409f00   («C078 M3(governance): STATUS_M3 and the stop»)
            == ожидаемому be3c40d                                              OK
база пути   C:\Users\nirrt\AppData\Local\Temp\m3rev  (короткая — F-0004)
диапазон    153b70b..be3c40d = 22 коммита, C057–C078                           OK
рабочее     git status --porcelain -> 0 строк после всех прогонов              OK
харнесс     %TEMP%\m3adv  (отдельный crate `m3adv`, зависит от vice-bench по path)
```

Основной репозиторий не изменялся; клон и харнесс сохранены.

---

## 1. Выполненные команды и exit-коды

Все прогоны — из чистого клона, без авторских кэшей (`target/` создан с нуля).

| # | Команда | exit | время | результат |
|---|---|---|---|---|
| 1 | `cargo fmt --all --check` | 0 | 1 с | чисто |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 14 с | **0 warning / 0 error** |
| 3 | `cargo test --workspace` (прогон 1) | 0 | 129 с | **300 passed / 0 failed / 0 ignored** |
| 4 | `cargo test --workspace` (прогон 2) | 0 | 105 с | **300 / 0** — тот же состав |
| 5 | `cargo test --release --workspace` | 0 | 55 с | **300 / 0** |
| 6 | `gt-corpus verify --manifest docs/gt/CORPUS_MANIFEST.json` | 0 | 274 с | `corpus reproduced: 1086 renders`, `corpus_hash 5067b378…e224da` |
| 7 | `gt-corpus audit-status --seal … --manifest … --gates …` | 0 | <1 с | `generation 1 status Sealed`; `sealed and never opened` |
| 8 | `gt-corpus report --manifest … --gates … --seal … --out …` | 0 | 276 с | 5/5 `[MET]`; выданный scorecard **побайтово равен** `docs/gt/SCORECARD_M3.json` |
| 9 | `gt-corpus gates-check --changed crates/…/gates.rs --changed docs/STATUS_M3.md` | 0 | <1 с | нарушения нет |
| 10 | `gt-corpus gates-check` на файловом наборе C072 | **1** | <1 с | `spec §27.7 violation` — см. M3-N2 |
| 11 | `gt-corpus build --scope test` ×2 | 0 | 1 с | два файла с одинаковым sha256 `0CC6E5C4…5601A` |
| 12 | `gen-smoke --out tests/fixtures/smoke --check` | 0 | 2 с | 5/5 OK (путь M0 цел) |
| 13 | `baseline-runner verify-corpus --corpus … --manifest … --config …` | 0 | 2 с | 5/5 Ok |
| 14 | `baseline-runner selftest --out runs/selftest …` | 0 | 1 с | `runner pipeline deterministic across 2 repeats` |
| 15 | `baseline-runner env --normative` | 0 | <1 с | нормативная проекция печатается |

Итог: заявленные автором 300/0 в обоих профилях **воспроизведены точно**; `cargo test --workspace` дважды даёт одинаковый состав; M0/M1/M2-пути живы.

### Разбивка 300 тестов (сверена мной по выводу, debug)

`99 + 1 + 8 + 31 + 6 + 6 + 18 + 7 + 3 + 43 + 18 + 5 + 4 + 3 + 5 + 9 + 8 + 4 + 14 + 4 + 4 = 300`. В release суммирование даёт те же 300 (другой порядок бинарей). 207 → 300, то есть M3 добавил 93 теста.

### Ключевые измерения, перепрогнанные мной с `--nocapture`

```text
measured: supersample max 0.018635 edge-mean 0.011819; vice-render max 3.664e-15
tiny-skia:  max 0.0793 edge-mean 0.0226
raqote:     max 0.2387 edge-mean 0.0804
hole 0.200 render px: vs 1.4x rival   2, label InformationLost
hole 0.400 render px: vs 1.4x rival  10, label Identifiable
hole 0.600 / 1.000 / 2.000 / 4.000 px: 22 / 61 / 102 / 204, все Identifiable
aa-model-disagreement@64:      corr-len x=1 y=1, iid overcount 1.0x
formation-mismatch-blur@64:    corr-len x=2 y=2, iid overcount 9.0x
```

Все совпадают с числами, записанными в REQUIREMENTS_TRACEABILITY (0.0186 / 3.7e-15), в F-0016 (кривая 2/10/22/61/102/204) и в STATUS_M3 (corr-len 2, overcount 9×). Расхождений нет.

---

## 2. CI

`https://api.github.com/repos/lomatoq/vice-classic/actions/runs` — всего 15 прогонов, все `success`. Прогон **#15** на `be3c40d4a8ade2…9f00` — `completed / success`, event `push`. Требование выполнено.

Изменение CI относительно M2 (`git diff 153b70b..be3c40d -- .github/workflows/ci.yml`) — **усиление, а не ослабление** (§32 п. 9), проверено построчно:

- добавлен шаг `cargo test --release --workspace` (раньше был только debug);
- добавлен целый job `gt-corpus`: frozen-gate rule §27.7, sealed-audit burn policy, две сборки корпуса с побайтовым `diff`, scorecard + gate table;
- `verify-corpus` получил `--config` (раньше лимиты не читались из конфига);
- `env` получил дополнительный прогон `--normative`;
- `fetch-depth: 2` добавлен, чтобы правило §27.7 могло сравниться с родителем.

Ни один шаг не удалён и ни одна проверка не ослаблена. **Но** у нового job-а есть два измеренных мной провала покрытия — M3-N2 и M3-N7 ниже.

---

## 3. Gate §28 M3 по существу — мои собственные измерения

Строка гейта §28 M3: *«reports reproduce from clean checkout; no test leakage; sealed-audit burn policy active; source-group independence defined; supported universe is finite/versioned; correlation-aware likelihood protocol exists before any confidence claim»*.

### 3.1. reports reproduce from clean checkout — **ВЫПОЛНЕНО**

Прогнал документированные команды из `docs/REPRODUCIBILITY_M3.md` §«GT-корпус» на своём клоне (другая база пути, свежий `target/`):

```text
gt-corpus verify --manifest docs/gt/CORPUS_MANIFEST.json
  -> corpus reproduced: 1086 renders
  -> corpus_hash: 5067b3789836fcfbac6f0c35259b7c6b20d5e7cf6ea5ed935df890f506e224da
```

Хеш совпадает с `SCORECARD_M3.json → hashes.corpus`. `report` выдал файл, **побайтово равный** закоммиченному `docs/gt/SCORECARD_M3.json` (сравнил целиком). Смоук-корпус M0 регенерируется побайтово (`gen-smoke --check` 5/5). Две сборки `--scope test` дали одинаковый sha256.

Проверил состав манифеста арифметикой, независимо от манифеста: матрица `matrix_v1()` = spine (6 размеров × 5 профилей − 3 supersample при size>64 = 27) + excursions (2 якоря × 7 = 14) + PSF (2 × 3 = 6) = **47** ячеек; `fast` = размеры ≤32 без supersample-box-спайна = 8 + 7 + 3 = **18**. Рендеры: exact-clip 9 ячеек × 63 сцены = 567; supersample 3 × 63 = 189; raqote 2 × 63 = 126; vice-render 2 × 63 = 126; tiny-skia 2 × (63 − 24 development-сцен) = 78. Сумма **1086** — сходится с манифестом до штуки.

### 3.2. no test leakage — **ВЫПОЛНЕНО** (главный пункт, проверен четырьмя способами)

Все числа — мои, из `docs/gt/CORPUS_MANIFEST.json` и из харнесса.

**(а) Ни одно семейство не попадает в два split-а.** 24 семейства, из них **0** straddling. Имена namespace-ированы по источнику (`polygon…star` — процедурные, `authored/<stem>`, `adversarial/<name>`, `ambiguity/<name>`), поэтому пересечения имён между источниками невозможны.

```text
procedural  polygon, star, two_islands, triple_junction        -> development
procedural  annulus, bezier_blob, l_shape                      -> calibration
procedural  arc_disk, dot_cluster, nested_island, shared_edge, thin_bridge -> sealed_audit
authored    lobed, pennant -> development;  keyhole, twotone -> calibration;  bracket, leaf -> sealed_audit
adversarial checkerboard, sliver, ambiguity/hole, ambiguity/paint -> development;
            near-tangent, ambiguity/topology -> calibration
```

**(б) Назначение не переставляется при росте корпуса.** Добавил в своей копии 200 новых имён семейств и перепроверил старые: **0 семейств сместилось**. Механизм — чистая функция FNV-1a(salt+family) % 100, не зависящая ни от состава корпуса, ни от порядка обхода, ни от HashMap. На 2000 синтетических семейств распределение 1097/411/492 = 54.9/20.6/24.6 % — то есть сама хеш-функция даёт заявленные 55/20/25 асимптотически (о реализованных долях см. M3-N9).

**(в) Утечка через растеризатор-профиль.** `tiny-skia` в development: **0 рендеров**; вне development: **78**. То есть held-out профиль и правда held out, и проверка не вакуумна.

**(г) Утечка через тождественные байты.** Это единственный канал, который проверка «по семействам» не закрывает конструктивно, поэтому измерил сам:

```text
distinct scene digests    63 из 63 сцен;  общих между split-ами: 0
distinct render digests  899 из 1086;     общих между split-ами: 0
```

187 повторов render-digest-ов существуют (одна и та же сцена под ячейками, различающимися в метаданных, но не в пикселях), но **ни один** не пересекает границу split-а. Прямой утечки «одинаковый ответ по обе стороны» нет.

**(д) Единый генератор для процедурного и authored корпуса.** Проверил: `gt::authored` читает шесть SVG-файлов из `tests/fixtures/gt/authored/` собственным строгим парсером подмножества и не вызывает ни одной функции `gt::grammar`/`gt::recipes`, кроме `flat2_formation` и `AUTHORING_CANVAS_PX` (константа холста и общая formation-гипотеза — это не генератор геометрии). Провенанс-дыры «оба источника — один генератор» нет.

### 3.3. sealed-audit burn policy active — **НЕ ВЫПОЛНЕНО** (блокер, M3-N1)

Механизм записан правильно: открытие фиксирует три хеша, `AuditSeal::check` сравнивает их на каждом прогоне, `SEALED` → `StillSealed` (скоринг отказывает), пустая запись → `IncompleteRecord`, три ветви `ChangedAfterOpening` покрыты юнит-тестом. Попытки обхода:

- **скорить по sealed-у прямо сейчас** — отказ (`Err(StillSealed)`), CI это печатает;
- **изменить corpus/prereg/gates после открытия** — вот здесь механизм ломается. Я записал OPEN-seal, положив в `corpus_hash` **ровно тот хеш, который печатает `gt-corpus build` и который записан в `SCORECARD_M3.json`** (`5067b378…e224da`), корпус не трогал вообще, и прогнал штатную CI-команду:

```text
$ gt-corpus audit-status --seal <opened> --manifest docs/gt/CORPUS_MANIFEST.json --gates configs/GATES_V1.toml
audit generation 1 status Opened
BURN POLICY VIOLATION: sealed-audit generation 1 was opened at corpus
5067b3789836fcfbac6f0c35259b7c6b20d5e7cf6ea5ed935df890f506e224da,
but the current value is 304759fa703d6f8b6fff3f3e5d3969adcd5e1dea8d4e3ff3d9bbf4aab1461d48
```

Ничего не менялось. Подробности и причина — M3-N1.

### 3.4. source-group independence defined — **ВЫПОЛНЕНО**

Проверил, что единица испытания ВЫЧИСЛЯЕТСЯ, а не декларируется. Мой прогон:

```text
3 группы × 50 фазовых сдвигов = 150 рендеров -> trials 3, accepted 3,
                                 CP upper 0.784557, deficit 456
те же 3 группы по 1 рендеру   ->               CP upper 0.784557 (совпадает бит в бит)
```

То есть 150 коррелированных рендеров не покупают ни одной десятой процента границы. `group_verdicts` агрегирует по `group_id`, non-mandatory и inverse-crime отбрасываются до агрегации, «одна катастрофа на принятом mandatory-варианте осуждает группу» реализовано буквально.

**Sample-size contract: 459 — вывел сам, не процитировал.** Замкнутая форма нулевых отказов: `CP_upper = 1 − (1−c)^(1/n)`; требуется `< 0.01` при `c = 0.99`, то есть `n > ln(0.01)/ln(0.99) = 458.21`, значит `n = 459`. Реализация даёт то же с точностью до 1e-9:

```text
n=457 -> 0.010026356 (не проходит)   n=458 -> 0.010004575 (не проходит)
n=459 -> 0.009982887 (проходит)      n=460 -> 0.009961294
ceil(ln(0.01)/ln(0.99)) = 459
```

Дефицит не замаскирован: во всех пяти преregистрированных buckets scorecard пишет `groups_accepted 0`, `catastrophic_risk_upper 1.0`, `required_groups 459`, `group_deficit 459`, `contract_met false`. Ни одного reliability-claim в артефактах M3 нет.

### 3.5. supported universe is finite/versioned — **ВЫПОЛНЕНО** (с оговоркой M3-N8)

`check_finite` невакуумен: семь клауз, каждая ломается собственным контрпримером в тесте. Связь с IR — исчерпывающий `match` в шести `ir_*_family` (новый вариант IR = ошибка компиляции) плюс обратный тест «каждое имя объявлено». `admissible` совпадает с тем, что рендерер реально исполняет (`pixel_filters` = только `box`).

Каноничность `model_universe_hash` проверил сам:

```text
frozen                              fed2af8642ee3bdd6be85fead97f5ae834622ad0f68525cad9889d17845b8f5d
+1 к topology.max_visible_faces     меняется
изменение содержания на 1e-13       c4ee0a03…  меняется
перестановка двух segment families  a97fb90d…  меняется, хотя семантика та же
                                    (check_finite при этом по-прежнему Ok)
`:null` в каноническом JSON         отсутствует -> не-конечных величин нет
```

Чувствительность к содержанию — есть, включая 1e-13. Перестановочной инвариантности нет (M3-N8), но это консервативная сторона ошибки: она даёт ложную тревогу, а не молчание.

### 3.6. correlation-aware likelihood protocol before any confidence claim — **ВЫПОЛНЕНО для исполнимых путей**

Benchmark содержателен и контролируется в обе стороны (белый шум меряется белым, гладкое поле — коррелированным); измерение на корпусе я воспроизвёл: AA-расхождение corr-len 1 / overcount 1.0×, несовпадение формации corr-len 2 / overcount 9.0×. Запрет `iid_pixel` обоснован тем случаем, который в нём нуждается, а не лозунгом.

Guard отвергает: без модели, с `iid_pixel` даже при `calibrated = true`, и с допустимой но некалиброванной моделью. `scorecard::build` всегда получает отказ и кладёт его в `confidence.refusal`; ни один артефакт M3 не содержит confidence-числа.

**Попытка найти путь, выдающий confidence-число без протокола.** Нашёл один, но он не исполним в M3: `reliability::risk_coverage` — публичная функция, которая при 459 сфабрикованных принятых группах возвращает `catastrophic_risk_upper = 0.00998289`, `group_deficit = 0`, `contract_met = true`, ни разу не обратившись к `guard_confidence_claim`. Это статистическая граница §27.4 (Bernoulli по группам), а не posterior §17.1, и её производство — как раз то, что §27.4 требует; но заявление STATUS_M3 «guard отвергает **все** пути M3» шире, чем измеренное: guard — это функция, которую надо не забыть позвать, а не тип, без которого число нельзя построить. В M3 ни один исполнимый путь `RenderOutcome` не производит, поэтому гейт §28 я считаю выполненным, а формулировку — см. M3-N6.

---

## 4. Отдельные проверки по существу

### 4.1. Inverse crime (§27.1) — enforced кодом, с одной дырой в GT-истине

- `RasterProfile::is_inverse_crime()` — свойство типа; `DegradationCell::is_inverse_crime()` берётся из профиля; `RenderedFixture.inverse_crime` — из ячейки. В манифесте **0 рендеров** с флагом, расходящимся с `cell_id` (проверил все 1086).
- `reliability::group_verdicts` и `risk_coverage` выбрасывают `inverse_crime` **до** агрегации; проверено моим прогоном (`groups_total` не считает inverse-crime-группу).
- Профиль не исключён из корпуса, а помечен и измерен: `vice-render` расходится с точным интегратором на 3.664e-15 — и это ровно объяснение, почему он не GT: такое согласие меряет общие допущения.
- Процедурная грамматика **не** переиспользует production-генераторы: `gt::build::SceneBuilder` — собственный region-first строитель; фикстуры `vice-render`/`vice-ir` в `gt` не импортируются (проверил импорты всех модулей `gt/`).
- **Дыра:** одно поле GT-истины производит сам production-рендерер. `PartitionTruth::measure` вычисляет `exterior_visible_px2` вызовом `vice_render::render_mesh_partition`, и все 63 значения лежат в манифесте и хешируются в `corpus_hash`. См. M3-N5.

`tiny-skia` held out of development — подтверждено (3.2в).

### 4.2. Scope `fast` (18 из 47) — матрица НЕ сужена, подделка не проходит

- `matrix_v1()` содержит все 47 ячеек и покрывает все оси §27.2; JPEG/WebP отсутствуют, и это проверяется тестом.
- `--scope full` доступен; `cells` записан в манифест; хеш зависит от scope.
- **Атака.** Сфабриковал манифест, объявляющий 47 ячеек при 18 ячейках рендеров, и подал `verify`:

```text
render digests match; metadata differs (scope, splits or truth)
corpus did NOT reproduce           exit 1
```

Подделать «полный» прогон не удалось. Второй вариант — подмена одного render-digest-а на нули — тоже пойман, с именем фикстуры:

```text
differs: adv/ambiguous/bridge-or-gap#bridged s16_pexact-clip_… recorded 0000… actual e57526f4…
corpus did NOT reproduce           exit 1
```

Замечание к `report`: он манифест НЕ валидирует — пересобирает корпус по списку ячеек и хеширует пересборку. Scorecard, построенный из подделанного манифеста, вышел **побайтово равным** закоммиченному, с gate table 5/5 `[MET]`. Подлога это не даёт (пересборка честная), но `hashes.corpus` в scorecard-е — хеш свежей пересборки, а не аттестация проверяемого артефакта; см. M3-N7.

### 4.3. D-2 (ADR-0014) — ancestry перемерил сам; главное верно, сопутствующее — нет

В зеркале `C:\Users\nirrt\Toolset\v-ice` (read-only):

```text
git merge-base 9211b321 59ab86d1                -> 49f308d9fef48d475546692f00b0f62d884c5a8c
git merge-base --is-ancestor 9211b321 59ab86d1  -> exit 1  (НЕ предок)
git merge-base --is-ancestor 59ab86d1 9211b321  -> exit 1  (НЕ предок)
git rev-list --left-right --count 9211…...59ab… -> 1  1
git diff --stat 49f308d9 59ab86d1               -> Cargo.lock -261, Cargo.toml ±1  (2 файла)
```

**Автор прав в том, на чём стоит решение:** пины — сиблинги с общим родителем `49f308d9` и расхождением 1↔1, «просто передвинуть пин» было недоступно, поэтому добавление второго объявленного пина вместо замены audit anchor §2 — правильный ход, и я его подтверждаю.

**Автор неправ в сопутствующем утверждении, и оно записано в трёх местах.** См. M3-N4: относительно того, что зеркало считает upstream, соотношение обратное.

### 4.4. Asset-pin для `Vice-` — механизм состоятелен

Проверил `assets.rs` целиком. Длина и sha256 проверяются **на источнике до копирования**, поэтому несовпавший файл в checkout не попадает вовсе (тест это ассертит явно). Длина проверяется первой (читаемый диагноз для «не тот файл»), хеш — вторым (случай «та же длина, другое содержимое» покрыт отдельным тестом). Экранирующие пути отвергаются на СТРОКЕ, с обоими разделителями, и — что важно по M-1 — на **обоих** полях (`path` и `source`), а не только на предъявленном; девять форм экранирования перечислены. `stage` отказывает и при прямом вызове, не только при загрузке конфига. Provenance-дыру это не открывает: единственный вход для вне-деревянного файла — объявленный `[[baseline.asset]]` с хешем и длиной, а `AssetRecord` попадает в нормативную часть `hashes.json`. `--asset-root` — явный CLI-флаг, не скрытый env (§32 п. 4).

Эффект (по `docs/baselines/M3/`): Vice- 6/10 → 10/10; сам ассет (104 MiB) в репозиторий не кладётся, что честно и записано.

### 4.5. M2-A-N8 / `CertifiedMesh` — заявление сужено и проверено

Тип имеет приватные поля и ровно два конструктора, оба прогоняют `CanvasTooLarge` → `check_numeric_domain` → `verify_embedding`; аксессоров `&mut` нет; `render_mesh_partition`/`…_roi` принимают только его. Получить `CertifiedMesh` без сертификации нельзя — проверил по всем путям конструирования, включая `certify(mesh, opts)` с руками испорченным `RenderMesh` (отказ `CanvasTooLarge`).

Заявление **уже** доказанного, и это зафиксировано тестом `certification_does_not_claim_the_faces_tile_the_window`: B2-класс (вложенный остров, проводом к экстерьеру) сертифицируется и падает на попиксельном range check. Доккомментарий типа перечисляет CLAIMS/DOES NOT CLAIM явно. Побайтовая неизменность рендера после введения витнеса проверена на четырёх сценах и на ROI. Это ровно тот класс, за который автор уже получал находки, и здесь он закрыт правильно.

F-0015 показывает, что автор сам же переобещал витнес через один коммит и поймал это собственным тестом; выведенное правило («любое `expect`, ссылающееся на сертификат, обязано цитировать точную формулировку») сформулировано как класс, а не как заплатка.

### 4.6. Долги red team (REDTEAM_M2 addendum 4)

- **п. 2 — закрыт (C076).** k 24 → 96, допуск 0.1 → 0.05, добавлен ассерт `ARBITER_TOLERANCE >= 4.0 / ARBITER_RATE`. Мои числа: `0.05 ≥ 0.041667` — выполняется. Но комментарий рядом заявляет доминирование «~5×» (это `0.05 / (1/96) = 4.8×`), тогда как **ассертируемая** модель даёт `0.05 / (4/96) = 1.20×`. См. M3-N10.
- **п. 3 — закрыт как документированный предел.** `REPRODUCIBILITY_M3` §«Что при сверке может законно отличаться» прямо пишет: render-digest-ы сравнимы только в пределах одной версии renderer-а, с числами C052 (0.38 % значений, max |Δ| 5.25e-14). Именно та оговорка, которой red team и требовал, ровно там, где M3 вводит сравнение digest-ов.
- **п. 1 (четвёртый метод) — остаётся ОТКРЫТЫМ и записан честно**, не снят тихо: `STATUS_M3` §5 п. 8 («Свидетельств нет, но и проверки нет»), `STATUS_M3` §6 п. 2 (в списке blockers перед M3.5), и в теле коммита C076 («Debt 1 … remains open»). Проверил все три места — формулировки согласованы и остаточный риск назван (согласованная ошибка всех трёх инструментов, при которой арбитраж не запускается).

### 4.7. F-0015 / F-0016 / F-0017 — правила выведены общими

- **F-0015** (переобещанный витнес): правило — «любое `expect` со ссылкой на сертификат обязано цитировать точную формулировку; если цитаты нет в доккомментарии типа — это не сертификат, а надежда». Класс, не адрес.
- **F-0016** (порог откалиброван не на тот вопрос): правило — «назвать ВОПРОС до калибровки и проверить, что измеряемая величина отвечает именно на него; „что-то изменилось“ почти никогда не тот вопрос». Обе опровергнутые формулировки оставлены в доккомментарии теста; я воспроизвёл всю кривую и обе стороны порога.
- **F-0017** (оценщик корреляции): правило — «инструмент, измеряющий структуру, обязан иметь контроль в обе стороны и быть проверен на том, что считаемая величина несёт искомую структуру; знаковое поле и его модуль — разные величины». Оба контроля реализованы тестами.

Все три — самонайденные, все три дают общий вывод. Мета-правила M-1…M-4 в FAILURE_LEDGER выведены из повторений, а не из единичных случаев. Это сильная сторона milestone-а. Ирония в том, что M-1 («правило применяется к классу, а не к перечню») нарушен внутри самого M3 — см. M3-N3.

### 4.8. Замороженное M0/M1/M2, порт, лицензии, гигиена

| Проверка | Результат |
|---|---|
| `docs/baselines/M0/**` | **не тронуты** (нет в diff 153b70b..be3c40d) |
| `docs/REPRODUCIBILITY_M0.md` | +10 строк — баннер «исторический документ», артефакты не тронуты; приемлемо |
| `docs/STATUS_M2.md` | изменена **одна строка** (G11 ОТКРЫТ → ЗАКРЫТ, C057). Подписанный отчёт больше не побайтово тот, что подписывали ревьюеры M2; помечено как «запись факта». См. M3-N11 |
| `crates/vice-geom`, `crates/vice-ir` | **не тронуты вообще** |
| `crates/vice-render` | тронуты `certified.rs` (новый), `partition.rs`, `roi.rs`, `lib.rs`, тесты — рефакторинг D-4, побайтовая неизменность рендера доказана тестом |
| замороженные golden digests | не сдвинулись (300/0 включает seal/digest-гейты M1/M2) |
| `PORTING_MANIFEST.toml` | **0 units**, файл в diff отсутствует; но не обновлён под M3 — см. M3-N12 |
| скрытые env-флаги | нет. `std::env` встречается 4 раза: отчёт о PATH, проверка ambient-переменных в `envinfo`, `env!("CARGO_PKG_VERSION")`. Поведение флагами окружения не управляется |
| `#![forbid(unsafe_code)]` | во всех четырёх crates + workspace lint `unsafe_code = "forbid"` |
| продакшн-модули > 800 LOC | нет. Максимум **779** (`vice-bench/src/runner.rs`), затем 775 (`vice-render/src/coverage.rs`), 752 (`gt/degradation.rs`). STATUS называет максимумом 752 — см. M3-N13 |
| placeholder API для M3.5/M4 | нет: `todo!`/`unimplemented!`/`PF00`/`intervention_schema` в коде отсутствуют. Отсутствующее названо данными в `scorecard.not_yet_produced`, а не типами |

**Лицензии — проверил ФАКТИЧЕСКИ по распакованным пакетам реестра**, а не по crates.io:

```text
~/.cargo/registry/src/**/tiny-skia-0.11.4/Cargo.toml       license = "BSD-3-Clause"  texts=[LICENSE]
~/.cargo/registry/src/**/tiny-skia-path-0.11.4/Cargo.toml  license = "BSD-3-Clause"  texts=[LICENSE]
~/.cargo/registry/src/**/raqote-0.8.5/Cargo.toml           license = "BSD-3-Clause"  texts=[LICENSE.md]
~/.cargo/registry/src/**/sw-composite-0.7.16/Cargo.toml    license = "BSD-3-Clause"  texts=[]   <-- текста нет
```

Это в точности то, что THIRD_PARTY_NOTICES §2c пишет, включая честную оговорку про `sw-composite` («re-check before any redistribution»), которая повторена именно потому, что крейт перешёл из dev- в обычный граф.

**Не линкуются в core-crates — проверил `cargo tree -e normal`:** `vice-geom`, `vice-ir`, `vice-render` в нормальном графе содержат только `robust`, `serde`, `serde_json`, `sha2`, `hex`, `thiserror` и их транзитивы. `tiny-skia`/`raqote` там **отсутствуют**. Заявление верно.

Potrace (GPL-2.0-or-later) — только внеprocess, никогда не линкуется; на машине не установлен, записан типизированным `binary_missing`, а не тихо выброшен. Это правильное обращение с §36.

---

## 5. Adversarial-проверки (мои; требовалось четыре, сделал тринадцать)

| # | Атака | Результат | Вывод |
|---|---|---|---|
| A1 | Открыть audit «правильным» хешем и прогнать штатный CI-шаг | **BURN POLICY VIOLATION при неизменённом корпусе** (`5067b378…` записано, `304759fa…` пересчитано) | **находка, блокер (M3-N1)** |
| A2 | Сравнить два способа посчитать corpus hash | `CorpusManifest::hash()` = порядок объявления полей; `audit-status` = `to_string` разобранного `serde_json::Value`, где ключи BTree-сортированы | причина A1 |
| A3 | Скорить sealed_audit-группы, не трогая seal | 22 группы, `CP upper 0.188869`, seal не спрошен ни разу | наблюдение (M3-N6) |
| A4 | Получить проходное reliability-число без протокола | 459 групп → `CP upper 0.00998289`, `contract_met true`, guard не вызван | наблюдение (M3-N6) |
| A5 | Протащить inverse-crime-рендер в выборку | `RenderOutcome { cell_id: "…vice-render…", inverse_crime: false }` **считается** — флаг задаёт вызывающий, а не профиль | наблюдение (M3-N5) |
| A6 | Утечка между split-ами: семейство, сцена, байты | 0 / 0 / 0 (см. 3.2) | защищено |
| A7 | Сдвинуть корпус ростом: 200 новых семейств | 0 смещений | защищено |
| A8 | Сломать канонический хеш universe | содержание (даже 1e-13) меняет; перестановка полей списка тоже меняет; `:null` отсутствует | защищено, оговорка M3-N8 |
| A9 | Сломать хеш preregistration | **коллизия: `+inf`, `−inf`, `NaN` в `boundary_p95_px` дают ОДИН хеш** `ea04a7b4…`, и `check()` принимает все три | **находка (M3-N14)** |
| A10 | Выдать частичный прогон за полный (`cells` = 47, рендеры = 18) | `verify` → exit 1, «metadata differs» | защищено |
| A11 | Подменить один render digest | `verify` → exit 1, называет фикстуру | защищено |
| A12 | Скорить по подделанному манифесту через `report` | scorecard **побайтово равен** честному, gate table 5/5 MET | наблюдение (M3-N7) |
| A13 | «Разоткрыть» seal (вернуть `status: sealed` руками) | `audit-status` печатает «sealed and never opened», exit 0 | наблюдение (M3-N15) |

---

## 6. Построчная валидация gate table `docs/STATUS_M3.md` §4

| # | Заявление автора | Моя проверка | Вердикт |
|---|---|---|---|
| G1 | Гетерогенный корпус; equivalence/ambiguity fixtures | 3 источника, 60 групп (48/6/6), 63 сцены; 3 ambiguity-группы, у каждой 2 сцены и equivalence class; коллапс и различимость ассертятся; метаморфные свойства инструмента прогнаны | **подтверждаю** |
| G2 | Degradation matrix §27.2 | 47 ячеек, посчитал независимо; все оси присутствуют; JPEG/WebP отсутствуют и это ассертится; оси проверены в ПИКСЕЛЯХ (PSF монотонно расширяет переход, blend-space меняет байты на двухцветной фикстуре, контраст сжимает separation) | **подтверждаю** |
| G3 | Три-ступенчатый split; no test leakage | 0 straddling семейств, 0 общих scene-digest-ов, 0 общих render-digest-ов между split-ами, 0 tiny-skia в development, 0 смещений при +200 семейств | **подтверждаю** |
| G4 | Sealed-audit burn policy active | SEALED — да, скоринг отказывает — да; но сверка corpus-хеша сравнивает величину, которую не производит ни один компонент: ложный BURN при неизменном корпусе | **НЕ подтверждаю — M3-N1** |
| G5 | Preregistration до открытия audit | 5 buckets, 7 catastrophic kinds с `measured_by`, pooling policy, `not_preregistered`; structural check невакуумен (6 клауз); хеш заморожен. **Но** хеш слеп к не-конечным значениям (M3-N14) | подтверждаю с оговоркой |
| G6 | Baselines §27.3 | 4 pinned; лицензии проверил по фактическим пакетам; ассет-пин работает; potrace типизированно отсутствует | **подтверждаю** |
| G7 | Source-group independence + sample-size | 150 рендеров → 3 испытания, граница бит в бит та же; 459 вывел независимо двумя способами | **подтверждаю** |
| G8 | Supported universe finite/versioned | `check_finite` невакуумен (7 клауз), исчерпывающий match с IR, хеш заморожен и чувствителен к 1e-13 | **подтверждаю** |
| G9 | Correlation-aware likelihood protocol before any confidence claim | guard отвергает все три способа; ни одного confidence-числа в артефактах; benchmark контролируется в обе стороны; corr-len 2 / overcount 9× воспроизвёл | **подтверждаю для исполнимых путей**; формулировка «все пути» шире измеренного (M3-N6) |
| G10 | Frozen gates / §27.7 как предикат в CI | placeholder отвергается (`PlaceholderUsedAsGate`, milestone `M7`) — да. **Но** предикат в CI смотрит один коммит на push, и в самом M3 есть коммит, который он отвергает; и 11 из 17 замороженных значений не читает никто | **НЕ подтверждаю — M3-N2, M3-N3** |
| G11 | Reports reproduce from clean checkout, scope `fast` | воспроизвёл: 1086 рендеров, тот же corpus_hash, scorecard побайтово; подделку «полного» прогона `verify` не пропускает | **подтверждаю** |
| G12 | Scorecard: дефицит и отказ вместо цифры | 5 строк, все с `deficit 459` / `contract_met false` / `CP upper 1.0`; `not_yet_produced` называет 6 отсутствующих групп полей §31 с милестоуном | **подтверждаю** |
| G13 | Долги D-1, D-2, D-4, red team п. 2/3 | D-4 и asset-pin проверил по существу; ancestry D-2 перемерил — главное верно, сопутствующее нет (M3-N4); red team п. 2 закрыт, комментарий переоценивает запас (M3-N10); п. 3 закрыт документированным пределом | подтверждаю с оговорками |
| G14 | Независимый cold review — ОТКРЫТ | это и есть настоящий документ | — |

Числа состава корпуса из STATUS_M3 §2 сверил построчно с манифестом и scorecard-ом: 60 групп / 63 сцены / 1086 рендеров / 18 ячеек; splits 22-10 / 16-7 / 22-7; identifiability 912 / 142 / 32; профили 567 / 189 / 126 / 126 / 78; четыре хеша. **Все сходятся.**

---

## 7. Замечания

### M3-N1 (**БЛОКЕР**, §28 M3 «sealed-audit burn policy active»): corpus-хеш burn-политики посчитан двумя разными функциями, и сравнение не может совпасть

**Что.** Величина «текущий corpus hash» вычисляется в системе двумя несовместимыми способами:

- `CorpusManifest::hash()` = `sha256(serde_json::to_string(&self))` — ключи в порядке объявления полей структуры. Именно это печатает `gt-corpus build` (`corpus_hash: …`), именно это `scorecard::build` кладёт в `hashes.corpus`, именно это записано в `docs/gt/SCORECARD_M3.json`: `5067b378…e224da`.
- `gt-corpus audit-status` (`bin/gt-corpus.rs:281-292`) читает файл, парсит в `serde_json::Value` и хеширует `to_string` **разобранного значения**. У `serde_json` без feature `preserve_order` `Value::Object` — это `BTreeMap` (проверил по Cargo.lock: `indexmap` в зависимостях `serde_json` нет), поэтому ключи сортируются лексикографически. Порядок разбора: `cells, groups, identifiability_counts, procedural_variants_per_family, renders, renders_by_profile, schema, split_policy_version, split_summary`. Хеш: **`304759fa…61d48`**.

Плюс ветка `recorded["corpus_hash_recorded"]` мертва: такого поля в манифесте нет и никто его не пишет, поэтому берётся всегда второй способ.

**Демонстрация (мой прогон, корпус не менялся).**

```text
$ gt-corpus verify --manifest docs/gt/CORPUS_MANIFEST.json
corpus reproduced: 1086 renders
corpus_hash: 5067b3789836fcfbac6f0c35259b7c6b20d5e7cf6ea5ed935df890f506e224da

# записываю OPEN-seal с ровно этим хешем и прогоняю штатный CI-шаг:
$ gt-corpus audit-status --seal <opened> --manifest docs/gt/CORPUS_MANIFEST.json --gates configs/GATES_V1.toml
audit generation 1 status Opened
BURN POLICY VIOLATION: … opened at corpus 5067b378…, but the current value is 304759fa…   exit 1
```

**Почему это не поймано.** `AuditSeal::check` покрыт юнит-тестами, которые подают литералы `"c"/"p"/"g"` — они проверяют логику сравнения и ничего не говорят о том, откуда берутся операнды. Интеграционных тестов у бинаря `gt-corpus` **нет вообще** (`crates/vice-bench/tests/` содержит только `cli.rs` и `child_env.rs`, оба про `baseline-runner`). CI гоняет `audit-status` на каждом push, но при `status = sealed` `check()` возвращает `StillSealed` **до** любого сравнения, поэтому CI-шаг за 15 прогонов ни разу не исполнил ветку сравнения. Это ровно мета-правило M-2: зелено потому, что состояние принадлежит подклассу, где проверка не работает.

**Почему это блокер, а не замечание.** §28 M3 называет клаузу «sealed-audit burn policy active». Механизм активен как ЗАПИСЬ, но сравнение, ради которого запись существует, при первом же честном использовании даёт ложную тревогу — то есть оператор, открывший audit по документированной процедуре, получит `BURNED` на неизменном корпусе, а оператор, подобравший второй хеш «чтобы CI зеленел», получит seal, который не согласован со scorecard-ом. По собственному стандарту проекта (F-0009: «явный лимит, не покрывающий доминирующий ресурс, хуже отсутствующего — он создаёт ложную уверенность») такой сторож хуже отсутствующего.

**Что закрывает.** Одна из двух развязок: (а) `audit-status` пересобирает манифест (как это уже делают `verify`/`report`) и берёт `CorpusManifest::hash()`; либо (б) манифест несёт собственный хеш отдельным файлом-сателлитом, который обе стороны читают. Плюс — обязательно — интеграционный тест `gt-corpus`, который открывает seal реальными хешами, прогоняет `audit-status` (ожидая exit 0), меняет один байт корпуса/gates/prereg и ожидает `ChangedAfterOpening`. Пока такого теста нет, клауза «burn policy active» держится на прозе.

### M3-N2 (**БЛОКЕР**, §27.7 / §32 п. 9): предикат frozen-gate в CI смотрит один коммит на push, и в самом M3 есть коммит, который он отвергает

**Что.** `configs/GATES_V1.toml` был создан коммитом **C072 (`4a3190c`)**, и тот же коммит меняет `crates/vice-bench/src/gates.rs`, `.../lib.rs`, `.../prereg.rs`. Прогнал на этом файловом наборе штатный инструмент проекта:

```text
$ gt-corpus gates-check --changed configs/GATES_V1.toml \
    --changed crates/vice-bench/src/gates.rs --changed crates/vice-bench/src/lib.rs \
    --changed crates/vice-bench/src/prereg.rs
spec §27.7 violation: configs/GATES_V1.toml changed together with crates/vice-bench/src/gates.rs.
A gate change is a separate reviewed commit.                                          exit 1
```

**Почему CI этого не увидел.** Шаг «Frozen-gate rule» берёт `git diff --name-only HEAD~1 HEAD` — то есть ровно один коммит, вершину push-а. Вся серия M3 (C057–C078) пришла одним push-ем: по API GitHub существует 15 прогонов, и среди их `head_sha` есть `be3c40d` и `153b70b`, но нет ни одного из C057–C077. Предикат физически не мог быть применён к `4a3190c`. Заявление в шапке `GATES_V1.toml` («That rule is enforced, not requested… CI runs it on every push») и G10 STATUS_M3 («§27.7 — предикат в CI») шире, чем реально исполняется.

**Смягчающее и не смягчающее.** По существу §27.7 («feature PR не может одновременно ОСЛАБИТЬ собственный gate») нарушения нет: я прошёл все 22 коммита M3 — `configs/GATES_V1.toml` трогается ровно один раз, при создании, и после этого не менялся, то есть ни одно число не ослаблялось. Но проверяемое правило, которое сам автор объявил механическим, репозиторий не выполняет, и CI-механизм имеет глубину один коммит.

**Что закрывает.** (а) Прогонять предикат по всему диапазону push-а (`git diff --name-only ${{ github.event.before }} ${{ github.sha }}`) или покоммитно; (б) разнести C072 на два коммита либо записать исключение «создание gate-файла вместе с его загрузчиком» явным правилом в самом предикате — но именно правилом, а не умолчанием.

### M3-N3 (**БЛОКЕР**, §27.7): 11 из 17 замороженных значений не читает никто, включая всю секцию `[corpus_instruments]`

Собрал все вызовы `gate_value(section, key)` в workspace и сопоставил с содержимым `configs/GATES_V1.toml`:

| Значение | читается кодом? | сверено с кодом тестом? |
|---|---|---|
| `reliability.confidence`, `.catastrophic_risk_target`, `.min_accepted_source_groups_zero_failures` | нет | **да** |
| `identifiability.observability_floor_px` | нет | **да** |
| `split.development_pct`, `.sealed_audit_pct` | нет | **да** |
| `likelihood.allowed_production_residual_models`, `.diagnostic_only_residual_models` | нет | **да** |
| `reliability.unit_of_trial` | нет | нет |
| `corpus_instruments.supersample_max_abs` = 0.06 | нет | нет |
| `corpus_instruments.supersample_edge_mean_abs` = 0.02 | нет | нет |
| `corpus_instruments.vice_render_max_abs` = 1e-9 | нет | нет |
| `corpus_instruments.tiny_skia_max_abs` = 0.35 | нет | нет |
| `corpus_instruments.raqote_max_abs` = 0.35 | нет | нет |
| `identifiability.rival_indistinguishable_codes` = 4 | нет | нет |
| `identifiability.quantization_floor_codes` = 1.0 | нет | нет |
| `split.calibration_pct` = 20 | нет | нет |
| `split.unit_of_assignment` | нет | нет |
| `split.held_out_profiles` = `["tiny-skia"]` | нет | нет |

Каждое из этих одиннадцати чисел продублировано литералом или константой в коде (`worst_super < 0.06`, `worst_vice < 1e-9`, `worst < 0.35`, `RIVAL_INDISTINGUISHABLE_CODES = 4`, `QUANTIZATION_FLOOR_CODES`, `SPLIT_POLICY_V1.held_out_profiles`), и эти литералы — и есть действующий порог. Следствие, проверяемое: ослабить `worst_super < 0.06` до `< 0.2` в `gt/raster.rs` можно, не тронув `configs/GATES_V1.toml`, — тогда предикат §27.7 не сработает (gate-файл не менялся), тест `the_frozen_numbers_agree_with_the_code_that_produced_them` не сработает (эти ключи он не смотрит), а gates-хеш `bbe7f4dd…` останется прежним и burn-политика ничего не заметит. Секция `[corpus_instruments]`, которая несёт именно измеренные в C066 числа, целиком мертва.

Это буквально мета-правило **M-1**: правило («замороженные числа обязаны совпадать с кодом») применено к перечню из шести адресов, а не к классу из семнадцати. Закрывается расширением теста на все frozen-ключи — механически, через обход `frozen_sections()`.

### M3-N4 (существенное, фактическая ошибка в reviewed-ADR): ancestry-утверждение о `v-ice-mainline` противоречит зеркалу

ADR-0014 («Что было измерено») и `SOURCE_PINS.toml` (дважды, включая поле `role`) утверждают: *«`59ab86d1` — предок upstream `main`; `9211b321` — нет ни для одного коммита main. То есть spec-пин лежит на невлитой боковой ветке.»*

Мои измерения в том же зеркале `C:\Users\nirrt\Toolset\v-ice`:

```text
refs/remotes/origin/main   -> 9211b32   (fetch 2026-07-07 13:38)
refs/remotes/origin/HEAD   -> 9211b32
refs/heads/main            -> 45b6f5c   (локальная, непушенная; == refs/heads/levertwo-colorguard)

merge-base --is-ancestor 9211b321 origin/main  -> exit 0   (ЕСТЬ предок; более того, это сам tip)
merge-base --is-ancestor 59ab86d1 origin/main  -> exit 1   (НЕ предок)
merge-base --is-ancestor 9211b321 main         -> exit 1
merge-base --is-ancestor 59ab86d1 main         -> exit 0
```

То есть относительно единственных ссылок, которые в зеркале обозначают upstream (`origin/main`, `origin/HEAD`), соотношение **обратное записанному**: spec-пин §2 — это и есть вершина upstream main, а «исправляющий» `59ab86d1` лежит на локальной непушенной линии. Утверждение про «невлитую боковую ветку» и имя `v-ice-mainline` относятся к локальной ветке `main` рабочей копии, а не к upstream.

**Что это НЕ меняет.** Несущий факт — сиблинги, общий родитель `49f308d9`, расхождение 1↔1, diff только `Cargo.toml`/`Cargo.lock` — я подтвердил полностью. Решение «добавить второй объявленный пин, а не двигать audit anchor §2» от этого только выигрывает: двигать пин было бы ещё хуже, чем считал автор. Но в документе, вся рамка которого — «измерено до решения», стоит измеримое утверждение, которое я не воспроизвожу, и оно повторено в трёх местах плюс закодировано в имени пина. По правилу M-4 («инструмент/предпосылку надо измерять») здесь измерена не та ссылка.

**Что закрывает.** Исправить формулировку в ADR-0014 и `SOURCE_PINS.toml` на то, что проверяемо (`origin/main` в зеркале указывает на spec-пин; `59ab86d1` — сиблинг на линии, не представленной remote-ссылками), и явно записать, ПРОТИВ КАКОЙ ссылки утверждение измеряется. Имя `v-ice-mainline` стоит либо переименовать, либо снабдить оговоркой.

### M3-N5 (умеренное, §27.1): поле GT-истины производит inverse-crime-рендерер

`PartitionTruth::measure` (`gt/mod.rs:252`) считает `exterior_visible_px2` вызовом `vice_render::render_mesh_partition(certified)`. Все 63 значения попадают в `docs/gt/CORPUS_MANIFEST.json` внутри `partition_truth` и хешируются в `corpus_hash`. Дисциплина inverse crime в M3 применена к РЕНДЕРАМ (флаг + исключение из выборки), но не к ПОЛЯМ ИСТИНЫ: этот скаляр нигде не помечен и от него ничто не защищает будущего скорера, который сравнит с ним восстановленное покрытие экстерьера.

Доккомментарий самого поля честен («MEASURED by rendering»), но модульный комментарий `gt/mod.rs` («Every truth field here is either MEASURED from the certified scene or declared as diagnostic-only»), комментарий `PartitionTruth` («Areas come from the shoelace … not from a render») и `gt/raster.rs` («never counted as ground truth») шире того, что делает код. Это тот же класс, что F-0015.

Смежное: `RenderOutcome.inverse_crime` — обычный `bool`, задаваемый вызывающим, а не выводимый из профиля. Я предъявил `RenderOutcome` с `cell_id`, содержащим `vice-render`, и флагом `false` — он был посчитан в выборку. В манифесте связь корректна (0 расхождений из 1086), но она не типизирована, и первый же потребитель, который построит `RenderOutcome` не из манифеста, её потеряет.

**Что закрывает.** Пометить `exterior_visible_px2` как диагностику inverse-crime-происхождения (в типе и в манифесте) либо считать его независимым интегратором (`exact_clip_face` уже есть); и вывести `inverse_crime` из профиля типом, а не аргументом.

### M3-N6 (умеренное, точность заявления): guard и burn-политика — вызываемые функции, а не барьеры типа

STATUS_M3 §7 пишет «каждый M3-путь заканчивается типизированным отказом», G9 — «guard отвергает все пути M3». Измеренное:

- `risk_coverage` при 459 сфабрикованных принятых группах возвращает `catastrophic_risk_upper 0.00998289`, `contract_met true`, ни разу не обратившись к `guard_confidence_claim`;
- `risk_coverage` над 22 группами split-а `sealed_audit` считается нормально (`CP upper 0.188869`), при том что `AuditSeal::check` на реальном seal-е вернул бы `StillSealed`: `RenderOutcome` не несёт split, и агрегатор физически не способен отказать по этому основанию.

В M3 это не эксплуатируется — `RenderOutcome` не производит ни один исполнимый путь, и все артефакты содержат отказ. Но заявление «все пути» относится к CLI/артефактам, а не к библиотечному API, и это стоит записать именно так. Отдельно: в `scorecard::build` вызов `guard_confidence_claim(Some(Block), false, 9.0)` содержит литерал `false`, поэтому строка gate table «correlation-aware likelihood protocol…» истинна по построению, а не по состоянию системы; невакуумность доказана только для строки про seal.

### M3-N7 (умеренное, покрытие): ни CI, ни `report` не сверяют закоммиченный манифест

- CI-job `gt-corpus` собирает СВОЙ манифест (`--scope test`) и им же отчитывается; закоммиченный `docs/gt/CORPUS_MANIFEST.json` в CI не верифицируется ни разу.
- `gt-corpus report` манифест не валидирует: он берёт из него только список ячеек, пересобирает корпус и хеширует пересборку. Я подал ему манифест с занулённым render-digest-ом — вышел scorecard, **побайтово равный** закоммиченному, с gate table 5/5 `[MET]`.

Подлога это не даёт (пересборка честная, а `verify` подмену ловит и называет фикстуру), но означает: единственная проверка закоммиченного артефакта — ручной `gt-corpus verify` ревьюера. Я его выполнил (274 с, сошлось). Стоит либо добавить его в CI (в `fast` scope ~4.5 мин — вполне бюджет), либо записать в REPRODUCIBILITY_M3 явно, что артефакт держится на ревьюере.

### M3-N8 (наблюдение): `model_universe_hash` детерминирован, но не перестановочно-каноничен

`canonical_json` = `serde_json::to_string` по структуре, то есть порядок ключей задан порядком объявления полей, а порядок элементов — порядком в `Vec<Family>`. Перестановка двух семейств в `segment_families` меняет хеш (`fed2af86…` → `a97fb90d…`) при том, что `check_finite` по-прежнему `Ok` и семантика universe та же. Ошибка консервативная — семантически эквивалентная правка поднимет тревогу, а не спрячется, — но слово «канонический» стоит понимать как «детерминированный при фиксированной декларации», и это стоит написать в доккомментарии. Сравнение хешей между версиями кода при этом бессмысленно ровно по той же причине, по какой оно бессмысленно для render digest-ов (долг red team п. 3).

### M3-N9 (наблюдение, числа): реализованные доли split-а заметно отличаются от замороженных

`configs/GATES_V1.toml` замораживает `development_pct = 55`, `calibration_pct = 20`, `sealed_audit_pct = 25`. Реализовано на корпусе:

```text
development   22 групп / 60 = 36.7 %   (10 из 24 семейств)
calibration   16 / 60       = 26.7 %   ( 7 из 24)
sealed_audit  22 / 60       = 36.7 %   ( 7 из 24)
```

Механизм не виноват: на 2000 синтетических семейств я получил 54.9/20.6/24.6 %, то есть хеш распределяет как заявлено. Отклонение — малая выборка (24 семейства) плюс неравный вес семейств (процедурное несёт 4 группы, authored/adversarial — 1). Ничего не сломано, но замороженные проценты описывают намерение, которого у корпуса нет, а единственный тест на баланс требует лишь «≥3 семейства и ≥3 группы в каждом split-е». Стоит либо записать реализованные доли рядом с намеренными, либо ассертировать реализованные с честным допуском.

### M3-N10 (мелкое, заявление шире ассерта): «доминирует в ~5×» против ассертированных 1.20×

C076 и комментарий в `coverage_props.rs` пишут: «k = 96 resolves ~0.010, and the tolerance drops to 0.05 — still dominating the arbiter own resolution by ~5x». Ассертируется же `ARBITER_TOLERANCE >= 4.0 / ARBITER_RATE`:

```text
модель ассерта  4/k = 0.041667  ->  0.05 / 0.041667 = 1.20x
модель текста   1/k = 0.010417  ->  0.05 / 0.010417 = 4.80x
```

То есть проверяемое утверждение даёт запас 1.20×, а написанное — 4.8×. Одна из двух моделей разрешения арбитра лишняя; выбрать надо ту, что подтверждается измерением (при k = 24 измеренный max-разброс суперсэмплера 0.0186 против 1/24 = 0.042 — то есть 1/k консервативен, а 4/k очень консервативен). Отдельно: ассерт стоит внутри ветки «пиксель оспорен», поэтому при отсутствии спорных пикселей он не исполняется вовсе; инвариант инструмента лучше проверять безусловно.

### M3-N11 (мелкое, governance): подписанный `docs/STATUS_M2.md` отредактирован после подписи

C057 меняет одну строку в отчёте принятого milestone-а (G11 «ОТКРЫТ» → «ЗАКРЫТ»). Помечено как «запись факта, внесена в начале M3», содержание корректно и проверяемо. Но артефакт, на который ссылались три подписи M2, больше не побайтово тот, что подписывали. Чище было бы дописать addendum, как это сделано в REVIEW_*/REDTEAM_*, а не править таблицу.

### M3-N12 (мелкое, §32 п. 3/23): `PORTING_MANIFEST.toml` не обновлён под M3 и содержит устаревшее утверждение

Файл кончается на «M2 status: STILL ZERO ported units»; строки «M3 status» нет, хотя STATUS_M3 и REQUIREMENTS_TRACEABILITY на D-3 ссылаются. Кроме того, в M2-абзаце написано, что tiny-skia/raqote — «cargo DEV-dependencies»; в M3 они стали обычными зависимостями `vice-bench`. Само число units (0) верно; неверна одна фраза и отсутствует запись милестоуна.

### M3-N13 (мелкое, точность отчёта): STATUS_M3 называет неверный максимум LOC

STATUS_M3 §3: «Продакшн-модулей >800 LOC нет (максимум `gt/degradation.rs` 752)». Измерено: 779 `crates/vice-bench/src/runner.rs` (изменён в M3, +215 строк), 775 `crates/vice-render/src/coverage.rs`, 752 `gt/degradation.rs`. Утверждение «нет >800» верно; названный максимум — нет.

### M3-N14 (мелкое, но по классу неприятное): хеш preregistration слеп к не-конечным значениям

`Preregistration::v1().buckets[4].boundary_p95_px = f64::INFINITY`, и `serde_json` печатает любое не-конечное f64 как `null`. Следствие, измеренное:

```text
+inf  -> ea04a7b409c3cd42049be40108dce1ef918ea84ecc904824d6cca9afc316f0ba
-inf  -> ea04a7b409c3cd42049be40108dce1ef918ea84ecc904824d6cca9afc316f0ba
NaN   -> ea04a7b409c3cd42049be40108dce1ef918ea84ecc904824d6cca9afc316f0ba
1e308 -> 40dd19f6…  (меняется)
check() принимает все три не-конечных значения
```

То есть «изменение плана анализа меняет хеш» имеет исключение, и в `canonical_json` ровно одно `:null` — именно этот порог. Практически эксплуатировать нечего (значение семантически «нет предела»), но тест `the_preregistration_hash_is_frozen` заявляет чувствительность, которой в этом углу нет, а `check()` не требует конечности порогов. Класс тот же, что F-0012: конечность/осмысленность полей не проверена там, где хеш её не видит. Закрывается либо конечным сентинелом, либо явной проверкой `is_finite()` в `check()` плюс отдельным полем «предел не задан».

### M3-N15 (наблюдение, предел механизма): seal-артефакт самозаверяющий и обратимый

`docs/gt/AUDIT_SEAL.json` — обычный закоммиченный JSON без привязки к истории. Я взял OPEN-запись, вернул `status: "sealed"` и очистил три хеша — `audit-status` печатает «sealed and never opened» и возвращает 0, то есть факт открытия стирается полностью. Единственная защита — git-история и ревью. Это не дефект реализации (иначе и быть не может без подписи), но заявление «не обещание, а сравнение» стоит дополнить: сравнение честное, эталон — самопредоставленный и откатываемый.

### M3-N16 (косметика): mojibake в доккомментарии и расхождение имени команды

- `crates/vice-bench/src/gt/mod.rs:402` содержит двойно-кодированную кириллицу (`Ð´Ð¾Ð¿ÑƒÑÑ‚Ð¸Ð¼Ð°Ñ` вместо «допустимая»). Байты `c3 90 c2 b4 …`. Единственное такое место во всём репозитории — проверил весь workspace по `\xc3[\x90\x91]`. Тот же класс, что addendum 2 к F-0004 (инструмент правки повредил текст).
- Шапка `configs/GATES_V1.toml` называет команду `baseline-runner gates check-commit`; реальная — `gt-corpus gates-check`.

### Что проверялось и НЕ дало находок

Определение `count_components` (донат = одна компонента) — согласовано с §5.3 и с тестом; `polyline_signed_area` shoelace против аналитики; строгий загрузчик authored-подмножества (10 отвергаемых входов, включая `out_of_subset_input_is_refused_rather_than_guessed`); порядок правил identifiability (потеря информации старше эквивалентности), проверен тестом в обе стороны; `ResizeChain` действительно меняет байты и не меняет размер; `is_realizable` запрещает нереализуемые ячейки, и non-box PSF типизированно отвергается четырьмя движками из пяти; `#![forbid(unsafe_code)]`; отсутствие env-флагов поведения; child-env-политика; отсутствие placeholder-типов для M3.5/M4; `Cargo.lock` закоммичен и toolchain пиннится 1.96.0 (совпал с моим); все 47 идентификаторов ячеек уникальны; scorecard детерминирован; смоук-корпус M0 регенерируется побайтово; `selftest` детерминирован.

---

## 8. Итоговая оценка

Это сильный milestone. Корпус воспроизводится из чистого checkout-а с точностью до хеша, скоркард — побайтово; истина ИЗМЕРЯЕТСЯ, а не объявляется; порог идентифицируемости откалиброван против соперника по семейству и проверен в обе стороны; оценщик корреляции контролируется в обе стороны и даёт содержательный результат (белое AA-расхождение против 9× переучёта на несовпадении формации); 459 выведено, а не процитировано, и дефицит опубликован числом вместо надёжности; ни одного confidence-числа в артефактах; утечку между split-ами я искал четырьмя независимыми способами и не нашёл; подделать «полный» прогон и подменить render-digest не удалось; D-4 закрыт правильно и с СУЖЕННЫМ заявлением; лицензии проверены по фактическим пакетам и подтвердились; долг red team п. 1 записан честно, а не снят. Три записи в FAILURE_LEDGER о собственных ошибках дают общие правила.

Но подписать gate в текущем виде я не могу. Три дефекта — про механизмы, которые §28 M3 называет поимённо и которые сегодня зелены потому, что ни разу не были приведены в рабочее состояние; четвёртый — измеримое утверждение в reviewed-ADR, которое я не воспроизвожу.

1. burn-политика сравнивает corpus-хеш с величиной, которую не производит ни один компонент системы, и первый же честный `open` даёт `BURNED` на неизменном корпусе (**воспроизведено**);
2. предикат §27.7 в CI имеет глубину один коммит, и в самом M3 есть коммит, который этот предикат отвергает (**воспроизведено**);
3. 11 из 17 замороженных чисел, включая всю секцию измеренных в C066 `[corpus_instruments]`, не читает никто и не сверяет ни один тест — то есть действующие пороги живут литералами в коде и их можно ослабить, не тронув gate-файл и не подняв ни одной тревоги.
4. ADR-0014 и `SOURCE_PINS.toml` утверждают про ancestry `v-ice` ровно обратное тому, что показывают remote-ссылки зеркала (**перемерено**).

Ни одно из четырёх не влияет ни на одну цифру M3 и ни на один артефакт; первые три чинятся малым объёмом работы и одним интеграционным тестом каждый, четвёртое — правкой формулировки в трёх местах.

### Условия снятия блокеров

1. **M3-N1.** `audit-status` берёт corpus-хеш из `CorpusManifest::hash()` пересобранного манифеста (как `verify`/`report`) либо манифест несёт собственный хеш, читаемый обеими сторонами. Обязателен интеграционный тест бинаря `gt-corpus`: open реальными хешами → `audit-status` exit 0; затем изменить corpus / prereg / gates по одному → три `ChangedAfterOpening`. Мёртвую ветку `corpus_hash_recorded` убрать или заполнять.
2. **M3-N2.** CI прогоняет предикат по всему диапазону push-а или покоммитно; C072 либо разнесён на два коммита, либо в предикате появляется явное, названное правилом исключение для создания gate-файла вместе с его загрузчиком.
3. **M3-N3.** Тест согласия расширен на КЛАСС: обход всех `frozen_sections()` и всех их ключей, каждый — против константы/литерала в коде; значения без потребителя либо получают потребителя, либо удаляются из «frozen».
4. **M3-N4.** ADR-0014 и `SOURCE_PINS.toml` исправлены на проверяемую формулировку с указанием ссылки, против которой измерялась ancestry.

M3-N5…M3-N16 блокерами не считаю; M3-N5, M3-N6 и M3-N7 прошу закрыть или сузить формулировки до измеренного к следующему gate.

---

## VERDICT: REJECT

Блокеры:

1. **M3-N1** — corpus-хеш sealed-audit burn policy вычисляется двумя несовместимыми функциями (`5067b378…` против `304759fa…`); открытие audit-а по документированной процедуре даёт `BURN POLICY VIOLATION` на неизменном корпусе; ветка сравнения никогда не исполнялась ни тестом, ни CI (у бинаря `gt-corpus` интеграционных тестов нет). Клауза §28 M3 «sealed-audit burn policy active» не выполнена по существу.
2. **M3-N2** — предикат §27.7 в CI применяется только к вершине push-а; коммит C072 (`4a3190c`) меняет `configs/GATES_V1.toml` вместе с тремя файлами `crates/`, `gt-corpus gates-check` на этом наборе даёт exit 1, и CI этот коммит не видел ни разу (15 прогонов, `head_sha` C057–C077 отсутствуют).
3. **M3-N3** — 11 из 17 замороженных значений `configs/GATES_V1.toml` (вся секция `[corpus_instruments]`, `identifiability.rival_indistinguishable_codes`, `.quantization_floor_codes`, `split.calibration_pct`, `.unit_of_assignment`, `.held_out_profiles`, `reliability.unit_of_trial`) не читаются кодом и не сверяются ни одним тестом; действующие пороги — литералы в коде, и их ослабление не поднимет ни gates-хеш, ни предикат §27.7, ни burn-политику.
4. **M3-N4** — ADR-0014 и `SOURCE_PINS.toml` (дважды) утверждают, что `59ab86d1` — предок upstream main, а spec-пин `9211b321` лежит на невлитой боковой ветке; в зеркале `origin/main` и `origin/HEAD` указывают **на `9211b321`**, а `59ab86d1` не является его предком. Несущий факт (сиблинги, merge-base `49f308d9`, 1↔1) подтверждён и решение не меняется, но измеримое утверждение в reviewed-ADR неверно и повторено в трёх местах.

Клаузы §28 M3, которые я подтверждаю выполненными собственными измерениями: *reports reproduce from clean checkout*, *no test leakage*, *source-group independence defined*, *supported universe is finite/versioned*, *correlation-aware likelihood protocol exists before any confidence claim* (для всех исполнимых путей). Не подтверждаю: *sealed-audit burn policy active*.

Independent reviewer (cold agent context, Opus)

---

## Addendum — дельта-review C080–C088

Тот же независимый контекст, что подписал REJECT выше. Клон `%TEMP%\m3rev` переведён `fetch` + `reset --hard` на **`c7e727a8449876a5ded5b1050c381843903e2f0f`** («C088 M3(F-0020): corpus digests are Tier A, and the manifest now says so»); рабочее дерево чистое до и после всех правок-атак; харнесс `%TEMP%\m3adv` тот же. Дельта — 10 коммитов `be3c40d..c7e727a`, где C079 (`d92093f`) — дословная фиксация моего отчёта.

---

## A1. Команды и exit-коды

| # | Команда | exit | время | результат |
|---|---|---|---|---|
| 1 | `cargo fmt --all --check` | 0 | 1 с | чисто |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | 0 | 3 с | 0 warning |
| 3 | `cargo test --workspace` (прогон 1) | 0 | 232 с | **313 passed / 0 failed** |
| 4 | `cargo test --workspace` (прогон 2) | 0 | 228 с | **313 / 0** |
| 5 | `cargo test --release --workspace` | 0 | 43 с | **313 / 0** |
| 6 | `gt-corpus verify --manifest docs/gt/CORPUS_MANIFEST.json` | 0 | 276 с | `corpus reproduced: 1086 renders`; `corpus_hash 325429a2…6191d` |
| 7 | `gt-corpus audit-status` (seal = sealed) | 0 | <1 с | `sealed and never opened` |
| 8 | `gt-corpus report …` | 0 | 289 с | 5/5 `[MET]`; выданный scorecard **побайтово равен** закоммиченному (4840 байт) |

313/0 в обоих профилях подтверждаю: сумма по всем тестовым бинарям сверена построчно, строк `test result: FAILED` нет ни в одном из трёх прогонов. 300 → 313.

**CI, проверено по API.** Прогон **#18** на `c7e727a` — все три job-а `success`. Отдельно поднял прогон **#17** на `5ec01e3` (C087): job `gt-corpus` **failure**, при этом шаг 6 «GT corpus rebuilds deterministically (test scope)» — `success`, а шаг 7 «The committed corpus manifest reproduces» — `failure`. Это и есть F-0020 в публичной записи: рассказ автора проверяем, а не принимается на слово.

---

## A2. Аудит закрытия четырёх блокеров

### M3-N1 — ЗАКРЫТ

`audit-status` пересобирает манифест в его собственном scope и берёт `CorpusManifest::hash()` — ту же функцию, что `build`, `verify` и `report`. Мёртвая ветка `corpus_hash_recorded` удалена.

**Моё репро, повторённое дословно:** OPEN-seal с `corpus_hash`, `prereg_hash`, `gates_hash`, взятыми прямо из закоммиченного `SCORECARD_M3.json`, корпус не тронут:

```text
audit generation 1 status Opened
audit is open and untouched since it was opened          exit 0
```

Прежде эта же команда печатала `BURN POLICY VIOLATION … 5067b378 vs 304759fa`.

**Невакуумность семи новых интеграционных тестов.** Они гоняют БИНАРЬ теми же командами, что CI, и каждый негативный случай снабжён контролем в том же теле (`control: unperturbed passes`, `control: untouched passes`, `control: the untouched manifest verifies`). Покрыты: sealed → отказ; открытие на собственных хешах системы → проход; каждый из ТРЁХ хешей по отдельности → burn; изменение самого КОРПУСА (удалена ячейка из манифеста) → burn; burned остаётся burned; «открыто, но хеши пусты» → отказ; `build`/`report`/`audit-status` ассертированы на согласие об ОДНОМ хеше. Проверка gates идёт правкой ФАЙЛА, а не подстановкой строки, то есть путь «байты → хеш» реально исполняется. Это ровно то, чего не хватало.

### M3-N2 — ЗАКРЫТ

**Мой прогон предиката по КАЖДОМУ коммиту `153b70b..c7e727a` — 32 коммита, вся серия M3 плюс дельта:**

```text
commits checked: 32, violations: 0
```

C072 (`4a3190c`), дававший exit 1 на первой подаче, проходит: git выдаёт `A<TAB>configs/GATES_V1.toml`, и исключение по статусу `A` его освобождает.

**«Только добавление освобождает» — настоящее правило или удобная выемка?** Проверил класс сам:

```text
A                                         -> exempt
M, D, R100, C75, T, U, X, a, ??           -> violation
голый путь без статуса                    -> violation
gate MODIFIED + gates.rs ADDED            -> violation
gate DELETED  + lib.rs MODIFIED           -> violation
```

Настоящее правило: (1) сформулировано как утверждение о семантике §27.7 («нельзя ослабить то, чего нет»), а не как список файлов; (2) сужено до одного статуса из восьми; (3) снабжено тестами в обе стороны; (4) мои негативные контроли дают exit 1 и на модификации, и на удалении. Выемка выглядела бы иначе — исключением по пути или по имени коммита. Одна щель найдена (M3-D1 ниже).

### M3-N3 — ЗАКРЫТ

Тест обходит класс: клауза 1 — каждый ключ каждой frozen-секции обязан быть заявлен кодом; клауза 2 — заявленный обязан совпадать; клауза 3 — обход обязан покрыть ВСЕ frozen-секции и ≥17 значений.

**Обе мои атаки теперь ловятся** (правки делались в клоне и откатывались):

```text
1) ослабить литерал, НЕ трогая gate-файл:
   SUPERSAMPLE_MAX_ABS_GATE 0.06 -> 0.2   (configs/GATES_V1.toml не изменён)
   -> panicked: corpus_instruments.supersample_max_abs: the gate file and the code disagree
      test result: FAILED. 5 passed; 1 failed

2) протащить замороженное значение без потребителя:
   `smuggled_threshold = 0.99` в frozen-секцию [reliability]
   -> panicked: frozen values that nothing in the code reads or checks:
      ["reliability.smuggled_threshold"]. A value with no consumer is not a gate
      test result: FAILED. 5 passed; 1 failed
```

Контроль на вакуумность: сначала я по ошибке дописал ключ в конец файла, где он попал в PLACEHOLDER-секцию `[noise_scales]` — и тест правильно прошёл. Ловится именно frozen-класс, а не любая правка файла. Замороженных ключей теперь 20 (5+3+2+4+6), все с потребителями; клауза 1 делает счёт неважным.

### M3-N4 — ЗАКРЫТ

ADR-0014 и `SOURCE_PINS.toml` перемерены против REMOTE-ссылок и приводят их дословно; мои измерения из первой подачи воспроизводятся один в один (`origin/main` и `origin/HEAD` → `9211b321`; `59ab86d1` не предок `origin/main`; merge-base `49f308d9`; расхождение 1↔1; diff в двух файлах). Пин переименован, `role` называет измеримое: «SIBLING of the §2 pin (merge base 49f308d9), reachable from no remote ref of the mirror». Вывод усилен корректно: сдвиг anchor означал бы движение ОТ upstream. Урок записан классом — «утверждение об upstream обязано называть ссылку, против которой проверяется».

---

## A3. F-0020 — разбор по трём вопросам

### (а) Остаётся ли клауза «reports reproduce from clean checkout» выполненной?

**Да. Сузилась не клауза, а заявление, стоявшее НАД ней.**

Спека §5.5 распределяет обязательства явно:

```text
Tier A: same binary/platform → byte-identical digests            (обязателен M1+)
Tier B: supported platforms  → scene-equivalent within tolerance (M12)
```
и добавляет: «Не обещать cross-platform byte identity, пока libm/FMA/thread reductions не зафиксированы».

Кросс-платформенная битовая идентичность в M3 спекой НЕ требуется: она отнесена к M12 и там она вообще не про хеши, а про эквивалентность в пределах допуска. Клауза §28 M3 на этом милестоуне может означать только Tier A — и Tier A я подтвердил дважды: 274 с на первой подаче (`5067b378…`) и 276 с сейчас (`325429a2…`), плюс scorecard побайтово оба раза.

Ложной была не клауза, а строка REPRODUCIBILITY_M3 «Что при сверке может законно отличаться: **Ничего**». Она обещала Tier B, которого никто не проверял и которого спека здесь не просит. Проверка, добавленная ради моего M3-N7, это обещание и опровергла. Сейчас документ говорит «Платформа», приводит обе команды и заканчивает: «На ОДНОЙ платформе не может отличаться ничего: расхождение digest-а — находка».

Побочный эффект в мою пользу, который надо назвать прямо: **моё первое подтверждение было Tier A-подтверждением, и я не мог этого знать** — манифест не нёс платформу, поэтому «воспроизводится» и «воспроизводится здесь» были неразличимы. Тот же прогон сегодня говорит строго больше. Свидетельство не ослабло — оно стало правильно подписанным.

Остаточное сужение, честно: **digest-ы закоммиченного манифеста в CI по-прежнему не проверяются ничем** — ubuntu-раннер физически не может, windows-раннера в workflow нет. M3-N7 закрыт полностью как покрытие СОСТАВА и лишь частично как покрытие ЦИФР; последние держатся на ручном прогоне на платформе записи (моём и авторском). См. A7.2.

### (б) Типизированный отказ — честная граница или способ увести класс из-под проверки?

Это ровно вопрос, на котором в M2 провалился предикат `is_resolvable`. Прогоняю те же три теста, которыми red team его тогда завалил.

**Тест 1 — существует ли в исключённом классе ПРАВИЛЬНЫЙ ответ?** У `is_resolvable` — да: `0.278144151`, подтверждённый четырьмя методами, а аккумулятор молча возвращал 0. Здесь — **нет**. `sin`/`cos`/`powf`/`exp` в Rust не имеют контракта correctly-rounded; две соответствующие стандарту платформы вправе дать разные последние биты, а сравниваемая величина — sha256, на котором допуск невыразим в принципе. Не существует «правильного кросс-платформенного digest-а», от которого система уклоняется. Это решающее отличие.

**Тест 2 — критерий модель или измеренное свойство?** `is_resolvable` был АНАЛИТИЧЕСКОЙ моделью ошибки реализации (`eps·mag_x + eps·mag_y·|slope|`), и обоснование под ним оказалось неверным в двух точках из трёх. Здесь критерий — записанный ФАКТ об артефакте (`platform = {os, arch}`), без порога, без оценки, без возможности дрейфа. Он входит в `corpus_hash`, то есть подделать его молча нельзя. Сама граница установлена различающим экспериментом (две сборки на одном раннере совпали, кросс-платформенная не совпала), который я перепроверил по CI-логу.

**Тест 3 — не прячет ли исключение случай, где система может быть неправа?** Прячет, и это единственная содержательная слабость. Измерил её:

```text
манифест «с другой платформы», у которого обнулены ВСЕ 1086 render-digest-ов,
все 63 scene-digest-а, total_ink_px2, exterior_visible_px2 и palette:

  verify --structural  ->  corpus reproduced STRUCTURALLY … exit 0
```

То есть структурный режим не увидел бы ни одного численно неверного пикселя. Слепота ровно такая, как объявлена.

Три конструктивных свойства, которых у M2-предиката не было и которые удерживают эту границу на честной стороне — все три проверены прогоном:

```text
1. отказ — УМОЛЧАНИЕ
   verify (чужая платформа, без флага)
   -> error: this manifest records digests for platform {"os":"linux",…}, and this is
      {"os":"windows",…}. Render digests are a TIER A artifact (spec 5.5) …   exit 2

2. ослабленный режим САМ О СЕБЕ ГОВОРИТ
   verify --structural (чужая платформа)
   -> corpus reproduced STRUCTURALLY across platforms (1086 renders): … Render digests
      NOT compared - they are Tier A …                                        exit 0
   и он не беззубый: та же чужая платформа + одна группа, переставленная между split-ами
   -> corpus did NOT reproduce structurally - the difference is composition, not
      float noise                                                             exit 1

3. флаг ИНЕРТЕН на платформе записи  (`if structural && !same_platform`)
   verify --structural (МОЯ платформа)      -> corpus reproduced: 1086 renders  exit 0
      — то есть выполнилось ПОЛНОЕ сравнение, а не структурное
   verify --structural (МОЯ платформа, один digest занулён)
   -> differs: … recorded 0000… actual e57526f4…; corpus did NOT reproduce      exit 1
```

Пункт 3 — главный: флагом нельзя замять собственный провал.

Отдельно отмечу свойство, делающее проекцию не чисто комбинаторной: она сохраняет метки `identifiability` и `identifiability_counts`, а они ВЫЧИСЛЯЮТСЯ из float через пороги, и сохраняет `holes`/`components`/`visible_faces`. Численный дрейф, достаточный чтобы перебросить метку через порог или изменить топологию, структурный режим увидит. Чувствительность слабая и пороговая, но не нулевая.

И ещё деталь в пользу конструкции: `recorded.get("platform")` при отсутствии поля даёт `Null`, поэтому манифест БЕЗ платформы (любой до C088) не считается «своим» — он тоже получает типизированный отказ, а не молчаливое допущение.

**Вывод по (б): честная граница.** Она проходит все три теста, на которых `is_resolvable` провалился, и её единственная реальная слабость объявляется самим механизмом в его собственном выводе — то, чего предикат разрешимости не делал.

### (в) Достаточно ли «манифест несёт платформу»?

**Для M3 — достаточно и правильно.** Инструмент Tier A — это именно побайтовый digest, и §5.5 требует Tier A с M1. Дефект был не в том, что digest-ы стали артефактом сверки, а в том, что артефакт не нёс условия своей воспроизводимости. Минимальная честная правка — записать условие В артефакт и сделать его частью хеша — сделана.

**Как постоянный ответ — недостаточно, и это надо назвать.** Digest над изображением, порождённым `sin`/`powf`/`exp`, по построению не может стать кросс-платформенным инструментом: нулевой допуск на величине, воспроизводимой лишь в пределах платформы. `--structural` им тоже не является — я измерил его слепоту выше. То есть у проекта сегодня есть Tier A-инструмент и композиционный инструмент, и НЕТ инструмента для того, что §5.5 называет Tier B. Пока его нет, кросс-платформенная КОРРЕКТНОСТЬ корпуса не проверяется ничем.

Это не блокер M3 — Tier B спека относит к M12 — но это ограничение, которое обязано стоять в §5 «Известные ограничения» отдельным пунктом с милестоуном-владельцем. Условие A7.1.

---

## A4. Adversarial-проверки дельты

| # | Атака | Результат | Вывод |
|---|---|---|---|
| D1 | Повторить моё репро M3-N1 дословно | `audit is open and untouched`, **exit 0** | блокер закрыт |
| D2 | Ослабить литерал 0.06 → 0.2, не трогая gate-файл | тест падает: `the gate file and the code disagree` | закрыт |
| D3 | Протащить frozen-ключ без потребителя | тест падает: `nothing in the code reads or checks` | закрыт |
| D4 | Тот же ключ в placeholder-секцию | тест проходит | ловится frozen-класс, а не любая правка |
| D5 | Предикат §27.7 по каждому из 32 коммитов | 0 violations; контроли `M`+код и `D`+код → exit 1 | закрыт |
| D6 | `--structural` на платформе записи | ПОЛНОЕ сравнение, exit 0 | флаг инертен |
| D7 | То же + занулённый digest | `corpus did NOT reproduce` exit 1 | флагом нельзя замять свой провал |
| D8 | `verify` чужой платформы без флага | exit 2, отказ называет обе платформы и §5.5 | типизированный отказ |
| D9 | `--structural` на чужой платформе | exit 0 + «Render digests NOT compared» | ослабление объявлено |
| D10 | То же + группа переставлена между split-ами | exit 1 «composition, not float noise» | состав реально проверяется |
| D11 | То же + ВСЕ digest-ы и float-истина обнулены | exit 0 | **измеренная слепота структурного режима** |
| D12 | Реальная git-строка переименования | exit 0 — **не пойман** | M3-D1 |
| D13 | 459 групп профиля `ViceRender` | `groups_total 0` | inverse crime выведен из типа |
| D14 | 459 групп: без модели / iid / block-некалибр. / block-калибр. | `false/false/false/true`; CP 0.00998289 | guard внутри вычисления |
| D15 | Не-конечные пороги в preregistration | `check()` отвергает все | N14 закрыт структурно |
| D16 | Leakage-пробы заново на новом манифесте | все нули | без регрессии |

---

## A5. Регрессия первой подачи и цепочка хешей

**`corpus_hash` менялся дважды, и каждое изменение вызвано и названо:**

| Коммит | corpus_hash | Причина |
|---|---|---|
| be3c40d…abf1e38 | `5067b378…` | C080–C083 корпус не трогают |
| **cf8f29b (C084)** | → `86f19425…` | M3-N5: `exterior_visible_px2` считается независимым exact-clip-интегратором |
| eb6602f…5ec01e3 | `86f19425…` | без изменений |
| **c7e727a (C088)** | → `325429a2…` | F-0020: в манифест добавлен блок `platform` |

Сверил диффом самого манифеста `be3c40d..c7e727a`: изменились **ровно** блок `platform` и **четыре** значения `exterior_visible_px2` — на один ULP каждое. **Ни один из 1086 render-digest-ов и ни одно `face_area_px2` не сдвинулись.** Это ровно то, чего ждёшь от замены инструмента, согласного с прежним до 3.7e-15, и ничего сверх.

`gates_sha256` `bbe7f4dd…` → `b3973732…` — коммит **C085 (`eb6602f`)**, трогающий **ТОЛЬКО** `configs/GATES_V1.toml`. Образцовая форма §27.7, и мой покоммитный обход её подтверждает.

Leakage-пробы, source-group independence, sample size, universe/prereg-хеши, доли split-ов — перепроверены, все совпали с первой подачей.

Прочие: N5 (`exterior_visible_px2` из `exact_clip_loops`; значение рендерера отброшено, используется только факт успеха; `inverse_crime()` — производный метод от профиля) · N6 (guard ВНУТРИ `risk_coverage`; `contract_met` требует `likelihood_refusal.is_none()`) · N9 (доли ассертируются 36.7/26.7/36.7 ± 2 п.п., механизм отдельно на 4000 семействах) · N10 (заявление сужено до ассертируемых 1.20×) · **N11 (diff против подписанного `153b70b` — чисто адитивный, +37 строк, 0 удалённых; строка G11 вернулась дословно)** · N12/N13/N14/N8/N15/N16 закрыты.

---

## A6. Новое, найденное в дельте

### M3-D1 (мелкое): git-строка ПЕРЕИМЕНОВАНИЯ обходит предикат §27.7

Шапка gate-файла утверждает: «Only status `A` exempts; modification, deletion and rename do not». Про rename это неверно — `git diff --name-status` выдаёт переименование ТРЕМЯ колонками:

```text
вход:   R100<TAB>configs/GATES_V1.toml<TAB>configs/GATES_V2.toml
        M<TAB>crates/lib.rs
выход:  no gate/feature co-change in 2 path(s)                       exit 0   <-- НЕ пойман

тот же коммит с --no-renames:  D/A/M -> spec §27.7 violation           exit 1
```

`ChangedPath::parse` режет строку по первому пробельному символу; собственный тест использует двухколоночную форму `"R100\tconfigs/OLD.toml"`, которой git не порождает. Свежий экземпляр **M-2** внутри правки, закрывавшей M-1 и M-2 — иллюстрация к собственному выводу автора лучше любой прозы.

Эксплуатируемость близка к нулю: переименование ломает `GatesFile::load` по захардкоженному пути. Но заявление шире парсера, а разбор третьей колонки — одна строка.

### M3-D2 (наблюдения)

- `ChangeKind::from_status` смотрит первый символ, поэтому любой статус на `A` освобождает; через git недостижимо, через ручной `--changed` — да.
- Типизированный отказ по платформе стоит ПОСЛЕ полной пересборки корпуса: 292 с до `exit 2`. Проверка дешёвая и могла бы идти первой.

### M3-D3 (косметика)

ASCII-схема в ADR-0014 потеряла ветвление; там же фраза «Имя … заменено на фактическое» называет одно и то же имя дважды — след массовой замены.

---

## A7. Условия к следующему gate (не блокеры)

1. **Tier B отсутствует и должен быть назван ограничением.** Есть Tier A-инструмент и композиционный (`--structural`, слепота измерена в D11). Инструмента §5.5 Tier B нет, поэтому кросс-платформенная КОРРЕКТНОСТЬ корпуса не проверяется ничем. Записать в §5 отдельным пунктом с милестоуном-владельцем.
2. **Digest-ы закоммиченного манифеста в CI не проверяются.** Либо windows-job на один шаг `verify` (fast scope ≈ 4.6 мин), либо явная запись, что цифры артефакта держатся на ручном прогоне.
3. **M3-D1** — разобрать третью колонку `R`/`C`-строк либо сузить заявление в шапке gate-файла до того, что делает парсер.

---

## A8. Итог

Все четыре блокера закрыты по существу, а не по форме: каждый — механизмом, каждый — с тестом, который я пытался обойти и не смог, и ни один — переписью заявления. Две из четырёх правок породили записи в ledger, где автор называет собственные мета-правила M-1 и M-2, повторившиеся ВНУТРИ милестоуна, который их сформулировал, и выводит общее: **знание мета-правила не является его исполнением**.

F-0020 я считаю не издержкой дельты, а её главным результатом. Проверка, добавленная по моему M3-N7, на первом же прогоне опровергла заявление, которого я НЕ опроверг: я подтверждал воспроизводимость на платформе записи и не мог отличить «воспроизводится» от «воспроизводится здесь», потому что артефакт не нёс условия. Теперь несёт, и условие входит в хеш. Так измерение и должно работать — не подтверждать ожидание, а находить границу; и здесь границу нашёл инструмент, а не отчёт.

Клаузы §28 M3 после дельты, по моим измерениям:

```text
reports reproduce from clean checkout      ВЫПОЛНЕНА  Tier A дважды (274 с / 276 с), scorecard побайтово
no test leakage                            ВЫПОЛНЕНА  перепроверена четырьмя способами, все нули
sealed-audit burn policy active            ВЫПОЛНЕНА  моё репро даёт exit 0; 7 интеграционных тестов бинаря
source-group independence defined          ВЫПОЛНЕНА  150 рендеров -> 3 испытания; 459 выведено
supported universe is finite/versioned     ВЫПОЛНЕНА  хеш не изменился, невакуумность на месте
correlation-aware likelihood protocol …    ВЫПОЛНЕНА  guard внутри вычисления, а не рядом с ним
```

Шесть из шести. На первой подаче было пять.

---

## VERDICT (addendum): ACCEPT

Блокеров нет. M3-D1 — мелкое, M3-D2 — наблюдения, M3-D3 — косметика; условия A7.1–A7.3 адресованы следующему gate, а не этому.

Долг red team addendum 4 п. 1 (периодический четвёртый метод) остаётся открытым и записан честно в трёх местах; §28 M3 его не требует.

Independent reviewer (cold agent context, Opus)
