# STATUS_M0 — Bootstrap / provenance / baselines

Дата: 2026-07-26.
Spec: `VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md`
(SHA-256 `652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1`).
Автор: coding-агент (Claude Code), single-milestone run по §34.

> **Этот отчёт — author report. Он сам по себе НЕ делает M0 green.**
> M0 ожидает независимого clean-checkout review (`docs/REVIEW_M0.md`) от
> отдельного контекста/человека. **M1 не начат и не разрешён.**

## 1. Что сделано

- Rust workspace с единственным реально нужным M0 crate: `crates/vice-bench`
  (ADR-0001). Ноль placeholder-crates, ноль speculative API.
- Provenance: `SOURCE_PINS.toml` (3 пина из spec §2), `PORTING_MANIFEST.toml`
  (**ноль перенесённых блоков** — доноры исполняются как внешние чёрные
  ящики), `THIRD_PARTY_NOTICES.md` (включая license-находки по зеркалам).
- Governance: `REQUIREMENTS_TRACEABILITY.md`, `FAILURE_LEDGER.md`
  (4 записи: F-0001/F-0004 исправлены; F-0002/F-0003 — решения приняты
  REVIEW_M0), `docs/ADR/ADR-0001..0003`.
- Репродуцируемость: `rust-toolchain.toml` (1.96.0), committed `Cargo.lock`,
  env-manifest в каждом отчёте (`env.json` + `environment_sha256`),
  `docs/REPRODUCIBILITY_M0.md` с точными командами.
- Deterministic baseline runner (`baseline-runner`): свежий
  `git clone --local --no-checkout` + `checkout --detach <pin>` со сверкой
  `rev-parse HEAD`, сборка с таймаутом, прогон smoke corpus ×2 повтора,
  SHA-256 всех артефактов, typed errors, изоляция сбоев (ADR-0002).
- Input resource limits: размер файла до чтения, размеры из IHDR без
  декодирования (decompression-bomb-safe), таймауты build/run, лимит output.
- Фиксированный smoke corpus: 5 процедурных PNG + `SMOKE_MANIFEST.toml`
  (ADR-0003); `gen-smoke --check` побайтово регенерирует corpus из исходника.
- CI (`.github/workflows/ci.yml`): fmt, clippy `-D warnings`, tests,
  clean-checkout smoke (corpus check, verify-corpus, selftest, env).
- Записанный M0 baseline run: `docs/baselines/M0/{report,hashes,env}.json`
  + evidence-логи.

## 2. Записанный baseline run (repeats=2)

Команда — см. `docs/REPRODUCIBILITY_M0.md`. Ключевые значения:

```text
config_sha256      2376b348e77f9d2077750696360d48652bc94ed90e437b31ea0a896f07450a9c
environment_sha256 d1ccab0ce0ae698605c0ba55ba1dc63579f014973b8007655aae22f9da627d5e
runner exe sha256  de58f10d77412496733715c766dd3236e35abe814af3d216012d96250b077e14   # non-normative
```

`runner exe sha256` — non-normative (как и `binary_sha256` донорских
сборок, невоспроизводим между пересборками; REVIEW_M0 N7): это provenance
конкретного прогона, не критерий сверки.

| Baseline | Pin resolved | Статус | Runs ok | Primary det. | All-artifacts det. |
|---|---|---|---|---|---|
| v-ice | `9211b321…` ✓ | **failed: build_failed** (dav1d/pkg-config, F-0002) | 0/0 | n/a | n/a |
| v-ize | `95a65194…` ✓ | completed | 10/10 | **true** | **true** |
| Vice- | `200897ab…` ✓ | completed | 6/10 | **true** | **true** |

Детали:

- **v-ice**: checkout пина корректен; `cargo build --release --bin vice-cli`
  падает на `dav1d-sys` (пин требует avif-native/pkg-config; upstream сам
  удалил эту feature позже). Evidence:
  `docs/baselines/M0/evidence/v-ice-build.log`. Пин НЕ патчился (spec §2:
  не переключаться молча).
- **v-ize**: 5 входов × 2 повтора, все SVG побайтово идентичны между
  повторами. Известный риск недетерминизма (порядок stroke-слоёв из
  `centerline.rs`) на этом corpus не сработал — задокументирован в
  `configs/baselines.toml` notes и остаётся риском для будущих corpus.
- **Vice-**: rect_32 / circle_64 / ring_64 → SVG получены, детерминированы
  между повторами. triangle_128 / glyph_16 → `output_missing`: донор молча
  ловит `FileNotFoundError: models/corner_rf.joblib` (gitignored model,
  F-0003) и **выходит с кодом 0** — поймано проверкой declared outputs.
  Evidence: `docs/baselines/M0/evidence/vice--triangle_128-rep0.log`.

Сбой v-ice не помешал полным отчётам v-ize и Vice- (изоляция подтверждена
и интеграционным тестом `missing_mirror_is_isolated_typed_failure`).

## 3. Известный nondeterminism (честная фиксация)

1. `report.json` намеренно машинно/время-специфичен (runtimes, абс. пути);
   сравнивается только `hashes.json`.
2. `binary_sha256` донорских сборок нестабилен: эмпирически два полных
   record-прогона на ОДНОЙ машине и ОДНОМ пути дали разные хеши бинаря
   v-ize, при этом ВСЕ artifact-хеши (SVG) совпали побайтово. Поле
   записывается как provenance-факт конкретного прогона и исключено из
   критерия воспроизводимости (детерминизм считается по artifacts).
