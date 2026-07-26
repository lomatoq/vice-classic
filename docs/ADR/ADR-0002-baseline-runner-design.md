# ADR-0002 — Дизайн deterministic baseline runner

Дата: 2026-07-26. Статус: accepted (M0).

## Контекст

M0 gate требует: воспроизводимый прогон pinned систем на фиксированном
smoke corpus; hashes binary/source/config/input/toolchain/environment;
runtime и exit status; typed errors; изоляцию сбоев одного baseline от
отчётов остальных; честную фиксацию nondeterminism.

## Решения

1. **Provenance через git, а не через worktree.** Каждый запуск делает
   свежий `git clone --local --no-checkout <mirror>` +
   `git checkout --detach <pin_sha>` и сверяет `rev-parse HEAD` с пином.
   Состояние рабочей копии зеркала (ветка, dirty-файлы, untracked модели)
   в прогон не попадает. Расхождение — typed `checkout_mismatch` /
   `pin_unavailable`.
2. **Typed error taxonomy** (`error.rs`): `BaselineError` (mirror/checkout/
   build уровень) и `RunError` (input/timeout/exit/output уровень).
   Ошибка помечает только свой baseline/run; цикл продолжается. Fatal — лишь
   непарсабельный config и непрошедший integrity-проверку corpus (отчёт
   против дрейфнувшего corpus не имеет смысла).
3. **Determinism через повторы.** Каждый (baseline, input) исполняется
   `--repeats` раз (по умолчанию 2) в свежих work-каталогах. Все записанные
   файлы хешируются; расхождения перечисляются пофайлово. Разделение:
   `primary_deterministic` (declared outputs) vs
   `all_artifacts_deterministic` (все side-файлы) — донор, пишущий
   wall-clock ms в summary.json, не маскирует и не «заваливает» вердикт по
   основному SVG.
4. **Два отчёта.** `report.json` — полный, машинно-специфичный (runtimes,
   абсолютные пути). `hashes.json` — детерминированное подмножество в
   сортированных map-ах для побайтового сравнения между прогонами на одной
   машине/binary.
5. **Resource limits до запуска** (`limits.rs`): размер файла из metadata до
   чтения; размеры из IHDR без декодирования (защита от decompression
   bomb); таймауты build/run; лимит размера output.
6. **Никаких hidden env flags** — всё поведение из CLI-флагов и
   `configs/baselines.toml` (spec §32 правило 4).
7. **Selftest без зеркал**: встроенный copy-adapter гоняет весь механизм
   (exec, хеши, повторы, отчёты) в CI из clean checkout.
8. **Пути нормализуются в абсолютные на входе** (см. FAILURE_LEDGER
   F-0001).

## Известные ограничения (задокументировано, не скрыто)

- На Windows kill по таймауту убивает только прямого ребёнка (cargo), не
  внуков (rustc).
- `binary_sha256` зависит от toolchain и абсолютного пути checkout-а
  (паник-строки в release-бинарях) — сравним только в пределах одной
  машины и одинакового out-пути.
- Донорские сборки используют default host toolchain (доноры не пиннят
  свой); версия фиксируется в env.json каждого прогона.
