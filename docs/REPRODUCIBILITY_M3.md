# REPRODUCIBILITY_M3 — воспроизведение записанного baseline-прогона

Дата записи: 2026-07-26. Артефакты: `docs/baselines/M3/`
(`hashes.json`, `report.json`, `env.json`, `evidence/`).
Схема артефактов: `vice-classic/baseline-{report,hashes}/v3`.

Этот документ заменяет `docs/REPRODUCIBILITY_M0.md` в роли действующей
инструкции. M0-документ и `docs/baselines/M0/**` **не изменялись** и
остаются историческими: они описывают прогон схемы v1 и воспроизводимы
только на M0-коммитах, что для `env.json` было верно уже с C009.

## Что изменилось относительно M0-прогона (и почему)

| | M0 (`docs/baselines/M0/`) | M3 (`docs/baselines/M3/`) |
|---|---|---|
| baseline-ов | 3 | **4** (добавлен `v-ice-avif-fix`, ADR-0014 / блокер B1) |
| v-ice @ spec pin | `build_failed` (dav1d/pkg-config) | `build_failed` — **без изменений, намеренно** |
| v-ice-avif-fix | — | `completed`, 10/10 ok, primary deterministic |
| v-ize | `completed`, 10/10 | `completed`, 10/10, primary+all deterministic |
| Vice- | `completed`, **6/10** (`output_missing` ×4) | `completed`, **10/10**, primary+all deterministic |
| ассеты | не пиннятся | `models/corner_rf.joblib` пиннится sha256+длиной (B2) |
| `hashes.json` | плоский, содержал `binary_sha256` | **normative / informational** (D-1) |
| `environment_sha256` | покрывал весь манифест | покрывает **нормативную проекцию** (D-1) |

Итог по существу: было 26 из 40 объявленных прогонов ok, стало 30 из 30 у
трёх исполнимых baseline-ов; четвёртый честно не собирается и это записано.

## Требования окружения

- Windows 11 x86_64 (записывающая машина); `rustc`/`cargo` **1.96.0**
  (пин действует на runner, донорские сборки берут default host toolchain);
  git ≥ 2.40; python 3.12 для `Vice-`.
- Runner сам передаёт `-c core.longpaths=true` git-подпроцессам (F-0004).
- **Не покрывается longpaths:** MSVC-линкер v-ize падает `LNK1104`, если
  `<база checkout-а>/runs/<out>` длиннее ~137 символов. Держите базу
  короткой (записывающий прогон: `<repo>/runs/m3`, 42 символа).
- `pkg-config`/`dav1d` НЕ требуются и намеренно не устанавливаются: их
  отсутствие — это и есть содержание записанного `build_failed` v-ice
  (ADR-0014 отвергает установку системной зависимости как способ сделать
  baseline свойством одной машины).

## Зеркала и ассеты

Runner не ходит в сеть.

| mirror_hint | репозиторий | pin |
|---|---|---|
| `v-ice` | lomatoq/v-ice | `9211b3213d9b47defdf19ae4d0842af1c3ade45f` (spec §2) |
| `v-ice` | lomatoq/v-ice | `59ab86d16458a43877a72270dfd71a68ff9eecb7` (`v-ice-avif-fix`; сиблинг spec-пина, вне remote-ссылок зеркала — ADR-0014) |
| `v-ize` | lomatoq/v-ize | `95a65194cf34e2d96b41eb299b4769eac624be80` |
| `v-ice part` | lomatoq/Vice- | `200897ab3e888970e330deeb3bb9e157923cc0aa` |

Состояние рабочей копии зеркала не важно: всегда свежий
`git clone --local --no-checkout` + `git checkout --detach <pin>` + сверка
`rev-parse HEAD`.

Ассеты (`--asset-root`, раскладка `<asset-root>/<mirror_hint>/<path>`):

