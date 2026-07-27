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
| sealed-audit групп ПРОПУЩЕНО | `topology:sealed_audit_groups_skipped` | 22 |
| ambiguity-пар пропущено по sealed audit | `topology:ambiguity_pairs_in_sealed_audit_skipped` | 0 |
| arms с НЕПРОЗРАЧНЫМ exterior-ом, исключены по имени | `topology:opaque_exterior_arms_excluded` | 38 |
| recall-популяция (identifiable + supported) | `topology:identifiable_supported_arms` | 100 |
| независимых source groups в ней | `topology:recall_source_groups` | 18 |
| GT-топология в конверте | `topology:recall_all.hits` | 100 |
| из скольких | `topology:recall_all.arms` | 100 |
| то же без единой фиксированной пробы | `topology:recall_events_only.hits` | 100 |
| то же ТОЛЬКО по фиксированным пробам | `topology:recall_fixed_only.hits` | 100 |
| arms, чья GT-топология НЕ тривиальна | `topology:non_trivial_gt_arms` | 31 |
| на них recall | `topology:recall_non_trivial.hits` | 31 |
| ambiguity-пар в корпусе всего | `topology:ambiguity_pairs` | 3 |
| из них ТОПОЛОГИЧЕСКИХ | `topology:topology_pairs` | 2 |
| расхождение рендеров пары hole-or-not на collapse-ячейке, кодов | `topology:ambiguity.0.collapse_max_code_diff` | 0 |
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
| arms, где бюджет мог потерять ответ | `topology:pruning.arms_where_budget_pruning_could_have_lost_the_answer` | 0 |
| планов §11.4 построено | `topology:continuation.plans` | 304 |
| шагов ИСПОЛНЕНО | `topology:continuation.executed_steps` | 304 |
| шагов исполнено НАПОЛОВИНУ | `topology:continuation.partially_executed_steps` | 304 |
| шагов ОТКАЗАНО типизированно | `topology:continuation.refused_steps` | 1520 |
| recall на held-out движке tiny-skia | `topology:buckets.3.recall.hits` | 10 |
| из скольких | `topology:buckets.3.recall.arms` | 10 |

## CI

| job | раннер | что добавил M4.5 |
|---|---|---|
| `checks` | ubuntu | шаг замороженных измерений переехал на `--test frozen_calibration` (D1) |
| `gt-corpus` | ubuntu | прогоняет `topology --scope full` целиком (гейт-таблица исполняется) и `topology-check --structural` на закоммиченном артефакте |
| `tier-a-digests` | windows | `topology-check` БЕЗ флага, то есть **сами цифры** |
| `clean-checkout-smoke` | ubuntu | без изменений |
