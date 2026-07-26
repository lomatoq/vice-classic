# REVIEW_M0 — независимый clean-checkout review milestone M0

## 0. Контекст ревьюера

- **Кто:** независимый agent-контекст (cold review), не автор кода M0.
  Никаких авторских выводов/вердиктов на вход не подавалось.
- **Модель:** Claude Opus 5 (`claude-opus-5[1m]`).
- **Дата:** 2026-07-26.
- **Source of truth:** `VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`,
  SHA-256 проверен ревьюером: `652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1` — **совпал**.
  Прочитаны §0, §2, §2.1, §3 (обзорно), §4, §4.1, §27.3, §28 (M0), §32, §33, §34, §36, §37.
- **Основание ревью:** spec §32 правило 29 и §34 («milestone green требует
  независимого clean-checkout review; author agent не может самосертифицироваться»).

### Проверенный объект

| Параметр | Значение |
|---|---|
| Репозиторий | `C:\Users\nirrt\Toolset\vice-classic` (git, ветка `main`) |
| **HEAD sha** | **`d3bbe738a6c4632130f539c13e91ce9c6e2f42ad`** (`C003 M0(docs): record author-side clean-checkout reproduction verdict`) |
| Предыдущие коммиты | `fec205e` (C002), `8c53f0f` (C001) |
| Рабочее дерево автора | чистое (`git status --porcelain` пуст) |
| Клон ревьюера | `%TEMP%\m0rev` → `C:\Users\nirrt\AppData\Local\Temp\m0rev` (HEAD совпадает) |
| Второй клон (adversarial) | `%TEMP%\m0adv` (HEAD совпадает) |

Требование окружения из `docs/REPRODUCIBILITY_M0.md` (база checkout-а
короткая) соблюдено: `<клон>/runs/m0` = 47 символов при документированном
пределе ~137. В основном репозитории автора **ничего не изменялось**; вся
работа выполнена в клонах. Донорские зеркала открывались только на чтение.

### Окружение прогона ревьюера

```
windows / x86_64 / 16 logical CPUs
rustc 1.96.0 (ac68faa20 2026-05-25)     cargo 1.96.0 (30a34c682 2026-05-25)
git version 2.55.0.windows.2            Python 3.12.1
```

`env.json` моего прогона **побайтово совпал** с записанным
`docs/baselines/M0/env.json`; `environment_sha256` совпал
(`d1ccab0ce0ae698605c0ba55ba1dc63579f014973b8007655aae22f9da627d5e`).
Pinned toolchain из `rust-toolchain.toml` (1.96.0) фактически применился.

---

## 1. Выполненные команды и exit-коды

Все команды — из `docs/REPRODUCIBILITY_M0.md`, в документированном порядке,
из свежего клона без авторских кэшей (`target/` и `runs/` отсутствовали,
т.к. gitignored и в клон не попадают).

| # | Команда | Exit | Факт |
|---|---|---|---|
| 1 | `cargo test --workspace` | **0** | 17 тестов (12 unit + 5 integration), 0 failed — совпадает с заявкой T7 |
| 2 | `cargo run --release --bin gen-smoke -- --out tests/fixtures/smoke --check` | **0** | 5/5 `OK` — corpus побайтово регенерируется из исходника |
| 3 | `... baseline-runner -- verify-corpus --corpus tests/fixtures/smoke --manifest .../SMOKE_MANIFEST.toml` | **0** | 5/5 `Ok` |
| 4 | `... baseline-runner -- selftest --out runs/selftest --corpus ... --manifest ...` | **0** | `selftest OK: runner pipeline deterministic across 2 repeats` |
| 5 | `... baseline-runner -- run --config configs/baselines.toml --corpus tests/fixtures/smoke --manifest .../SMOKE_MANIFEST.toml --mirror-root "C:\Users\nirrt\Toolset" --out runs/m0 --repeats 2` | **0** | 5 мин 52 с; см. §2 |
| 6 | `cargo fmt --all --check` (шаг CI) | **0** | — |
| 7 | `cargo clippy --workspace --all-targets -- -D warnings` (шаг CI) | **0** | — |

