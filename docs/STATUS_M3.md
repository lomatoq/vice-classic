# STATUS_M3 — GT / identifiability / scorecard

Дата: 2026-07-26.
Spec: `VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`
(SHA-256 `652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1`).
Автор: coding-агент (Claude Code), single-milestone run по §34.
Стартовая точка: HEAD `153b70b` (M2 принят: REVIEW_M2_A addendum 4 ACCEPT,
REVIEW_M2_B addendum 4 ACCEPT, REDTEAM_M2 addendum 4 PASS).
Коммиты milestone: **C057–C078**.

> **Этот отчёт — author report. Он сам по себе НЕ делает M3 green.**
> §32 правило 29 и §34: milestone требует **одного независимого cold
> review** с чистым checkout-ом; автор не самосертифицирует. Отдельный
> numerical/topology red-team pass §34 требует для M2, M5 и M7 — для M3 он
> не обязателен, но и не запрещён.

## 1. Что сделано

### Долги, срок которых наступил

- **D-2 / B1+B2 (C059, C060, ADR-0014).** Измерено до решения: пин спеки
  `9211b321` и upstream-фикс `59ab86d1` — **сиблинги** (общий предок
  `49f308d9`, расхождение 1↔1), и только второй лежит на линии main. То
  есть «просто передвинуть пин» было недоступно. Решение: spec-пин остаётся
  на месте с честным `build_failed`, ДОБАВЛЕН объявленный `v-ice-mainline`.
  B2: механизм `[[baseline.asset]]` — path + sha256 + bytes, проверка **до**
  копирования, typed `asset_mismatch`/`asset_root_missing`. Измеренный
  эффект: было 26 из 40 объявленных прогонов ok, стало **30 из 30** у трёх
  исполнимых baseline-ов (Vice- 6/10 → 10/10).
- **D-1 (C061).** `hashes.json` v3 разделён на `normative`/`informational`;
  `environment_sha256` покрывает нормативную проекцию `env.json`. Инструкция
  «сравните файлы» была НЕВЫПОЛНИМА (в файле лежал `binary_sha256`, законно
  различный у двух корректных прогонов) — теперь это команда
  `baseline-runner compare-hashes`. `docs/baselines/M0/**` не тронуты;
  действующие артефакты — `docs/baselines/M3/**`.
- **D-3.** Соблюдён: 0 units в PORTING_MANIFEST. В M3 соблазн был
  максимальным (GT-корпус) — ни одного донорского ассета.
- **D-4 / M2-A-N8 / M2-B-N5 (C062).** `CertifiedMesh`: приватные поля, два
  конструктора, несёт свои `RenderOptions`; mesh-входы рендера принимают
  только его. Первый не-рендерящий потребитель — `GtScene::new`. Заявление
  СУЖЕНО и проверено тестом: витнес **не** утверждает замощение окна.
- **Red team addendum 4, п. 2 (C076).** Ставка арбитража поднята там, где
  арбитраж и происходит: k = 24 → 96, допуск 0.1 → 0.05, и свойство
  «допуск доминирует собственное разрешение арбитра» теперь ассертируется.
- **Red team addendum 4, п. 3 (C074).** M3 действительно вводит сравнение
  digest-ов, поэтому ограничение записано: render digest-ы сравнимы только
  в пределах одной версии renderer-а.

### Корпус и измерительная инфраструктура

- **Три независимых источника (§27.1).** Процедурная грамматика (12 shape
  families × 4 СТРУКТУРНЫХ варианта), 6 hand-authored SVG со строгим
  загрузчиком подмножества, adversarial + 3 ambiguity pairs. Планарный
  builder написан заново от контракта §6.1/§12, region-first, а не
  переиспользован из тестовых фикстур.
- **Truth измеряется**, authored truth — только диагностика.
- **Пять rasterizer-профилей**, точность НАШИХ измерена (M-4), а
  `vice-render` помечен inverse-crime и исключён из выборки.
- **Degradation matrix** покрывает все оси §27.2; JPEG/WebP отсутствуют, и
  это проверяется тестом.
- **Identifiability на РЕНДЕРЕ**, порог откалиброван измерением в обе
  стороны.
- **Split по shape family** + held-out профиль; **burn policy** — сверка
  трёх хешей, не обещание.
