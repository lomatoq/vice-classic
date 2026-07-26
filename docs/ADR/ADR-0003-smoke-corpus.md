# ADR-0003 — Процедурный smoke corpus вместо донорских картинок

Дата: 2026-07-26. Статус: accepted (M0).

## Контекст

M0 нужен маленький фиксированный smoke corpus. Донорские репозитории
содержат тестовые изображения, но их лицензионный статус owner-controlled,
а сами картинки не воспроизводимы из исходника.

## Решение

Corpus из 5 PNG генерируется детерминированным `gen-smoke`
(8×8 supersampling, только f64 mul/add/round — воспроизводимо побайтово;
энкодер зафиксирован `Cargo.lock`):

| Файл | Что покрывает |
|---|---|
| rect_32.png | двухцветный hard-edge, без AA |
| circle_64.png | AA-диск (кривизна) |
| ring_64.png | топология с дыркой (annulus) |
| triangle_128.png | наклонные AA-рёбра |
| glyph_16.png | RGBA, opaque shape на полностью прозрачном exterior |

Файлы закоммичены; `SMOKE_MANIFEST.toml` пиннит SHA-256 и размеры;
`verify-corpus` отклоняет drift (missing/extra/mismatch). CI дополнительно
проверяет `gen-smoke --check`: регенерация побайтово равна закоммиченным
файлам (заодно это кросс-платформенный determinism-пробник).

## Последствия

- Чистая provenance: corpus порождён этим репозиторием, лицензионных
  вопросов нет.
- Это smoke, а НЕ GT-бенчмарк: identifiability-classes, degradation matrix
  и heterogeneous corpus — задачи M3, здесь их осознанно нет.
