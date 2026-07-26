# ADR-0001 — M0 scope: один crate, ноль placeholder-ов

Дата: 2026-07-26. Статус: accepted (M0).

## Контекст

Spec v1.3 §4.1/§28 запрещает создавать пустые crates «на будущее» и
speculative interfaces. Единственная executable-ответственность M0 —
deterministic baseline runner для pinned донорских систем.

## Решение

- Workspace содержит ровно один crate: `crates/vice-bench`.
- `vice-geom`, `vice-ir`, `vice-image`, `vice-evidence`, `vice-render`,
  `vice-cli` НЕ создаются в M0 — их первая реальная ответственность
  наступает в M1+.
- Донорские системы исполняются как внешние чёрные ящики: их код не
  линкуется, не вендорится и не копируется (`PORTING_MANIFEST.toml` в M0
  пуст — ноль units).
- `[workspace] exclude = ["runs", "baselines-work"]`, потому что checkout-ы
  доноров клонируются под `runs/<out>/checkouts/`, а single-crate донор без
  собственного `[workspace]` иначе был бы захвачен нашим workspace и ломал
  бы `cargo build` пина.

## Последствия

M1 начнёт canonical conventions/IR в новых crates только после независимого
REVIEW_M0. Runner из M0 остаётся measurement-инфраструктурой (spec §32
правило 2: measurement раньше algorithm change).
