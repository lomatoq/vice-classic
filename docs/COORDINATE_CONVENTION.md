# COORDINATE_CONVENTION — неподвижные конвенции vice-classic

Дата фиксации: 2026-07-26 (M1). Источник: spec v1.3 §5 (§5.1–§5.5).
Эти решения принимаются ДО algorithm tuning; изменение любого пункта —
отдельный reviewed commit, потому что оно инвалидирует канонические байты,
digests и записанные gate-артефакты.

Для каждого пункта указано, ГДЕ он enforced кодом в M1 и что остаётся
documented-only до своего milestone. Ничего из documented-only не считается
«проверенным» — это честная граница M1.

## 1. Координатная рамка (§5.1)

```text
Frame: x вправо, y ВНИЗ.
Pixel (ix, iy) = закрытый бокс [ix, ix+1] × [iy, iy+1].
Pixel center   = (ix + 0.5, iy + 0.5).
Canvas W×H     = [0, W] × [0, H].
Internal geometry = f64 (до seal).
```

- **Единственный transform-модуль:** `crates/vice-geom/src/coords.rs`
  (`pixel_center`, `pixel_box`, `canvas_box`). Случайные `±0.5` в любом
  другом месте pipeline запрещены; каждый будущий crate обязан вызывать эти
  функции.
- Enforced: unit-тесты `coords.rs` (центр, закрытые границы бокса, общая
  грань соседних пикселей).
- Следствие y-вниз: алгебраически положительный cross product выглядит на
  экране ПО часовой стрелке. Topology-код обязан рассуждать в алгебраической
  конвенции (`vice-geom::predicates`), «визуальный» язык в topology-коде
  запрещён.

## 2. Цвет и observation space (§5.2)

- Canonical scene colors: **linear straight RGBA** (`vice-ir::color`,
  `LinearRgb`/`LinearRgba`). В M1 flat-core paint — `Paint::OpaqueSolid`
  (linear RGB, [0,1]) и `Paint::TransparentExterior`.
- Observation/compositing space: **premultiplied linear RGBA**
  (`PremulRgba`, `premultiply`). RGB под `alpha = 0` НЕ является цветовым
  доказательством: `unpremultiply` возвращает `None` при нулевой альфе, тест
  фиксирует, что premultiply стирает RGB при `a = 0`.
- Формация наблюдения может смешивать coverage до или после transfer
  function — это hypothesis, а не константа:

```rust
enum BlendSpace { LinearLight, EncodedSrgb }
```

- Transfer function: IEC 61966-2-1 (sRGB), реализована в `vice-ir::color`
  (`srgb_encoded_to_linear` / `linear_to_srgb_encoded`), unit-тесты: точки
  излома, монотонность, точный u8-roundtrip (все 256 значений).
- Documented-only в M1: decode pipeline (`bytes → ICC assumption → straight
  RGBA → linear RGBA → premultiplied tensor`) появляется вместе с
  `vice-image` (M2+/M4); 8-bit quantization как часть formation likelihood —
  M4.

## 3. Digital topology (§5.3)

- Binary topology использует **complementary connectivity**: foreground и
  background НЕ могут иметь одинаковую связность. Тип
  `vice-ir::connectivity::ComplementaryConnectivity` хранит только
  foreground-arm; background вычисляется — одинаковая связность обеих сторон
  непредставима структурно. Обе дуги (fg4/bg8 и fg8/bg4) доступны как
  candidate arms.
- **Exterior — настоящий `FaceId`**: в `PlanarGraph { exterior: FaceId, .. }`
  exterior face — обычный элемент `faces`, с paint
  `TransparentExterior`; никаких отрицательных magic labels. Enforced
  валидацией graph (exterior id в диапазоне, paint-инвариант в обе стороны).
- Documented-only в M1: cubical cell complex, well-composedness,
  saddle-resolution hypotheses — milestone M4.5.

## 4. Robust predicates (§5.4)

- Combinatorial topology decisions не полагаются на `abs(cross) < 1e-9`:
  единственная точка входа — `vice-geom::predicates` (adaptive-precision
  Shewchuk predicates через внешний OSS-crate `robust`, лицензия проверена —
  THIRD_PARTY_NOTICES.md; ADR-0004).
- Typed результаты: `Orientation { CounterClockwise, Clockwise, Collinear }`,
  `CirclePosition { Inside, Outside, OnBoundary }` (+ `None` для
  вырожденного треугольника). Голый f64-знак наружу не отдаётся.
- Enforced: unit-тесты near-degenerate случаев + property-тесты против
  независимой exact-i128 реализации на диадической решётке.
- Documented-only в M1: certified curve–curve intersection (analytic
  predicates / certified subdivision) — M2+; `DecisionInterval` для
  score/likelihood появляется вместе с первым producer score-ов (M5+).
  Валидация M1 использует предикаты для точных line-line проверок и
  консервативные AABB-сертификаты для кривых (граница задокументирована в
  ADR-0005 и STATUS_M1).

## 5. Quantization и determinism (§5.5)

- Geometry остаётся f64 до seal; canonical parameters квантуются ОДИН раз
  (полный seal — M7).
- **Shared parameters хранятся один раз** уже в M1-типах: позиция общей
  вершины — только в `GraphVertex`; кривая общей границы — только в
  `Boundary.curve`; смежные faces ссылаются на них по id. Endpoint-ы chain-а
  берутся из graph vertices, НЕ дублируются в кривой; shared tangent
  smooth-узла хранится один раз в `JoinKind::SmoothG1`. Поэтому «квантовать
  shared parameter дважды по-разному» непредставимо структурно.
- **`-0`, NaN, Inf запрещены** в canonical scene: валидация отклоняет их до
  сериализации (typed reject, не panic; тесты).
- **Ordered maps/reductions**: canonical serialization использует только
  `Vec` в каноническом порядке и сортированные структуры; HashMap в
  canonical-пути запрещён. Canonical byte-форма инвариантна к порядку
  построения графа (canonical relabeling, ADR-0006; property-тест
  пермутаций).
- Determinism tiers (§5.5):
  - **Tier A (обязателен с M1):** same binary/platform → byte-identical
    canonical bytes и sha256-digest. Enforced: property roundtrip/digest
    тесты + golden digest тест.
  - **Tier B (M12):** cross-platform scene-equivalence — НЕ обещается в M1.

## 6. Что где лежит

| Конвенция | Код | Тесты |
|---|---|---|
| Frame / pixel / canvas transforms | `vice-geom/src/coords.rs` | unit |
| Vec2/Pt, cross-sign, `-0`-детект | `vice-geom/src/vec2.rs` | unit |
| Robust orientation/incircle/segment | `vice-geom/src/predicates.rs` | unit + property (exact i128 reference) |
| Linear/premul цвет, sRGB transfer, BlendSpace | `vice-ir/src/color.rs` | unit |
| Complementary connectivity | `vice-ir/src/connectivity.rs` | unit |
| Exterior как FaceId, shared parameters | `vice-ir/src/graph.rs` + `validate.rs` | unit + property |
| Canonical bytes / digest / -0/NaN/Inf reject | `vice-ir/src/canonical.rs` + `validate.rs` | property + golden |