- **Preregistration**, **`SupportedModelUniverseV1`**, **frozen gates**,
  **residual-correlation benchmark**, **risk–coverage / Clopper–Pearson /
  sample-size contract** — все с замороженными хешами и невакуумными
  проверками.

## 2. Состав корпуса (записанный манифест)

```text
source groups        60      (procedural 48, authored 6, adversarial 6)
scenes               63      (3 группы несут по 2 сцены — ambiguity pairs)
renders            1086      over 18 degradation cells (scope `fast`)
splits:  development 22 групп / 10 семейств
         calibration 16 / 7
         sealed_audit 22 / 7        ← НЕ открывался
identifiability:  identifiable 912, information_lost 142, equivalent_family 32
profiles: exact-clip 567, supersample 189, raqote 126, vice-render 126*, tiny-skia 78
                                            * inverse-crime, помечен, вне выборки
corpus_hash            5067b378…      model_universe_hash  fed2af86…
preregistration_hash   ea04a7b4…      gates sha256         bbe7f4dd…
```

`tiny-skia` встречается реже прочих потому, что это held-out профиль: в
`development` его нет вовсе.

## 3. Итоги проверок (author-side, эта машина)

```text
cargo fmt --all --check                                  OK
cargo clippy --workspace --all-targets -- -D warnings    OK (0 warnings)
cargo test --workspace                                   300 passed / 0 failed
cargo test --release --workspace                         300 passed / 0 failed
```

207 в M2 → **300**. Оба профиля обязательны (расхождение debug/release само
по себе является находкой — F-0007, F-0012).

Классы новых тестов: детерминизм и структурная различимость грамматики;
строгость загрузчика авторского подмножества (10 отвергаемых входов);
измерение точности собственных растеризаторов; калибровка порога
identifiability в обе стороны; коллапс И различимость ambiguity pairs;
метаморфные свойства инструмента (трансляция, отражение, независимость от
paint); отсутствие протечки split-а; burn policy по всем трём хешам;
невакуумность `check_finite` и структурной проверки preregistration;
Clopper–Pearson против независимо посчитанных значений и замкнутой формы;
контроль correlation-оценщика в обе стороны; §27.7 как предикат.

`#![forbid(unsafe_code)]` во всех четырёх crates. Никаких env-флагов
поведения. Продакшн-модулей >800 LOC нет (максимум `gt/degradation.rs` 752).
Новых crates не создано; `vice-bench` — дом GT/oracles/baselines/reports по
целевой раскладке §4.

## 4. Gate table (author-side; §28 M3)

| # | Gate | Статус | Evidence |
|---|---|---|---|
| G1 | Heterogeneous corpus; equivalence/ambiguity fixtures | PASS | 3 источника, 60 групп; ambiguity pairs с ИЗМЕРЕННЫМ коллапсом и различимостью; метаморфные свойства инструмента |
| G2 | Degradation matrix (§27.2) | PASS | все оси, JPEG/WebP отсутствуют (проверено); оси проверены В ПИКСЕЛЯХ, не в манифесте |
| G3 | Три-ступенчатый split; **no test leakage** | PASS | целые shape families; held-out профиль вне development; стабильность при росте корпуса |
| G4 | **Sealed-audit burn policy active** | PASS | generation 1 SEALED; открытие записывает 3 хеша, любое последующее изменение — typed `BurnViolation`; проверка в CI |
| G5 | Preregistration до открытия audit | PASS | 5 buckets, 7 catastrophic kinds с измеримой величиной, pooling policy, хеш заморожен |
| G6 | Baselines (§27.3) | PASS | 4 pinned (30/30 у исполнимых) + vtracer 5/5 + potrace typed `binary_missing`; лицензии по фактическим пакетам |
| G7 | **Source-group independence defined** + sample-size contract | PASS | единица — source group; 459 ВЫВЕДЕНО; 3 группы × 50 фаз = 3 испытания |
| G8 | **Supported universe is finite/versioned** | PASS | `check_finite` + невакуумность; связь с IR исчерпывающим match; `model_universe_hash` заморожен |
| G9 | **Correlation-aware likelihood protocol before any confidence claim** | PASS | guard отвергает все пути M3; benchmark: blur corr-len 2, iid overcount **9×**; AA-расхождение честно белое |
| G10 | Frozen gates / code-table placeholders (§27.7) | PASS | placeholder ≠ threshold (отказ); замороженные числа сверены с кодом; §27.7 — предикат в CI |
| G11 | **Reports reproduce from clean checkout** | PASS **с явным scope** | `gt-corpus build/verify`; CI: две сборки побайтово. Записанный манифест — scope `fast`; `full` идёт часами и это записано, а не обойдено (см. §6) |
| G12 | Scorecard | PASS | дефицит 459 групп и отказ вместо цифры; `not_yet_produced` называет отсутствующие поля §31 и их милестоун |
| G13 | Долги D-1, D-2, D-4, red team п.2/п.3 | PASS | C059–C062, C076; измеренные эффекты выше |
| G14 | **Независимый cold review (§32 п.29, §34)** | **ОТКРЫТ — блокирует M3.5** | Автор не самосертифицирует. Требуется ОДИН независимый clean-checkout review |