| файл | bytes | sha256 |
|---|---|---|
| `v-ice part/models/corner_rf.joblib` | 109 328 564 | `0b9e30375acc6b7c28eb02331a102242723a19b58cc7db5c96eebc780e0f3941` |

Файл в репозиторий не кладётся (104 MiB) — пиннится хеш. Он проверяется
ДО копирования: другой файл даёт типизированный `asset_mismatch`, а
отсутствие `--asset-root` — `asset_root_missing`. На записывающей машине
mirror-root и asset-root совпадают (`C:\Users\nirrt\Toolset`); это не
требование — источник задаётся флагом, а доверие даёт хеш.

## Команды (из корня чистого checkout-а)

```bash
cargo test --workspace
cargo run --release --bin gen-smoke -- --out tests/fixtures/smoke --check
cargo run --release --bin baseline-runner -- verify-corpus \
  --corpus tests/fixtures/smoke --manifest tests/fixtures/smoke/SMOKE_MANIFEST.toml
cargo run --release --bin baseline-runner -- selftest \
  --out runs/selftest --corpus tests/fixtures/smoke \
  --manifest tests/fixtures/smoke/SMOKE_MANIFEST.toml
```

Записанный прогон:

```bash
cargo run --release --bin baseline-runner -- run \
  --config configs/baselines.toml \
  --corpus tests/fixtures/smoke \
  --manifest tests/fixtures/smoke/SMOKE_MANIFEST.toml \
  --mirror-root "C:\Users\nirrt\Toolset" \
  --asset-root  "C:\Users\nirrt\Toolset" \
  --out runs/m3 --repeats 2
```

## Сверка — теперь команда, а не просьба

```bash
cargo run --release --bin baseline-runner -- compare-hashes \
  --recorded docs/baselines/M3/hashes.json \
  --actual   runs/m3/hashes.json
```

Exit 0 — воспроизведено; exit 1 — печатает JSON-pointer каждого
расхождения; exit 2 — файл не прочитался/не распарсился.

Раньше инструкция «сравните `hashes.json`» была **невыполнима**: файл
содержал `binary_sha256`, который законно отличается у двух корректных
прогонов (проба P1 в REVIEW_M0: пересборка того же исходника тем же
toolchain по тому же пути дала другой хеш при побайтово совпавших SVG).
Теперь `binary_sha256` живёт в секции `informational`, а `compare-hashes`
сравнивает только `normative`.

Что входит в `normative`: `config_sha256`, `environment_sha256`, хеши
корпуса, а по каждому baseline — `status`, `error_kind`, `resolved_sha`,
хеши **застейдженных ассетов**, хеши всех артефактов repeat 0 и оба вердикта
детерминизма.

`environment_sha256` считается по **нормативной проекции** `env.json`
(os/arch/family, четыре версии инструментов, политика child-env). Вне
хеша, но внутри файла: `logical_cpus` и `command_env.ambient_overrides_present`
— две одинаковые машины, отличающиеся лишь наличием `CARGO_INCREMENTAL`,
больше не дают разные env-хеши (M1-N4). Проверить проекцию:

```bash
cargo run --release --bin baseline-runner -- env --normative
```

## Что может законно отличаться

- `report.json` целиком (runtimes, абсолютные пути, `tool.exe_sha256`);
- `informational.binary_sha256` донорских сборок;
- побочные артефакты с wall-clock. Именно они дают
  `all_artifacts_deterministic: false` у `v-ice-avif-fix`: пять
  расхождений, все — `summary.json`, который донор пишет с миллисекундами
  (задокументированный caveat конфига). `primary_deterministic` при этом
  `true`: объявленные `{stem}/{stem}.svg` совпадают побайтово.

Любое расхождение сверх этого списка — находка, а не шум.

## GT-корпус (M3)

Корпус НЕ хранится в репозитории: он регенерируется из исходников. Хранятся
только шесть авторских SVG (`tests/fixtures/gt/authored/`) и манифест с
sha256 каждого рендера — тот же контракт, что у `SMOKE_MANIFEST.toml`, на
корпусе на три порядка больше.

