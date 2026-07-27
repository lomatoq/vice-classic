# REDTEAM_M4_5 — независимая red team по milestone M4.5

Дата: 2026-07-27.
Объект: `vice-classic` @ `2d3e9c9` («C167 M4.5: the STATUS records the CI it actually measured, including the moment the instrument was unavailable»).
Основание: сверх обязательного cold review governor назначил отдельный adversarial pass. Мандат отличался от рецензентского: не оценить работу, а **сломать её утверждения** — найти состояние мира, в котором зелёные строки остаются зелёными, а свойство, которое они удостоверяют, не выполняется.

> Отчёт публикуется **дословно**, как он вернулся из холодного контекста. Governor его не редактировал и не сокращал. Подписанный артефакт: изменению не подлежит, только addendum.

---

## §0. Гигиена и проверка спеки

**Основной репозиторий `C:\Users\nirrt\Toolset\vice-classic` в начале работы:**

```text
$ git status --porcelain
(пусто)

$ git rev-parse HEAD
2d3e9c974de0d3d5fa43d3962b0078558d62ef61
```

**SHA-256 спеки, проверен первым действием:**

```text
$ sha256sum /c/Users/nirrt/Downloads/VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md
652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1  *VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md
```

Совпадает с обязательным значением. Спека — единственный source of truth ниже.

**Метод.** По основному дереву — только чтение. Все атаки, требующие правок, проведены в отдельном `git worktree` (`…/scratchpad/attack1`, detached HEAD `2d3e9c9`, `CARGO_TARGET_DIR` вне проекта), который в конце работы удалён (`git worktree remove --force` + `git worktree prune`). Донорские исходники (`v-ice`, `v-ize`, `v-ice part`) не открывались (D-3).

**Замечание о постороннем worktree.** `git worktree list` на основном репозитории не пуст и без моего: зарегистрирован второй worktree
`…/AppData/Local/Temp/claude/C--Users-nirrt-Toolset-Vice-classic/f0169b6d-…/scratchpad/wt` (создан 20:24, до моего 20:38), в нём `M crates/vice-bench/src/topology/mod.rs`, `?? crates/vice-bench/src/topology/probe.rs`, `?? crates/vice-bench/tests/reviewer_d1_attack.rs`. Он **не мой** — это изолированная копия параллельного рецензента, и это правильная гигиена с его стороны. Но запись о нём живёт в `.git/worktrees/` основного репозитория, поэтому фиксирую факт: рабочее дерево основного репо чистое, HEAD не двигался, а `git worktree list` — не пустой.

---

## §1. Удавшиеся атаки

### RT45-A1 — гейт-таблица §28 M4.5 слепа к удалению второго арма комплементарной связности — **MAJOR**

**Утверждение, которое ломается.** §28 M4.5, первый пункт объёма: «complementary connectivity». Модульная документация `vice-topology`: «Both arms are legitimate hypotheses and the envelope carries both». И — принципиально — обоснование самой метрики recall в `crates/vice-bench/src/topology/mod.rs`: «§5.3 gives TWO admissible conventions and this milestone treats both as hypotheses, so the truth is computed under both and a candidate matching EITHER counts». Послабление на стороне ИСТИНЫ (совпадение с любой из двух конвенций засчитывается) оправдано тем, что конверт несёт оба арма. Ничто не проверяет, что он их несёт.

**Процедура воспроизведения.**

```bash
git worktree add /tmp/attack1 HEAD --detach && cd /tmp/attack1
# единственная правка: генератор кандидатов обходит ОДИН арм вместо двух
#   crates/vice-topology/src/lib.rs, fn propose
# -            for conn in ComplementaryConnectivity::arms() {
# +            for conn in [ComplementaryConnectivity::arms()[0]] {
cargo build --release --bin gt-corpus
./target/release/gt-corpus topology --out /tmp/k1.json --scope full
cargo test --release --workspace
```

**Наблюдаемый результат.**

```text
topology: 41 scenes, 132 arms, 52 refused, 22 sealed-audit groups skipped,
          38 opaque-exterior arms excluded, config_hash 7b5e901b0db235cc…  ← ХЭШ НЕ ИЗМЕНИЛСЯ
  recall all 100/100  events-only 100/100  fixed-only 100/100
M4.5 gate table (all three clauses of the spec):
  [MET] GT-equivalent topology present in envelope: … 100/100 arms = 1.0000; … 31/31 = 1.0000 …
  [MET] ambiguous fixtures retain alternatives: … 1 of 2 do …
  [MET] no magic-threshold-only architecture: … Saddle alternatives generated: 520 …
EXIT=0
```

`cargo test --release --workspace`: **0 failures** (весь набор проходит).

И главное: платформенно-независимая проекция k1.json **побайтово совпадает** с проекцией закоммиченного артефакта. Проверено воспроизведением `topology::report::structural_projection` на Python:

```text
structural projections identical: True
full artifacts identical: False
top-level fields that differ in the FULL artifact: ['pruning','continuation','saddle_alternatives_total','arms']
```

Все различающиеся поля лежат ВНЕ проекции. Значит ubuntu-джоб CI `topology-check --structural` на этой правке **зелёный**. Красным становится только windows-джоб `tier-a-digests` (`topology-check` без флага) — и только потому, что закоммиченный артефакт старше правки. Если выполнить документированную в `docs/REPRODUCIBILITY_M4_5.md` команду регенерации (`gt-corpus topology --out docs/gt/TOPOLOGY_M4_5.json --scope full`) — не остаётся ни одного механизма, который бы это заметил, кроме сверки объявленных чисел в документах.

**Почему это существенно, а не косметика.** `config_hash` (компонент compatibility key §27.6) не меняется — набор конвенций в `TopologyConfigRecord` не сериализуется. Флаг `both_connectivity_arms_agree` перестаёт устанавливаться когда-либо, и никто этого не проверяет. Клауза 1 продолжает засчитывать совпадение с 8-связной истиной кандидату, порождённому только под 4-связной конвенцией.

**Общее правило класса.** Если метрика ослабляет условие успеха ссылкой на существование механизма («годится любая из двух конвенций, потому что мы держим обе»), она обязана измерять существование этого механизма. Иначе послабление остаётся, а его основание — нет.

---

### RT45-A2 — recall не может обнаружить ошибку в самой сигнатуре: истина и кандидат считаются ОДНОЙ функцией — **MAJOR**

**Утверждение, которое ломается.** §5.3 дословно: «Нельзя использовать одинаковую 4- или 8-connectivity для foreground/background и затем считать Euler signature доказанным». T5 объявлен PASS. Клауза 1 подана как измерение над независимо вычисленной истиной («вычисляется НЕЗАВИСИМЫМ exact-clip интегратором», «в истину не входит ни одна оценка»).

Независим только **интегратор площади**. Сама **сигнатура** истины берётся той же функцией, что и сигнатура кандидата:

```rust
// crates/vice-bench/src/topology/mod.rs, fn gt_signature
let l = vice_topology::threshold(&ink, w, h, 0.5, SaddleResolution::Thresholded);
let s = vice_topology::signature(&l, conn);          // ← та же функция, что у кандидатов
```

Любая ошибка в конвенции связности, в подсчёте дырок или в паддинге сокращается между двумя сторонами, и recall остаётся 100/100.

**Процедура воспроизведения.**

```bash
cd /tmp/attack1 && git checkout -- crates/vice-topology/src/lib.rs
# crates/vice-topology/src/cubical.rs, fn signature — ровно то, что §5.3 запрещает по имени
# -    let background = count_regions(l, false, conn.background()).max(1);
# +    let background = count_regions(l, false, conn.foreground()).max(1);
cargo build --release --bin gt-corpus
./target/release/gt-corpus topology --out /tmp/k2.json --scope full
cargo test --release --workspace
```

**Наблюдаемый результат.**