Итог полного прогона (шаг 5), вывод ревьюера дословно:

```
baseline v-ice: failed (build_failed)      runs ok: 0/0
baseline v-ize: completed                  runs ok: 10/10
  primary_deterministic: Some(true)  all_artifacts_deterministic: Some(true)  mismatches: 0
baseline Vice-: completed                  runs ok: 6/10
  primary_deterministic: Some(true)  all_artifacts_deterministic: Some(true)  mismatches: 0
```

Это **построчно совпадает** с таблицей `docs/STATUS_M0.md` §2
(v-ice `build_failed`, v-ize 10/10, Vice- 6/10). «Законно падающий»
baseline — v-ice — упал ровно по заявленной причине: я независимо
воспроизвёл panic `dav1d-sys-0.8.3/build.rs`
(`The pkg-config command could not be found`), exit 101, что совпадает с
закоммиченным `docs/baselines/M0/evidence/v-ice-build.log` и с F-0002.
Пин **не патчился** — проверено: `resolved_sha` = ровно пин из spec §2.
Аналогично Vice-: `output_missing` на triangle_128/glyph_16 вызван
`FileNotFoundError: models/corner_rf.joblib` при exit-коде донора **0** —
т.е. молчаливый провал донора пойман проверкой declared outputs, а не
замолчан. Это добросовестная фиксация, а не подгонка.

---

## 2. Сверка gate artifacts (мой `runs/m0/hashes.json` vs `docs/baselines/M0/hashes.json`)

Сравнение выполнено пополе, включая полный обход всех artifact-хешей.

| Категория | Кол-во | Вердикт |
|---|---|---|
| `schema` | 1 | совпало |
| `config_sha256` | 1 | совпало (`2376b348e77f9d20…50a9c`) |
| corpus-хеши | 5 | **5/5 совпало** |
| `resolved_sha` пинов | 3 | **3/3 совпало** (`9211b321…`, `95a65194…`, `200897ab…`) |
| `status` / `error_kind` | 3 | совпало (`failed/build_failed`, `completed`, `completed`) |
| `primary_deterministic` / `all_artifacts_deterministic` | 6 | совпало |
| artifact-хеши v-ize | 5 | **5/5 побайтово совпало** |
| artifact-хеши Vice- | 24 | **24/24 побайтово совпало** |
| artifact-хеши v-ice | 0 | совпало (пусто, build failed) |
| **Итого artifact-хешей** | **29** | **29/29 совпало, 0 расхождений, 0 лишних, 0 пропавших** |
| `environment_sha256` (report.json) | 1 | совпало |
| `binary_sha256` v-ize | 1 | **отличается** (ref `658f3cd1…`, mine `8cd4e6ca…`) |
| `tool.exe_sha256` (report.json) | 1 | **отличается** (ref `de58f10d…`, mine `decade0d…`) |

Побайтовое равенство файла `hashes.json` — **нет**, и единственная причина —
`binary_sha256`.

**Оба расхождения попадают строго в задокументированный список допустимых**
(`docs/REPRODUCIBILITY_M0.md`, раздел «Что обязано совпасть, что нет»:
`binary_sha256` донорских сборок и `report.json` целиком). Расхождений
сверх этого списка **не обнаружено**.

### Независимая проверка самого «допущения»

Я не принял заявление автора о нестабильности `binary_sha256` на веру и
поставил собственный эксперимент (проба P1), которого нет в тестах автора:

