# Population Growth Model — stock, flow, and the three fertility factors

Answers **#286**: *is growth driven by accumulated food surplus or by food/turn, and does negative
net food stop growth?*

Short answers, from the code as it stands: **accumulated stock only**, and **no — negative net food
does not stop growth, and barely even slows it.** This doc records why that is wrong, the model that
replaces it, and the levers it exposes.

---

## 1. What the model does today

`advance_demographics` (`core_sim/src/systems/population.rs`):

```text
demand        = per_capita_draw × weighted_mouths        // 0.16 × (0.6·C + 1.0·W + 0.8·E)
consumed      = min(demand, larder)
fed_ratio     = consumed / demand
surplus_ratio = min((larder − consumed) / demand, 1.0)   // ← saturates at a ONE-turn buffer
fertility     = birth_rate × fed_ratio × (1 + surplus_bonus × surplus_ratio)
births        = working × fertility
```

Both fertility terms are derived from `food_store` — the **larder**. Nothing in the birth path reads
income. Flow is not an input to this function.

### 1.1 The two failure quadrants

|  | positive net flow | negative net flow |
|---|---|---|
| **fat larder** | thriving *(correct)* | **grows at full rate into a cliff** ← the filed bug |
| **thin larder** | **births at 2/3 rate while perfectly self-sufficient** ← the unfiled mirror | dying *(correct)* |

**Top-right.** `surplus_ratio` saturates once the larder holds **two turns** of demand, so a band
with a 2-turn buffer and a band with a 50-turn buffer birth at the identical maximum rate
`0.03 × 1.5 = 0.045` per working adult. Worked on the shipped 30-person start (C 9 / W 16.5 / E 4.5
→ demand 4.08/turn, `startup.food_reserve_days: 20` → larder 81.6): with income at zero, the larder
stays above the 8.16 saturation bar for roughly **18 of its 20 turns of runway**. The band births at
full speed for eighteen turns of a terminal decline; the brake engages one or two turns before
starvation deaths begin. Growth itself raises demand, so the deficit accelerates into the cliff it
is causing.

**Bottom-left.** A band whose income exactly covers consumption and carries a one-turn buffer gets
`surplus_ratio = 0` → fertility `0.03` instead of `0.045`. It is *fine* — it feeds itself
indefinitely — and the model reads it as poor, purely because it is not hoarding.

### 1.2 Ordering

`simulate_population` runs **before** `advance_labor_allocation` (`lib.rs:713` vs `:722`), so births
read the larder *before* this turn's income lands. The stock signal is a turn stale on top of being
the wrong signal. The new model keeps this ordering and reads last turn's flow telemetry, which is
correct: fertility should respond to the trend a band has been living, not to a single turn's haul.

---

## 2. The model

Fertility becomes a **product of three named factors**:

```text
fertility = birth_rate × hunger × reserve × trend
births    = working × fertility
```