```text
  [MET] GT-equivalent topology present in envelope: … 100/100 arms = 1.0000; … 31/31 = 1.0000 …
  [MET] ambiguous fixtures retain alternatives …
  [MET] no magic-threshold-only architecture …
EXIT=0

cargo test --release --workspace:  489 passed / 0 failed
non_trivial_gt_arms: 31 -> 31
arms whose GT signature moved under the forbidden convention: 0
recall stayed: {'arms': 100, 'hits': 100, 'fraction': 1.0}
```

Ни одна из 489 проверок и ни одна из трёх клауз §28 не краснеет. Артефакт при этом отличается (`field_contributions`, `arms`), то есть `topology-check` против ЗАМОРОЖЕННОГО артефакта поймает — но снова только пока артефакт не перегенерирован.

**Доказательство, что нокаут не пустой.** Классический свидетель — диагональное кольцо (11×11, `|x-5|+|y-5| == 4`). Тест `crates/vice-topology/tests/rt45_complementary.rs`, один и тот же файл на пропатченном и на чистом дереве:

```text
=== WITH K2 (запрещённая одинаковая связность) ===
arm fg=4 bg=8: components 16 holes 1 euler 15
arm fg=8 bg=4: components 1 holes 0 euler 1        ← дырка ИСЧЕЗЛА
=== PRISTINE (комплементарная) ===
arm fg=4 bg=8: components 16 holes 0 euler 16
arm fg=8 bg=4: components 1 holes 1 euler 0        ← дырка есть
```

То есть правка меняет ровно тот ответ, ради которого §5.3 запрещает одинаковую связность, — и корпус M4.5 на неё не реагирует, потому что на его 132 arm-ах разметки достаточно well-composed, чтобы 4-vs-8 фон ничего не двигал (0 arm-ов из 132 сменили GT-сигнатуру).

Существующий позитивный тест (`кольцо = 1 компонента + 1 дыра под ОБЕИМИ конвенциями`) проходит и в пропатченном дереве: обычное кольцо — не свидетель для этой конвенции. Фикстура принадлежит подклассу, где дефект недостижим — мета-правило M-2 дословно.

**Общее правило класса.** Ground truth, вычисленный тем же кодом, что и проверяемая величина, измеряет согласованность, а не правильность. Независимость должна распространяться на ВСЮ цепочку от сцены до сравниваемого числа, а не только на её самый дорогой шаг.

---

### RT45-A3 — условие D1 не закрыто: широкая популяция достижима, включая sealed audit — **BLOCKER**

**Утверждение, которое ломается.** Строка гейт-таблицы **T10: «Условие D1: широкая популяция НЕДОСТИЖИМА | PASS»**; §1 STATUS: «закрыто печатью, а не привычкой»; докблок `frozen_calibration.rs`: «The legal population is reachable through exactly one public function, `frozen_calibration_groups`, and the compiler is what says so».

Печать наложена на ДВА ИМЕНИ (`all_groups`, `procedural_groups` → `pub(crate)`), а не на класс. Публичными остались `vice_bench::gt::authored::authored_groups()` и `vice_bench::gt::adversarial::all_adversarial_groups()` — два из трёх слагаемых `all_groups()`:

```rust
pub(crate) fn all_groups() -> Result<Vec<GtSourceGroup>, String> {
    let mut groups = procedural_groups(PROCEDURAL_VARIANTS);   // pub(crate) — закрыт
    groups.extend(authored_groups().map_err(|e| e.to_string())?);   // pub — открыт
    groups.extend(all_adversarial_groups());                        // pub — открыт
    …
}
```

**Процедура воспроизведения.** Файл `crates/vice-bench/tests/rt45_d1_bypass.rs` в интеграционных тестах (отдельный крейт, ровно там, где живут замороженные измерения D1):

```rust
use vice_bench::gt::adversarial::all_adversarial_groups;
use vice_bench::gt::authored::authored_groups;
use vice_bench::gt::split::{Split, SPLIT_POLICY_V1};

#[test]
fn an_integration_test_can_still_reach_the_sealed_audit() {
    let mut groups = authored_groups().unwrap();
    groups.extend(all_adversarial_groups());
    let sealed: Vec<&str> = groups.iter()
        .filter(|g| SPLIT_POLICY_V1.split_of_group(g) == Split::SealedAudit)
        .map(|g| g.id.as_str()).collect();
    /* печать */ assert!(!sealed.is_empty());
}
```

```bash
cargo test --release -p vice-bench --test rt45_d1_bypass -- --nocapture
cargo test --release -p vice-bench --test hygiene
```

**Наблюдаемый результат.**

```text
RT45: reached 12 source groups from an integration test WITHOUT naming all_groups or
      procedural_groups: 6 development, 4 calibration, 2 SEALED AUDIT
RT45: sealed-audit groups reachable: ["authored/leaf", "authored/bracket"]
test an_integration_test_can_still_reach_the_sealed_audit ... ok

running 5 tests (hygiene.rs)
test the_wide_corpus_population_is_unreachable_from_the_measurements ... ok
test result: ok. 5 passed; 0 failed
```

Компилируется. Проходит. Страж `hygiene.rs` остаётся зелёным: его клауза 3 ищет в интеграционных тестах ровно подстроки `all_groups(` и `procedural_groups(`, а клауза 4 перечисляет модули ВНУТРИ крейта и на новый интеграционный тест не смотрит.

**Почему это BLOCKER.** Исходный дефект M4-N1 / F-0026 — «замороженная константа измерена на популяции, включающей sealed audit». Он остаётся достижимым: замороженное измерение в `frozen_calibration.rs` может сегодня, не нарушая ни одного теста, взять `authored_groups()` и измерить коэффициент на `authored/leaf` и `authored/bracket` — двух из 22 групп, которые сам харнесс M4.5 демонстративно пропускает по §27.1. Строка T10 утверждает свойство («популяция НЕДОСТИЖИМА»), которое не выполняется; закрыто конкретное написание атаки ревьюера, а не класс. Это ровно то, о чём мета-правило M-1 («поверхность, а не место») — и печать нарушает его сама.

**Общее правило класса.** Печать по имени функции закрывает имя. Чтобы закрыть популяцию, `pub(crate)` должен стоять на КАЖДОМ публичном пути, из которого популяция собирается, — и это свойство должно проверяться не текстовым сканом, а тем же способом, каким проверяется поверхность чтения env: перечислением ВСЕХ `pub fn`, возвращающих `Vec<GtSourceGroup>`, с обязательной сверкой множества.

---

### RT45-A4 — сверка чисел в клаузных строках — проверка на ПРИНАДЛЕЖНОСТЬ МНОЖЕСТВУ, а не на равенство — **MAJOR**

**Утверждение, которое ломается.** T11 и §1 STATUS: условие D2 закрыто, «атака NEW-2 воспроизведена: подделанное `0.9999` роняет тест». Докблок `doc_claims.rs`: «What becomes impossible is the specific thing that happened twice: the rows that PRESENT the gate verdict drifting from the run that produced it».

`the_status_clause_rows_quote_only_declared_numbers` проверяет: `∀ токен ∈ строка: токен ∈ {78 объявленных величин}`. Не «токен равен той величине, о которой строка говорит». Среди 78 объявленных величин много мелких целых (0, 1, 2, 3, 5, 8, 10, 11, 18, 22, 31, 38, 41, 52, 56, 64, 90, 100, 132, …), поэтому ложное измерение почти всегда можно собрать из чужих объявленных чисел.

**Процедура воспроизведения.** В `docs/STATUS_M4_5.md`, строка T1:

```diff
-recall 100 из 100; на 31 arm-е с нетривиальной GT — 31; бюджет потерял ответ на 0
+recall 56 из 132; на 8 arm-е с нетривиальной GT — 5; бюджет потерял ответ на 2
```

```bash
cargo test --release -p vice-bench --test doc_claims -- --nocapture
```

**Наблюдаемый результат.**

```text
running 4 tests
test the_row_split_honours_the_markdown_escape ... ok
test a_wrong_declaration_would_be_caught ... ok
78 declared numbers agree with the committed artifacts
test every_declared_number_matches_the_committed_artifact ... ok
52 numbers across 2 gate tables are declared measurements
test the_status_clause_rows_quote_only_declared_numbers ... ok
test result: ok. 4 passed; 0 failed
```