1. В **том же** checkout-е, по **тому же** абсолютному пути, тем же
   toolchain, без изменения исходников: `cargo clean --release -p vize-cli`
   → `cargo build --release -p vize-cli`.
   - до: `8cd4e6ca97ab72e379b9f83a9d6a009ab3131e61fe14be2e481fe48d6e9f448a`
   - после: `d96ee9c78c8c218b92b63b0b8b8145468887dd386c793b55761a17304680df6a`
   → **хеш бинаря изменился**. Заявление STATUS_M0 §3 п.2 **подтверждено
   экспериментально**, а не просто продекларировано.
2. Тем же новым (иначе хешируемым) бинарём прогнаны ring_64 / glyph_16 /
   triangle_128 → SHA-256 полученных SVG:
   `2da63c90…`, `19b70809…`, `78b53898…` — **побайтово равны записанному
   эталону**.

Вывод: исключение `binary_sha256` из критерия воспроизводимости и перенос
критерия на artifact-хеши — обоснованное инженерное решение, а не удобная
отговорка.

---

## 3. Adversarial / негативные проверки ревьюера

Все пять случаев придуманы и исполнены ревьюером в отдельном клоне
`%TEMP%\m0adv` **против реального `configs/baselines.toml`, реального
закоммиченного corpus и реального mirror-root** — то есть по постановке
отличаются от интеграционных тестов автора (те работают на синтетическом
temp-corpus и фиктивных baseline-ах).

| # | Сценарий | Ожидание | Факт | Вердикт |
|---|---|---|---|---|
| **A1** | Один бит перевёрнут внутри закоммиченного `tests/fixtures/smoke/ring_64.png` (byte[600] `^= 0x01`; длина 2391 и размеры не менялись — чистый hash-mismatch) | полный run обязан abort-иться с nonzero exit | `verify-corpus` → **exit 1** (`HashMismatch expected 207b334d… actual b17cf819…`); `gen-smoke --check` → **exit 1** (`ring_64.png: DIFFERENT`); полный `run` → **exit 2**, `error: corpus integrity failure`. `runs/a1/report.json` **не создан**, `runs/a1/checkouts` **не создан** → abort произошёл ДО единого git clone/build/spawn. После восстановления байта — `verify-corpus` exit 0 | **PASS** |
| **A2** | `--mirror-root C:\definitely\no\such\mirror\root`, реальный config | отчёт обязан записаться с typed-ошибками и exit 0 | **exit 0**; записаны `report.json` + `hashes.json` + `env.json`; все 3 baseline `status=failed`, `error.kind=mirror_missing`, сообщения показывают фактически резолвнутые каталоги (`…\v-ice`, `…\v-ize`, `…\v-ice part`) | **PASS** |
| **A3** (мой третий) | «Decompression bomb»: 29-байтовый PNG-префикс, чей IHDR объявляет 1000000×1000000, с корректным sha256 и согласованным манифестом | limits обязаны сработать ДО запуска baseline, без декодирования | **exit 2**, `bomb.png: Invalid { error: "dimensions 1000000x1000000 exceed per-axis limit 4096" }`; `report.json` не создан, `checkouts` не создан. Файл в 29 байт никогда не декодировался | **PASS** |
| **A4** (мой четвёртый) | `pin_sha` v-ize подменён на валидный 40-hex, отсутствующий в зеркале (`deadbeef…`) | никакого тихого отката на HEAD/ветку зеркала | **exit 0**, typed `pin_unavailable`, `resolved_sha=null`, сообщение `pinned commit deadbeef… not available from mirror …\v-ize`. Provenance-контроль реален | **PASS** |
| **A5** (мой пятый) | `pin_sha` длиной 60 hex-символов («слишком длинный пин») | config-валидация обязана отвергнуть | **exit 2**, `config error: baseline "v-ize" pin_sha is not a 40-hex commit sha`, отчёт не создан | **PASS** |

Дополнительно проба **P1** (см. §2) — воспроизведение заявленного
недетерминизма донорской сборки: **подтверждено**.

Итог: обязательный минимум (два случая) перевыполнен — 5 adversarial +
1 репродукция заявленного failure. Все повели себя так, как требуется.

---

