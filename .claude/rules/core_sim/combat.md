---
paths:
  - "core_sim/src/combat/**"
  - "core_sim/src/combat_config.rs"
  - "core_sim/src/data/combat_config.json"
  - "core_sim/tests/predators.rs"
---

<!-- Extracted verbatim from lines 54-54;3194-3429 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Combat & casualties, predation

## Config files

| File | Purpose |
|------|---------|
| `src/data/combat_config.json` | **Combat resolver tuning** (Predators Phase 0, `docs/plan_predators.md`; loader `combat_config.rs`, env override `COMBAT_CONFIG_PATH`). The severity constants the pure `combat::resolve_fight` reads — `lethality` (**1.0** — scales the damage every side deals) / **`hit_chance`** (**1.0** — P(one unit's attack lands), drawn **per unit** so variance is *binomial in force size*; `1.0` is an **exact identity**, no draw made and no randomness consumed, which is what keeps the take deterministic, `forecast == actual` an exact identity, and the pre-commit **range a point**) / `disengage_fraction` (**0.5** — a loser past this loss share is driven off, not annihilated) / **`wound_recovery_rate`** (**0.2** — share of its own `durability` a wounded body knits back per turn **out of contact**, the decay half of `combat::DamageLedger`; linear in `durability` so the ledger empties in exactly `ceil(1/rate)` idle turns rather than asymptoting at a sliver, and applied **only** on turns with no contact — see "Damage carries between turns") — plus two dials only the **hunt adapter** reads: **`expedition_danger_multiplier`** (**1.5** — a multiplier on `lethality` applied **only** in the expedition-hunt adapter, never the resident-band path: a detached party is far from home, unsupported and tired, so the same beast costs it more; a deferred general combat-modifiers layer — proximity/fatigue/supply + a home-advantage discount for local hunts — will supersede this flat dial) and **`hunt_injury_damage_per_animal`** (**0.15** — the hunt's own hazard per **animal engaged**, independent of the quarry; see "The hunt itself injures people") — plus **`forecast_range_sigmas`** (**2.0** — how wide the pre-commit forecast's reported band is, in standard deviations of the take's own binomials; a **READOUT** width no resolution path reads, so widening it cannot move an animal. `~95%` of a normal-approximated binomial, i.e. §6.4's *"6–11, likely 9"*; see `yield-forecast.md` → "THE INVARIANT IS RESTATED"). **Resolver tuning, NOT creature identity** (creature stats live on `fauna_config`/`creatures.json`). **Validated** inside `from_json_str` (all seven finite & `> 0`; `disengage_fraction`, `hit_chance` and `wound_recovery_rate` `≤ 1`); a broken invariant is rejected at **error** level (`combat_config.invalid_rejected`) and the builtin used. Not on the hot-reload path. See "Combat & Casualties" |

## Combat & Casualties (Predators arc — Phase 0)

The **combat subsystem** and the first live consumer of the long-inert **Warrior** role. Authoritative
design: `docs/plan_predators.md`. Phase 0 stands up the *seam* — a dangerous hunt is just a *fight*, so
the casualty math lives in a first-class module, never as a bespoke hunt formula.

- **`core_sim/src/combat/` — a first-class module that imports NOTHING from fauna/labor/population**
  (dependency inversion is one-way: fauna/labor → combat, never back). It owns the algorithm and the
  neutral types; domains adapt *into* it. Types: **`CombatStats { attack, defense, durability, range, wariness }`**
  (the shared per-unit body — the *same* struct describes a wolf and a human), `RangeBand` (`Melee|Ranged`,
  persisted-enum, accepted/ignored by the placeholder resolver), `Posture`, `FightPayload { sides,
  terrain, seed }`, `Force { id, posture, contingents }`, `Contingent { kind, count, profile }`,
  `FightOutcome { results, victor, disengaged }`, `ContingentResult { force, kind, killed, wounded }`.
  Callers describe **composition** (contingents), never an aggregate "power" scalar.
- **`resolve_fight(&FightPayload, &CombatTuning) -> FightOutcome` — a PURE function of its payload**
  (deterministic and rollback-safe: any variance is drawn from `FightPayload::seed`, never a shared
  stream). **The attrition model is damage against durability, and the gate is hard**
  (`docs/plan_hunt_through_combat.md` §4.2):

  ```text
  per-strike damage = max(0, attacker.attack − target.defense)    // THE GATE — combat::strike_damage
  damage into a contingent = Σ landed strikes × per-strike damage × lethality
  units down = min(count, damage / durability)                    // combat::units_brought_down
  ```

  Three properties this shape exists to guarantee, each of which a more natural formulation silently
  breaks: **headcount cannot substitute for a weapon** (below the gate the per-strike damage is
  *exactly* `0`, so no quantity of attackers and no length of horizon produces a casualty — §0.2's
  eight hundred bare-handed people and a mammoth); **"many small animals per turn" is not authored**
  (the division is continuous, so excess damage spills to the next unit and 20 damage into
  2-durability quarry brings down ten — rising on its own with a better weapon); and **high-`defense`
  quarry gains super-linearly from a better weapon** (doubling `attack` 20 → 40 takes a `defense 12`
  target's effective attack 8 → 28 while a `defense 0` target's merely doubles).

  **Variance lives in `hit_chance`, drawn per unit** — binomial in force size, so three hunters are a
  gamble and thirty are reliable. It never softens the gate: a sub-gate pairing does not even roll.
  At the shipped `1.0` no draw is made at all.

  **`CombatTuning::draw` decides whether the roll is DRAWN or READ OFF ITS DISTRIBUTION**
  (`combat::StrikeDraw`, `docs/plan_hunt_through_combat.md` §6.4). `Seeded` is a live fight, taking
  its stream from `FightPayload::seed`; `Quantile { sigmas }` is a **forecast**, which makes no draw
  at all and reads the binomial `sigmas` standard deviations from its mean
  (`combat::attacks_landed_at`). Both go through the one `combat::landed_strikes` seam, which is what
  lets a forecast resolve `resolve_fight` itself instead of a second copy of the model. **The
  quantile's identities are the draw's identities** — `chance >= 1` answers `attackers` and
  `chance <= 0` answers `0` whatever `sigmas` says — which is what makes the shipped forecast
  bit-identical to the take rather than merely close (`a_certain_hit_chance_has_no_spread_to_quantile`).
  A forecast has no event seed because `fauna::retreat_seed` needs a tick and a projection cannot know
  a future one; `.claude/rules/core_sim/yield-forecast.md` → "THE INVARIANT IS RESTATED" owns the
  consequence.

  **`ContingentResult` carries `damage_dealt` beside `killed`/`wounded`** — the raw flow that landed
  on that contingent, after `lethality` and *before* the division by `durability`. It exists because
  `killed + wounded` has already been divided and clamped and therefore cannot be inverted, and a
  fight that spans turns has to bank what did not finish a body (below).

  Unchanged from the placeholder: `power(side) = Σ count × attack`; `victor` = strictly-higher power
  (`None` on a tie); the kill/wound split `killed_frac = incoming_per_defender /
  (incoming_per_defender + own.defense)` with `incoming_per_defender = power_enemy / count_self` — so
  **more defenders → more wounded** (a bigger party thins each blow) and **higher own defense → more
  wounded** (the equip-to-shift-severity lever); `disengaged` iff the loser's losses exceed
  `disengage_fraction` of its headcount. `range`/`terrain`/`posture` are accepted and **ignored**
  (reserved for the real resolver). Casualties stay **f32**; the *caller* quantizes (a hunt floors the
  animals it brings down, a cohort's brackets are genuinely fractional).

  **A side's own losses no longer shrink with its own headcount, and that is the model change.** The
  placeholder scaled losses by `power_enemy / power_self`; damage is a quantity the enemy *deals*, so
  a bigger party takes the same absolute damage and spreads it thinner — which the split above then
  reports as a shift toward **wounded**. Mitigation by numbers survives as severity, not as an
  exemption from being hit. Damage is **not** redistributed when a contingent is annihilated (one more
  piece of the placeholder). **Note:** the design prose spelled the split's denominator `power_enemy /
  count_enemy`; with the enemy a single beast that leaves the split constant regardless of party size,
  so the seam divides by the **defenders** (`count_self`).

### Damage carries between turns — `combat::DamageLedger`

**A fight that does not kill this turn is not forgotten** (`docs/plan_hunt_through_combat.md` §4.2).
`DamageLedger { pending, in_contact }` is combat's own state type, so the hunt, a predator raid and a
future TOE-vs-TOE battle bank a multi-turn fight the same way instead of each growing a private meter.

- **Without it the gate is ABSOLUTE rather than steep.** A stateless resolver makes
  `ceil(durability / (attack − defense))` a hard threshold — **63** hunters for a mammoth at the
  shipped spear — so a party of 62 takes casualties every turn and kills nothing on any horizon.
  *"Twenty weak spears and then follow it around for days"* is the intended low-tech experience and it
  needs the days to count for something. **The gate itself is untouched**: below it `strike_damage` is
  exactly `0`, and banking zero forever is still zero.
- **Banking damage is legitimate where banking the ceiling was not**, and the distinction is
  load-bearing. `hunt_credit`'s resident arm was deleted because the escapement ceiling is a **stock**
  and accumulating a stock compounds it; damage is a **flow**, and an accumulator is the correct
  integral of a rate. *A stock must not bank; a rate must.* The long form lives on `DamageLedger`
  itself, because that is the objection a reader will raise at the state, not at a design doc.
- **`strike(damage, profile, standing) -> whole units down`** — banks the damage, hands back only
  **completed** bodies and keeps the remainder, clamped so a blow struck at bodies that are not there
  cannot be banked and spent on the next thing that walks past. Invariant: `pending < durability`.
- **`recover(rate, profile)`** runs in `advance_herds` (Logistics) and heals **only on a turn with no
  contact**; a struck turn merely clears the flag. Logistics precedes Population, so a party that
  keeps hunting never lets a turn of healing through and one that breaks off gets a single turn of
  grace. Healing *every* turn would make `hunters × effective_attack > rate × durability` a **second**
  absolute threshold — the exact shape the accumulator exists to remove. `advance_herds` therefore
  takes `Res<CombatConfigHandle>`; hand-built fauna harnesses must insert it.
- **`Herd::wounds` is the herd-level home** — the animal does not care who wounded it, so two bands
  working one herd wear it down together. It is authoritative sim state and rides the checkpoint with
  the cloned `HerdRegistry` (`SimState::herds`), **both halves**: restoring the damage while dropping
  `in_contact` would grant a free heal after every rollback. Guard:
  `integration_tests/tests/fauna_rollback.rs::a_quarry_s_accumulated_wounds_rewind_on_rollback`.
- **`fauna::herd_quarry_fight(herd, fauna)` is THE seam** between the ledger and the fight —
  `FaunaConfig::quarry_fight_for` alone answers the *species*, i.e. an un-hunted animal, so a take
  path that skipped the seam would silently restart the wounds every turn (the stateless behaviour,
  failing quietly). `QuarryFight::wounds` goes in, `HuntFight::wounds` comes out, and
  `resolve_hunt_fight` stays a **pure** function; the caller stores the ledger back.
- **Every FORWARD PROJECTION resolves the fight INSIDE its loop now.** `project_realized_hunt`,
  `project_arrivals_hunt` and `hunt_trip_forecast` used to hoist it out as a constant, which was right
  for a stateless resolver and is now wrong: a sub-threshold party's first turn is zero and its
  seventh is a whole animal, so a frozen answer quotes **zero forever** for exactly the parties the
  accumulator serves. `project_realized_hunt`'s early break also moved from *"the take was zero"* to
  *"the **source** offered nothing"* — a zero take is now a **wait** turn and stays in the average's
  denominator, matching the `0.0` slots the arrivals schedule already publishes. Guard:
  `hunt_yield_vector::the_forecast_equals_the_paid_take_across_a_multi_turn_kill`.

### The hunt itself injures people

**Casualties used to come only from the animal fighting back**, so on the shipped roster only mammoth,
aurochs and wolf could hurt anyone and a boar cost nothing — contradicting §4.2's own *"survives by
ferocity alone — frail, still costs you people"*. `fauna::hunt_injuries` adds a **baseline hazard**:
`hunt_injury_damage_per_animal × animals engaged × lethality`, put through the resolver's own
`units_brought_down` and **added into** the same `FightCasualties` the fight produces.

- **It scales with the ENGAGEMENT, not the quarry** — more animals worked, more chances to get hurt —
  which is why it is one config lever rather than a per-species field: the danger is in the activity,
  not in the rabbit. It rides `HuntingParty::injury_damage_per_animal` and is scaled by the party's
  `lethality`, so an expedition's `expedition_danger_multiplier` reaches it like every other blow.
- **It is never gated** — a ravine does not have to beat your `defense` to hurt you, so
  `strike_damage` is deliberately absent from it.
- **It is always `wounded`, never `killed`, and that is the gate doing its job rather than an
  exemption.** What makes a blow lethal is an attacker landing it past your defense; a hunt's hazards
  have none. It is also what keeps this *texture*: `available_workers` **floors** a cohort's working
  scalar, so **any** fatality, however fractional, costs a whole worker of throughput on the spot — a
  four-hunter raid losing a quarter of its capacity to a rabbit would be a balance change wearing a
  flavour note's clothes. (Measured: it broke two expedition tests before the fatal share went to `0`.)
- **The `HuntDanger` feed line is now gated on a DEATH** (`fauna::NO_DEATHS_TO_REPORT`), not on
  `FightCasualties::any()`. Every hunt produces some `wounded` now, so `any()` would push a
  "cost N lives" line reading `0` for every band, every turn. The injury is real in the numbers and
  becomes mechanically live with the rest of `wounded` when the recovery slice lands. **The gate did
  not widen when the hunt report landed and must not** — the wounded ride `hunters_wounded` on every
  `hunt_report` instead, which is where a wounded-only turn became visible (`event-feed.md` → "The
  hunt report").

### Strength, behaviour and the roster

- **Strength ≠ danger — `SpeciesDef` splits STRENGTH from BEHAVIOUR.** A big `attack` does **not** make
  a thing dangerous to you (a mammoth is immensely strong but will never raid your camp); danger is
  **DERIVED, never stored**. Four fields, all `#[serde(default)]` (so every existing species is
  byte-identical):
  - **STRENGTH** — `combat: CombatStats { attack, defense, range }` (open-ended, human = 1; defaults to
    `{ attack 0, defense 1, Melee }`).
  - **BEHAVIOUR** — `aggression: f32` (0..1 — P(initiates a raid unprovoked)) and **`ferocity: f32`**
    (0..1 — P(fights back when hunted, vs flees)). `aggression` is *"does it start it"*, `ferocity` is
    *"does it finish it"*.
  - **`diet: Diet` (`Herbivore|Carnivore`)** — the trophic knob. Inert this phase.
  Danger is composed client-side, never a stored scalar: **hunt-danger ≈ `attack × ferocity`**,
  **camp-threat ≈ `attack × aggression`** (Phase 1). `diet` and `aggression` are **inert this phase**
  (Phase 1 consumes them: prey-derived carrying capacity + the predator-raid trigger). `FaunaConfig::validate`
  enforces `combat.attack ≥ 0` finite, **`combat.defense ≥ 0`** finite, **`combat.durability > 0`**
  finite, `0 ≤ aggression ≤ 1`, `0 ≤ ferocity ≤ 1`, `0 ≤ combat.wariness ≤ 1`.

  **`defense 0` is legal and authored; `durability 0` is not** — and the asymmetry is the point.
  `defense` is the gate, so `0` is the meaningful statement *"no protection at all"* that rabbit /
  fowl / grouse / snow hare / catfish carry, and it is the whole of why a **bare-handed** band can
  still eat (`attack 1` clears `0` and nothing else). `durability` is the attrition *denominator*, so
  `0` would let one point of damage bring down every animal in the engagement — a body that soaks
  nothing is not "unprotected", it is absent.
- **The durability roster** (`fauna_config.json`, playtest dials, `docs/plan_hunt_through_combat.md`
  §4.2): mammoth **500**, aurochs **150**, steppe runner / marsh grazer / wild elk **60**, wild horse
  **35**, reindeer / deer **25**, boar / wolf **20**, ibex **15**, seal / crag goat / wild sheep
  **12**, gazelle **8**, grouse / snow hare **3**, catfish / rabbit / fowl **2**; `person` is **20**
  (`creatures.json` — the boar/wolf tier, a body with neither bulk nor armour). **The decoupling from
  mass is real where it matters:** boar and seal are the same body mass and boar is nearly twice as
  durable; wolf is lighter than a wild sheep and tougher; ibex outlasts a seal at less than half the
  weight. The five small-game rows also carry **`defense 0`**, which is what a bare-handed band can
  hunt and nothing else.
- **The graduated roster** (`fauna_config.json`, playtest dials): mammoth `attack 8 / ferocity 0.9`
  (strong AND fights back → deadly), aurochs `4 / 0.7 / aggression 0.1`, boar `1.5 / 0.6` (cornered
  and mean), steppe/marsh grazer `2.5 / 0.4`, elk `2 / 0.4`, horse `2 / 0.3`, seal `1 / 0.3`, down to
  deer `0.8 / 0.15` and gazelle `0.3 / 0.05` (skittish — flee, cost almost nothing). Rabbit / fowl /
  snow hare / grouse / catfish stay fully default (attack 0 → harmless).
- **THE HUNT IS ONE RESOLUTION NOW — the take's kill arm IS `resolve_fight`'s enemy losses**
  (`docs/plan_hunt_through_combat.md` §0.1, slice 4). It used to be **two**: what happened to the
  hunters went through this module while what happened to the animals came out of the party's
  *carrying capacity*, and nothing reconciled them — a party could succeed at the take on one path
  while the other said the mammoth routed it. Both separate danger adapters (the `advance_labor_allocation`
  Hunt arm's and `advance_expeditions`' `Hunting` arm's) are **deleted**; each site now applies the
  band-side casualties that came back **with** the take.
  - **`fauna::resolve_hunt_fight(stayed, hunters, &HuntingParty, &QuarryFight, draw) -> HuntFight`**
    is the single seam, and **all six take/forecast paths call it** — `systems::hunt_take` (resident
    band + scout replenish), `expedition_take_biomass`, `project_arrivals_hunt`,
    `project_realized_hunt`, `forecast_production_and_take` and `hunt_source_yield_preview` — so
    `forecast == actual` per component cannot drift into two answers. `HuntFight { brought_down,
    casualties, fought }`; `brought_down` is **floored to whole animals** and becomes
    `quantise_animal_take`'s fourth bound (see `fauna.md`).
  - **A herd is a `Force`** (§4.1): `stayed` animals map to one `Contingent` at the species'
    `CombatStats` with **attack scaled to `attack × ferocity`** (`QuarryFight::fighting_profile` — the
    strength-vs-behaviour split, unchanged), the party to one `"person"` contingent at the
    **kit-composed** profile (`EquipmentConfig::hunter_profile` — `attack 1` bare, `20` speared).
    `AnimalTake` needed no new field.
  - **Brought down = `killed + wounded`**, not `killed` alone. The split models *recoverable* losses
    for a force that fights again; a hunting party finishes what it puts on the ground. Reading only
    `killed` would apply a silent few-percent haircut varying with party size and break §4.6's
    ceiling arithmetic, which is `min(engage_rate, (attack − defense)/durability) × body_mass` exactly.
  - **The one-sided fast path** (§4.5): a quarry contributing no attack (`attack × ferocity == 0`)
    has structurally-zero casualties, so the payload and the party's `Force` are skipped and
    `fought == false` — which is what the systems read to decide whether a `HuntDanger` line fires.
    It is a **short-circuit, not a second model**: it composes the same `combat::strike_damage` /
    `combat::units_brought_down` primitives `resolve_fight` does, so the kill count is identical.
  - **The dip multiplies the crew here too** — every caller passes `workers × build_dip`, the same
    term `animals_engaged` is handed, or a band mid-Tame would fight at full strength for free.
  - **A pen has no fight at all**: the corral-tend branch passes `f32::INFINITY`, which is returned
    untouched with no casualties and `fought == false`. A penned animal is not stalked, not fought
    and not wary.
  - **Restraint is free**: the escapement floor bounds **`engaged`**, not `killed`
    (`fauna::animals_affordable`), so a crew at its floor does not engage, take casualties and wear
    its kit for animals it was never going to keep. It cannot move the take — the quantiser clamps by
    the same `affordable` regardless — which is why the forecast paths omit it.
  - **A detached party fights at the `expedition_danger_multiplier`-scaled lethality** (that rides
    `HuntingParty::tuning`), and **now wears its own kit**: `advance_expeditions` queries
    `&mut BandEquipment`, resolves both tiers through the same `EquipmentConfig` seams a resident band
    does, and charges `wear_hunting` per animal killed + `wear_carry` per biomass hauled. Before slice
    4 a raid ran on free, immortal equipment.
  - `killed` comes out of the cohort's **working-age** bracket via
    `PopulationCohort::apply_combat_casualties`; `wounded` is computed and surfaced but mechanically
    inert (recovery is a later slice). The `CommandEventKind::HuntDanger` feed line is unchanged —
    label names the **species**, detail carries the fractional `killed=<k> wounded=<w> species=<s>`.
  - **The gate cuts both ways, and it narrowed hunt DEATHS sharply.** An animal must clear a human's
    `defense 1` to *kill* anyone, so at the shipped roster only **mammoth** (`8 × 0.9 = 7.2`),
    **aurochs** (`4 × 0.7 = 2.8`) and **wolf** (`3 × 0.8 = 2.4`) can cost a band lives. A boar
    (`1.5 × 0.6 = 0.9`) and a deer (`0.8 × 0.15 = 0.12`) kill **nobody** — where the retired
    power-ratio model gave every positive attack some casualties. **They still cost you people**, via
    the baseline injury risk (see "The hunt itself injures people"), which is what closes §4.2's
    "survives by ferocity alone" gap; the levers for the *fatal* half are `ferocity`, the species'
    `attack`, or `person.defense`, all config.
- **Warrior stays inert in Phase 0.** Warriors are a band-wide **standing guard** (border/camp patrol),
  not a hunting escort — they do **not** mitigate hunt danger (the hunting party answers that itself,
  via its own equipment). Its labor arm remains a no-op branch. **Its first live consumer is the
  Phase 1 predator-raid path** — a carnivore with `aggression > 0` raiding a band, band as Defender.
- **The wire carries the RAW combat components, not a stored `danger`** — danger is derived, so
  `HerdTelemetryState` publishes `attack` / `defense` / `ferocity` / `aggression` (append-only, from
  `snapshot/subsistence.rs`'s `herd_snapshot_entries` via `fauna.species_by_display`) and the **client
  derives** hunt-danger (`attack × ferocity`) and camp-threat (`attack × aggression`) itself. There is
  **no stored `danger` scalar** (a big `attack` alone is strength, not danger) and **no TileState danger
  field** — the overlay projects the derived per-herd value onto tiles client-side. `HerdTelemetryEntry`
  is untouched. **Client follow-up:** the native reader + band-panel/overlay display of the four
  components are a separate client-dev task.

Tests: `core_sim/src/combat/mod.rs` unit tests (even fight, 5:1, adding-defenders mitigation + wounded
shift, defense→wounded shift, determinism, zero-attack → zero casualties, **the gate draws no blood**,
**spillover is exact and rises with the weapon**, **the shipped `hit_chance` is seed-independent**,
**variance shrinks as the force grows**, **no seed lets a sub-gate force through**, **the ledger turns
sub-threshold damage into a kill / never banks damage the bodies could not soak / cannot accumulate a
sub-gate party's nothing**, **a result carries `damage_dealt`**);
**`core_sim/tests/hunt_fight.rs`** (the take-side properties: no headcount of bare hands kills a
mammoth, over any horizon; a better weapon pays off on big game and not on small; no weapon tier beats
`engage_rate × body_mass`, swept over the whole roster with per-species liveness; the kill rate
responds to party, weapon and quarry; a fractional engagement reaches one animal and fails at the
*fight*; **a harmless quarry is no battle but still hurts someone**; the fast path agrees with the full
resolver; a pen has no fight at all; hunt ordering does not change outcomes at a live sub-1
`hit_chance`; **a sub-threshold party kills after enough turns**, **more hunters shorten the wait**,
**wounds decay out of contact but not instantly**, **the baseline injury wounds and never kills**,
**it tracks the engagement and never dominates a real fight**); `core_sim/tests/predators.rs`
(a mammoth hunt costs working-age lives with a killed/wounded split; a **rabbit** hunt — ferocity 0 —
costs nobody; ferocity scales hunt-danger; config-validation rejections including ferocity); `core_sim/tests/expedition_hunt.rs`
(a hunting expedition takes casualties against a mammoth; the `expedition_danger_multiplier` scales
losses). **Note:** `hunt_trip_forecast` still does not model casualties (it projects the take, and the take's
*kill* arm is the fight — the party's own losses are not fed back into the projected party size), so
`the_raid_forecast_matches_a_real_party_run` hunts a harmless species. Wiring casualties into the
launch forecast remains a follow-up, and the resident-band preview has the same gap:
`hunt_yield_vector::the_forecast_equals_the_paid_take_across_a_multi_turn_kill` has to re-read the
band's live head count each turn, because a mammoth hunt shrinks it under the run while
`hunt_source_yield_preview` quotes whatever staffing it is handed.

See Also: `docs/plan_predators.md` (the whole arc), "Fauna & Wild Game" (the `SpeciesDef` table + the
Warrior role), "Population & Demographics" (the `death_fraction`/bracket seam casualties apply at).

### Predation (Phase 1a) — carnivore herds

**A predator is an ordinary `Herd` whose food layer is *other herds* (prey) instead of the per-tile
`GrazeRegistry`** — the trophic transpose of the grazer model. `SpeciesDef.diet` (`herbivore` |
`carnivore`, inert since Phase 0) is now consumed at the **one K seam**. Design: `docs/plan_predators.md`
§ "Phase 1a"; **1b (the raid trigger + Warrior) is a later PR — untouched here.**

- **Diet-branched carrying capacity** (`fauna::ecological_carrying_capacity`): an herbivore is unchanged
  (graze path); a **carnivore**'s `K_pred = Σ_prey prey_sustainable_flow(prey) / prey_per_biomass` over
  the prey herds in its **prey-sensing disk**, ignoring graze / `fodder_per_biomass` /
  `herd_density_gain` entirely. `prey_sustainable_flow(B, cap, r)` is `graze_sustainable_flow`'s exact
  logistic shape against the **prey herd's own** `regrowth_rate`, read at the prey's **current** (drawn-
  down) biomass — so a thinned prey base lowers `K_pred` (the coupled feedback). `SpeciesDef.prey_per_biomass`
  (`#[serde(default)]` 0.0, inert for herbivores) is the carnivore analog of `fodder_per_biomass`;
  `validate` requires a carnivore's `prey_per_biomass > 0` **and** `combat.attack > 0` (a predator that
  clears no defense is incoherent).
- **Prey = herbivore herds whose `defense ≤ predator.attack`** — the pure `attack ≥ defense` rule
  (idea 7: a wolf's `attack 3` never counts a mammoth's `defense 12` or an aurochs' `6`), no `is_prey`
  flag. This is **one definition, three readers**: the clearance comparison lives in the single
  `attack_clears_defense` helper, and "which herds are prey candidates" (herbivore, with cached
  `defense`) lives in `build_prey_index`/`PreyDatum` — so the carnivore `K`, `advance_predation`, and
  the prey-derived spawn count never carry a second, divergent predicate. The **prey-sensing disk**
  (`predators.prey_sense_radius`, default **4**) is deliberately **wider** than a graze footprint (0–1)
  because prey are sparse points; a graze-sized disk would contain zero prey most turns and snap `K→0`.
  It was **widened 3 → 4** because a pack roaming transiently out of prey range got `K→0` and was
  clamped away — measured ~45% of packs despawning within 10 turns at radius 3; the 61-tile disk (vs 37)
  cuts those transient-zero-prey despawns (the deeper fix is Phase-2 prey-pursuit).
- **The cross-herd borrow.** `ecological_carrying_capacity` runs inside `advance_herds`' `iter_mut`
  loop, so it cannot read the other live herds. `advance_herds` snapshots a **prey index**
  (`Vec<PreyDatum>`, one per herbivore herd) in an immutable pass *before* the loop — start-of-turn prey
  biomass, the same one-turn lag graze `K` has — and passes it in.
- **`advance_predation`** (new system) mirrors `advance_herd_grazing` with prey herds as the layer: each
  carnivore demands `prey_per_biomass × biomass` and draws it from the in-range prey herds it can clear,
  **proportional to each prey herd's available biomass** (above the functional-response floor
  `predators.predation_escapement_fraction × prey.cap`, default **0.15**) — the taper that makes a pack
  take less as prey thins and stop before zero. Index-based over the herd Vec (predator `i` mutates prey
  `j`, always distinct), deterministic in registry order. **Credits no food to anyone** (a wolf's dinner
  is abstracted biomass). Registered in Logistics **after `advance_herd_grazing`, before
  `advance_graze_regrowth`**.
- **Idea 6 falls out of shared machinery:** a pack with no prey in its disk gets `K_pred → 0`,
  `regrow_biomass`'s `clamp(0, cap)` drives its biomass to 0, and the existing extinction `retain`
  despawns it — no game, they leave/die.
- **The dedicated predator pass** (`spawn_predators`, called from `spawn_initial_herds` **after both
  prey passes**): same winner-collection → shuffle → greedy-`min_spacing`-spaced placement as
  `spawn_short_range_game`, drawing **only carnivore** species, so predators are rare and do **not**
  consume the `abundance.max_total_game` prey budget. Predator ids carry the `pred_` prefix. Carnivores
  are filtered **out** of the herbivore short-range pool *and* `repopulate_fauna` immigration
  (`game_species_for_biome` is herbivore-only; `carnivore_species_for_biome` is its twin), so a predator
  seeds **once** and does not respawn.
- **The count is PREY-DERIVED, not a fixed cap** — `max_packs` is **gone**. Each carnivore species
  carries a **target** = `round(eligible_prey_herds × SpeciesDef.prey_ratio)` — its own prey set (every
  herbivore herd its `attack` clears, map-wide, counted once both prey passes have run) × its own ratio,
  because a predator population is *defined by* its prey base. A winning tile seats one of the carnivore
  species hosting its biome **whose per-species target is not yet met** (uniform among them, as before),
  and the loop ends when every species' target is met or the winners are exhausted. For the single
  shipped carnivore this is "place up to `target` packs", but it generalizes to N predators (a future
  big cat with its own `prey_ratio`/prey set). **Placement can cap below the target on prey-rich maps**:
  measured on the 6-seed standard sweep the wolf target is 10–11 (prey herds 97–108, `prey_ratio 0.10`)
  and packs come in `11 / 6 / 10 / 11 / 10 / 11` — five hit the target, seed 2 is placement-starved (the
  shipped `min_spacing 6` + low per-biome probabilities cannot seat 11 well-spaced packs there). The
  lever to close that gap, if wanted, is `min_spacing`/`predators.per_biome`, not the target.
  (Guard: `predators::the_predator_count_scales_with_the_prey_base`.)
- **Spawn is PREY-GATED — no stranded pack** (idea 6 at spawn). A winning tile only seats a carnivore
  species whose **prey-derived `K` at that tile reaches its `min_spawn_biomass()`** (the low end of its
  `biomass` range) — the gate lives in `spawn_predator_group_at`'s candidate filter, measured with
  **`carnivore_k_at(pos, attack, prey_per_biomass, prey_index, prey_sense_radius, …)`**, the *same*
  position-parameterized formula the live per-turn `K` reads (`carnivore_carrying_capacity` now
  delegates to it), so the spawn gate and the running `K` can never diverge (DRY, and the prey rule
  stays the single `attack_clears_defense`). Without it a pack drops onto an isolated tundra tile with
  no game in reach, gets `K ≈ 0`, and despawns almost immediately (observed live: a wolf stranded on
  Tundra with the nearest game several tiles off across water). On a prey-sparse map this places
  **fewer** than the derived target — correct: a viable pack near game beats a stillborn one. **Measured
  a no-op on the standard-map sweep** (prey is dense enough that every winning wolf-host tile already
  clears the gate — counts still `11 / 6 / 10 / 11 / 10 / 11`); it only bites where prey is genuinely
  sparse. (Guard: `predators::every_spawned_predator_lands_where_the_prey_can_feed_it`.)
- **The wolf row** (`fauna_config.json`, playtest anchors): `Grey Wolf Pack`, `diet carnivore`,
  `combat { attack 3, defense 3 }`, `prey_per_biomass 0.3`, **`prey_ratio 0.10`** (a pack per ~10 prey
  herds), `regrowth_rate 0.15`, `ferocity 0.8`, `aggression 0.6` (set now, **inert until 1b**),
  `husbandry_ceiling wild`, hosting savanna / temperate-forest / boreal / highland. `attack 3` fixes its
  prey set for free. `FaunaConfig::validate` requires a carnivore's `prey_ratio` finite `> 0` (a `0`
  seats no packs). Plus a **`predators` config block** (`per_biome` / `min_spacing` /
  `predation_escapement_fraction` / `prey_sense_radius`, all validated — no `max_packs`).
- **The prey-sense ring is on the wire** — `HerdTelemetryState.preySenseRadius:uint` (append-only,
  strictly after `aggression`; `snapshot/subsistence.rs`'s `herd_snapshot_entries`) =
  `fauna.predators.prey_sense_radius` when the herd's species is a **carnivore** (`def.diet ==
  Diet::Carnivore`, resolved via `species_by_display`), else **0**. So `preySenseRadius > 0` is BOTH the
  client's "this is a predator" signal AND its view-ring radius: a carnivore's graze-range ring is
  meaningless (it hunts other herds), so the client draws a prey-sense "view" ring of this radius
  instead; a herbivore reads 0 and keeps drawing its `grazeRangeRadius` ring. **Client half (a separate
  task):** the native reader + the view-ring render. Guard:
  `snapshot::tests::herd_snapshot_reports_prey_sense_radius_for_carnivores_only`.

### Prey pursuit (Phase 2) — a wild carnivore steps toward its dinner

**A wild carnivore now `pursue`s the nearest clearable prey** (the Phase-2 movement primitive) instead
of roaming toward grass, so a pack **tracks a moving herd** rather than idling on empty ground. This is
the deeper fix for the transient-zero-prey stranding that widening `prey_sense_radius` 3→4 only
band-aided, and it makes raids dynamic (a pursuing pack closes on camps). The movement lives on the
fauna side — it is the trophic transpose of `drift_to_owner`, reusing the shared prey rule
(`attack_clears_defense` over `prey_index`, **not** `HerdDensityMap`) so a wolf chases only prey it can
actually eat, out to the wider `predators.pursuit_radius` (default **8**, vs the feeding disk's 4). See
`fauna.md` → "Herd movement is a rung primitive" (the `pursue` bullet) for the mechanism, ordering,
one-turn prey-position lag, and config.

### Predator raids (Phase 1b) — the raid trigger + the Warrior goes live

**A carnivore with `aggression > 0` within `predators.raid_radius` of a resident band raids its camp**,
and the band is defended by its **Warriors** — the long-inert Warrior role's **first live consumer**.
Design: `docs/plan_predators.md` § "Phase 1b". `SpeciesDef.aggression` (set on the wolf row since
Phase 0, inert until now) is the trigger; `diet` gates it to carnivores.

- **`advance_predator_raids`** (`systems/labor.rs`, sibling of the Phase-0 hunt-danger adapter) runs in
  the **Population stage right after `advance_labor_allocation`** (so warrior counts and band positions
  are current) and **before `advance_population_migration`**. For each `ResidentBand` × each carnivore
  herd, if the pack's **raid attack `= combat.attack × aggression > 0`** (aggression 0 ⇒ no raid) and it
  is within `predators.raid_radius` (odd-r `hex_distance_wrapped`) of the band, it builds a
  `FightPayload`, resolves it through the neutral `combat::resolve_fight`, and applies **only the
  band/defender side's** casualties — **working-age only this phase** (`cohort.apply_combat_casualties`,
  one mutation per band; multiple raiders are additive/order-independent). `wounded` is surfaced in the
  feed but mechanically inert (recovery is a later slice, exactly as in Phase 0).
- **The band side is TWO contingents, and that is load-bearing** — the placeholder resolver clamps a
  side's losses to *its own* headcount, so a `count 0` side takes ZERO losses. A warriors-only band side
  would therefore give a **0-warrior band zero casualties** — the exact inverse of "an under-guarded
  band costs it people". So the fight is: **Aggressor** = one representative of the pack
  (`count 1.0`, profile `combat` with `attack = attack × aggression`) — a Phase-1b simplification that
  keeps `power_enemy` modest so a handful of warriors can meaningfully cut the loss ratio (the whole pack
  engaged would make every raid a massacre); **Defender** = the band's **Warriors** (`count =
  workers_on(Warrior)` clamped to working-age, profile = the creatures roster's `person` — the armed
  defenders that add power and shift the split toward wounded) **plus the exposed populace**
  (`count = min(predators.raid_exposure, working_age − warriors)`, profile `{ attack 0, person.defense,
  person.range }` — the unarmed folk that can die and dilute the blow but add no offense). The seed is
  rollback-stable and distinct per (predator, band) pair (both the herd id and the band entity hashed).
- **Config** — two `#[serde(default)]` levers on `PredatorConfig` (`fauna_config.json` `predators`
  block), both playtest dials: **`raid_radius`** (`2` — how close the pack must be to raid; its own
  lever, deliberately tighter than the `prey_sense_radius` disk) and **`raid_exposure`** (`4.0` — how
  many working-age folk stand exposed beyond the warriors, bounding a raid to a skirmish).
  `FaunaConfig::validate` rejects `raid_radius < 1` and a non-finite/`<= 0` `raid_exposure`.
- **Feed** — `CommandEventKind::PredatorRaid` (`"predator_raid"`, server label "Predator raid") fires
  each casualty-causing raid turn: label names the **species** (`"A {species} raid cost {N} lives"`),
  detail carries the fractional `killed`/`wounded` + `warriors` + `species`. Edge-gating a repeated raid
  to one line is deferred to Phase 3.
- **The Warrior labor branch stays a no-op in the labor pass** (warriors do no per-worker yield) but is
  **no longer inert overall** — the warrior head-count is consumed here as the band's defending
  contingent. Tests: `core_sim/tests/predator_raid.rs` (unguarded band bleeds + narrates; warriors cut
  losses; no raid from a herbivore or an out-of-range carnivore; aggression scales lethality;
  determinism) + the two `fauna_config` rejection tests.

#### Phase 3 — raids forfeit food + the raid legibility pair on the wire

**A casualty-causing raid now also forfeits food** (`docs/plan_predators.md` § "Phase 3"). The band's
people were defending or fleeing, not gathering, so a raid that costs lives (`total_killed > 0`) also
costs a fraction of **that turn's food income** — a real larder debit, capped at what the larder holds.

- **In `advance_predator_raids`** the band query is now `&mut LaborAllocation`. Each band's
  `allocation.last_raid_forfeit` is **reset to `0.0` at the top of its iteration** (this system is its
  only writer, so a non-raided band reads `0`). After the raid loop, if casualties occurred:
  `income = Σ alloc.last_yields[i].actual` (this turn's income — `advance_labor_allocation` ran earlier
  in the Population stage and already credited it), `forfeit = predators.raid_yield_forfeit_fraction ×
  income`, debited via `cohort.stores.take(FOOD, forfeit)` (whose **return** — the actually-taken
  amount — is recorded in `last_raid_forfeit`, so a thin larder forfeits only what it held and an idle
  band with `income == 0` forfeits nothing). The forfeit is folded into the `PredatorRaid` feed line's
  detail (` forfeit=<f>`); the feed lines are **deferred** so the band-level forfeit can be appended
  before they are pushed.
- **`predators.raid_yield_forfeit_fraction`** (`fauna_config.json` `predators` block, default **0.25**,
  a playtest dial) — `#[serde(default)]` on `PredatorConfig`; `FaunaConfig::validate` rejects
  not-finite or outside `[0, 1]` (`validate_rejects_an_out_of_range_raid_yield_forfeit_fraction`).
- **`LaborAllocation.last_raid_forfeit: f32`** is a **derived, per-turn, NOT-persisted** field treated
  exactly like `last_pen_feed_upkeep` (excluded from the manual `PartialEq`, absent from serde/rollback
  state, defaults `0.0`).
- **Two new `PopulationCohortState` wire fields** (append-only, after `fodderStore`), captured in
  `snapshot/population.rs`:
  - **`raidRadius:uint`** — echo of `fauna.predators.raid_radius`, surfaced per-cohort exactly like
    `workRange` (a global lever the client needs per-band to check whether a visible aggressive predator
    is within exact raid range).
  - **`raidForfeit:float`** — `last_raid_forfeit`, a negative food-ledger line, the raid twin of
    `penFeedUpkeep`.
- **The ledger identity gains a term** (see `campaign.md`): the forfeit is a real larder debit in
  neither `foodIncome` nor `foodConsumption`, so
  `larder_delta == foodIncome − foodConsumption − penFeedUpkeep − raidForfeit`, pinned through a real
  raid turn by `integration_tests/tests/raid_food_ledger.rs`. **`raidForfeit` is a PAST-turn stochastic
  debit, NOT a recurring cost** — it is deliberately **absent** from the `turnsOfFood` forward-runway
  drain (`larder_runway_turns`), which drains only by `consumption + penFeedUpkeep`.
- Tests: `core_sim/tests/predator_raid.rs` gains the working-band-forfeits / idle-band-forfeits-nothing /
  forfeit-capped-at-larder cases; the config rejection rides `fauna_config.rs`'s unit tests.
- **Client half — LANDED (this PR).** The native reader (`native/src/dict/population.rs`) decodes
  `raidRadius`/`raidForfeit` into the cohort dict; the client's extensible `KIND_STYLE` (then on `CommandFeedController`, now `HudEventVocab`)
  table styles the threat/casualty feed events (`predator_raid` → ⚔ crimson, `hunt_danger` → ⚠ amber,
  reusing the `HudStyle` palette so the accent matches the map-overlay hues); the Warrior card shows a
  live **"⚠ Predator nearby — N on guard"** warning (a visible camp-threat predator within `raidRadius`
  of the band); and the `raidForfeit` **"⚔ Lost to raids"** negative food-ledger line renders beside
  `penFeedUpkeep`. ui_preview fixtures `predator_feed` / `predator_band_raided` pin both. **NOTE — Phase 4
  (`docs/plan_predators.md`) replaces this client-side `raidRadius` proximity check with a
  visibility-gated, server-computed per-band alert.**

---