Строка, объявляющая клаузу 1 спеки, теперь утверждает recall 56/132 при бюджете, потерявшем 2 ответа, — и «52 числа проверены» печатается как успех.

**Калибровка (насколько страж всё же силён).** Та же строка с числом, которого нет в объявленной таблице:

```diff
-recall 100 из 100
+recall  99 из 100
```

```text
panicked at crates\vice-bench\tests\doc_claims.rs:407:17:
docs/STATUS_M4_5.md clause row "| T1 " quotes 99, which is not one of the 78 declared
measurements: it is either stale (F-0028) or it needs a row in a declared table.
test result: FAILED. 3 passed; 1 failed
```

То есть исторический случай F-0028 (устаревшее число из прошлого прогона) страж ловит. Заявленное свойство — «строки, которые ПРЕДЪЯВЛЯЮТ гейт-вердикт, не могут разойтись с прогоном» — не выполняется.

**Общее правило класса.** Проверка «число встречается где-то в артефактах» не является проверкой «число верно». Пока ключ (`topology:recall_all.hits`) не привязан к позиции в строке, клаузная строка проверяется на словарь, а не на смысл.

---

### RT45-A5 — §27.7 не действует ни на одну гейт-константу M4.5 — **BLOCKER**

**Утверждение, которое ломается.** STATUS §3: «`configs/GATES_V1.toml` **не тронут**: M4.5 не замораживает ни одной новой константы, поэтому §27.7 здесь нечего исполнять».

M4.5 замораживает как минимум:

| константа | где | что решает |
|---|---|---|
| `budget: 48`, `per_quota_class: 2`, `mass_scale: 8.0` | `envelope.rs::ENVELOPE_CONFIG_V1` | tier 3 pruning, §11.3 |
| `max_plateau_levels: 6`, `max_event_levels: 8`, `min_event_persistence: 0.02`, `fixed_smoke_levels: [0.5]` | `events.rs::LEVEL_CONFIG_V1` | какие уровни вообще порождаются |
| `tv_iterations`, `tv_step`, `tv_huber_delta`, `deconv_iterations`, `deconv_step` | `field.rs::FIELD_CONFIG_V1` | §11.1 поля |
| `halo_px: 3`, `max_plans: 8` | `continuation.rs` | §11.4 |
| `r.arms >= 20`, `groups >= 5`, `non_trivial_gt_arms >= 5` | `report.rs::gate_table` | **пороги популяции самой клаузы 1** |
| `pairs.len() >= 2`, `classes_from_* >= 2` | `report.rs::gate_table` | **пороги клаузы 2** |

`GATE_PATHS = &["configs/GATES_V1.toml"]`; в файле девять секций (`reliability`, `corpus_instruments`, `identifiability`, `split`, `likelihood`, `boundary_accuracy`, `geometry_code_table`, `noise_scales`, `noise_scales_degraded`) — топологической нет. Соответственно правило «gate-файл только отдельным коммитом без кода» физически не может сработать.

**Процедура воспроизведения.**

```bash
cd /tmp/attack1
# 1) ослабляем пороги популяции клаузы 1 в report.rs
#    -  let recall_row = r.arms >= 20 && groups >= 5 && self.non_trivial_gt_arms >= 5
#    +  let recall_row = r.arms >= 1  && groups >= 1 && self.non_trivial_gt_arms >= 1
# 2) в ТОМ ЖЕ коммите двигаем замороженный бюджет конверта
#    envelope.rs:  budget: 48 -> 6,  per_quota_class: 2 -> 1
git add -A && git commit -m "RT45: a feature commit that weakens its own M4.5 gate"
git diff --name-status HEAD^ HEAD | ./target/release/gt-corpus gates-check --stdin \
  --existing-gate configs/GATES_V1.toml
```

**Наблюдаемый результат.**

```text
M	crates/vice-bench/src/topology/report.rs
A	crates/vice-bench/tests/rt45_d1_bypass.rs
M	crates/vice-topology/src/envelope.rs
A	crates/vice-topology/tests/rt45_complementary.rs
A	crates/vice-topology/tests/rt45_symmetry.rs
--- gates-check verdict on it ---
no gate/feature co-change in 5 path(s)
EXIT=0
```

Один коммит одновременно ослабляет собственный гейт и меняет код. §27.7, второе предложение — «Feature PR не может одновременно ослабить собственный gate» — не исполняется для M4.5 ни в какой форме.

Оговорка в пользу автора: механизм `gates-check` сам по себе исправен и хорошо продуман (разбор `--name-status`, `R100`, `-z`, quotePath, per-commit по всему пушу). Дефект не в нём, а в утверждении, что исполнять нечего. `QUANTIZATION_FLOOR_CODES` — единственная константа, которую клауза 2 читает и которая ДЕЙСТВИТЕЛЬНО зарегистрирована (`identifiability.quantization_floor_codes`); остальные шестнадцать — нет.

**Общее правило класса.** «Мы не заморозили новых констант» — проверяемое утверждение, и проверять его надо не перечитыванием, а тем же способом, что и поверхность чтения env: тестом, который требует, чтобы каждая `*_CONFIG_V1`/порог, входящая в вычисление гейт-строки, имела запись в `GATE_PATHS`-файле.

---

### RT45-A6 — три конъюнкта клаузы 1 логически следуют из четвёртого — **MINOR**

`report.rs::gate_table`:

```rust
let recall_row = r.arms >= 20 && groups >= 5 && self.non_trivial_gt_arms >= 5
    && r.hits == r.arms
    && nt.hits == nt.arms
    && self.pruning.arms_where_budget_pruning_could_have_lost_the_answer == 0;
```

где

```rust
r  = recall(&pop, |a| a.gt_in_envelope);
nt = recall(&nontrivial, |a| a.gt_in_envelope);           // nontrivial ⊆ pop
arms_where_budget_pruning_could_have_lost_the_answer =
    pop.iter().filter(|a| !a.gt_in_envelope && a.budget_removed > 0).count();
```

`r.hits == r.arms` ⟺ ∀a∈pop: `gt_in_envelope`. Отсюда немедленно `nt.hits == nt.arms` и счётчик = 0. То есть **два последних конъюнкта не могут провалиться отдельно**, а число `0`, которое STATUS §2 и строка T1 подают как отдельное измерение стоп-условия §36 («arms, где бюджет мог потерять ответ — 0») и которое объявлено в `REPRODUCIBILITY_M4_5.md` как `topology:pruning.arms_where_budget_pruning_could_have_lost_the_answer`, равно нулю **по построению** во всяком мире, где строка зелёная.

Настоящее измерение §36 потребовало бы сравнить конверт ДО и ПОСЛЕ tier 3, чего харнесс не делает: `gt_in_envelope` вычисляется только на оставленном множестве.

Правило класса: конъюнкт, импликуемый другим конъюнктом той же конъюнкции, — это не контроль, а перефраз; его публикация как отдельного числа завышает число независимых свидетельств.

---

### RT45-A7 — два «контроля» клаузы 3 — теоремы о голубях, а не измерения — **MINOR**

`threshold_row` требует `self.tie_batches_max > 0` и печатает `largest_batch_pixels`. Уровни квантуются на сетку `LEVEL_QUANTA = 512` (513 возможных значений), а изображения — 32×32…128×128 (1024…16384 пикселей). По принципу Дирихле хотя бы один уровень всегда содержит больше одного пикселя, и `largest_batch_pixels >= ceil(N/513) >= 2`. То есть `tie_batches_max > 0` **не может быть ложным** ни на одном изображении корпуса, а объявленные `377` (уровней с ничьёй) и `14821` (наибольшая партия) — это в основном фон нулевого покрытия 128×128 кадра, а не свидетельство того, что batch-правило §11.2 работает.