## 4. Построчная валидация gate table из `docs/STATUS_M0.md` §4

Колонка «Вердикт ревьюера» — независимая; «PASS» означает, что я проверил
это сам по коду и/или прогоном, а не поверил тексту STATUS.

| # | Заявка автора | Заявленный статус | Проверка ревьюера | Вердикт |
|---|---|---|---|---|
| **T1** | Только нужные crates, без placeholder-ов | PASS | `Cargo.toml`: `members = ["crates/vice-bench"]` — ровно один crate. Все 9 модулей `lib.rs` реально вызываются двумя бинарями (`baseline-runner`, `gen-smoke`); мёртвых/спекулятивных traits и API «на M5–M12» нет; `vice-geom/ir/image/evidence/render/cli` не созданы. Соответствует §4.1 («не создавать пустые crates на будущее») и §32 п.7. Крупнейший модуль `runner.rs` = 786 строк, в рамках рекомендации §4.1 (<800–1000 LOC) | **PASS** |
| **T2** | Pins/licenses/provenance записаны | PASS | `SOURCE_PINS.toml` содержит ровно три пина spec §2 (sha/repo/role/license_status — дословно). `PORTING_MANIFEST.toml` — 0 units. Проверено **по коду доноров, а не по словам**: поиск отличительных идентификаторов runner-а (`validate_png_bytes`, `hashes_from_report`, `primary_deterministic`, `all_artifacts_deterministic`, `run_with_timeout`, `force_remove_dir_all`, `substitute_tokens`, `CorpusManifest`, `mirror_hint`, `baseline-runner`, `gen-smoke`, `vice-classic`) по **закоммиченным деревьям всех трёх пинов** (`git grep <pin>`) → ноль содержательных совпадений. Единственный хит — python-dataclass `MasterResourceLimits` в `Vice-` с полями `fitting_ms/render_pixels/memory_bytes/solver_variables`, не имеющими ничего общего с Rust-структурой `ResourceLimits` (`max_input_bytes/max_png_dimension/run_timeout_secs/build_timeout_secs/max_output_bytes`) — случайное совпадение подстроки, не порт. License-находки `THIRD_PARTY_NOTICES.md` перепроверены на пинах и **все три подтверждены**: v-ice — нет LICENSE и нет ключа `license`; v-ize — `license = "MIT OR Apache-2.0"` в `[workspace.package]`, текстов лицензий нет, `vize-gpu` и `bsplat-spike` действительно не наследуют `license` (а `vize-cli` зависит от `vize-gpu`); Vice- — файлов лицензии нет | **PASS** (с замечанием N2) |
| **T3** | Traceability / failure ledger / ADR | PASS | `REQUIREMENTS_TRACEABILITY.md` (11 строк M0-1…M0-11, ссылки на реальные файлы/тесты) и три ADR — содержательны и соответствуют коду. **Но** `FAILURE_LEDGER.md` повреждён: заголовок `## F-0003` физически отсутствует (склеен в строку «Статус» записи F-0004), и при этом два governance-документа утверждают «3 записи», тогда как в обороте четыре ID (F-0001…F-0004). См. N1 | **PARTIAL** |
| **T4** | Toolchain/lock/env manifest зафиксированы | PASS | `rust-toolchain.toml` = 1.96.0 и фактически применился (мой rustc — ровно 1.96.0); `Cargo.lock` закоммичен; `env.json` + `environment_sha256` пишутся в каждый отчёт и **воспроизвелись побайтово**. Замечание N6 (окружение дочерних процессов не контролируется) | **PASS** (с замечанием N6) |
| **T5** | Deterministic baseline runner на фиксированном corpus | PASS | Воспроизведено независимо: v-ize `primary`+`all_artifacts` det = true (10/10), Vice- det = true на выданных артефактах, `mismatches: 0` у обоих. Механизм проверен по коду: свежий `git clone --local --no-checkout` + `checkout --detach <pin>` + сверка `rev-parse HEAD` с пином (`runner.rs::fresh_checkout`), состояние рабочей копии зеркала в прогон не попадает. Формулировка «det. на выданных артефактах» честна: 4 из 10 runs Vice- зафиксированы как `output_missing`, и это видно в отчёте | **PASS** |
| **T6** | Hashes/runtime/exit/artifacts записаны | PASS | `report.json` содержит команды, exit-коды, `duration_ms`, `timed_out`, пути логов, все артефакты (`declared` флаг, размер, sha256), build-record, env, config_sha256, exe_sha256 | **PASS** |
| **T7** | CI: fmt, clippy, tests, clean-checkout smoke | PASS (локальный эквивалент) | Шаги workflow воспроизведены мной локально: `cargo fmt --all --check` = 0, `cargo clippy --workspace --all-targets -- -D warnings` = 0, `cargo test --workspace` = 0, `gen-smoke --check` = 0, `verify-corpus` = 0, `selftest` = 0. Сам GitHub-прогон **никогда не исполнялся** (remote отсутствует) — автор это прямо заявляет (B4), формулировка «PASS (локальный эквивалент)» точна. Замечание N5: кросс-платформенная часть (побайтовая регенерация corpus на `ubuntu-latest`) остаётся непроверенной ни разу | **PASS с оговоркой** |
| **T8** | Resource limits + typed errors | PASS | По коду: `corpus::verify(... &cfg.limits)` вызывается **до** любого baseline (`runner.rs:90`), `validate_png_file` — до каждого spawn (`runner.rs:413`); размер берётся из metadata до чтения, размеры — из IHDR без декодирования. Подтверждено экспериментом A3 (checkouts не создавались). Typed-таксономия двухуровневая (`BaselineError`/`RunError`) с machine-readable `kind`, плюс fatal `TopError` только на config/corpus. Подтверждено A2/A4/A5 | **PASS** |
| **T9** | Сбой одного baseline не ломает отчёты остальных | PASS | Подтверждено реальным прогоном (v-ice `build_failed`, при этом v-ize и Vice- дали полные отчёты) и A2 (все три baseline присутствуют в отчёте с typed-ошибками). По коду: `BaselineCtx::execute` ловит ошибку в `rep.error` и цикл продолжается | **PASS** |
| **T10** | Corpus/commands/outputs зафиксированы | PASS | `SMOKE_MANIFEST.toml` пиннит sha256 + размеры 5 файлов; `verify` детектирует missing / hash-mismatch / dimension-mismatch / invalid / `NotInManifest` (дрейф в обе стороны); команды baseline-ов полностью явные в `configs/baselines.toml` и продублированы в `report.json`; известные caveats доноров записаны в `notes` | **PASS** |
| **T11** | Воспроизводимость из clean checkout | PASS (author-side) | **Подтверждено независимо этим ревью**: 29/29 artifact-хешей, config/corpus/pin/status/determinism — всё совпало; расхождения только из допущенного списка | **PASS** |
| **T12** | Независимый review подписан | **NOT DONE — блокирует M1** | Закрывается настоящим документом | **CLOSED** |

