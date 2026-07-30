# ADR-0006 — Canonical serialization и digests (M1 seal skeleton)

Дата: 2026-07-26. Статус: accepted (M1).

## Контекст

Spec v1.3 §5.5 и §28 M1: canonical shared-parameter serialization, digests,
gate «property roundtrips, same-platform byte determinism, invalid graphs
rejected». Требования: ordered maps, запрет `-0`/NaN/Inf, shared parameters
сериализуются один раз, побайтовый roundtrip.

## Решения

1. **Формат: canonical JSON** с фиксированным порядком полей (порядок
   объявления структур; serde) и компактной записью без пробелов. Версия
   формата — тег `schema = "vice-classic/scene/v2"` в envelope; изменение
   формата = смена тега = reviewed change (golden-тест делает дрейф
   видимым и не даёт «починить» digest молча).
2. **Canonical relabeling перед сериализацией.** Байты обязаны быть чистой
   функцией СОДЕРЖАНИЯ, а не порядка построения. `canonicalize_graph`
   пере-нумеровывает: вершины — лексикографически по позиции; boundaries —
   по (start', end', контент кривой); faces — exterior первым (index 0),
   далее по отсортированным rotated loop-keys; half-edges — boundary-major
   (forward = 2b, reverse = 2b+1); loops — минимальный half-edge id цикла.
   Тотальность порядка обеспечена валидацией (запрет дубликатов вершинных
   позиций/boundaries; loop-keys дизъюнктны). Идемпотентность и
   инвариантность к перестановкам construction order — property-тесты.
3. **Ordered structures only:** на canonical-пути только `Vec` в
   каноническом порядке; HashMap запрещён.
4. **Shared parameters один раз — структурно:** позиции вершин только в
   vertex table; endpoints chain-ов не хранятся в кривых; shared tangent —
   один раз в узле. «Квантовать shared parameter дважды» непредставимо.
5. **Числа.** `-0.0`, NaN, ±Inf отклоняются валидацией ДО сериализации
   (typed error). Печать — shortest roundtrip decimal (serde_json/ryu).
   **Критично: feature `float_roundtrip` у serde_json обязательна** — без
   неё ПАРСИНГ f64 имеет ошибку до 2 ulp и parse→re-serialize не
   побайтовый (реальный сбой, пойманный property-тестом: FAILURE_LEDGER
   F-0005).
6. **Digest** = sha256(canonical bytes), lower-case hex. Tier A
   determinism (§5.5): same binary/platform ⇒ byte-identical bytes/digest.
   Cross-platform Tier B НЕ заявляется до M12.
7. **Parsing строгий:** `deny_unknown_fields` везде (serde также отклоняет
   дубликаты полей структур), затем schema-tag check, затем ПОЛНАЯ
   валидация сцены. Мусор не может пройти в IR через сериализацию.
8. **Golden-артефакт:** `crates/vice-ir/tests/golden/scene_v2.json`
   (сцена со всеми видами сегментов, smooth joins, transparent hole,
   loop-boundary) + замороженный sha256 в `golden_digest.rs`.

## Проверки (tests/canonical_props.rs, golden_digest.rs, unit)

- generated scenes valid; roundtrip побайтово; digest стабилен;
- повторная сериализация детерминирована;
- перестановка construction order всех четырёх entity-векторов с
  ремапом ссылок → те же байты/digest;
- NaN/Inf/`-0`, сломанные twin/crack/paint, усечённые байты, чужой
  schema-тег, unknown/duplicate поля, `1e999`-литерал → typed reject;
- golden bytes/digest/parse-roundtrip.

## Последствия

- Любое изменение IR-типов меняет canonical формат → golden-тест упадёт →
  осознанный bump `SCENE_SCHEMA` в том же reviewed change.

## Эволюция v2

M7 добавил к каждой `Boundary` поле `closure_join`. Оно обязательно для
self-loop boundary и отсутствует для открытой boundary. Так угол или G1-параметр
в точке замыкания больше не теряется между refit, квантизацией, сериализацией и
независимым verifier. Это изменило канонические байты, поэтому schema и golden
артефакт подняты с v1 до v2 в одном reviewed change.
- Квантование параметров (полный seal) появится в M7 поверх этого
  скелета: оно будет менять ЗНАЧЕНИЯ shared-объектов один раз перед той же
  сериализацией.