Строка T3 и §2 STATUS предъявляют эти числа как содержательные («живая событийная машинерия: … 377 уровней с ничьёй, наибольшая партия 14821»). Они получаются на любом изображении, включая изображение, обрабатываемое порядком обхода.

Правило класса: контроль, который истинен по размеру входа, измеряет размер входа.

---

### RT45-A8 — состав популяции клаузы 1 не раскрыт: систематически исключены именно многокомпонентные фикстуры — **MAJOR**

Строка T1 раскрывает популяцию тремя числами: 100 из 132, 38 непрозрачных, «52 отказа». Ни `RefusedArm`, ни отчёт не несут identifiability отказанных arm-ов, поэтому раскрыт РАЗМЕР исключения, но не его СОСТАВ. Я его измерил.

**Процедура воспроизведения.** В `crates/vice-bench/src/topology/mod.rs::measure_arm` дописать `identifiability.as_str()` в строку отказа (переменная уже вычислена выше по функции), пересобрать, прогнать `topology --scope full`, сгруппировать `refused`.

**Наблюдаемый результат.**

```text
REFUSED arms by (evidence outcome, corpus identifiability label):
   ('ambiguous',   'equivalent_family')   6
   ('ambiguous',   'identifiable')        3
   ('ambiguous',   'information_lost')    1
   ('unsupported', 'identifiable')       41
   ('unsupported', 'information_lost')    1
total refused 52
IDENTIFIABLE renders refused before topology ran: 44
recall population: 100
identifiable transparent-exterior arms attempted: 144
```

**44 из 52 отказов — это IDENTIFIABLE рендеры.** Из 144 identifiable прозрачно-exterior arm-ов в популяцию клаузы попадают 100 (69 %); 44 отсеяны стадией evidence M4 ДО того, как топология вообще запустилась. И отсеяны они не случайно:

```text
identifiable refusals by scene:
   proc/two_islands/{000,001,003}   5 + 5 + 5 = 15     ← ДВЕ КОМПОНЕНТЫ
   proc/triple_junction/{000,001,003} 5 + 5 + 5 = 15
   authored/twotone                  6
   adv/near-tangent                  5
   adv/sliver                        3
```

То есть фикстуры, чья топология многокомпонентна (`two_islands`) или содержит тройные стыки, — ровно те, которых в популяции клаузы «GT-топология присутствует в конверте» нет.

**Что на самом деле стоит за «31 arm с нетривиальной GT».** Разбор закоммиченного артефакта:

```text
GT-классы всей популяции (gt_four):  (1,0)→69   (1,1)→24   (3,0)→3   (5,0)→4
31 нетривиальный arm, пары (four / eight):
   ((1,1),(1,1)) → 24   из authored/keyhole(6) + proc/annulus/{000,001,003}(6+6+6)
   ((3,0),(1,0)) →  3   из proc/star/001
   ((5,0),(1,0)) →  4   из proc/star/003
```

Для семи звёздных arm-ов конвенции РАСХОДЯТСЯ, а `matches_gt` засчитывает совпадение с любой — и 8-связное чтение там равно `(1,0)`, обычному диску. То есть на 7 из 31 «нетривиальных» arm-ов **тривиальный ответ является правильным**. Реально нетривиальных — 24 arm-а, и они происходят из **двух** shape family: `keyhole` и `annulus`.

**Отдельно про ширину.** `recall_source_groups = 18`; его докблок ссылается на §27.4 («the source-scene family is the unit of a reliability trial… this is the number a gate row may honestly quote about breadth»), но считает `group_id`, а не `shape_family`. 18 групп популяции — это 8 семейств: `keyhole, lobed, pennant, annulus, bezier_blob, l_shape, polygon, star`; `proc/annulus/{000,001,003}` — три варианта ОДНОГО семейства, а `split.rs` в первом же абзаце объясняет, почему варианты одного семейства не являются независимыми единицами. Порог `groups >= 5` в клаузе 1 меряется в единицах, которые сам проект объявил зависимыми.

Правило класса: если исключающий предикат вычисляется тем же конвейером, чей выход проверяется, размер исключения — не достаточное раскрытие; нужен состав, потому что трудность коррелирует.

---

## §2. Атаки, которые НЕ удались (измерение прочности)

**F1. Честность артефакта.** Свежий worktree, чистая сборка, `gt-corpus topology-check --report docs/gt/TOPOLOGY_M4_5.json` → `topology report reproduced with every metric compared`, **exit 0**. Закоммиченный `TOPOLOGY_M4_5.json` — это ровно то, что производит харнесс на записанной платформе. Расхождения committed-vs-live не существует; расхождение, которое CI не заметит, я построить не смог, кроме случая RT45-A1 (там расхождение остаётся, но только в непроецируемых полях).

**F2. Детерминизм: зависимость от порядка обхода.** Построил поля с массивными плато и ничьими: четырёхтеррасный «зиккурат» (терраса ровно на 0.5 — одновременно фиксированная проба и максимальная ничья) и шахматное ядро (максимальная плотность критических 2×2, все пиксели на двух значениях), в квадратном и прямоугольном кадре. Сравнил множества сигнатурных классов конверта на исходном поле, на транспонированном, на повёрнутом на 180° и на зеркальном (обе связности инвариантны ко всем трём):

```text
saddle 32x32: base   [(1,0),(1,288),(3,0),(244,0),(338,0)]
saddle 32x32: transp [(1,0),(1,288),(3,0),(244,0),(338,0)]
saddle 32x32: rot180 [(1,0),(1,288),(3,0),(244,0),(338,0)]
saddle 32x32: mirror [(1,0),(1,288),(3,0),(244,0),(338,0)]
saddle 40x24 …  идентично; plateau 32x32 и 48x32 … идентично
```

Защита выстояла. Разбор `merge_tree` объясняет почему и подтверждается: рёбра строятся и сортируются; выживает меньший индекс, поэтому корень компоненты = её минимальный индекс независимо от порядка; при elder-правиле мультимножество записанных `min(birth)` при слиянии множества компонент равно множеству рождений без максимума при ЛЮБОМ порядке слияний; активация всей партии до первого объединения плюс охрана `younger > *level` убирают ложные смерти внутри партии. §11.2 здесь исполнена, а не заявлена.

**F3. Потеря топологического класса при pruning (§36, «GT-equivalent topology выпадает из envelope из-за proxy/budget pruning»).** Инструментировал `envelope::prune`: множество классов `(components, holes)` ДО tier 2 и ПОСЛЕ tier 3, печать каждой разности. Полный прогон `topology --scope full`:

```text
--- class losses observed ---
0
```

Ни одного топологического класса не потеряно ни доминированием (2 удаления), ни бюджетом (308 удалений на 11 arm-ах) нигде на корпусе. Это сильный результат в пользу автора: атака на конверт (пункт 5 брифа) на этом корпусе не проходит эмпирически. Оговорка, которая остаётся открытой и которую автор сам записал (§5 п. 6): tier 2 сертифицирован для пары *(разметка, уровень)*, а не для топологического КЛАССА; сертификата «ни одна сцена этого класса не конкурентоспособна» нет. Свипа по бюджету я не делал (см. §3).

**F4. Уровневый нокаут по «magic threshold».** Клауза 3 всё-таки не пустая: она держится на `e.hits == r.hits` и на `one_level_is_not_enough`. Если убрать событийные источники уровней целиком, `gt_in_envelope_events_only` обнуляется и строка краснеет. Разрушающая сила у клаузы 3 есть — но она сосредоточена на ОДНОЙ паре фикстур (`ambiguity/topology`), потому что из трёх ambiguity-пар топологических две, а из этих двух `ambiguity/hole` извинена по замороженному полу. n = 1.

**F5. Удаление min-дерева (§11.2 «min-tree sublevel/background events», §28 M4.5 «max/min tree events»).** Вырезал фоновое дерево из `batch_critical_events` (bb/bm пустые). **Гейт-таблица: три клаузы MET, exit 0** — включая клаузу 2, которую §2 STATUS объясняет именно через bridge-vs-gap. Однако `cargo test --release --workspace` падает: `events::tests::a_hole_shows_up_as_a_background_event ... FAILED` (37 passed, 1 failed). То есть CI краснеет — на тестовом эшелоне, а не на гейтовом. Атака как срыв гейта не удалась; как измерение — показывает ту же слепоту гейт-таблицы, что RT45-A1/A2, но здесь юнит-тест оказался адекватным свидетелем.