Отдельно проверено, что STATUS не приукрашивает: §3 «Известный
nondeterminism» перечисляет 5 пунктов, из которых п.2 я подтвердил
экспериментально (P1), п.1 и п.5 очевидны из кода/`env.json`, п.3 и п.4
корректно помечены как «в этом прогоне не проявилось». Заявление §7
(«STOPPED AFTER M0 — M1 NOT STARTED») соответствует содержимому дерева:
никаких M1-crates/типов/IR в репозитории нет.

---

## 5. Список расхождений и замечаний

Ни одно из найденного не опровергает воспроизводимость M0; все — уровня
документации/гигиены либо латентного риска. Нумерация — для трекинга.

**N1 (существенное, документация; §32 правило 24).**
`FAILURE_LEDGER.md` повреждён: заголовок `## F-0003 — Vice- pin не
самодостаточен (M0, 2026-07-26)` уничтожен и вклеен хвостом в строку
«**Статус.** Исправлено в M0 (до коммита C002). — Vice- pin не
самодостаточен (M0, 2026-07-26)» записи F-0004. Поиск по заголовкам даёт
только F-0001, F-0002, F-0004; тело F-0003 существует, но безымянно.
Повреждение внесено ещё в C002 (`git show fec205e:FAILURE_LEDGER.md`
уже без заголовка F-0003) и не замечено при C003, который дописал туда же
addendum. Дополнительно: `docs/STATUS_M0.md` §1 и
`REQUIREMENTS_TRACEABILITY.md` M0-3 оба утверждают «3 записи», тогда как в
обороте четыре ID, и на F-0003 ссылаются `docs/REPRODUCIBILITY_M0.md:21`,
`STATUS_M0.md:68` и `STATUS_M0.md:142`. Это фактическая неточность в
milestone-отчёте. **Обязательно исправить (восстановить заголовок F-0003 и
счётчик записей) до старта M1.**

