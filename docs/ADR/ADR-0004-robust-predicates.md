# ADR-0004 — Robust predicates: внешний crate `robust`, тонкий typed adapter

Дата: 2026-07-26. Статус: accepted (M1).

## Контекст

Spec v1.3 §5.4: combinatorial topology decisions обязаны использовать
adaptive/exact predicates, не `abs(cross) < eps` в f64. REVIEW_M0 условие 6:
первый `[[unit]]` в PORTING_MANIFEST.toml разрешён только после license/IP
review донора; у `v-ice` лицензии нет вовсе. Значит порт донорского кода для
predicates невозможен.

## Рассмотренные варианты

1. **Порт из донора** — запрещён (условие 6; v-ice unlicensed).
2. **Clean-room реализация по статье Shewchuk (§37 п.11)** — законно, но
   это многомесячная работа с высокой ценой ошибки (адаптивная арифметика
   ошибается тихо), и она не входит в критический путь M1.
3. **Внешний OSS-crate `robust` (georust/robust)** — выбран.

## Решение

- Зависимость `robust = "1"` (разрешилось в **robust 1.2.0**).
- **Лицензия проверена по фактическому пакету из registry**: `license =
  "MIT OR Apache-2.0"`, файлы LICENSE-MIT и LICENSE-APACHE присутствуют
  в crate. Записано в THIRD_PARTY_NOTICES.md. Это Rust-порт predicates
  Shewchuk-а (исходник — public domain C).
- Адаптер `vice-geom::predicates` — тонкий и typed:
  - `orient2d -> Orientation {CounterClockwise, Clockwise, Collinear}`;
  - `incircle -> Option<CirclePosition>`: нормализован по ориентации
    треугольника (ответ не зависит от порядка a,b,c) и возвращает `None`
    для коллинеарного треугольника — вырожденность не маскируется
    произвольным ответом;
  - точные segment-предикаты (`point_on_closed_segment`,
    `closed_segments_intersect`, `shared_endpoint_segments_overlap`),
    построенные ТОЛЬКО из orient2d + точных сравнений координат.
- Голый f64-знак из модуля не экспортируется: вызывающий код не может
  переинтерпретировать знак (особенно в y-вниз рамке, где алгебраический
  CCW выглядит по часовой стрелке).

## Верификация

- Unit-тесты near-degenerate случаев: решётка 0.5 + i·2⁻⁵³ против линии
  (12,12)–(24,24) со сверкой знака с точной i128-арифметикой; cocircular
  прямоугольник и ±1 ulp пертурбации для incircle.
- Property-тесты (proptest) против независимой exact-i128 реализации на
  диадической решётке: orient2d, incircle, point-on-segment,
  segment-intersection; сконструированные точные коллинеарные тройки и
  one-step пертурбации.

## Последствия

- Whole-file вклад `robust` остаётся внешней зависимостью, не vendored —
  PORTING_MANIFEST.toml по-прежнему 0 units.
- Certified curve–curve intersection (Bézier/arc) НЕ входит в M1; adapter
  покрывает точки/отрезки. Полная машинерия — M2+ (см. ADR-0005, граница
  валидации M1).
- `DecisionInterval` (§5.4) появится вместе с первым producer score-ов;
  в M1 нет ни одного score, интервал без call site не заводится (§32 п.7).
