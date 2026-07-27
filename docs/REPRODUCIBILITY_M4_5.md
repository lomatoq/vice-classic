# REPRODUCIBILITY_M4_5 — воспроизведение артефакта M4.5

Дополняет `docs/REPRODUCIBILITY_M4.md`, `M3_5.md` и `M3.md`, не заменяет их:
корпус, scorecard, burn policy, корридорная калибровка и oracle
воспроизводятся теми же командами. Здесь — только то, что добавил M4.5.

## Окружение

То же, что в `REPRODUCIBILITY_M3.md`: `rust-toolchain.toml` пиннит 1.96.0,
`Cargo.lock` закоммичен, короткий путь checkout-а (F-0004).

## Команды

```bash
# Recall-харнесс §11.3 / §28 M4.5: полный scope. На машине автора около
# 2.5 минут в release. Exit 0 — все три клаузы MET, 1 — хотя бы одна не
# выполнена, 2 — типизированный отказ.
cargo run --release --bin gt-corpus -- topology \
  --out docs/gt/TOPOLOGY_M4_5.json --scope full

# Сверить ЗАКОММИЧЕННЫЙ артефакт: пересобирает в его собственном scope и
# сравнивает ВСЁ.
cargo run --release --bin gt-corpus -- topology-check \
  --report docs/gt/TOPOLOGY_M4_5.json
```

Замороженные константы M4 по-прежнему измеряются отдельным шагом; с C154 он
живёт в ИНТЕГРАЦИОННОМ тесте, то есть в отдельном крейте, который физически не
может назвать широкую популяцию корпуса (условие D1):

```bash
cargo test --release -p vice-bench --test frozen_calibration -- --ignored --nocapture
```

## Что при сверке может законно отличаться

**Платформа.** Метрики артефакта — float-ы через libm, поэтому он **Tier A**
по §5.5 (F-0020, ADR-0008 §8). Артефакт НЕСЁТ свою платформу, и
`topology-check` на другой платформе **отказывает** с exit 2, называя обе.
Кросс-платформенно сравнима только платформенно-НЕЗАВИСИМАЯ проекция и только
по явному флагу `--structural`.

Проекция M4.5 отличается от корридорной и oracle одним и это НАМЕРЕННО:
она **сохраняет `gt_four`/`gt_eight` и все recall-булевы**. Число компонент —
целое, и чужой libm не может его сдвинуть, не сдвинув топологию; то есть
дрейф, достаточный чтобы изменить ОТВЕТ, структурный режим увидит. Отброшено
всё, что является функцией float-а: `fixture_set_hash` (sha256 по scene
digest-ам), доли, стоимости, персистентности. `config_hash` сохраняется — у
него нет ни одного float-входа, вычисленного libm.

Инструментом §5.5 Tier B проекция не является и за него не выдаётся —
**условие A7.1 остаётся открытым с владельцем M12**.

**На ОДНОЙ платформе не может отличаться ничего.** Проверено дважды:
`the_topology_report_is_deterministic` и побайтовый `diff` двух полных
прогонов.

## Что именно сверяется

| Величина | Где | Как проверяется |
|---|---|---|
| `config_hash` | шапка артефакта | компонент compatibility key (§27.6) |
| arms recall-прогона | `arms` | пересборка и побайтовое сравнение JSON |
| GT-топология каждого arm-а | `gt_four`, `gt_eight` | целые пары, выживают в структурной проекции |
| recall по трём генераторам | `recall_all`, `recall_events_only`, `recall_fixed_only` | клаузы 1 и 3 |
| вклад каждого поля §11.1 | `field_contributions` | matched и sole_source на arm |
| условная recall по трём осям | `buckets` | профиль, разрешение, сплит |
| ambiguity-пары | `ambiguity` | клауза 2, обе стороны каждой пары |
| pruning-запись | `pruning` | сколько удалил каждый tier и потерял ли ответ бюджет |
| §11.4 continuation | `continuation` | сколько шагов исполнено, наполовину исполнено и отказано |
| гейт-таблица §28 M4.5 | вывод команды | ненулевой exit при провале любой клаузы |

## Числа, которые цитируют документы M4.5 (СВЕРЯЮТСЯ ТЕСТОМ)

Тот же механизм, что закрыл блокер 3 REVIEW_M4: величина объявлена ОДИН раз,
вместе с путём в артефакт, и `crates/vice-bench/tests/doc_claims.rs` резолвит
каждый путь и сравнивает. С C153 разбор понимает markdown-экранирование `\|`
и несёт контроль на себя: число строк с ключом обязано совпадать с числом
разобранных, и каждая клаузная строка обязана дать ненулевое число
проверенных токенов (условие D2).

Float-ы округлены до четырёх знаков и сверяются в этой же точности.