**N2 (умеренное, provenance-документация).**
`SOURCE_PINS.toml` объявляет для `Vice-` `local_mirror_hint = "Vice-"`, но
такого каталога зеркала не существует; фактический —
`C:\Users\nirrt\Toolset\v-ice part`. Runner берёт hint из
`configs/baselines.toml` (там верно, `mirror_hint = "v-ice part"`), и
`docs/REPRODUCIBILITY_M0.md` тоже содержит верную таблицу, поэтому
документированная процедура работает — что я и подтвердил. Но поле в
`SOURCE_PINS.toml` фактически ложно и никем не проверяется. Подтверждено
эмпирически сообщением A2: `mirror directory not found:
C:\definitely\no\such\mirror\root\v-ice part`. Исправить или удалить поле.

**N3 (умеренное, дизайн артефакта).**
`hashes.json` объявлен «детерминированным подмножеством для побайтового
сравнения» (`report.rs`, ADR-0002 п.4), а `REPRODUCIBILITY_M0.md` предлагает
«сравните `runs/m0/hashes.json` с записанным». При этом в тот же файл
включён `binary_sha256`, который эти же документы объявляют законно
нестабильным (и что я подтвердил экспериментом P1). Следствие: побайтовое
сравнение назначенного эталонного артефакта **всегда** даёт различие, и
сверка требует ручного пополевого разбора. Рекомендация к M1: вынести
`binary_sha256` в `report.json` (как provenance) либо разделить
`hashes.json` на нормативную и информационную секции, чтобы «сравните
файлы» стало исполнимой инструкцией.

**N4 (мелкое, код).**
Подкоманда `verify-corpus` (`baseline-runner.rs:150`) всегда использует
`ResourceLimits::default()` и не принимает `--config`, тогда как `run`
использует лимиты из `configs/baselines.toml`. Сейчас значения совпадают,
поэтому наблюдаемого эффекта нет, но документированная автономная проверка
corpus не уважает настроенные лимиты проекта — расхождение проявится при
первом же изменении `[limits]`.

**N5 (мелкое, покрытие).**
ADR-0003 называет `gen-smoke --check` в CI «кросс-платформенным
determinism-пробником», но CI на `ubuntu-latest` ни разу не исполнялся
(B4), а локально всё подтверждено только на Windows. Побайтовая
воспроизводимость PNG-энкодера между платформами пока — гипотеза, а не
измерение. Первый push обязан её проверить.

