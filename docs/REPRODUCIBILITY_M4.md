# REPRODUCIBILITY_M4 — воспроизведение артефактов M4

Дополняет `docs/REPRODUCIBILITY_M3.md` и `docs/REPRODUCIBILITY_M3_5.md`, не
заменяет их: корпус, scorecard и burn policy воспроизводятся теми же
командами. Здесь — только то, что добавил M4.

## Окружение

То же, что в `REPRODUCIBILITY_M3.md`: `rust-toolchain.toml` пиннит 1.96.0,
`Cargo.lock` закоммичен, короткий путь checkout-а (F-0004).

## Команды

```bash
# Корридорная калибровка §13.1: полный scope — 41 сцена (sealed audit
# пропущен), 14 ячеек, 497 arms. На машине автора около 3 минут в release.
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
| 497 arms корридора | `arms` | пересборка и побайтовое сравнение JSON |
| покрытие/резкость по 7 осям | `buckets` | условная калибровка §13.1 |
| held-out растеризатор | `held_out` | tiny-skia, которого split-политика не пускает в development |
| 1233 §1.6-пробы | `probes` | каждая — отдельная строка с исходом |
| 1132 arms oracle | `ceiling_arms` | PF10 и PF11 как отдельные плечи |
| 6 факториалов × 3 эффекта | `factorial` | каждый — commensurable contrast либо типизированный отказ |
| гейт-таблицы §28 M4 | вывод команд | ненулевой exit при провале любой клаузы |

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