## 5. Известные ограничения (честная граница M3)

1. **Записанный манифест — scope `fast`** (18 из 47 ячеек). `full` на одном
   ядре идёт часами (exact-clip 512×512 ≈ 1e6 отсечений на face); прогон
   был прерван после часа. Матрица НЕ сужена: `--scope full` доступен,
   манифест несёт свой список ячеек, его хеш зависит от scope, и тест
   запрещает выдать частичный прогон за полный. Ускорение exact-clip —
   работа M3.5+.
2. **Sample-size contract не выполнен и не может быть выполнен в M3**: 60
   независимых групп против требуемых 459 при нуле отказов. Дефицит
   опубликован числом. Дорасти до 459 честных групп — отдельная работа, и
   её нельзя сделать вариациями параметров (§27.4).
3. **Никаких метрик качества**: нет вектораизатора, поэтому нет boundary
   p50/p95/p99, primitive accuracy, posterior-калибровки. Ось «метрики»
   §27.4 реализована как типы и агрегация, а не как измерения.
4. **PSF-ось меряется только на размерах ≤64** — предел суперсэмплера
   (576 inside-тестов на пиксель), записан как предел инструмента.
5. **Порог identifiability откалиброван на ОДНОМ семействе признаков**
   (дырка против дырки в 1.4×) и перенесён на остальные по длине. Это
   допущение; расширение калибровки на thin-feature и component — работа
   M4+.
6. **Render digest-ы сравнимы только внутри одной версии renderer-а**
   (C052 не бит-нейтрален между версиями).
7. **Human court (§27.4) отсутствует**: нет выхода, который можно судить.
8. **Долг red team addendum 4 п. 1 остаётся открытым**: периодический
   прогон differential-свойства с ЧЕТВЁРТЫМ методом (замкнутая форма на
   семействе, где она есть). Остаточный риск — согласованная ошибка всех
   трёх инструментов, при которой арбитраж не запускается. Свидетельств нет,
   но и проверки нет.
9. **Внешние оппоненты не оценены по качеству**: их SVG записаны как
   артефакты с хешами, но сравнения с GT нет — для этого нужен SVG→raster
   путь, которого в M3 нет.

## 6. Blockers перед M3.5

1. **G14**: один независимый cold review с чистым checkout-ом (§34).
   Ревьюер: клонирует, гоняет документированные команды без авторских
   кэшей, проверяет gate-артефакты и негативные тесты, пытается
   воспроизвести минимум один failure/adversarial случай, подписывает
   `docs/REVIEW_M3.md` либо возвращает blockers. До подписи M3.5 не
   разрешён.
2. Долг red team п. 1 (четвёртый метод) — к следующему gate.
3. Ускорение exact-clip, чтобы `--scope full` стал практичным.

## 7. Явное заявление об остановке

Автор НЕ самосертифицирует M3. Ни одна цифра надёжности в этом милестоуне
не является утверждением: каждая строка — либо дефицит (459 групп при 0
принятых), либо типизированный отказ. Три записи в FAILURE_LEDGER (F-0015,
F-0016, F-0017) — мои собственные ошибки, найденные измерением, и две из
них состояли в том, что я калибровал порог не на тот вопрос и переобещал
собственный витнес через один коммит после того, как написал тест,
запрещающий это.

Никакой код M3.5 (factorial oracle harness, PF/G arms, intervention
schemas) не начат.

**STOPPED AFTER M3 — M3.5 NOT STARTED.**