**N6 (умеренное, латентный риск воспроизводимости).**
`exec.rs::run_with_timeout` спавнит дочерние процессы с полностью
унаследованным окружением: нет `env_clear`, нет фильтрации/фиксации
`RUSTFLAGS`, `CARGO_*`, `PATH`, `PYTHONPATH`, `PYTHONHASHSEED`. `env.json`
фиксирует только четыре версии инструментов. Правило §32 п.4 («никаких
hidden env vars») формально соблюдено — в коде проекта нет ни одного
`std::env::var` (проверено grep-ом: встречаются только `env::consts::*`,
`current_exe`, `env!("CARGO_PKG_VERSION")`, `env!("CARGO_BIN_EXE_*")` в
тестах). Но требование §28 M0 «reproducible command environment» покрыто
неполно: сторонняя ambient-переменная может изменить донорскую сборку
и артефакты, не оставив следа в отчёте. Мой прогон совпал с эталонным, что
показывает — на практике сейчас всё сходится; риск латентный. Рекомендация
к M1: фиксировать/санировать env дочерних процессов и хешировать
релевантный срез в `env.json`.

**N7 (мелкое, формулировка отчёта).**
`STATUS_M0.md` §2 приводит `runner exe sha256 de58f10d…` в списке
«ключевых значений» прогона. Это значение невоспроизводимо (у меня
`decade0d…`) по той же причине, что и `binary_sha256`. Формально оно
покрыто допущением «report.json целиком», но подача его как «ключевого
значения» вводит в заблуждение — стоит явно пометить как non-normative.

### Что проверялось и НЕ дало находок

- Скрытые env-флаги в `crates/` — отсутствуют (см. N6).
- Sample-specific logic — отсутствует: имена/размеры corpus-файлов
  (`rect_32`, `circle_64`, `ring_64`, `triangle_128`, `glyph_16`) не
  встречаются нигде, кроме генератора `gen-smoke.rs`; ветвлений по
  ширине/высоте в runner нет (единственное сравнение размеров —
  `width == 0 || height == 0` в проверке лимитов).
- Placeholder crates/traits — отсутствуют.
- Копирование донорского кода — не обнаружено (см. T2).
- Недокументированный nondeterminism — не обнаружен: 29/29 artifact-хешей
  совпали, `mismatches: 0`, а оба выявленных расхождения были заранее
  задокументированы и одно из них я подтвердил экспериментально.
- Расхождения STATUS с фактами по gate-таблице — не обнаружены
  (единственные фактические неточности STATUS — N1 и N7).
- `unsafe` — запрещён на уровне workspace (`unsafe_code = "forbid"`) и
  повторно в `lib.rs`.
- Коммиты соответствуют §33 (C001, C002, STOP STATUS_M0; C003 —
  дополнительный docs-only коммит, допустимо).

### Открытые блокеры автора (B1–B5) — решение ревьюера

- **B1 (F-0002, v-ice не собирается без dav1d/pkg-config):** принимаю
  вариант (c) — **v-ice остаётся честно зафиксированным
  `build_failed`-baseline-ом M0**. Молчаливая смена пина запрещена §2;
  установка системного dav1d или reviewed-смена пина обязана быть
  выполнена и перезаписана **до M3**, где baseline-ы входят в scorecard
  (§27.3). Для M0 typed-исход достаточен и информативен.
- **B2 (F-0003, Vice- pin не самодостаточен):** принимаю вариант (b) —
  частичное покрытие 3/5 входов как честный baseline. Подмешивание
  untracked-ассетов зеркала правильно запрещено. Явный asset-pin с хешами
  — решение к M3, не к M0.
- **B3 (license-пробелы доноров):** подтверждаю независимо (см. T2).
  Не блокирует M0, поскольку в M0 ноль перенесённого кода и репозиторий
  явно помечен «не публиковать». **Жёсткое условие: любой первый
  `[[unit]]` в `PORTING_MANIFEST.toml` требует предварительного
  license/IP review** (§2.1, §36).
- **B4 (CI ни разу не исполнялся):** не блокирует M0 (локальные
  эквиваленты воспроизведены мной с exit 0), но **обязан быть закрыт
  первым же push-ем**, вместе с N5.
