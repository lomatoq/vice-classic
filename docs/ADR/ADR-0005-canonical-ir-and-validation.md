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
5. **Не создано (§32 п.7):** `SceneHypothesis` c
   posterior/delivery-полями (первый producer — M5+), `EvidenceRef`,
   `SupportRef`, `SceneProvenance`, `DecisionInterval`, `ExportPlan`.
   Появятся вместе со своими producers.

   **Критерий применения §32 п.7 (уточнение по REVIEW_M1 M1-N9).**
   Решающий вопрос — не «есть ли call site прямо сейчас», а
   «принадлежит ли СЕМАНТИКА типа текущему милестоуну». Конвенции §5
   (connectivity, color/premultiply, sRGB transfer) — прямые deliverables
   M1 по §28 («coordinates/color/topology conventions»), поэтому они
   созданы и покрыты тестами, хотя их первый продакшн-потребитель
   появляется позже. Типы из списка выше, напротив, семантически
   определены через сущности M4–M7 (posterior, evidence, delivery
   equivalence, export) — созданные в M1, они были бы пустыми оболочками,
   то есть placeholder API. Одним «нет call site» можно было бы срезать и
   конвенции — этот тест НЕ является критерием; милестоун-принадлежность
   семантики — является. Первый милестоун, порождающий
   posterior/score/evidence, обязан ввести соответствующий тип вместе с
   его producer-ом.
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

### Chain-point distinctness: закрытие блокера REVIEW_M1 M1-N1 (C012)

REVIEW_M1 (§5) доказал, что первоначальная реализация определяла смежность
сегментов по РАВЕНСТВУ ПОЗИЦИЙ концов, а не по тождеству graph-вершин:
непредставленный junction (interior-узел цепочки, совпадающий с чужой
graph-вершиной — сцена C1, или два interior-узла разных boundaries в одной
точке — сцена C4) принимался молча и не попадал в uncertified worklist.

**Выбранное исправление — вариант (b) из review: инвариант попарной
различности ВСЕХ точек цепочек.** Graph-вершины уже были попарно различны
(`DuplicateVertexPosition`); новый инвариант
(`check_chain_point_distinctness`, typed-ошибка
`UnrepresentedJunction { a, b: ChainPointRef }`) требует того же от
interior-узлов: interior-узел не может совпадать по позиции ни с одной
graph-вершиной и ни с одним другим interior-узлом (включая узлы своей же
цепочки — самокасание). Это прямое прочтение §12 «robust junctions»: в
арранжировке ЛЮБАЯ точка, где сходятся дуги, обязана быть вершиной.

Почему (b), а не (a) («смежность по тождеству VertexId»): после инварианта
(b) равенство позиций точек цепочек СТАНОВИТСЯ тождеством точек — у двух
различных точек цепочек не бывает одной позиции. Поэтому существующий
позиционный тест смежности из эвристики превращается в точный, что
зафиксировано в доккомментариях `shared_points` и
`uncertified_interference_pairs`; вариант (a) оставил бы совпадающие
позиции легальными и потребовал бы более сложной классификации пар. Ошибка
называет обе точки (`ChainPointRef::Vertex` / `InteriorNode{boundary,node}`)
— это осмысленный диагноз «нужна общая вершина», а не общий intersect.

Порядок проверок: инвариант выполняется после per-segment проверок
(zero-length span остаётся `DegenerateSegment`) и ДО interference-этапа,
чья корректность от него зависит.

### Граница M1 для «non-adjacent boundaries не пересекаются»

Смежность пары сегментов = наличие общей ТОЧКИ ЦЕПОЧКИ; после инварианта
различности это тождественно наличию общего узла (общая graph-вершина между
boundaries либо общий interior-узел/loop-вершина последовательных сегментов
одной цепочки). Формулировка «с общей вершиной» в прежней редакции этого
ADR не совпадала с кодом — REVIEW_M1 M1-N1; текущая редакция описывает
код точно.

- **Line-line пары — точное решение** robust-предикатами: proper crossing,
  T-touch, endpoint touch, collinear overlap (в т.ч. за общим узлом —
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

Смежные (с общим узлом) кривые пары вне §12-инварианта: их самопересечение
вдали от общего узла — предмет geometric verification M2+/§20
(задокументировано в STATUS_M1 known limitations).

## Последствия

- Полнота отклонения инвалидных графов в M1 — структурно-комбинаторная +
  точная линейная геометрия; никакая «неопределённая» пара не считается
  доказанно безопасной.
- Каждый вариант ошибки покрыт негативным тестом
  (`tests/validate_rejects.rs`), включая Euler/torus и near-adjacent
  случаи.
