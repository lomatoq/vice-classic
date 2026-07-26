# ADR-0010 — Partition renderer, embedding-сертификация (M1-N5) и типизированные пороги

Дата: 2026-07-26. Статус: accepted (M2).

## Контекст

Spec §16.1: renderer обязан растеризовать shared faces включая exterior,
использовать premultiplied compositing / area fractions, проверять
per-pixel partition sum ≈ 1 и отсутствие скрытых gaps/overlaps, суммировать
area fractions triple junction напрямую и быть детерминированным. REVIEW_M1
M1-N5 требует, чтобы геометрическая проверка вложения/ориентации loops
стала HARD-проверкой в M2.

## Ключевое наблюдение: «сумма ≡ 1» — алгебраическое тождество

Если считать coverage всех faces одним signed-winding проходом, то
Σ_faces winding ≡ 0 ТОЖДЕСТВЕННО (каждая boundary входит дважды с
противоположными ориентациями), и «проверка суммы» не могла бы провалиться
даже на бессмысленной сцене. Поэтому проверки построены так:

1. **Coverage каждой face считается НЕЗАВИСИМО из её собственных loops**:
   `coverage_f(pixel) = ∫∫_pixel w_f dA`, exterior = `1 + ∫∫ w_ext`
   (окно плюс его негативные loops). Формулы см. модуль `coverage`
   (точные трапеции по cell pieces, детерминированная right-to-left
   свёртка).
2. **Range-check:** каждая независимая coverage обязана лежать в
   `[0−tol, 1+tol]`. Именно он ловит нарушения ВЛОЖЕНИЯ: сцена B2
   (остров внутри острова, скоммутированный на exterior) даёт exterior
   coverage −1 в области вложенного острова. Ориентации loops при этом
   безупречны — range-check единственный судья таких сцен.
3. **Sum-check:** Σ_f coverage_f = 1 ± tol на каждом пикселе. После (1)–(2)
   он ограничивает f64-накопление и подтверждает согласованность shared
   tessellation (полилиния границы строится ОДИН раз и обходится обеими
   faces — cracks непредставимы структурно, sum-check проверяет это
   числом на каждом рендере).
4. **Ориентация loops (embedding-модуль, M1-N5):** для каждого loop
   считается shoelace polyline-площадь и СЕРТИФИЦИРОВАННАЯ
   неопределённость = tessellation area budget (ADR-0008) + f64-оценка
   округления shoelace (8·eps·n·max|coord|²). Знак признаётся только если
   |area| превышает эту границу; иначе typed reject
   `UncertifiableLoopOrientation` — честный отказ вместо угадывания.
   Правила: bounded face — ровно один certified-положительный loop,
   остальные отрицательные; exterior — только отрицательные. Ловит
   B4-класс (перевёрнутое кольцо) ДО пиксельной работы.

Пара (2)+(4) — это и есть закрытие M1-N5: ориентация — прямой
сертифицированный тест, вложение — полный per-pixel тест на точной
coverage фиксированной tessellation. Проверки выполняются на КАЖДОМ
рендере и на seal-пути; renderer отказывает typed-ошибкой, ничего не
чинит молча. `validate_scene` (M1-контракт) сознательно не расширялся:
кривые машины там нет, а замороженный golden-артефакт M1 не может быть
затронут feature-коммитом (§32 п.9); STATUS_M2 фиксирует это разделение.

## Compositing

`composite(pixel) = Σ_f coverage_f · premul(paint_f)` — прямая сумма в
premultiplied linear RGBA (типы vice-ir). На triple junction fractions
нескольких faces складываются напрямую; painter-style pairwise
alpha-композитинг в crate-е отсутствует, а тест
`triple_junction_fractions_sum_directly` проверяет и аналитические
fractions (0.375/0.375/0.25), и то, что painter-математика дала бы ДРУГОЙ
результат (иначе тест ничего бы не доказывал).

## Типизированные пороги (spec §5.4)

`PartitionTolerances { sum_abs_tol = 1e-9, range_tol = 1e-9 }` (единицы —
pixel area). Обоснование: каждое значение coverage — сумма точных
трапецеидальных слагаемых, каждое из нескольких correctly-rounded
f64-операций на величинах порядка canvas; при canvas ≤ 2^12 и тысячах
edge pieces на строку суммарная абсолютная ошибка < ~1e-11 (два порядка
запаса). Реальные геометрические нарушения дают отклонения порядка
пиксельной площади (~10 порядков больше порога) — разделение классов
надёжное. Пороги — константы кода, не env и не конфиг.

## Resource limit

`MAX_COVERAGE_ELEMENTS = 2^27` элементов coverage (faces × W × H);
превышение — typed `CanvasTooLarge` ДО аллокаций (линия M0 «explicit
resource limits»).

## Digest

`render_digest_sha256`: sha256 поверх schema-тега `vice-classic/render/v1`,
размеров (LE) и row-major premultiplied RGBA как little-endian f64 bit
patterns. Tier A determinism; сцены с дугами используют libm sin/cos и в
замороженные digest-артефакты не входят (ADR-0008 §8).

## Отказ от рендера не-Box фильтров

M2 renderer считает exact box-filter area coverage. Formation с
Triangle/Gaussian фильтром сегодня НЕ рендерится «как Box молча», а даёт
typed `UnsupportedPixelFilter`: рендер неверной formation хуже честного
отказа. Это scope-guard, не placeholder API — расширение придёт со своим
producer-ом (M4 formation) и своим reviewed-изменением.
