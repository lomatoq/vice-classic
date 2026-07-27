# REPRODUCIBILITY_M4 — воспроизведение артефактов M4

Дополняет `docs/REPRODUCIBILITY_M3.md` и `docs/REPRODUCIBILITY_M3_5.md`, не
заменяет их: корпус, scorecard и burn policy воспроизводятся теми же
командами. Здесь — только то, что добавил M4.

## Окружение

То же, что в `REPRODUCIBILITY_M3.md`: `rust-toolchain.toml` пиннит 1.96.0,
`Cargo.lock` закоммичен, короткий путь checkout-а (F-0004).

## Команды

```bash
# Корридорная калибровка §13.1: полный scope. Числа прогона — в таблице
# «Числа, которые цитируют документы M4» ниже; она сверяется тестом.
# На машине автора около 3 минут в release.
cargo run --release --bin gt-corpus -- corridor \
  --out docs/gt/CORRIDOR_M4.json --scope full

# Сверить ЗАКОММИЧЕННЫЙ артефакт: пересобирает в его собственном scope и
# сравнивает всё.
cargo run --release --bin gt-corpus -- corridor-check \
  --report docs/gt/CORRIDOR_M4.json

# Факториальный oracle с ВТОРЫМ измеренным плечом (PF10):
cargo run --release --bin gt-corpus -- oracle \
  --out docs/gt/ORACLE_M4.json --scope full
cargo run --release --bin gt-corpus -- oracle-check \
  --report docs/gt/ORACLE_M4.json

# Исполняемый путь милестоуна на закоммиченной фикстуре:
cargo run --release --bin vicec -- evidence \
  tests/fixtures/smoke/circle_64.png --out runs/m4-cli
```

```bash
# Корпусные измерения, которыми заморожены константы M4 (таблица ядер,
# clean-bucket шум, §1.6-порог толщины, residual-tolerance, взвешивание
# интерьера). Они помечены #[ignore] и не входят в путь по умолчанию:
# сотни рендеров и анализов — это секунды в release и МИНУТЫ в debug, а
# проверка, которой никто не дожидается, — это проверка, которую никто не
# запускает. CI гоняет их отдельным шагом на каждом push.
cargo test --release -p vice-bench --lib corridor::tests -- --ignored --nocapture
```

Exit-коды `corridor` и `oracle`: `0` — все гейт-строки выполнены; `1` — хотя
бы одна не выполнена; `2` — типизированный отказ. Exit-коды `vicec evidence`
— это §1.4 outcomes: `0` supported, `3` ambiguous, `4` unsupported, `2`
failed (сбой декодирования, не вердикт о модели).

## Что при сверке может законно отличаться

**Платформа.** Метрики обоих артефактов — float-ы через libm, поэтому оба —
**Tier A** по §5.5 (F-0020, ADR-0008 §8). Артефакт НЕСЁТ свою платформу, и
`*-check` на другой платформе **отказывает** с exit 2, называя обе.
Кросс-платформенно сравнима только платформенно-НЕЗАВИСИМАЯ проекция и только
по явному флагу:

```bash
cargo run --release --bin gt-corpus -- corridor-check \
  --report docs/gt/CORRIDOR_M4.json --structural
```

Проекция сохраняет состав, идентичности arms, outcome-имена, статусы
inverse crime, типизированные отказы и `config_hash`; она НЕ сохраняет ни
одной измеренной величины и ни одного хеша, вычисленного по scene digest-ам
(F-0022). Инструментом §5.5 Tier B она не является и за него не выдаётся —
условие A7.1 остаётся открытым с владельцем M12.

**На ОДНОЙ платформе не может отличаться ничего.** Расхождение при
совпадающей платформе — находка: оба харнесса детерминированы, что проверяется
тестами `the_corridor_report_is_deterministic` и `the_report_is_deterministic`.

## Что именно сверяется

| Величина | Где | Как проверяется |
|---|---|---|
| `config_hash` | шапка обоих артефактов | компонент compatibility key (§27.6) |
| arms корридора | `arms` | пересборка и побайтовое сравнение JSON |
| покрытие/резкость по 7 осям | `buckets` | условная калибровка §13.1 |
| held-out растеризатор | `held_out` | tiny-skia, которого split-политика не пускает ни в development, ни в калибровку замороженных коэффициентов |
| §1.6-пробы | `probes` | каждая — отдельная строка с исходом |
| arms oracle | `ceiling_arms` | PF10 и PF11 как отдельные плечи |
| 6 факториалов × 3 эффекта | `factorial` | каждый — commensurable contrast либо типизированный отказ |
| гейт-таблицы §28 M4 | вывод команд | ненулевой exit при провале любой клаузы |