```bash
# Пересобрать корпус и записать манифест (scope: full | fast | test)
cargo run --release --bin gt-corpus -- build \
  --out docs/gt/CORPUS_MANIFEST.json --scope fast

# Воспроизвести: пересобрать в ТОМ ЖЕ scope, что записан в манифесте,
# и сверить каждый render digest
cargo run --release --bin gt-corpus -- verify \
  --manifest docs/gt/CORPUS_MANIFEST.json

# Отчёт M3 и gate table
cargo run --release --bin gt-corpus -- report \
  --manifest docs/gt/CORPUS_MANIFEST.json \
  --gates configs/GATES_V1.toml \
  --seal docs/gt/AUDIT_SEAL.json \
  --out docs/gt/SCORECARD_M3.json

# Burn policy sealed audit (в CI на каждом push)
cargo run --release --bin gt-corpus -- audit-status \
  --seal docs/gt/AUDIT_SEAL.json \
  --manifest docs/gt/CORPUS_MANIFEST.json \
  --gates configs/GATES_V1.toml
```

### Честно про SCOPE записанного манифеста

Записанный `docs/gt/CORPUS_MANIFEST.json` собран в scope **`fast`**
(18 ячеек, размеры ≤32, без суперсэмплированного box-спайна): 60 групп,
63 сцены, **1086 рендеров**, 4 мин 23 с на записывающей машине.

Scope `full` (47 ячеек, включая 512 px) на одном ядре идёт **часами**:
exact-clip на 512×512 — это ~1e6 отсечений полигона на face, и прогон был
прерван спустя час. Это предел ИНСТРУМЕНТА, и он записан как факт, а не
обойдён: манифест содержит собственный список ячеек, его хеш зависит от
scope, а тест `a_partial_manifest_cannot_be_mistaken_for_the_full_one`
запрещает выдать частичный прогон за полный. Ускорение exact-clip (обход по
строкам вместо bbox-сканирования) — работа M3.5+, а не тихое сужение матрицы:
матрица заморожена целиком, и `--scope full` доступен любому, кто готов ждать.

CI использует scope `test` (одна размерность, секунды) и проверяет
ДЕТЕРМИНИЗМ (две сборки побайтово), а не полноту.

### Что при сверке может законно отличаться

**Платформа.** Digest-ы корпуса — артефакт **Tier A** (§5.5): геометрия
строится `sin`/`cos`, цвет `powf`, гауссов PSF `exp`, а Rust не гарантирует
битовую идентичность libm между платформами (ADR-0008 §8 записал это ещё в
M2). Поэтому:

```bash
# на платформе записи (windows/x86_64) — полная сверка 1086 digest-ов
cargo run --release --bin gt-corpus -- verify --manifest docs/gt/CORPUS_MANIFEST.json

# на другой платформе БЕЗ флага — типизированный отказ, exit 2
# со флагом — платформенно-независимая проекция, и она это печатает
cargo run --release --bin gt-corpus -- verify   --manifest docs/gt/CORPUS_MANIFEST.json --structural
```

Структурный режим сверяет состав, split-ы, список ячеек, метки
identifiability и флаги inverse-crime — всё, кроме float-величин. Он НЕ
является заменой полной сверке и говорит об этом в выводе.

На ОДНОЙ платформе не может отличаться ничего: расхождение digest-а —
находка (F-0020).

**Оговорка, которую надо знать (REDTEAM_M2 addendum 4, п. 3):** digest-ы
рендеров сравнимы ТОЛЬКО в пределах одной версии renderer-а. C052 показал,
что арифметика покрытия не бит-нейтральна между версиями (0.38 % значений,
max |Δ| = 5.25e-14). Поэтому `verify` сверяет корпус с самим собой на одном
коммите; межверсионная сверка render digest-ов НЕ заявляется и заявляться не
будет без отдельного бит-нейтрального контракта.