| Величина | Путь | Значение |
|---|---|---|
| сцен в прогоне | `topology:scenes` | 41 |
| arms измерено | `topology:arms_measured` | 132 |
| arms отказано | `topology:arms_refused` | 52 |
| из них renders, помеченных корпусом IDENTIFIABLE | `topology:identifiable_arms_refused_before_topology` | 44 |
| sealed-audit групп ПРОПУЩЕНО | `topology:sealed_audit_groups_skipped` | 22 |
| ambiguity-пар пропущено по sealed audit | `topology:ambiguity_pairs_in_sealed_audit_skipped` | 0 |
| arms с НЕПРОЗРАЧНЫМ exterior-ом, исключены по имени | `topology:opaque_exterior_arms_excluded` | 38 |
| recall-популяция (identifiable + supported) | `topology:identifiable_supported_arms` | 100 |
| независимых source groups в ней | `topology:recall_source_groups` | 18 |
| SHAPE FAMILIES в ней (единица §27.1/§27.4) | `topology:recall_shape_families` | 8 |
| GT-топология в конверте | `topology:recall_all.hits` | 100 |
| из скольких | `topology:recall_all.arms` | 100 |
| то же без единой фиксированной пробы | `topology:recall_events_only.hits` | 100 |
| то же ТОЛЬКО по фиксированным пробам | `topology:recall_fixed_only.hits` | 100 |
| НОКАУТ: то же из ПОСТОРОННЕГО поля | `topology:recall_unrelated_field.hits` | 76 |
| из скольких | `topology:recall_unrelated_field.arms` | 100 |
| нокаут на нетривиальных GT | `topology:recall_unrelated_field_non_trivial.hits` | 7 |
| из скольких | `topology:recall_unrelated_field_non_trivial.arms` | 31 |
| arms, чья GT-топология НЕ тривиальна | `topology:non_trivial_gt_arms` | 31 |
| на них recall | `topology:recall_non_trivial.hits` | 31 |
| ПОЛОЖИТЕЛЬНЫЙ контроль нокаута: попадания на тривиальных | `topology:recall_unrelated_field_trivial.hits` | 69 |
| из скольких | `topology:recall_unrelated_field_trivial.arms` | 69 |
| arms, нетривиальные под ОБЕИМИ конвенциями | `topology:non_trivial_gt_arms_both_conventions` | 24 |
| их shape-семейств | `topology:non_trivial_both_shape_families_count` | 2 |
| на них recall настоящего конверта | `topology:recall_non_trivial_both_conventions.hits` | 24 |
| из скольких | `topology:recall_non_trivial_both_conventions.arms` | 24 |
| на них НОКАУТ | `topology:recall_unrelated_field_non_trivial_both.hits` | 0 |
| из скольких | `topology:recall_unrelated_field_non_trivial_both.arms` | 24 |
| arms, несущие ОБА плеча связности | `topology:arms_with_both_connectivity_arms` | 100 |
| arms, где одно плечо не дало ничего | `topology:arms_missing_a_connectivity_arm` | 0 |
| ambiguity-пар в корпусе всего | `topology:ambiguity_pairs` | 3 |
| из них ТОПОЛОГИЧЕСКИХ | `topology:topology_pairs` | 2 |
| из них НЕСУЩИХ строку (оба чтения, без извинения) | `topology:topology_pairs_carrying_the_row` | 1 |
| расхождение рендеров пары hole-or-not, кодов | `topology:ambiguity.0.collapse_max_code_diff` | 0 |
| то же для bridge-or-gap, кодов | `topology:ambiguity.1.collapse_max_code_diff` | 1 |
| saddle-альтернатив порождено | `topology:saddle_alternatives_total` | 596 |
| наибольшая партия равнозначных пикселей | `topology:largest_batch_pixels` | 14821 |
| уровней с ничьёй | `topology:tie_batches_max` | 377 |
| raw coverage дал совпадение на | `topology:field_contributions.0.matched` | 100 |
| и был ЕДИНСТВЕННЫМ источником на | `topology:field_contributions.0.sole_source` | 5 |
| TV-Huber дал совпадение на | `topology:field_contributions.1.matched` | 56 |
| detail-preserving дал совпадение на | `topology:field_contributions.2.matched` | 90 |
| denoised дал совпадение на | `topology:field_contributions.3.matched` | 64 |
| bounded deconvolution дал совпадение на | `topology:field_contributions.4.matched` | 8 |
| arms, где бюджет что-то удалил | `topology:pruning.arms_with_budget_pruning` | 11 |
| кандидатов удалено бюджетом | `topology:pruning.budget_removed` | 308 |
| кандидатов удалено доминированием | `topology:pruning.dominated_removed` | 2 |
| arms, где бюджет удалил кандидата с GT-классом | `topology:pruning.arms_where_budget_removed_a_gt_class_candidate` | 9 |
| arms, где бюджет удалил ПОСЛЕДНЕГО такого | `topology:pruning.arms_where_budget_pruning_lost_the_last_gt_candidate` | 0 |
| планов §11.4 построено | `topology:continuation.plans` | 304 |
| шагов исполнено НАПОЛОВИНУ | `topology:continuation.partially_executed_steps` | 608 |
| шагов ОТКАЗАНО типизированно | `topology:continuation.refused_steps` | 1520 |
| recall на held-out движке tiny-skia | `topology:buckets.3.recall.hits` | 10 |
| из скольких | `topology:buckets.3.recall.arms` | 10 |