## Числа, которые цитируют документы M4 (СВЕРЯЮТСЯ ТЕСТОМ)

Условие B3 прошлого гейта: «утверждения, которые STATUS и REPRODUCIBILITY
подают как измеренные факты, должны быть измерены». REVIEW_M4 нашёл третий
экземпляр класса — пять чисел, разошедшихся с артефактом. Поэтому величины
объявлены здесь ОДИН раз, каждая со своим путём в артефакте, и
`crates/vice-bench/tests/doc_claims.rs` резолвит каждый путь и сравнивает.
Разошедшееся число роняет тест, как уже роняют его модуль свыше 800 строк,
необъявленное чтение env и крейт без `forbid(unsafe_code)`.

Float-ы округлены до четырёх знаков и сверяются в этой же точности.

| Величина | Путь | Значение |
|---|---|---|
| сцен в корридорном прогоне | `corridor:scenes` | 41 |
| arms | `corridor:arms_measured` | 500 |
| отказов | `corridor:arms_refused` | 2 |
| sealed-audit групп ПРОПУЩЕНО | `corridor:sealed_audit_groups_skipped` | 22 |
| held-out сэмплов | `corridor:held_out.samples` | 14330 |
| held-out coverage@50 | `corridor:held_out.coverage@50` | 0.8532 |
| held-out coverage@90 | `corridor:held_out.coverage@90` | 0.9761 |
| held-out coverage@95 | `corridor:held_out.coverage@95` | 0.9964 |
| held-out coverage@99 | `corridor:held_out.coverage@99` | 0.9984 |
| held-out median halfwidth, px | `corridor:held_out.median_halfwidth_px` | 0.3111 |
| held-out p95 halfwidth, px | `corridor:held_out.p95_halfwidth_px` | 0.3959 |
| held-out bias вдоль нормали, px | `corridor:held_out.bias_px` | 0.0018 |
| контроль сдвига (held-out) | `corridor:held_out.coverage_under_displacement` | 0 |
| §1.6-проб всего | `corridor:semi_transparent.probes` | 1242 |
| из них разрешимых | `corridor:semi_transparent.probes_observable` | 849 |
| отвергнуто среди разрешимых | `corridor:semi_transparent.rejected_where_observable` | 841 |
| доставлено как two-colour среди разрешимых | `corridor:semi_transparent.delivered_as_two_colour_where_observable` | 0 |
| доставлено как two-colour ВСЕГО | `corridor:semi_transparent.delivered_as_two_colour` | 26 |
| чистых arms отвергнуто по §1.6 | `corridor:semi_transparent.clean_arms_rejected` | 0 |
| supported arms | `corridor:formation_recovery.supported_arms` | 342 |
| из них с неверным exterior | `corridor:formation_recovery.exterior_wrong` | 0 |
| arms oracle | `oracle:arms_measured` | 1162 |
| отказов oracle | `oracle:arms_refused` | 350 |
| общих (scene, cell) пар факториала | `oracle:factorial_common_fixtures` | 406 |
| отброшено факториалом | `oracle:factorial_dropped_fixtures` | 350 |

## CI

| job | раннер | что сверяет |
|---|---|---|
| `checks` | ubuntu | fmt, clippy, тесты в обоих профилях И отдельный шаг «M4 calibration measurements (release, --ignored)» |
| `gt-corpus` | ubuntu | прогоняет оба харнесса целиком (гейт-таблицы исполняются) и `*-check --structural` на обоих закоммиченных артефактах — состав, идентичности, исходы |
| `tier-a-digests` | windows | `verify`, `oracle-check` и `corridor-check` БЕЗ флага, то есть **сами цифры** |
| `clean-checkout-smoke` | ubuntu | добавлен шаг `vicec evidence` на закоммиченной фикстуре |

## Артефакт M3.5

`docs/gt/ORACLE_M3_5.json` УДАЛЁН, и это следствие, а не уборка: M4 изменил
и то, чем является arm (второй источник формации), и то, что говорит его
ключ (`candidate_budget` стал `Exhaustive`, где M3.5 имел `NotApplicable`).
Артефакт M3.5 не воспроизводится кодом в дереве, а закоммиченный артефакт,
который ничто не может проверить, — ровно то, о чём был долг A7.2. Числа
M3.5 остаются в подписанных `docs/STATUS_M3_5.md` и `docs/REVIEW_M3_5.md`,
которых M4 не трогает.