**F6. Обход `doc_claims` числом «из воздуха».** Не удалось: число, которого нет в объявленной таблице, роняет тест (см. калибровку в RT45-A4). Попытка спрятать объявление в HTML-комментарий тоже не проходит — строка с ключом, не являющаяся табличной, — это assert по имени файла и номеру строки.

**F7. Fake placeholder arms (§27.6).** Не найдено. `ContinuationStep` — три варианта (`Executed`, `PartiallyExecuted`, `Refused`), у двух последних обязательный `owner_milestone` и непустой `missing` (оба конструктора падают на пустом списке); `FieldRefusal` — два типизированных варианта с текстом причины (`KernelNotIdentifiable`, `KernelIsTheIdentity`), никаких «отказ = успех». `todo!`/`unimplemented!` в продакшене нет. Попытка найти арм, который «отказан», но где-то засчитан как успех или как отсутствие проблемы, результата не дала — единственная граничная точка — RT45-A8, и там отказ честно не засчитан ни в числитель, ни в знаменатель, просто состав не раскрыт.

**F8. Замороженные файлы.** `git diff --stat cf3c1be..HEAD -- configs/ docs/gt/AUDIT_SEAL.json docs/gt/CORPUS_MANIFEST.json docs/REVIEW_M4.md rust-toolchain.toml SOURCE_PINS.toml` — пусто. Заявление §3 STATUS о нулевом дифе подписанных артефактов подтверждается.

**F9. Хронология CI.** `git diff --name-only 275ddbd..HEAD` → ровно `docs/STATUS_M4_5.md`. Утверждение «прогон 46 на 275ddbd — последний коммит, содержащий код милестоуна» верно; C167 документный. Отказ прибора (`403 rate limit`) записан как отказ прибора — мета-правило M-4 соблюдено.

---

## §3. Что я не смог атаковать и почему

1. **Tier A на чужой платформе.** Работал только на windows-x86_64, той же паре `(os, arch)`, что записана в артефакте. Проверить, что `topology-check --structural` на ubuntu ведёт себя как заявлено, и что windows-джоб `tier-a-digests` действительно исполняется, я не мог. Из-за этого вывод RT45-A1 о зелёном ubuntu-джобе получен воспроизведением `structural_projection` на Python, а не запуском на Linux; транскрипция сверена со строкой в `report.rs`, но это реплика, а не оригинал.
2. **Сам CI.** Прогонов GitHub Actions я не читал (сеть не использовалась). Всё, что сказано о CI, выведено из `.github/workflows/ci.yml` и из локальных прогонов.
3. **Свип по бюджету конверта.** Ограничение §5 п. 5 («бюджет 48 и квота 2 НЕ калиброваны») я подтвердил только с одной стороны: на замороженном конфиге ни один класс не теряется (F3). Насколько близко корпус к границе — не измерено; для этого нужен свип, а один полный прогон — минуты.
4. **Донорские исходники** — не открывались (D-3, лицензионный периметр).
5. **Внутренности стадии evidence M4.** 41 «unsupported» отказ на identifiable рендерах — это поведение M4, а не M4.5; я зафиксировал факт и состав, но не проверял, справедлив ли сам отказ.
6. **Построение живой фикстуры, на которой tier 2 теряет класс.** Сконструировать такую пару кандидатов вручную тривиально, но это не доказывает достижимости через `propose` на легальном входе; достижимость я не установил, поэтому вынес это в §2 как открытую оговорку, а не в §1 как находку.
7. **`achievable` как граница внутри `SupportedModelUniverse`** (§5 п. 6 STATUS) — вопрос формулировки универсума, владелец M6/M7; атаковать нечего, пока универсум не определён.

---

## §4. RED TEAM VERDICT

**RED TEAM VERDICT: FAIL**

---

## §5. Гигиена в конце

**Основной репозиторий `C:\Users\nirrt\Toolset\vice-classic` после всей работы:**

```text
$ git status --porcelain
(пусто)

$ git rev-parse HEAD
2d3e9c974de0d3d5fa43d3962b0078558d62ef61
```

HEAD не изменился, рабочее дерево чистое, ничего не коммитилось и не пушилось из основного дерева. Мой worktree `…/scratchpad/attack1` удалён, `git worktree prune` выполнен.

**Что осталось зарегистрировано и создано не мной:** `git worktree list` показывает второй worktree
`…/AppData/Local/Temp/claude/C--Users-nirrt-Toolset-Vice-classic/f0169b6d-302f-4aed-aaf5-5f0555638d99/scratchpad/wt` (detached HEAD `2d3e9c9`), в нём незакоммиченные изменения (`M crates/vice-bench/src/topology/mod.rs`, `?? crates/vice-bench/src/topology/probe.rs`, `?? crates/vice-bench/tests/reviewer_d1_attack.rs`). Он существовал до начала моей работы и мною не создавался и не трогался. Фиксирую, потому что запись о нём лежит в `.git/worktrees/` основного репозитория, а прошлый инцидент («рецензент оставил в рабочем дереве подложенное значение») требует называть такие вещи вслух.

**SHA-256 спеки, повторно после работы:**

```text
652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1  *VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md
```

---

Red team (adversarial pass, cold agent context, Opus 5)

---
---

# ADDENDUM — RED TEAM M4.5, дельта-проход по C170–C185

> Addendum публикуется **дословно**. Governor его не редактировал. Подписанный текст выше не изменён ни в одной строке.

## §A0. Гигиена и спека, начало

```text
$ git status --porcelain
(пусто)

$ git rev-parse HEAD
d1ab2b990348a2f8bfc60df0df8ee3b5b1c9eb93

$ git worktree list
C:/Users/nirrt/Toolset/vice-classic  d1ab2b9 [main]        ← только основное дерево

$ sha256sum /c/Users/nirrt/Downloads/VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md
652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1
```

Посторонний worktree, зафиксированный в §0/§5 подписанного отчёта, действительно исчез. Спека та же. Метод тот же: основное дерево только на чтение, всё ломающее — в `git worktree` (`…/scratchpad/d2`, detached `d1ab2b9`, отдельный `CARGO_TARGET_DIR`), удалён в конце. Донорские исходники не открывались.

Базовые факты на HEAD, измеренные до атак:

```text
$ gt-corpus topology-check --report docs/gt/TOPOLOGY_M4_5.json
topology report reproduced with every metric compared          EXIT=0

$ cargo test --release --workspace          493 passed / 0 failed
$ gt-corpus topology --scope full           3/3 MET, EXIT=0
```

---

## §A1. Восемь атак подписанного отчёта, повторённые дословно

| # | что атаковалось | вердикт |
|---|---|---|
| RT45-A1 | нокаут второго плеча комплементарной связности | **ОТБИТА** |
| RT45-A2 | GT-сигнатура считалась той же функцией, что и кандидат | **ОТБИТА** (на юнит-эшелоне; гейт-строка по-прежнему слепа) |
| RT45-A3 | печать популяции D1 | **ПО-ПРЕЖНЕМУ ПРОХОДИТ** — в новой форме и шире прежнего (RT45-A9) |
| RT45-A4 | сверка чисел клаузной строки | **ОТБИТА ЧАСТИЧНО** — новый механизм на новых строках, старые не покрыты (RT45-A11) |
| RT45-A5 | §27.7 и гейт-константы | **ОТБИТА ЧАСТИЧНО** — значения закрыты, сравнения нет (RT45-A10) |
| RT45-A6 | импликуемые конъюнкты клаузы 1 | **ОТБИТА** |
| RT45-A7 | контроль, истинный по принципу Дирихле | **ПО-ПРЕЖНЕМУ ПРОХОДИТ** (в узкой форме, RT45-A13) |
| RT45-A8 | состав и ширина популяции | **ОТБИТА** (с остатком) |