## Клаузные строки: ключ на ПОЗИЦИЮ (условие 11)

Проверка на ПРИНАДЛЕЖНОСТЬ словарю не является проверкой на равенство. Среди
объявленных величин много мелких целых, поэтому ложное измерение почти всегда
собирается из чужих чисел: подмена `31 → 56` в строке, докладывающей клаузу
спеки 1, оставляла все четыре теста зелёными (M45-N11 K9, RT45-A4).

Ниже каждая ПОЗИЦИЯ числа в клаузной строке привязана к ключу артефакта.
`doc_claims.rs::the_delta_clause_rows_equal_their_declared_keys_position_by_position`
извлекает числа строки по порядку и требует РАВЕНСТВА с разрешённым значением
ключа; число без привязки роняет тест так же, как привязка без числа.

| строка | позиция | ключ |
|---|---|---|
| row T1d | 1 | `topology:identifiable_supported_arms` |
| row T1d | 2 | `topology:arms_measured` |
| row T1d | 3 | `topology:recall_shape_families` |
| row T1d | 4 | `topology:recall_source_groups` |
| row T1d | 5 | `topology:opaque_exterior_arms_excluded` |
| row T1d | 6 | `topology:identifiable_arms_refused_before_topology` |
| row T1d | 7 | `topology:recall_all.hits` |
| row T1d | 8 | `topology:recall_all.arms` |
| row T1d | 9 | `topology:recall_non_trivial.hits` |
| row T1d | 10 | `topology:recall_non_trivial.arms` |
| row T1d | 11 | `topology:arms_with_both_connectivity_arms` |
| row T1d | 12 | `topology:arms_missing_a_connectivity_arm` |
| row T1d | 13 | `topology:pruning.arms_where_budget_removed_a_gt_class_candidate` |
| row T1d | 14 | `topology:pruning.arms_where_budget_pruning_lost_the_last_gt_candidate` |
| row T1d | 15 | `topology:recall_unrelated_field.hits` |
| row T1d | 16 | `topology:recall_unrelated_field.arms` |
| row T1d | 17 | `topology:recall_unrelated_field_non_trivial.hits` |
| row T1d | 18 | `topology:recall_unrelated_field_non_trivial.arms` |
| row T1d | 19 | `topology:sealed_audit_groups_skipped` |
| row T1d | 20 | `topology:arms_refused` |
| row T1d | 21 | `topology:non_trivial_gt_arms_both_conventions` |
| row T1d | 22 | `topology:non_trivial_both_shape_families_count` |
| row T1d | 23 | `topology:recall_non_trivial_both_conventions.hits` |
| row T1d | 24 | `topology:recall_non_trivial_both_conventions.arms` |
| row T1d | 25 | `topology:recall_unrelated_field_non_trivial_both.hits` |
| row T1d | 26 | `topology:recall_unrelated_field_non_trivial_both.arms` |
| row T1d | 27 | `topology:recall_unrelated_field_trivial.hits` |
| row T1d | 28 | `topology:recall_unrelated_field_trivial.arms` |
| row T2d | 1 | `topology:topology_pairs_carrying_the_row` |
| row T2d | 2 | `topology:topology_pairs` |
| row T2d | 3 | `topology:ambiguity_pairs` |
| row T2d | 4 | `topology:ambiguity.0.collapse_max_code_diff` |
| row T2d | 5 | `topology:ambiguity.1.collapse_max_code_diff` |
| row T2d | 6 | `topology:ambiguity_pairs_in_sealed_audit_skipped` |
| row T3d | 1 | `topology:recall_events_only.hits` |
| row T3d | 2 | `topology:recall_events_only.arms` |
| row T3d | 3 | `topology:recall_all.hits` |
| row T3d | 4 | `topology:recall_all.arms` |
| row T3d | 5 | `topology:recall_fixed_only.hits` |
| row T3d | 6 | `topology:recall_fixed_only.arms` |
| row T3d | 7 | `topology:saddle_alternatives_total` |
| row T3d | 8 | `topology:tie_batches_max` |
| row T3d | 9 | `topology:largest_batch_pixels` |

## CI

| job | раннер | что добавил M4.5 |
|---|---|---|
| `checks` | ubuntu | шаг замороженных измерений переехал на `--test frozen_calibration` (D1) |
| `gt-corpus` | ubuntu | прогоняет `topology --scope full` целиком (гейт-таблица исполняется) и `topology-check --structural` на закоммиченном артефакте |
| `tier-a-digests` | windows | `topology-check` БЕЗ флага, то есть **сами цифры** |
| `clean-checkout-smoke` | ubuntu | без изменений |