- **B5 (timeout-kill не убивает внуков на Windows):** для M0 приемлемо,
  зафиксировано в ADR-0002 и `exec.rs`. Переоценить, когда появятся
  долгие/зависающие прогоны.

---

## 6. Итоговая оценка

M0 по §28 требует: только нужные crates; source pins/manifest/licenses;
`rust-toolchain.toml`, committed `Cargo.lock`, version manifest и
reproducible command environment; CI и deterministic smoke runner;
resource limits; baseline outputs; стоп после `STATUS_M0.md`.

Всё перечисленное присутствует и **воспроизведено независимо на чистом
клоне без авторских кэшей**: 29 из 29 artifact-хешей, config/corpus/pin/
status/determinism совпали; оба расхождения находятся строго внутри
заранее задокументированного списка допущений, причём ключевое из них
(`binary_sha256`) я подтвердил собственным экспериментом, а не принял на
веру. Пять adversarial-сценариев (включая три, которых нет в тестах
автора) повели себя корректно: порча corpus и превышение лимитов
абортируют прогон с exit 2 **до** любой работы с донорами, отсутствующее
зеркало и подменённый пин дают typed-ошибки при exit 0 с записанным
отчётом, невалидный пин отвергается конфиг-валидацией. Провал одного
baseline изолирован. Placeholder-ов, скрытых env-флагов, sample-specific
логики и следов копирования донорского кода не найдено. Провал v-ice и
частичный провал Vice- задокументированы **честно** — заявленное в
STATUS совпало с тем, что я получил сам, вплоть до причины и exit-кода.

Найденные расхождения (N1–N7) относятся к документации, гигиене отчётов и
латентным рискам; ни одно не подрывает ни один M0-гейт и ни одно не
является попыткой скрыть дефект. N1 и N2 — фактические неточности, которые
обязаны быть исправлены, но они не требуют перезаписи baseline-артефактов
и не влияют на воспроизводимость.

### Условия, обязательные к исполнению (не блокируют старт M1, проверяются на gate M1)

1. Восстановить заголовок `## F-0003` в `FAILURE_LEDGER.md` и привести
   счётчик записей в `STATUS_M0.md` §1 и `REQUIREMENTS_TRACEABILITY.md`
   M0-3 к фактическим четырём (N1).
2. Исправить или удалить `local_mirror_hint` для `Vice-` в
   `SOURCE_PINS.toml` (N2).
3. Пометить `binary_sha256` / `tool.exe_sha256` как non-normative либо
   вынести из `hashes.json`, чтобы документированная сверка стала
   побайтово исполнимой (N3, N7).
4. Первым push-ем получить зелёный CI, включая `gen-smoke --check` на
   Linux (B4, N5).
5. К M1: зафиксировать/санировать окружение дочерних процессов и
   отразить его в `env.json` (N6); `verify-corpus` должен принимать
   `--config` (N4).
6. Любой первый `[[unit]]` в `PORTING_MANIFEST.toml` — только после
   license/IP review донора (B3).
7. B1/B2 (v-ice build, Vice- ассеты) — закрыть отдельными reviewed-
   решениями **до M3**, не позже.

M1 разрешается в объёме §28 M1 (conventions + canonical IR + seal
skeleton) отдельным prompt-ом, одним milestone, с обязательной остановкой
на `STATUS_M1.md` и последующим независимым review (§32 п.29, §34).

---

## VERDICT: ACCEPT

M0 принят. Независимая clean-checkout репродукция состоялась, gate table
подтверждена построчно, adversarial-проверки пройдены. Gate T12 закрыт
настоящим документом. **M1 разрешён** при условии выполнения пунктов 1–7
§6 (проверяются на gate M1; перезаписи M0-baseline они не требуют).

—
*Independent reviewer (cold agent context, Opus)*
*2026-07-26 · reviewed HEAD `d3bbe738a6c4632130f539c13e91ce9c6e2f42ad` · spec v1.3 SHA-256 `652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1`*