### RT45-A1 — ОТБИТА

Процедура прежняя: в `crates/vice-topology/src/lib.rs::propose`

```diff
-            for conn in ComplementaryConnectivity::arms() {
+            for conn in [ComplementaryConnectivity::arms()[0]] {
```

```text
$ gt-corpus topology --out /tmp/d_k1.json --scope full
  [NOT MET] GT-equivalent topology present in envelope
  [MET]     ambiguous fixtures retain alternatives
  [MET]     no magic-threshold-only architecture
EXIT=1
… carry candidates from BOTH complementary arms, and 100 carry only one
```

В прошлый раз: 3/3 MET, exit 0, 489 тестов зелёные, структурная проекция побайтово совпадала. Теперь строка краснеет, `arms_missing_a_connectivity_arm` идёт 0 → 100. Послабление «годится любая из двух конвенций» теперь опирается на измеренный факт, а не на комментарий. Удаление `both_connectivity_arms_agree` (флаг, выставлявшийся на 0 кандидатов) — правильный шаг: мёртвый флаг заменён считаемой величиной.

### RT45-A2 — ОТБИТА на юнит-эшелоне; гейт-строка осталась слепа

Процедура прежняя: в `cubical.rs::signature` `conn.background()` → `conn.foreground()`.

```text
$ cargo test --release --workspace
test topology::independent::tests::the_independent_chain_agrees_with_the_production_signature ... FAILED
test result: FAILED. 158 passed; 1 failed
```

Это ровно то, чего атака требовала: раньше мутация §5.3 не роняла НИЧЕГО. Но полезно сказать вторую половину точно, потому что автор её не заявлял, а я её измерил:

```text
$ gt-corpus topology --out /tmp/d_k2.json --scope full     # с той же мутацией
  [MET] GT-equivalent topology present in envelope
  [MET] ambiguous fixtures retain alternatives
  [MET] no magic-threshold-only architecture
  recall all 100/100                                        EXIT=0
```

**Сама гейт-таблица §28 по-прежнему не видит нарушения §5.3.** Ловит его юнит-тест на диагональном кольце. Это законное разделение труда (ADR-0027 так и говорит), но строка T1 не является свидетельством о конвенции и не должна читаться как таковое: на 132 arm-ах корпуса GT-сигнатуры не двигаются ни при какой из двух конвенций.

### RT45-A3 — ПО-ПРЕЖНЕМУ ПРОХОДИТ

Атака в ДОСЛОВНОЙ прежней форме теперь не компилируется — это честная победа:

```text
error[E0603]: function `authored_groups` is private
error[E0603]: function `all_adversarial_groups` is private
```

Но класс не закрыт; подробности — RT45-A9 ниже. Коротко: две обычные Rust-сигнатуры, которые скан не моделирует, открывают из интеграционного теста **60 групп, включая ВСЕ 22 sealed-audit**, при `cargo fmt --check` чистом, `cargo clippy -- -D warnings` с нулём диагностик и всех семи тестах `hygiene.rs` зелёных.

### RT45-A4 — ОТБИТА ЧАСТИЧНО

Позиционная привязка работает — я это проверил в обе стороны. Подделка первого позиционного числа строки `T1d` (100 → 56, где 56 — объявленная величина):

```text
docs/STATUS_M4_5.md row "| T1d " position 1: the row says 56, but
TOPOLOGY_M4_5.json:identifiable_supported_arms is 100. This is the check
membership could not make
test result: FAILED. 4 passed; 1 failed
```

Но `POSITIONAL_ROWS` покрывает `T1d/T2d/T3d`, а моя исходная атака была на `T1`. Строка `T1` жива, стоит под заголовком **«## 4. Gate table (author-side; §28 M4.5)»**, помечена «клауза спеки 1», несёт PASS, и там же написано: «Числа в T1–T3 обязаны быть объявленными величинами и сверяются `doc_claims.rs`». Дословный повтор моей подделки:

```diff
-recall 100 из 100; на 31 arm-е с нетривиальной GT — 31
+recall  56 из 132; на  8 arm-е с нетривиальной GT —  5
```

```text
running 5 tests
test the_delta_clause_rows_equal_their_declared_keys_position_by_position ... ok
test the_status_clause_rows_quote_only_declared_numbers ... ok
test result: ok. 5 passed; 0 failed
```

Подробности — RT45-A11.

### RT45-A5 — ОТБИТА ЧАСТИЧНО

Половина, которая закрыта, закрыта по-настоящему. Калибровка:

```diff
- envelope.rs:  budget: 48,
+ envelope.rs:  budget: 6,
```
```text
gates::tests::every_frozen_value_agrees_with_the_code_that_uses_it ... FAILED
topology.envelope_budget: the gate file and the code disagree
```

21 ключ секции `[topology]` действительно связан с константами кода, и молча подвинуть замороженное ЗНАЧЕНИЕ больше нельзя. Половина, которая не закрыта, — СРАВНЕНИЕ: см. RT45-A10.

### RT45-A6 — ОТБИТА

Импликуемые конъюнкты (`nt.hits == nt.arms`, «бюджет потерял ответ == 0») удалены из конъюнкции и остались публикуемыми числами. Появилась величина, которая может двигаться при recall 100 %:

```text
pruning: {arms_with_budget_pruning: 11, budget_removed: 308, dominated_removed: 2,
          arms_where_budget_removed_a_gt_class_candidate: 9,
          arms_where_budget_pruning_lost_the_last_gt_candidate: 0}
```

`9` при recall 100/100 — это ровно то измерение, отсутствие которого я называл: near-miss, который существует и до сих пор был невидим. Хороший ответ.

### RT45-A7 — ПО-ПРЕЖНЕМУ ПРОХОДИТ (узко)

Заявление C177: «The conjunct is gone.» Исполнение:

```text
$ sed -n '502,506p' crates/vice-bench/src/topology/report.rs
        let threshold_row = r.arms > 0
            && e.hits == r.hits
            && one_level_is_not_enough
            && self.saddle_alternatives_total > 0
            && self.tie_batches_max > 0;          ← конъюнкт на месте

$ git log --oneline -S"tie_batches_max > 0" -- crates/vice-bench/src/topology/report.rs
fe45e18 C157 M4.5: the corpus recall harness, the artifact and the three gate rows
```

Строка `-S` показывает: подстрока внесена в C157 и ни одним коммитом дельты не удалялась. Изменился только ПРОЗАИЧЕСКИЙ текст гейт-строки (теперь он честно говорит, что 377 и 14821 — не свидетельство). См. RT45-A13.

### RT45-A8 — ОТБИТА, с остатком

Артефакт теперь несёт всё, чего не хватало:

```text
recall_shape_families: 8            (при recall_source_groups: 18)
identifiable_arms_refused_before_topology: 44
families_absent_from_recall_population:
  ['adversarial/near-tangent','adversarial/sliver','ambiguity/topology',
   'authored/twotone','triple_junction','two_islands']
recall_unrelated_field:            {arms:100, hits:76}
recall_unrelated_field_non_trivial:{arms: 31, hits: 7}
```

И вот что стоит отметить как независимое подтверждение: нокаут-контроль автора на нетривиальной части даёт **7 из 31**, а число arm-ов, чья 8-связная GT РАВНА обычному диску, — тоже ровно **7**:

```text
non-trivial GT classes: ((1,1),(1,1)) → 24;  ((3,0),(1,0)) → 3;  ((5,0),(1,0)) → 4
non-trivial arms whose 8-conn GT IS a plain disk: 7
```

То есть несвязанный диск выигрывает в точности на тех семи arm-ах, про которые я писал, что там правильным ответом является тривиальный. Автор отказался сузить «31» до «24», но опубликованный им контроль делает это сужение выводимым из артефакта — этого достаточно, и заявление больше не вводит в заблуждение.

