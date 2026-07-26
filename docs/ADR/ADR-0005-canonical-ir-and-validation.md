# ADR-0005 — Canonical IR M1: типы, shared parameters, граница валидации

Дата: 2026-07-26. Статус: accepted (M1).

## Контекст

Spec v1.3 §6 задаёт canonical IR (planar graph + typed curves + paint +
formation), §12 — инварианты graph, §28 M1 — «exterior face/DCEL types,
geometry/paint/formation/hypothesis types», §32 п.7 — запрет API без call
sites. REVIEW_M0 разрешил M1 в этом объёме.

## Решения по типам (crates/vice-ir)

1. **Идентичность = индекс.** `VertexId/BoundaryId/HalfEdgeId/FaceId` —
   newtype-индексы в соответствующие `Vec`. Поля `id` внутри структур из
   §6.1-эскиза не дублируются (валидация всё равно требовала бы
   `id == index`).
2. **Shared parameters хранятся один раз (§5.5).** Позиция общей вершины —
   только в `GraphVertex`; кривая общей границы — только в
   `Boundary.curve`; ENDPOINTS chain-а НЕ хранятся в `CurveChain` — они
   берутся из start/end vertex границы. Interior nodes хранятся один раз;
   smooth join хранит shared tangent parameter один раз в
   `JoinKind::SmoothG1 { tangent_angle_rad }` (канонический диапазон
   `(-pi, pi]`).
3. **Segment-параметры без дублирования endpoints:** `Line` (без
   параметров), `CircularArc { radius_px, large_arc, ccw }` и
   `EllipticArc { rx_px, ry_px, x_axis_rotation_rad ∈ [0, pi), … }` —
   endpoint-параметризация (валидируется представимость: `2r >= chord`,
   SVG-F.6.6 lambda ≤ 1); `Quad/Cubic` — абсолютные control points.
   Handles в polar-параметризации от shared tangent (§18.1) — решение
   этапа joint refit (M6): в M1 tangent хранится как заявленный shared
   parameter, а согласованность tangent ↔ geometry НЕ валидируется
   (известное ограничение; exact G1 по spec возникает из joint solve, не
   из проверки маленького угла).
4. **Exterior — настоящий `FaceId`** в `faces`; paint exterior обязан быть
   `TransparentExterior`. Interior faces ТОЖЕ могут быть
   `TransparentExterior`: прозрачные дыры/counters (внутренность бублика)
   — отдельные bounded faces партиции. Обратная импликация
   («transparent ⇒ exterior») неверна и не навязывается.
5. **Не создано (нет call sites в M1, §32 п.7):** `SceneHypothesis` c
   posterior/delivery-полями (первый producer — M5+), `EvidenceRef`,
   `SupportRef`, `SceneProvenance`, `DecisionInterval`, `ExportPlan`.
   Появятся вместе со своими producers.
6. **Formation-типы** — ровно минимальная M4-family (§10.1/§16.2):
   `BlendSpace{LinearLight,EncodedSrgb}`, глобальный
   `PixelFilter{Box,Triangle,Gaussian{sigma_px}}`,
   `QuantizationModel{Uint8}`, `ExteriorModel{Transparent,Opaque}`.
   Никаких per-edge kernels — параметра «на границу» в типах просто нет.

## Решения по валидации (§12, validate.rs)

Invalid graph — typed reject (`SceneError`/`GraphError` через thiserror),
никогда не panic. Порядок: number rules (§5.5: NaN/Inf/`-0` запрещены) →
canvas/formation → структурные инварианты → геометрия сегментов.

Проверяемые структурные инварианты: id-диапазоны; twin-инволюция (twin ≠
self, та же boundary, противоположное направление); ровно один
forward + один reverse half-edge на boundary («два владельца» interior
boundary; border boundary имеет interior+exterior владельца автоматически:
left ≠ right и exterior — обычный face); face half-edge = left/right
стороне boundary; `next` — перестановка с геометрической непрерывностью
(target(h) = origin(next(h))); циклы замкнуты и не смешивают faces;
`Face.loops` — ровно один представитель на фактический цикл; отсутствие
dangling cracks (left ≠ right); отсутствие isolated vertices; запрет
дубликатов вершинных позиций и дубликатов boundaries (нужен и для
canonical total order, ADR-0006); **Euler-проверка `V − E + F = 1 + C`** —
она отклоняет непланарные rotation systems (тест: тор из трёх петель),
что локальными проверками не ловится.

### Граница M1 для «non-adjacent boundaries не пересекаются»

- **Line-line пары — точное решение** robust-предикатами: proper crossing,
  T-touch, endpoint touch, collinear overlap (в т.ч. за общей вершиной —
  `shared_endpoint_segments_overlap`) → typed reject.
- **Пары с кривыми:** M1 умеет только СЕРТИФИЦИРОВАТЬ непересечение через
  консервативные enclosure-боксы (control-polygon hull для Bezier;
  endpoint-box + inflate 2r/2·max(rx,ry) c ulp-guard для дуг). Пара, чьи
  боксы пересекаются, — **UNDETERMINED**: она не отклоняется и не
  замалчивается, а возвращается `uncertified_interference_pairs()` как
  явный worklist для certified curve-curve intersection (M2+).
- Геометрическая ориентация loops (outer vs holes по signed area) тоже
  требует curve-машинерии → M2+; M1 проверяет только комбинаторную
  согласованность сторон.

Смежные (с общей вершиной) кривые пары вне §12-инварианта: их
самопересечение — предмет geometric verification M2+/§20 (задокументировано
в STATUS_M1 known limitations).

## Последствия

- Полнота отклонения инвалидных графов в M1 — структурно-комбинаторная +
  точная линейная геометрия; никакая «неопределённая» пара не считается
  доказанно безопасной.
- Каждый вариант ошибки покрыт негативным тестом
  (`tests/validate_rejects.rs`), включая Euler/torus и near-adjacent
  случаи.