3. v-ice пишет `summary.json` с wall-clock ms — в этом прогоне не возник
   (build failed), но задокументирован как undeclared-артефакт на будущее.
4. v-ize centerline-порядок — риск, на данном corpus не проявился.
5. Донорские сборки используют default host toolchain (доноры не пиннят);
   версия зафиксирована в `env.json`.

## 4. Gate table (author-side)

| # | Gate (spec §28 M0 + bootstrap) | Статус | Evidence |
|---|---|---|---|
| T1 | Только нужные crates, без placeholder-ов | PASS | `Cargo.toml`, ADR-0001 |
| T2 | Pins/licenses/provenance записаны | PASS | `SOURCE_PINS.toml`, `PORTING_MANIFEST.toml` (0 units), `THIRD_PARTY_NOTICES.md` |
| T3 | Traceability / failure ledger / ADR | PASS | `REQUIREMENTS_TRACEABILITY.md`, `FAILURE_LEDGER.md`, `docs/ADR/` |
| T4 | Toolchain/lock/env manifest зафиксированы | PASS | `rust-toolchain.toml`, `Cargo.lock`, `env.json` |
| T5 | Deterministic baseline runner на фиксированном corpus | PASS | `docs/baselines/M0/hashes.json`: v-ize 10/10 det., Vice- det. на выданных артефактах |
| T6 | Hashes/runtime/exit/artifacts записаны | PASS | `docs/baselines/M0/report.json` |
| T7 | CI: format, clippy, tests, clean-checkout smoke | PASS (локальный эквивалент; сам GitHub-прогон требует push) | `.github/workflows/ci.yml`; локально: fmt ✓, clippy -D warnings ✓, 17 тестов ✓, selftest ✓ |
| T8 | Resource limits + typed errors | PASS | `limits.rs`/`error.rs` + unit/integration тесты |
| T9 | Сбой одного baseline не ломает отчёты остальных | PASS | записанный run (v-ice failed, остальные полные) + тест |
| T10 | Corpus/commands/outputs зафиксированы | PASS | `SMOKE_MANIFEST.toml`, `configs/baselines.toml`, `docs/baselines/M0/` |
| T11 | Воспроизводимость из clean checkout | PASS (author-side) | см. §5 |
| T12 | Независимый review подписан | **NOT DONE — блокирует M1** | ожидается `docs/REVIEW_M0.md` |

## 5. Clean-checkout репродукция (author-side)

Выполнена свежим агент-контекстом по командам из
`docs/REPRODUCIBILITY_M0.md` (клон в отдельный каталог, полный прогон,
пофайловое сравнение `hashes.json` с записанным).

**Раунд 1 (против HEAD C001): NOT REPRODUCIBLE — и это сработало как надо.**
Свежий агент-контекст нашёл два реальных дефекта:

1. Фикс F-0001 (нормализация путей) существовал только в рабочей копии —
   HEAD C001 детерминированно ронял полный прогон при относительном
   `--out` из документации. Шаги a–d (tests, gen-smoke --check,
   verify-corpus, selftest) воспроизводились полностью; при абсолютном
   `--out` v-ice и v-ize совпали с записанными хешами по всем полям, кроме
   допустимого `binary_sha256`.
2. Новая находка **F-0004**: пины содержат пути до ~160 символов; при
   длинной базе checkout-а git падает `Filename too long` (MAX_PATH).

**Исправления, вошедшие в C002:** фикс F-0001 закоммичен; runner передаёт
`-c core.longpaths=true` git-подпроцессам (F-0004); требование к путям
задокументировано в REPRODUCIBILITY_M0.md; canonical baseline run
перезаписан кодом C002 (см. §2 — хеши артефактов доноров при этом
совпадают с раундом до фикса).

**Раунд 2 (против C002):** повторная clean-checkout репродукция из
каталога с длинной базой — вердикт зафиксирован в
`docs/baselines/M0/REPRO_NOTE.md`.

Это author-side проверка; она НЕ заменяет независимый review (§34).

## 6. Blockers перед M1 (решения для reviewer-а)

1. **B1 (F-0002)**: v-ice pin не собирается на Windows без dav1d/pkg-config.
   Варианты: (a) установить dav1d системно и перезаписать baseline;
   (b) reviewed-смена пина на коммит после «Drop avif-native»;
   (c) принять v-ice как build_failed-baseline для M0. Runner готов к любому.
2. **B2 (F-0003)**: Vice- pin не самодостаточен (нет манифеста зависимостей;
   models/ вне git). Варианты: (a) явный reviewed asset-pin с хешами;
   (b) принять частичное покрытие (3/5 входов) как честный baseline.
3. **B3**: license-пробелы доноров (v-ice/Vice- без LICENSE; v-ize без
   текстов лицензий, 2 crates без наследования) — до публичного релиза
   обязателен отдельный license/IP review (`THIRD_PARTY_NOTICES.md`).
4. **B4**: CI-workflow закоммичен, но ни разу не исполнялся на GitHub
   (репозиторий локальный, remote отсутствует). Первый push должен
   подтвердить зелёный прогон, включая кросс-платформенный
   `gen-smoke --check` на Linux.
5. **B5**: таймаут-kill на Windows не убивает внуков процесса (cargo→rustc);
   для M0 достаточно, зафиксировано в ADR-0002.

## 7. Явное заявление об остановке

Автор НЕ самосертифицирует M0. Milestone считается green только после
независимого clean-checkout review с подписью в `docs/REVIEW_M0.md`
(spec §32 правило 29, §34). До этого:

**STOPPED AFTER M0 — M1 NOT STARTED.**