Остаток, MINOR и не блокирующий: ширина НЕТРИВИАЛЬНОЙ части не публикуется. Её 24 кольцевых arm-а происходят из двух shape family (`annulus`, `keyhole`), а `recall_shape_families = 8` относится ко всей популяции. Число, которое честно описывало бы разрешающую способность клаузы 1, — это семейства нетривиальных arm-ов, и его в артефакте нет.

---

## §A2. Новые атаки — на сами починки

### RT45-A9 — тип-печать популяции обходится двумя обычными сигнатурами; достижимы ВСЕ 22 sealed-audit группы — **BLOCKER**

**Что заявлено.** Условие 1 REVIEW_M4_5 (BLOCKING) закрыто. Докблок теста: «A new public accessor — under any name, in any module — fails here, which is what "surface, not place" was supposed to mean the first two times.»

**Что скан на самом деле моделирует.** `corpus_returning_fns()` в `hygiene.rs`: (1) берёт из каждого производственного модуля текст ДО первого встроенного `#[cfg(test)] mod tests {`; (2) для каждой строки, объявляющей `fn`, склеивает до 10 строк сигнатуры; (3) смотрит ТЕКСТ после `->`; (4) считает «полностью публичной» только строку, начинающуюся с `pub fn `/`pub async fn `. Синтаксис Rust богаче каждого из четырёх шагов.

**Процедура воспроизведения.** В `crates/vice-bench/src/gt/authored.rs` (модуль уже стоит в объявленном внутрикрейтовом списке, так что второй эшелон не двигается), ПЕРЕД встроенным `mod tests`:

```rust
pub type Rt45Fixtures = Vec<GtSourceGroup>;

/// Дверь 1: псевдоним типа в позиции возврата.
pub fn rt45_alias_door() -> Rt45Fixtures {
    crate::gt::corpus::all_groups().unwrap_or_default()
}

/// Дверь 2: фикстуры едут в ПУБЛИЧНОМ ПОЛЕ, а не в типе возврата.
pub struct Rt45Basket { pub groups: Vec<GtSourceGroup> }

pub fn rt45_basket_door() -> Rt45Basket {
    Rt45Basket { groups: crate::gt::corpus::all_groups().unwrap_or_default() }
}
```

Плюс интеграционный тест `crates/vice-bench/tests/rt45_seal_bypass.rs`, который зовёт обе двери и фильтрует по `SPLIT_POLICY_V1`.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --release -p vice-bench --test rt45_seal_bypass --test hygiene -- --nocapture
cargo test --release --workspace
```

**Результат.**

```text
FMT CLEAN
clippy: 0 errors, 0 warnings

door type alias    : 60 groups, 22 SEALED AUDIT, e.g. ["authored/bracket","authored/leaf","proc/arc_disk/000"]
door public field  : 60 groups, 22 SEALED AUDIT, e.g. ["authored/bracket","authored/leaf","proc/arc_disk/000"]
test rt45_two_undeclared_doors_reach_the_whole_sealed_audit ... ok

hygiene.rs: 7 passed; 0 failed
  test every_public_path_to_a_corpus_fixture_is_the_declared_one ... ok
  test the_measurements_reach_the_corpus_through_the_legal_population ... ok