This is not a new pattern. Morale is already a named contributor set (`MoraleContributions` —
"adding a factor is a new variant + one field, not a rewrite") and productivity is already a
multiplicative stack (`output_multiplier` = Π modifiers — "adding an education/tech/government
modifier is one line"). Fertility was the last site still doing ad-hoc inline ratios; this brings the
outlier into line and makes a future itemized client breakdown fall out for free.

### 2.1 `hunger` — did we eat *this turn*

```text
hunger = consumed / demand          (1.0 when demand == 0)
```

Unchanged from today's `fed_ratio`. It is the **gate**: the only factor that reaches 0, so a band
with an empty larder produces zero births regardless of how the other two are tuned.

### 2.2 `reserve` — is there a cushion (**stock**)

```text
reserve_turns = (larder − consumed) / demand
reserve       = 1 + reserve.bonus × min(reserve_turns / reserve.saturation_turns, 1)
```

Same shape as today's `surplus_ratio` term, with the saturation point promoted to a **config lever**
instead of being hardcoded at one turn. `saturation_turns = 1.0` reproduces today's behaviour
exactly; the shipped default is **10.0**, so a band must bank roughly a season of food to earn the
full bonus rather than two turns' worth. Range: `[1, 1 + bonus]` = `[1.0, 1.5]`.

### 2.3 `trend` — is the cushion growing or shrinking (**flow**)

The new factor. Two-sided and centred at 1.0, so it both punishes decline *and* rewards genuine
surplus — a positive feedback loop for good play, not only a stick.

```text
net_flow  = steady_income − demand − pen_feed_upkeep
net_ratio = net_flow / demand                              // dimensionless; ≥ −1 by construction

trend = 1 + trend.surplus_gain     × min( net_ratio / trend.surplus_saturation, 1)   if net_ratio ≥ 0
      = 1 − trend.deficit_penalty  × min(−net_ratio / trend.deficit_saturation, 1)   if net_ratio < 0
```

**`steady_income` is Σ per-source `SourceYield.realized`, not Σ `actual`.** `actual` is the real
arrivals and is lumpy by design — a big-game hunt pays zero for six turns then spikes — and fertility
must not sawtooth with whole-animal timing. `realized` is the forward-projected steady food/turn
built precisely to be the stable headline, and it is a pure function of source state. Using it means
fertility responds to the same trend the player is *shown* on the band panel's Food line.

**Subtracting `pen_feed_upkeep` is what makes this the same quantity as `turnsOfFood`.** The
player-facing runway is `larder / (consumption + penFeedUpkeep − income)`; `net_flow` above is the
negation of that denominator. A band whose panel shows a shrinking runway is exactly a band whose
fertility trend factor is below 1. The two readouts cannot disagree about which direction a band is
heading. `last_pen_feed_upkeep` already lives on `LaborAllocation`, so this costs no new coupling.

Range with shipped defaults: `[0.25, 1.25]`.

### 2.4 Damp, don't stop — and why there is no separate floor lever

The deliberate answer to the second half of #286: negative net food **damps** growth, it does not
stop it. Real subsistence populations do not stop having children on a bad harvest — the deaths do
the work, and this model already has starvation mortality (deficit-capped, per-bracket
vulnerability). A hard flow brake on births *plus* starvation deaths would punish the same bad
stretch twice.

**`trend.deficit_penalty` is the single lever that expresses this.** At the shipped `0.75` a
fully-collapsed band still breeds at 25% of base; **`1.0` lets negative flow stop growth outright**.
One config change, no code change, if playtest says the early game is too forgiving.

An earlier draft of this design also floored the `reserve × trend` product, on the theory that
multiplicative factors compound. They do not compound *here*, and the floor was dropped before it
shipped: `hunger` is the only factor that reaches 0, while `reserve` ∈ `[1, 1.5]` and `trend` ∈
`[0.25, 1.25]` both bracket 1.0, so the product cannot fall below `0.25` at defaults and any floor
below that is inert. Two knobs for one behaviour is worse than one. If a future *penalising* factor
(< 1) joins the stack, revisit — a floor becomes load-bearing the moment two factors can both pull
down.

### 2.5 The no-data rule — an empty schedule is *no data*, never a famine

`last_yields` and `last_pen_feed_upkeep` are **derived, not persisted** — a rehydrated cohort reads
empty/zero until the next tick. Reading empty as "zero income" would suppress births on every
rollback, which is the exact trap already documented for the arrivals schedule in
`larder_runway_turns`. So:

| Cohort state | `steady_income` | `trend` |
|---|---|---|
| No `LaborAllocation` component at all | — | **1.0 (neutral)** |
| Has assignments, `last_yields` empty (rehydrated) | — | **1.0 (neutral)** |
| `assignments` empty (genuinely idle band) | `0.0` | computed — a real deficit |
| Otherwise | Σ `realized` | computed |

The third row is the one that needs care: an idle band with no assignments *also* has an empty
`last_yields`, but that emptiness is real — it produces nothing — so it must be read as genuine zero
income, not as missing data. The disambiguation is `assignments.is_empty()`, and it is pinned by
test.

---

## 3. Config

`core_sim/src/data/demographics_config.json`, `births` block. The flat `surplus_bonus` is **renamed**
into the `reserve` sub-block (the repo carries no shipped saves or clients to keep compatible).

```json
"births": {
  "birth_rate": 0.03,
  "reserve": {
    "bonus": 0.5,
    "saturation_turns": 10.0
  },
  "trend": {
    "surplus_gain": 0.25,
    "surplus_saturation": 0.5,
    "deficit_penalty": 0.75,
    "deficit_saturation": 1.0
  }
}
```

| Lever | Default | Meaning |
|---|---|---|
| `birth_rate` | 0.03 | Base births per working adult per turn. Unchanged. |
| `reserve.bonus` | 0.5 | Max fertility bonus from a full larder. Was `births.surplus_bonus`. |
| `reserve.saturation_turns` | 10.0 | Turns of banked demand earning the full bonus. **1.0 reproduces the old behaviour.** |
| `trend.surplus_gain` | 0.25 | Max fertility bonus from net-positive food. |
| `trend.surplus_saturation` | 0.5 | Net surplus (× demand) earning the full bonus. |
| `trend.deficit_penalty` | 0.75 | Max fertility penalty from net-negative food. **The damp-vs-stop lever: 1.0 = negative flow stops growth.** |
| `trend.deficit_saturation` | 1.0 | Net deficit (× demand) reaching the full penalty; 1.0 = zero income. |

### 3.1 What the defaults do to the shipped start

The 30-person opening band banks 20 turns of demand, so `reserve_turns ≈ 19` saturates the 10-turn
bar → `reserve = 1.5`, unchanged from today. A well-fed band with income covering consumption reads
`trend = 1.0` → **opening fertility is identical to today's**. The change bites exactly where it
should:

| Band | today | new | why |
|---|---|---|---|
| Fat larder, income collapsed to 0 | 0.045 | **0.0169** | `trend = 0.25`, reserve still 1.5 |
| Self-sufficient, 1-turn buffer | 0.030 | **0.0315** | reserve 1.05 but `trend = 1.0` |
| Self-sufficient, 10+ turn buffer | 0.045 | 0.045 | unchanged |
| Strong surplus, fat larder | 0.045 | **0.0563** | `trend = 1.25` rewards provisioning |
| Empty larder (starving) | 0.0 | 0.0 | `hunger = 0` gates the whole product |

The first row is the fix: a band bleeding out now breeds at roughly a third of the rate it used to
while doing so, so it arrives at the cliff with materially fewer mouths.

---

## 4. Scope

**In:** the sim model, the config, and tests. No schema change, no wire change, no client change.

**Out, deliberately:** exporting the three factors for an itemized client breakdown. The player
already sees both *inputs* on the band panel (larder, Food /turn) and the *effect* (population), and
`#286` is a model question. The breakdown is the natural parallel to `MoraleContributions` and should
ship, but as its own slice — filed as a follow-up rather than widening this PR.

**Unrelated hazard found while writing the tests, left alone:**
`DemographicsConsumption::default()` carries `per_capita_draw: 0.03` while the shipped
`demographics_config.json` says **0.16** — a 5.3× divergence. At runtime the JSON wins (it sets every
field), so only `DemographicsConfig::default()` callers see `0.03`, and those are the unit tests. The
worked numbers in §3.1 above are the *shipped* 0.16; the unit tests derive their demand from the
config in hand via the shared `food_demand` helper rather than hardcoding either, so they are correct
either way. Reconciling the two is its own change — a `Default` that disagrees with the data file is
a trap for the next person who assumes a test exercises shipped tuning.

**Known gap:** `steady_income` covers labor-assignment income only. Managed herds and pens *are*
labor assignments (herding is standing labor), so they are counted; any future income path that
credits the larder without a `SourceYield` row would read as a deficit. The no-data rule protects
rollback, not that case — a new income path must add its row.
