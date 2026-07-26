# REPRODUCIBILITY_M0 — как воспроизвести M0 baseline run

## Требования окружения

Записаны в `docs/baselines/M0/env.json` записанного прогона. Кратко:

- Windows 11 x86_64 (family=windows). Runner сам передаёт
  `-c core.longpaths=true` git-подпроцессам (пины содержат пути до ~160
  символов; при длинной базе checkout-а без этого ломается MAX_PATH —
  см. FAILURE_LEDGER F-0004). Если git старее 2.40 или окружение
  запрещает longpaths — используйте короткую базу для `--out`.
- ВАЖНО (не покрывается longpaths): MSVC-линкер донорской сборки v-ize
  падает `LNK1104`, если `<база checkout-а>/runs/<out>` длиннее
  ~137 символов (build-пути wgpu-стека упираются в MAX_PATH). Держите
  суммарную базу короткой; подтверждено репродукцией
  (docs/baselines/M0/REPRO_NOTE.md).
- rustc/cargo 1.96.0 (пин в `rust-toolchain.toml` действует на сам runner;
  донорские сборки используют default host toolchain — доноры свой не пиннят)
- git ≥ 2.40
- python 3.12 (для baseline `Vice-`; зависимости донора манифестом не
  зафиксированы — см. FAILURE_LEDGER F-0003)

## Локальные зеркала pinned-репозиториев

Runner не ходит в сеть. Нужен каталог (--mirror-root), содержащий клоны, в
объектах которых есть pinned commit SHA:

| mirror_hint | репозиторий | pin |
|---|---|---|
| `v-ice` | lomatoq/v-ice | `9211b3213d9b47defdf19ae4d0842af1c3ade45f` |
| `v-ize` | lomatoq/v-ize | `95a65194cf34e2d96b41eb299b4769eac624be80` |
| `v-ice part` | lomatoq/Vice- | `200897ab3e888970e330deeb3bb9e157923cc0aa` |

Состояние рабочей копии зеркала не важно: runner всегда делает свежий
`git clone --local --no-checkout` + `git checkout --detach <pin>` и сверяет
`rev-parse HEAD` с пином. На записывавшей машине mirror-root =
`C:\Users\nirrt\Toolset`.

## Команды (из корня clean checkout)

```bash
cargo test --workspace
```

```bash
cargo run --release --bin gen-smoke -- --out tests/fixtures/smoke --check
```

```bash
cargo run --release --bin baseline-runner -- verify-corpus --corpus tests/fixtures/smoke --manifest tests/fixtures/smoke/SMOKE_MANIFEST.toml
```

```bash
cargo run --release --bin baseline-runner -- selftest --out runs/selftest --corpus tests/fixtures/smoke --manifest tests/fixtures/smoke/SMOKE_MANIFEST.toml
```

Полный записанный M0-прогон (замените mirror-root на свой):

```bash
cargo run --release --bin baseline-runner -- run --config configs/baselines.toml --corpus tests/fixtures/smoke --manifest tests/fixtures/smoke/SMOKE_MANIFEST.toml --mirror-root "C:\Users\nirrt\Toolset" --out runs/m0 --repeats 2
```

Затем сравните `runs/m0/hashes.json` с записанным
`docs/baselines/M0/hashes.json`, исключив из сравнения поля
`binary_sha256` (non-normative provenance; REVIEW_M0 N3 — разделение
hashes.json на нормативную/информационную секции выполняется вместе с
pre-M3 перезаписью baseline-ов по B1/B2: менять схему artefact-файла без
одновременной перезаписи записанных артефактов нельзя, а
`docs/baselines/M0/**` неприкосновенны; см. STATUS_M1 §blockers).

**Схема env.json (примечание M1/C009):** начиная с C009 env.json содержит
дополнительный блок `command_env` (политика санации окружения дочерних
процессов, ADR-0007), поэтому побайтовое сравнение env.json с записанным
`docs/baselines/M0/env.json` корректно только на M0-коммитах (C002–C005).
Сами записанные M0-артефакты не изменялись.

## Что обязано совпасть, что нет

Совпадает (та же машина, тот же toolchain, тот же mirror-set):

- `config_sha256`, все corpus-хеши, `resolved_sha` каждого baseline;
- статусы/error kinds baseline-ов;
- хеши declared artifacts у детерминированных baseline-ов.

Может законно отличаться (задокументировано):

- `report.json` целиком (runtimes, абсолютные пути);
- `binary_sha256` донорских сборок — эмпирически нестабилен даже между
  пересборками на одном и том же пути/toolchain (записывается как
  provenance конкретного прогона, критерием воспроизводимости не является;
  критерий — artifact-хеши);
- side-артефакты доноров, содержащие wall-clock (например `summary.json`
  у v-ice) — они помечены в отчёте как undeclared и попадают в
  `all_artifacts_deterministic=false`, не трогая `primary_deterministic`;
- `environment_sha256` на другой машине.

Любое расхождение сверх этого списка — находка для REVIEW_M0, а не шум.