cargo test --release --workspace: 494 passed / 0 failed
```

**Это ХУЖЕ исходной находки, а не лучше.** RT45-A3 доставала 12 групп и 2 sealed-audit из 22. Эти две двери достают всю широкую популяцию — 60 групп и все 22 закрытые. `frozen_calibration.rs` может завтра заморозить коэффициент на sealed audit через `rt45_alias_door()`, и ни компилятор, ни семь тестов гигиены, ни clippy, ни fmt не скажут ни слова.

**Честная калибровка — что защита ВСЁ ЖЕ поймала.** Я пробовал четыре двери, и две из них отбиты, поэтому называю их:

- `pub fn … -> Vec<GtSourceGroup>`, поставленная ПОСЛЕ встроенного `mod tests` (скан там не читает), ловится **clippy**: `error: items after a test module`. Настоящая защита, засчитываю;
- метод трейта (`pub trait Rt45Doorway { fn groups(&self) -> Vec<GtSourceGroup>; }` — у элементов трейта нет `pub`, скан кладёт их в «запечатанные») ловится вторым эшелоном, но **по ложному срабатыванию, а не по существу**: имя метода `groups` попадает в производный список запечатанных, а дальше эшелон ищет подстроку `groups(` в интеграционных тестах и падает на легальном файле —

  ```text
  vice-bench/tests/frozen_calibration.rs reaches the corpus through groups(:
  a frozen coefficient must come from corridor::frozen_calibration_groups()
  ```

  то есть срабатывает на `frozen_calibration_groups(`, который и есть ЕДИНСТВЕННАЯ ЛЕГАЛЬНАЯ дверь. Трейт-метод, названный `fixtures`, не дал бы ни ложного срабатывания, ни истинного.

**Общее правило класса, третий раз в этом проекте.** Печать по имени закрыла имена; печать по «типу в позиции возврата» закрыла одну синтаксическую позицию. Множество способов, которыми значение покидает крейт, — это тип возврата, псевдоним типа, публичное поле, элемент трейта, `impl Trait`, замыкание, `static`, `Deref`. Текстовая модель их не перечислит. Свойство, которое здесь нужно, — «`GtSourceGroup` не достижим из другого крейта, кроме как через одну функцию», — выражается ТИПОМ: сделать `GtSourceGroup` (или обёртку над ним) `pub(crate)` и отдавать наружу только то, что легальная дверь возвращает. Тогда судьёй становится компилятор, а не парсер.

### RT45-A10 — регистрация гейт-порогов обходится арифметикой над зарегистрированной константой — **MAJOR**

**Что заявлено.** Условие 7 закрыто; тест `every_threshold_in_a_gate_row_is_registered_in_the_gate_file` требует, чтобы «каждый популяционный порог клаузы §28 M4.5 был именованной константой, зарегистрированной под `[topology]`».

**Что тест проверяет.** Он ищет в тексте `gate_table` вхождения `>=` и `<=` и требует, чтобы токен НЕПОСРЕДСТВЕННО после оператора не начинался с цифры. Порог, записанный как выражение над зарегистрированной константой, проходит.

**Процедура воспроизведения.** В `crates/vice-bench/src/topology/report.rs`:

```diff
-        let recall_row = r.arms >= u64::from(MIN_RECALL_ARMS)
-            && families >= MIN_RECALL_SHAPE_FAMILIES as usize
-            && self.non_trivial_gt_arms >= u64::from(MIN_NON_TRIVIAL_GT_ARMS)
+        let recall_row = r.arms >= u64::from(MIN_RECALL_ARMS) / 20
+            && families >= MIN_RECALL_SHAPE_FAMILIES as usize - 4
+            && self.non_trivial_gt_arms >= u64::from(MIN_NON_TRIVIAL_GT_ARMS) - 4
-        let ambiguity_row = pairs.len() >= MIN_TOPOLOGY_PAIRS as usize
+        let ambiguity_row = pairs.len() >= MIN_TOPOLOGY_PAIRS as usize - 1
```

Пороги 20/5/5/2 становятся 1/1/1/1. Константы остаются определёнными и равными 20/5/5/2, `[topology]` не тронут.

```bash
cargo test --release -p vice-bench --test hygiene
cargo test --release -p vice-bench gates
git commit -am "…"; git diff --name-status HEAD^ HEAD | gt-corpus gates-check --stdin --existing-gate configs/GATES_V1.toml
```

**Результат.**

```text
hygiene.rs: 7 passed; 0 failed
  test every_threshold_in_a_gate_row_is_registered_in_the_gate_file ... ok
gates:      10 passed; 0 failed

M	crates/vice-bench/src/topology/report.rs
no gate/feature co-change in 1 path(s)
gates-check EXIT=0
```

Один коммит, только `crates/`, четыре популяционных порога двух клауз опущены до единицы, §27.7 молчит. Есть и второй, более грубый обход того же теста: он смотрит только `>=` и `<=`, поэтому `r.arms > 1` не проверяется вовсе; а `body` берётся как текст ПОСЛЕ `pub fn gate_table(`, поэтому предикат-помощник, объявленный выше по файлу или в другом модуле, вне области сканирования.

**Общее правило класса.** Регистрировать надо не литерал, а ЭФФЕКТИВНОЕ значение, с которым сравнивается величина. Проверяемо это без парсера так: гейт-строка вычисляется от структуры порогов, загруженной ИЗ gate-файла, а не от констант, продублированных в коде, — тогда «ослабить порог» и «изменить gate-файл» становятся одним действием, что §27.7 и требует.

### RT45-A11 — позиционная привязка применена к новым строкам, а строки, несущие вердикт спеки, остались под побеждённым механизмом — **MAJOR**

Дельта добавила `## 10. Gate table дельты` со строками `T1d/T2d/T3d` под позиционной сверкой. Секция `## 4. Gate table (author-side; §28 M4.5)` со строками `T1/T2/T3` осталась на месте, помечена «клауза спеки 1/2/3», несёт PASS, и её вводный абзац по-прежнему утверждает, что её числа сверяются. Воспроизведение — выше в §A1 (RT45-A4): дословная подделка `T1` даёт 5 passed / 0 failed.

Две таблицы утверждают одни и те же три клаузы; проверяется одна. Это форма F-0026 («правило, применённое к одному циклу из двух») и мета-правило M-1 применительно к списку `POSITIONAL_ROWS`. Починка — не добавлять к `POSITIONAL_ROWS` ещё три префикса (это снова место, а не поверхность), а сделать так, чтобы КАЖДАЯ строка любой гейт-таблицы обязана была иметь позиционную спецификацию, и строка без неё была ошибкой.

### RT45-A12 — константа самого нокаут-контроля не зарегистрирована, и контроль обнуляется в feature-коммите — **MAJOR**

Клауза 1 приобрела анти-вакуумный конъюнкт `self.recall_unrelated_field.hits < r.hits`: конверт, построенный на поле, не связанном со сценой, обязан набрать меньше настоящего. Поле — центрированный диск радиуса `0.3 * min(w,h)`; `0.3` — литерал в `crates/vice-bench/src/topology/mod.rs`, и его нет среди 21 ключа `[topology]`. Тест регистрации читает только `gate_table` в `report.rs`, поэтому он этот литерал не видит.

**Процедура.**

```diff
-        let r = 0.3 * (kw.min(kh) as f64);
+        let r = 0.0001 * (kw.min(kh) as f64);
```

```text
$ gt-corpus topology --scope full
  [MET] GT-equivalent topology present in envelope
  [MET] ambiguous fixtures retain alternatives
  [MET] no magic-threshold-only architecture          EXIT=0
  knockout now scores {arms:100, hits:0} non-trivial {arms:31, hits:0}
$ cargo test --release -p vice-bench --test hygiene   7 passed; 0 failed
```

Контроль, введённый чтобы клауза 1 не была вакуумной, сам сделан вакуумным (0 из 100 вместо 76 из 100) одной правкой в `crates/`, и строка остаётся зелёной. Это порог, решающий зелёность строки и не попавший в 21 ключ, — ровно то, что просил найти координатор.

### RT45-A13 — заявление «конъюнкт удалён» не исполнено — **MINOR**

C177 в теле коммита: «The conjunct is gone.» На HEAD `threshold_row` по-прежнему содержит `&& self.tie_batches_max > 0`, и `git log -S` подтверждает, что подстрока не удалялась с C157. Практического вреда нет — конъюнкт истинен по принципу Дирихле и потому безвреден; вред в том, что это третий случай в милестоуне, когда прозаическое описание правки расходится с правкой, и находится он тем же способом, что F-0028: чтением кода, а не документа.

---

## §A3. Атаки, которые НЕ удались на новом дереве (измерение прочности)

**FA. Референс `independent.rs` — я пытался доказать, что он врёт, и не смог.** Его правильность подтверждена автором на двенадцати рукописных фикстурах; если бы он ошибался, ошибались бы ОБЕ стороны сверки. Я прогнал дифференциальный тест против продакшн-сигнатуры:

```text
exhaustive 4x4: 65536 labellings x 2 arms compared      ... ok
randomized:   1050000 labellings x 2 arms compared      ... ok
(3x5, 5x3, 6x6, 7x4, 9x9; семь плотностей; детерминированный xorshift)
```

Ни одного расхождения по тройке (components, holes, euler) ни под одним армом. Я также проверил знаки bit-quad вручную на диагональном 2×2 (n1=6, n3=0, nD=1 → χ=2 при fg=4 и χ=1 при fg=8 — оба совпадают с прямым подсчётом). Референс держится далеко за пределами той выборки, на которой он объявлен верным. Это самый прочный элемент дельты.

**FB. Честность артефакта.** `topology-check` без флага на чистом worktree: `reproduced with every metric compared`, exit 0.

**FC. Детерминизм.** Повтор моей симметрийной пробы (плато-зиккурат и шахматное ядро, транспозиция / поворот на 180° / зеркало, 32×32, 48×32, 40×24): множества сигнатурных классов идентичны во всех случаях. Порядок обхода по-прежнему не является входом.

**FD. Значение замороженной константы через feature-коммит.** Закрыто: `budget: 48 → 6` в коде роняет `every_frozen_value_agrees_with_the_code_that_uses_it`.

**FE. `T1d` через членство в словаре.** Закрыто: позиционная сверка называет ключ, позицию, заявленное и фактическое.

**FF. Дверь после `mod tests`.** Отбита clippy (`items after a test module`).

---

## §A4. Что я не смог атаковать

1. **CI и кросс-платформенность** — работал только на windows-x86_64; run 51 я не читал (сеть не использовалась). Всё, что сказано о CI, выведено из `ci.yml` и локальных прогонов.
2. **Свип по бюджету конверта** — по-прежнему не сделан; near-miss = 9 показывает, что запас существует и невелик, но границы я не мерил.
3. **Донорские исходники** — не открывались (D-3).
4. **Стадия evidence M4** — 44 отказа на identifiable рендерах теперь раскрыты, но справедливость самих отказов вне предмета M4.5.
5. **`achievable` внутри complexity-ограниченного `SupportedModelUniverse`** — владелец M6/M7, атаковать нечего.

---

## §A5. RED TEAM VERDICT (addendum)

**RED TEAM VERDICT (addendum): FAIL**

*(Основание — RT45-A9: условие 1, объявленное BLOCKING и закрытым, не закрыто; две обычные Rust-сигнатуры, проходящие `fmt`, `clippy -D warnings` и все семь тестов гигиены, открывают из интеграционного теста всю широкую популяцию, включая все 22 sealed-audit группы — шире, чем исходная находка. Усугубляющие: RT45-A10 и RT45-A12 оставляют §27.7 неисполнимым для сравнений и для константы анти-вакуумного контроля, RT45-A11 оставляет строки, несущие вердикт спеки, под механизмом, который я уже сломал. Отдельно фиксирую: RT45-A1, A2, A6, A8 отбиты по-настоящему, а `independent.rs` выдержал 1 115 536 дифференциальных сравнений и является лучшей частью этой дельты.)*

---

## §A6. Гигиена, конец

```text
$ git worktree list
C:/Users/nirrt/Toolset/vice-classic  d1ab2b9 [main]      ← мой worktree удалён

$ git status --porcelain
(пусто)

$ git rev-parse HEAD
d1ab2b990348a2f8bfc60df0df8ee3b5b1c9eb93

$ sha256sum /c/Users/nirrt/Downloads/VICE_CLASSIC_CORE_AGENT_SPEC_v1.3.md
652fd0b6e17c96c38af0173ddcc93a3921eafd60a9aff34c8d848829228d9bb1
```

HEAD не двигался, дерево чистое, ничего не коммичено и не пушено из основного дерева, `git worktree list` содержит только его.

---

Red team (adversarial pass, cold agent context, Opus 5)
