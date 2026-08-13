---
paths:
  - "core_sim/src/{fauna,fauna_config,intensification}.rs"
  - "core_sim/src/data/intensification_ladder.json"
  - "core_sim/tests/{fauna_husbandry,grazing_2d_pen,rollback_tended_survival}.rs"
---

<!-- Extracted verbatim from lines 1589-2062 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Husbandry — the yield ladder, the `Tame` verb, Corral

## The husbandry yield ladder — every rung pays MSY

Authoritative design: `docs/plan_corral_managed_population.md`. **Management buys a *growth rate*, not
a licence to eat the standing stock.** Every rung of the ladder pays the Maximum Sustainable Yield; the
rungs differ *only* in the **ecology** that MSY is computed against, and in what that ecology costs you.
**Every rung costs a worker** (intensification ladder §3): what climbing buys is **yield per worker**,
not a rung that works itself.

**The husbandry ladder scales BOTH `r` AND `K`, on two orthogonal per-species dials.** `r` (the growth
*rate*) climbs on the global gains `husbandry.pastoral_gain` / `pen_gain` (folded per-species by
`herd_ecology`). `K` (the carrying *ceiling*) climbs on the per-species **density** dials
`SpeciesDef.pastoral_density` / `pen_density` (`fauna_config.json`, default **1.0** = neutral, so a wild
herd is byte-identical) — *domestication makes the land hold more animals, non-linearly by species*.
Without the density axis a species on marginal range (a goat penned at `K≈24`) stayed tiny even penned
and a fast wild breeder out-yielded the prime domesticates at every rung, because taming touched only
`r`. The gain for the herd's **current rung** is `fauna::herd_density_gain` (corralled → `pen_density`,
tamed → `pastoral_density`, wild → `1.0`; mirrors `herd_ecology`'s dispatch, resolved live by display
name via `FaunaConfig::pen_density_for` / `pastoral_density_for`), applied at the **one K seam**
`ecological_carrying_capacity` — it multiplies the final range/footprint-derived `K` (`Some(flow /
fodder × gain)`), so it is recomputed fresh each turn (idempotent, never a compounding read). Roster:
crag_goat/aurochs **2.0 / 5.0**, boar **1.5 / 4.0**, rabbit/fowl **1.1 / 1.5**,
steppe_runner/marsh_grazer **1.5 / 1.0** (pastoral only, so `pen_density` is inert), deer/mammoth omit
both (wild ceiling → always `×1.0`). Validated finite & `>= 1.0` (a gain below 1 would make
domestication *reduce* capacity). **Playtest dials.** The density axis is **scale-free in the pen's
net-positive floor** (`K` cancels in `r·K/4·p` vs `u·K·(2+r)/4`), so it does not interact with that
invariant.

> #### The ladder is monotone in the LONG-RUN rate, NOT in any single turn — do not "fix" this back
>
> A **Sustain** hunt is **constant escapement on whole animals**: the herd hands over `B − K/2`, which
> is a **stock**, not a rate. At `B = K` Sustain's escapement is `K − K/2` = **`K/2` for every rung —
> `r` cancels out entirely**, so a full herd's first harvest is *identical* wild, pastoral and penned.
> **That is correct and load-bearing.** The surplus standing above the escapement point is
> *accumulated stock*, and stock does not care how fast you breed. What the ladder buys is that **the
> next animal comes sooner** — so *"management buys a growth rate"* is now literally and exclusively
> true, rather than being smeared across a stock term.
>
> **`docs/plan_harvest_floor.md` slice 1 made this true of the WILD hunt too, and changed nothing
> here.** It generalised the pen's harvest rule to a floor the stance names (`Surplus 0.30·K`,
> `Deplete 0.15·K`, `Eradicate 0`), so every rung on both webs is now the shape this callout already
> described. What the arc **did** change is which comparisons are meaningful: a stance ceiling is a
> stock and a rung payoff (`pastoralYield`, `corralYield`) is a long-run rate, so the two cannot be
> ordered against each other at a single turn — the ladder lives on the payoff axis, and the 600-turn
> average is where it is measured (`the_husbandry_ladder_is_a_per_species_growth_rate_ladder`, and the
> plan's own `stance_probe` property tests).
>
> A single turn therefore cannot see the ladder at **either** biomass: at `B = K` you read the
> rung-blind stock, and at `B = K/2` you read a **pulse** — zero for any species whose one-turn MSY is
> lighter than one animal (a wild mammoth regrows 120 biomass against an **800-unit** body, so it
> correctly **waits** ~7 turns, then pays 16 provisions at once). Both readings are honest; neither is
> the ladder.
>
> So the invariant is asserted as a **long-run average over enough turns to contain the refills**
> (`fauna_husbandry::the_husbandry_ladder_is_a_per_species_growth_rate_ladder`, 600 turns from `B*`),
> where the pulses and the stock both wash out and what remains is `r·K/4`. **Measured**
> (provisions/turn, barren harness ⇒ the pen is fully larder-fed):
>
> | species | `K` | wild | pastoral | pen gross | upkeep | pen net |
> |---|---|---|---|---|---|---|
> | Rabbit Warren | 200 | 0.350 | 0.700 | 1.000 | 0.280 | **0.720** |
> | Red Deer | 1200 | 0.598 | 1.196 | 2.392 | 1.432 | **0.960** |
> | Thunder Mammoths | 12000 | 2.373 | 4.746 | 9.492 | 13.506 | **−4.014** |
>
> Monotone in gross at every row, and `pastoral / wild` is exactly `pastoral_gain` (2.0). The **rabbit
> pen gross rides the cap** (`r_pen = min(husbandry_regrowth_cap 1.0, 0.35 × pen_gain 4.0) = 1.0`), so
> its pen/wild ratio is `1.0/0.35 ≈ 2.86`, not the full `pen_gain` — a fast breeder is clamped into the
> stable logistic band, the cap's whole job. The mammoth's negative *barren* pen net is the §2.4
> slow-breeder loss **by design** (a placement decision — on real pasture the footprint feeds it and
> `upkeep → 0`), not a regression.

| Rung | Ecology | `r` (Grazing 2d — **per-species**) | Costs |
|---|---|---|---|
| Wild, Sustain hunt | `ecology` | `wild_r` (rabbit 0.35 · deer 0.10 · mammoth 0.04) | a worker |
| Mobile domesticated (**pastoral**) | `husbandry.pastoral.ecology` | `min(cap, wild_r × pastoral_gain)` (gain 2.0) | **a worker** (a Hunt assignment, like a wild herd — passive-free pastoral is retired) |
| Corral, building | unchanged — the hunters beside the build carry what hunters carry | — | **the build's own crew** (`corral … <workers>`, hands not hunting) and **75 work units** (not a fixed turn count — the crew is the throughput, so the turns move with the number the player typed) |
| Corral, finished (**pen**) | `husbandry.pen.ecology` | `min(cap, wild_r × pen_gain)` (gain 4.0, cap 1.0) | a worker + **feed (footprint-offset)** + pinned |

- **Grazing 2d retired the flat pastoral 0.25 / pen 0.90.** The managed rungs now scale each species'
  **own wild `r`** by `husbandry.pastoral_gain` (2.0) / `pen_gain` (4.0), clamped to
  `husbandry_regrowth_cap` (1.0) — a penned rabbit (`r` 1.0, cap-bound and booming) and a penned mammoth
  (`r` 0.16, a long-haul investment) are different economies. This also fixes the fast-breeder pastoral
  inversion (pastoral `r` = `wild_r × 2.0 > wild_r` for every species). `fauna::herd_ecology` folds the per-species
  rate in; `pen_ecology_for` / `pastoral_ecology_for` are the seams, `managed_regrowth_rate` the `wild_r ×
  gain → capped` map.
- **A penned herd's `K` is its FENCED FOOTPRINT's graze flow** (`hex_range_tiles(corralled_at,
  pen_radius)`), recomputed each turn — penned herds are no longer frozen and `pen.capacity_fraction` /
  `pen_capacity` are **deleted** (a penned herd's `K` is just `herd.carrying_capacity`, so
  `herd_capacity` collapses to that field for every herd). A penned herd **grazes its footprint**
  (escapement-floored, like a wild herd) and the grass it eats **offsets its keeper's larder bill**:
  `larder_upkeep = pen.upkeep_per_biomass × biomass × (1 − pasture_fraction)`, `pasture_fraction =
  clamp(footprint_intake / (fodder_per_biomass × biomass), 0, 1)`. A pen on lush steppe feeds itself for
  free (`pasture_fraction → 1`, larder → 0); a **wholly-barren** footprint keeps the herd's frozen `K`
  and pays the full larder bill (the pre-2d worst case, preserved). See "Phase 2d".
- **`fauna::herd_ecology(herd, fauna)` and `fauna::herd_capacity(herd, fauna)` are THE single source of
  that mapping.** `regrow_biomass`, `hunt_escapement_ceiling` (capacity only — the take reads no
  ecology at all), `hunt_forecast`, `refresh_ecology_phase`,
  the expedition ceiling/bound/simulation — **every** consumer resolves through them. **No call site may
  re-derive an ecology or a capacity**: a second copy of this mapping is exactly how a forecast starts
  promising a number the take won't pay (see "Pre-commit Yield Forecast").
- **The managed harvest draws the herd down**, and that is what makes it sustainable: it converges the
  herd on `K/2` and holds it there, paying `r·K/4` forever. Both husbandry rungs take it through the one
  shared helper **`fauna::managed_yield_biomass`**.
- **The pastoral rung is worked, so it cannot be double-paid** (slice 3b). It *used* to pay its owner
  passively, and `advance_husbandry` had to **skip** that payment for any herd a labor assignment
  worked last turn (a `Herd::worked_this_turn` flag) — because without the skip a Red Deer under
  construction collected the then-live `Corral` dip (0.50 × 1.50 = 0.75) **plus** the passive rung
  (1.50) = 2.25/turn, *more* than the 1.50 of walking away, turning the pen's investment cost into a
  profit. Retiring the passive rung removes the hazard by construction (there is no second payment
  left to stack), so the flag and the skip are **deleted**. **The dip itself has since retired**
  (`docs/plan_standing_upkeep.md` §2.2): a build's cost is now the crew the player staffs on it,
  measured against what those same hands would have hunted — which is the comparison
  `fauna_husbandry::building_a_corral_costs_more_than_hunting_the_same_herd` makes.
- **It is constant-*escapement* MSY** — `take = min(peak_regrowth(K), max(0, B − K/2))` — **not** the
  constant-catch `sustainable_yield` a *wild* `Sustain` hunt takes. The sim regrows in Logistics and
  harvests in Population, so a constant-catch take is evaluated at the **post**-regrowth biomass; above
  `K/2` that is harmless (both forms cap at MSY and converge on `K/2`), but **below `K/2` it takes
  `g(B + g(B)) > g(B)`** — strictly more than the herd grew. At the wild `r` = 0.05 that leak is a
  rounding error; at the pen's fast per-species `r` (up to 0.75) it is fatal: a **fully fed** pen knocked below `K/2` spirals
  to zero in ~12 turns and can never recover. Escapement never takes a herd below `K/2`, so a depleted
  managed herd **rebuilds** (yielding less, or nothing, while it does) and then pays `r·K/4` forever —
  stable from *both* sides, same yield at capacity and at the operating point.
- A managed harvest therefore **never overdraws**: its yield telemetry reads `actual == sustainable`
  (no ⚠). Its `workers_needed` is **derived like every other rung's** (slice 7): the pen collapses the
  *policy* axis, never the **collection** cap — the keeper still carries the meat home, so the take is
  `min(pen MSY, hunters × hunt.per_worker_biomass_capacity)` and the surplus it offered beyond that is
  reported as `wasted`. The retired `TENDED_SOURCE_WORKERS_NEEDED = 1` claimed one keeper could collect
  a pen of any size.

Ecology/husbandry tunables live in the `ecology` (`regrowth_rate`, `collapse_fraction`,
`collapse_rate`, `stressed_fraction`, `extinction_floor`), `immigration`, and `husbandry`
(**`pastoral.ecology`**, **`pen`** — see "Corral" — plus the per-species growth gains) blocks of
`fauna_config.json`.
**The pen's BUILD dials live in `intensification_ladder.json`** (the `animal:pen`
rung's `build` block), and as of slice 4 so do the **earned-knowledge dials** (the ladder-level
`knowledge` block — the old `knowledge_progress_per_turn`, since split into `learn_rate` plus a
per-knowledge `lesson_costs` entry, and `knowledge_completion_threshold`, which `labor_config`
duplicated verbatim) — see "The Intensification Ladder": both food webs climb on the same
numbers.
**`FaunaConfig` is validated** (`FaunaConfig::validate`, run inside `from_json_str`, so every load path
— builtin, default file, `FAUNA_CONFIG_PATH` override — is covered; the `expedition_config.rs` /
`crisis_config.rs` convention). A broken invariant is logged at **error** level
(`fauna_config.invalid_rejected`) and the known-good builtin is used instead. Enforced: **the pen's
best-case net-positive floor** (Grazing 2d §2.4 — `pen.upkeep_per_biomass < r_pen · p / (2 + r_pen)`
for the **fastest** species' `r_pen = min(cap, max_wild_r × pen_gain)`; a slow breeder or poor-pasture
pen may run at a **loss by design**, so the old every-pen guarantee is retired for a best-case sanity
floor), **the ladder is monotone as gains** (`pen_gain > pastoral_gain > 1`), ordered ecology phase
bands (`extinction_floor < collapse_fraction < stressed_fraction < 1`) in all three ecologies, every
`regrowth_rate > 0`, `husbandry_regrowth_cap > 0`, `0 ≤ pen.starve_shrink_rate ≤ 1`,
`hunt.provisions_per_biomass > 0`, and the follow/market bounds. (The **knowledge** bounds moved to
`LadderConfig::validate` with the dials in slice 4, where they hold for both webs at once.)

### THE KEEPER DEMAND IS AN UPKEEP RATE, and the shed is its shortfall penalty

**`herders_needed` stopped being a declared head count and became work per turn**
(`docs/plan_standing_upkeep.md` §2.4). Both managed rungs declare
`upkeep: { work_per_turn: 1.0, scaled_by: source_load, meter_decay: null }`, and the source supplies
the **keeper load**
— `head count / animals_per_herder` (`fauna::herd_keeper_loads`). Since one worker-turn is
`PER_WORKER_OUTPUT`, `upkeep_crew_needed = ceil(load)` **is** the
`ceil((biomass/body_mass)/animals_per_herder)` the retired helper computed, so every species asks for
exactly the keepers it always asked for (`every_species_asks_for_the_keepers_it_asked_for_before`).

- **The species owns the ratio, the rung owns the rate** — the same division `taming_cost_multiplier`
  and `work_cost` make of the taming job. `animals_per_herder` stays in `fauna_config.json` and is
  folded into the *measure* before the ladder ever sees it, which is why the scale primitive is
  `source_load` and emphatically **not** a per-head rate: per-head says *"one keeper per 100 fowl but
  one per 2 boar"* and invents a 45-herder steppe megaherd that is a pure artifact of the unit.
- **There is exactly one definition of it.** `fauna::raw_herders_needed` is the rung's own
  `upkeep_crew_needed` at this herd's load; `herd_herders_needed` prefers the hysteresis-stabilized
  `Herd::herders_needed` and falls back to it; `Herd::stabilize_herders_needed` is *handed* it rather
  than recomputing a `ceil` of its own. The free `fauna::herders_needed` is retired with the second
  copy it would otherwise have been.
- **`Herd::herded_fraction` is retired too**, and `Herd::upkeep_supplied` carries what it did — the
  one stored fact of the keeping. The published ratio (`fauna::herd_herded_fraction`), the shortfall
  and the animals nobody can hold are all derived from it, so no two of them can describe different
  staffings. *"Zero keepers last turn"* — the total-abandonment gate `regrow_biomass` and the bleed-out
  read — is simply `upkeep_supplied <= 0`.

### The shed waits out a NEGLECT GRACE, and the notice does not

`Herd::neglect_turns` counts **consecutive** turns the keeping went unmet, reset outright by any turn
it was met — and by a herd not being managed at all, since a wild herd is nobody's to neglect.
**Animals leave only while that counter exceeds the herd's rung's `upkeep.grace_turns`**
(`RungDef::upkeep_grace_turns`), resolved through **`fauna::herd_keeping_rung`**: `animal:pen` once
there is any pen progress, `animal:pastoral` for any other managed herd.

- **THE SHED IS CONTINUOUS IN THE SHORTFALL.** `uncontained_overage` is the unmet demand converted
  back into animals (`shortfall_in_loads × animals_per_herder`), which is the same number the retired
  `herded_fraction × herders_needed × animals_per_herder` capacity reconstruction produced — so half
  the keepers a herd wants leaves half its animals uncontained. The retired
  `herded_fraction < FULLY_HERDED` gate was a threshold that answered only *whether* a herd was
  under-contained, the same step the plant web's binary `tended_this_turn` flag took.
- **`MIN_ESCAPE_ANIMALS` is the animal branch's quantum, and it is why its counter can differ from the
  plant web's.** A plant meter is continuous, so any shortfall bleeds; a herd loses **whole animals**,
  so a shortfall of less than one animal is not under-containment at all — the same whole-animal
  discipline `quantise_animal_take` imposes on the take.
- **A METER IS OWED BUILDERS WHILE INCOMPLETE AND KEEPERS ONCE HELD** (`fauna::herd_upkeep_supply`,
  the twin of `forage::patch_upkeep_supply`). A `Tame` in flight owes its build crew — you cannot be
  billed to keep a tameness you have not finished earning — and a domesticated herd owes keepers.
  **The verb names the meter**, so a `Corral` starting on a herd with no pen progress answers for
  `animal:pen` from its first turn: the supply is stamped in Population and read by the *next*
  Logistics pass, so it has to describe the meter that pass will judge.
- **The supply accumulates across the bands working the herd** (`+=`, cleared once per turn by
  `advance_husbandry` after everything downstream has read it). The demand is per-**source**, so two
  bands each put a fraction of it on the ground; assigning would let whichever band the loop visited
  last speak for all of them.
- **It is `herd_keeping_rung`, not `herd_rung`.** The latter answers which rung the herd has
  *completed*, and a half-tamed herd is already owned and already sheds — reading `animal:wild` there
  (no build, so no grace) would hand the herd in the middle of a 25-turn investment the *least*
  forgiveness on the ladder. It reads the **newest meter with progress**, so a half-raised pen already
  owes the pen rung's longer grace; the escape *rate* still reads `is_corralled()`, because a fence
  that is not up yet holds nothing.
- **The under-herded notice is deliberately NOT gated on the grace.** It fires the turn the herd
  genuinely becomes under-contained, which is exactly the window in which the player can still send
  hands and lose nothing; warning only once the animals were leaving would spend the grace on silence.
  This is why the measurement was split out of the shed: **`fauna::uncontained_overage`** answers *is
  this herd under-contained*, `shed_uncontained_animals` answers *do animals leave this turn*.
- **On the wire:** `HerdTelemetryState.hasNeglectGrace` / `neglectGraceRemaining` — the countdown, not
  the counter (`0` = shedding now), published through the same `herd_keeping_rung` seam the shed gates
  on, so the wire cannot count down against a rung the sim is not applying. See the plant twin in
  `cultivation.md`.
- **`decay_fraction_per_turn` IS GONE FROM THE LADDER ENTIRELY**, and the animal branch is why it was
  ever suspect: the neglect-escape arc made `domestication_progress` monotone-up, so the `0.01` on
  `animal:pastoral` (and the `0.0` on `animal:pen`) described a tameness-bleed the sim does not have
  and **nothing read**. It was deleted from the animal rungs first and retired outright when the plant
  branch moved onto the standing upkeep, where **shortfall is the decay**
  (`docs/plan_standing_upkeep.md` §2.4): a rung that loses exactly the work nobody supplied needs no
  second dial saying how fast it forgets. **`build.grace_turns` went the same way** — both animal
  rungs declare `null` and their live grace is `upkeep.grace_turns`, because two numbers for one
  trigger is what that arrangement exists to prevent.

### THE PEN'S FEED IS ITS OWN MECHANISM, and it is not the upkeep

The `upkeep` block is the **work** half of holding a rung — hands, in work units. What a pen *eats* is
a separate account with a separate currency and its own levers, all in `fauna_config.json`'s
`husbandry.pen`: `upkeep_per_biomass` is the gross feed rate, the fenced footprint's pasture and any
hay **offset** it into the net larder bill the keeper actually pays (`penLarderBill` / `penHayFood`),
`pen_fed_fraction` records how much of that bill was met, and `starve_shrink_rate` is what an underfed
pen loses. A keeper who is present but **broke** starves the herd; a keeper who is **absent** lets it
shed. The two penalties are orthogonal and a pen can take both in one turn.

Folding the feed into the `upkeep` block would put food and labour in one number, and the
`larder_delta == foodIncome − foodConsumption − penFeedUpkeep` identity — the one the client's larder
line is reconciled against — reads the feed account by name.

## The `Tame` verb (Intensification rung 2) — the grammar fix

**The animal twin of `Cultivate`**, and the correction the plant side already made
(`docs/plan_intensification_ladder.md` §4.1). Taming used to be a **hidden side effect of a harvest
policy**: one `Sustain` branch in `advance_labor_allocation` advanced Herding knowledge *and*
`accrue_domestication`, so the same action both taught and tamed, invisibly and for free — while the
*visible* verb (`Corral`) was disabled until the herd was already tame. Rung 2 is now an explicit,
gated, **paid** verb, so both food webs read the same:

| | plants | animals |
|---|---|---|
| rung 1 Sustain | earns **Cultivation** — *and tames nothing* | earns **Herding** — *and tames nothing* |
| rung 2 verb | `Cultivate` → tended patch | **`Tame`** → pastoral herd |
| rung 3 verb | *(`Sow` — a later slice)* | `Corral` → pen |

- **`Improvement::Tame`** (wire key `"tame"`) — **animal-only** (`Improvement::valid_for_hunt`; a
  Forage assignment carrying it is rejected at its command, exactly as `Corral` is). Since issue #442
  it is an `Improvement`, not a pressure setting: it rides `LaborAssignment.improvement` **beside**
  whatever floor the crew holds, so a herd being tamed is still being hunted at that floor. It emits no
  `huntTripEstimates` row, because an expedition's mission carries a **floor** and therefore
  cannot name it at all.
- **The investment is the keepers on the verb.** `tame` names its own crew, and those hands are not
  hunting — that, and nothing else, is what a Tame costs (`docs/plan_standing_upkeep.md` §2.2). The
  rung's `yield_fraction_while_building` is **retired**: the hunters beside the build carry exactly
  what they carried before, so the price is the same statement at every staffing, where the dip's
  depended on whether the herd's own escapement was binding the crew.
  `domestication_progress` accrues that crew's output in **work units** —
  `improvement_workers × PER_WORKER_OUTPUT`, **no floor term** (a build crew is not pulling on the
  herd) — against a job costing
  `work_cost × the species' taming_cost_multiplier` (**50 units for a rabbit, 250 for a Steppe
  Runner**), via the shared `RungDef::build_accrual` / `build_cost` seam. **The keepers' KIT is not
  in that expression**: the handling gear takes work off the **job**, never off the crew's output —
  see "The build axis" in `equipment.md`. **Turns are the output**,
  so a bigger keeper crew gentles the same herd sooner; see "An improvement costs WORK, not turns" in
  `intensification.md`. **Gates:** the faction knows **Herding**, the species'
  `husbandry_ceiling` allows domestication (Grazing 2d-δ), and **something stands above the crew's
  floor** (`systems::labor::crew_is_working_the_source` — the escapement room in biomass, read before
  the whole-animal quantiser, so a herd that cannot yet spare a whole body is still being worked).
  **THE `Thriving` GATE IS GONE** (`docs/plan_harvest_floor.md` §3.2 — the same deletion the plant
  rung 2 took, for the same reason): gentling a herd you are pulling hard on is now *slow*, not
  *stopped*, so there is no lapse state left and no "progress is held across it" rule to state.
  `validate_tame` never had a phase gate, so the command side was already consistent with removing it.
  The herd is still marked `tamed_this_turn`. Ownership is **not** in the gate: `accrue_domestication` owns the
  `owner is None || owner == faction` rule, exactly as `accrue_cultivation` does on the plant side.
  Accrued **after** the take (mirroring Cultivate/Corral), so the turn pays what the forecast promised.
  **The turn the herd becomes domesticated, the assignment's `improvement` is cleared** — the herd,
  the hunting crew **and the floor** all intact, and the **build's** crew handed on — to the keeping
  if the finished rung declares an upkeep, otherwise back to the idle pool, announced either way, so
  the player can re-task rather than leaving hands on a herd with nothing left to gentle. The
  completing turn still pays the build's whole price. One seam for all four rungs — see "Completion CLEARS the improvement" in
  `intensification.md`.
- **Tameness is PERMANENT once earned (neglect-escape arc, `docs/plan_fauna_neglect_escape.md` §2.1).**
  `domestication_progress` is monotone-up: `Tame` builds it and **nothing decays it** — the tameness-bleed
  (`decay_under_herded`/`decay_domestication`) is deleted. An abandoned part-tamed herd keeps its earned
  progress; what neglect costs is **animals** (the shed, see "Herding is standing labor"). `Herd::tamed_this_turn`
  is still cleared each turn so it can't go stale, but its consumer (the retired decay-sparing) is gone.
  `taming_cost_multiplier` now prices only the `Tame` *job*, never a decay. **Distinct from an ordinary
  hunt at any other floor**: a plain hunt *harvests* a herd; only `Tame` raises the taming meter.
- **`tame <faction_id> <herd_id> <workers>` command** (`handle_tame`; `TameCommand` proto field **40**,
  `CommandEventKind::Tame`) — **sets the `Tame` improvement** on the bands already hunting the herd,
  the command form of the client's checkbox (issue #442: the floor beside it is left alone). It **tames nothing outright**. It targets a **herd id**
  (not a tile like `corral`): taming is the verb you reach for on a *roaming* herd, identified by who
  follows it, not by where it stands this turn. Rejections, each distinct (`validate_tame`, reached through `validate_improvement`): faction hasn't learned Herding / no such herd / the species is wild
  game (hunt-only) / already domesticated (corral it instead) / another people are taming it / **no
  band is hunting it** (staff it first). It never carried a phase gate — a herd's `ecology_phase`
  swings as it is hunted — and since `docs/plan_harvest_floor.md` §3.2 neither does the labor arm.
- **The `domesticate` early-claim is REMOVED** — command, `claim_threshold` lever, its validate bound,
  and `Herd::claim_domestication`. It let the player snap progress to `1.0` and skip the investment,
  which is the entire decision (the plant side removed its twin for that exact reason). **Proto field
  30 is reserved and must never be reused.** Tests that needed a tamed-herd fixture now run the real
  accrual through **`Herd::tame_outright(faction)`**, which obeys the husbandry ceiling — you cannot
  fabricate a domesticated `wild` herd. (It replaced `accrue_domestication(faction, RUNG_COMPLETE)`,
  which stopped meaning anything once a job had a size; it pays a nominal one-worker-turn job, and the
  *size* of a fabricated job cannot affect any predicate, which all read `progress >= cost`.)
- **Per-species price.** The rung's single cost would make *every* species the same job — a rabbit
  costing what a Steppe Runner costs. The species declares its own **`taming_cost_multiplier`**
  (`fauna_config.json`, default 1.0), and `RungDef::build_cost` takes it — the one seam that honors
  it, so every caller pricing a Tame reads the same number. **It prices the JOB and nothing else.** It
  used to reach the decay as well (a bleed was a fraction of the rung's own cost, which kept a
  build:decay ratio invariant per species for free — *slow to tame, slow to forget*); shortfall is
  the decay now, so what an improvement loses is what its keepers did not supply — a fact about a
  crew and a rung, not about how big the job was. Every other rung passes `RUNG_COST_UNSCALED`
  (penning is a flat job for every species — a fence is a fence; only *taming* varies).
  > **It was `taming_rate`, a build TIMESCALE, and the inversion is the honest statement**
  > (`docs/plan_unit_costed_work.md` §3.1). `0.2` on a Steppe Runner said *your people are five times
  > worse at their job on this animal*; `taming_cost_multiplier: 5.0` says *the animal is five times
  > the work*, which is what anyone would have meant — and it composes with a later cost spread,
  > where a rate could not. Same pacing at the same crew.
  See the `fauna_config.json` row for the roster.
- **Config** — the whole rung is `intensification_ladder.json`'s `animal:pastoral` record: verb `tame`,
  `unlock_knowledge: "herding"`, **`earns_knowledge: "penning"`** (slice 4 — a config edit, exactly as
  promised),
  `ceiling_required: "pastoral"`, `build: { work_cost 50, grace_turns null }`,
  `upkeep: { work_per_turn 1.0, scaled_by source_load, grace_turns 2 }`.
  The **50 is a reference-crew choice, not a derivation** — the rung declares no crew, so there was
  nothing to multiply today's 25 turns by, and 2 keepers is what rung 2 of the plant web wants for the
  same claim. The **upkeep is the pacing-neutral inversion of `herders_needed`**: at `1.0` work per
  keeper-load, `ceil(demand)` is the count this rung has always asked for. `grace_turns 2` is this
  rung's own former *build* grace moved onto the upkeep's trigger unchanged — a tamed herd with no
  fence stays near its people for a turn or two, then drifts — and `build.grace_turns` is `null`
  because there is only one trigger now;
  **`crew_needed` and `yield_fraction_while_building` are both retired** — the player states the
  build's crew on the verb, so neither a rung-level staffing floor nor a dip has anything left to say.
  **The Tame IS crew-scaled now**, at **the crew the player staffs** — it was crew-*blind* before,
  taking 25 turns whether two hands or twenty worked the herd, and a crew-blind build is exactly what
  pricing improvements in work removes (`docs/plan_unit_costed_work.md` §1.2).
- **Slice 3b landed the rest of the rung:** passive-free pastoral is **retired** (a tamed herd yields
  only through a worker's Hunt assignment, at the pastoral `r` — see "Domestication / husbandry" and
  "The husbandry yield ladder") and the **`drift_to_owner`** movement primitive is live (see "Herd
  movement is a rung primitive").
- **Slice 4 completed the rung's knowledge half:** practising it (hunting the resulting **pastoral**
  herd) earns **Penning**, which now gates `corral` — so Herding gates `tame` and **only** `tame`.
  See "The knowledge pattern".

See Also: "Cultivation (Intensification Phase 1a)" (the plant rung 2 this now mirrors exactly), "Corral
(Intensification Rung 1c)" (the rung above), "The Intensification Ladder" (the engine + the config).

## Corral (Intensification Rung 1c)

The **animal mirror of the tended patch** (`docs/plan_intensification.md` §4b) — the place-bound form
of the *existing* herd domestication, and the fauna-side twin of "Cultivation" under Depletable
Forage. Taming a herd is *symmetric* with preparing a patch, but the **product differs and that
difference is the settle mechanic**: an *un*corralled domesticated herd stays **mobile** (pastoralism
travels with the band); **corralling pins it**. Like Cultivate, corralling is an **explicit `Corral`
policy with an investment cost** — not a free command. A `Herd` carries `corral_progress: f32` (**work
units**, complete at its stored `corral_cost`; the pen under construction), `corralled_at: Option<UVec2>` (`Some` = penned at that tile) + a transient
`corralled_tended_this_turn` flag. *Sim-only — the client readout is a follow-up (see below).*

- **Rung-3 earned-knowledge gate — PENNING** (slice 4's §4.3 reshuffle; **it was Herding**). *Learned
  by doing* and **never start-granted**: hunting a **pastoral** (tamed) herd accrues faction
  **Penning** knowledge (discovery `PENNING_DISCOVERY_ID` = 2006, `fauna.rs`) at the ladder's
  `knowledge.learn_rate` **scaled by the assignment's floor, over Penning's own `lesson_cost`** —
  *"you learn penning by managing tamed herds"*, and how fast depends on how much you leave standing.
  **It does NOT depend on how many hands are on the herd**: a lesson is credited once per source per
  turn, in **practice units**, which is the currency the build's work units are deliberately kept
  apart from (`intensification.md` → "A LESSON COSTS PRACTICE — and practice is NOT work")
  (`intensification::learn_multiplier`; the health gate is gone, `docs/plan_harvest_floor.md` §3.2). The **`Corral` policy** (and the `corral` / `extend_pen` commands, which ride the same
  `animal:pen` rung) is refused until the faction knows it; every gate resolves the id off the rung
  record, never a literal. The `penning` tag → discovery 2006 mapping is declared in
  `start_profile_knowledge_tags.json` purely so it is mappable; **no start profile lists it**
  (guarded by `start_profile::tests::no_start_profile_grants_a_ladder_knowledge`).
  **The old Cultivation asymmetry is gone:** taming is no longer ungated (Herding gates `Tame`), so
  both webs now gate rung 2 on the knowledge rung 1 teaches, and rung 3 on the knowledge rung 2
  teaches. One knowledge per transition. See "The knowledge pattern".
- **The `Corral` improvement — the investment.** In `advance_labor_allocation`'s **Hunt** arm, a herd
  worked with `Improvement::Corral` (animal-only) in flight costs **the fencing crew the command
  named** — hands not hunting (`docs/plan_standing_upkeep.md` §2.2; the rung's
  `yield_fraction_while_building` is retired, and the keepers beside the build take what they always
  took). `corral_progress` accrues that crew's own output in work units against the rung's
  `work_cost` of **75** (25 turns at a reference crew of 3). **Its `eligible`
  deliberately carries no work predicate**, unlike the two rung-2 builds: it replaced a rung's
  `Thriving` gate, rung 3 never had one, and fencing a herd is ground work — a pen goes up around a
  flock already drawn down to its keeper's own floor. **Gates:** the faction knows **Herding**
  AND owns the **domesticated** herd; a gate that lapses **mid-build** just stops accrual that turn
  (progress is kept — a half-built pen is materials on the ground; unlike cultivation it does **not**
  decay *gradually*). That "progress is kept" applies to a **mid-build** lapse only — a **completed
  pen whose herd escapes loses its progress outright** (reset to `0.0`; see *Escapes-if-untended*
  below). Accrued **after** the take, so the turn pays exactly what the forecast promised. At the job's
  cost `Herd::corral_at` pens it (sets `corralled_at`, stops roaming, grants the one-turn tended grace),
  pushes a `CommandEventKind::Corral` feed line, and **clears the assignment's `improvement`** — the
  keeper crew stays on the herd, under the stance it chose, rather than fencing a pen that is already
  up. Extending a pen is command-driven (`herd.pen_extending`), not improvement-driven, so the clear
  cannot block a later ring. One seam for all four rungs — see "Completion CLEARS the improvement"
  in `intensification.md`.
- **`corral` command (repurposed)** — `corral <faction> <x> <y> <workers>` (`handle_corral`; unchanged
  proto/runtime/text plumbing, `CommandEventKind::Corral`, `CorralCommand` proto field 38) **sets the
  `Corral` improvement** on the band(s) already hunting the herd standing on that tile — the command
  form of the client's checkbox. Since issue #442 it touches the improvement slot **only**: the band's
  stance and crew are untouched by construction. It **pens nothing outright**. Rejections: no herd there / faction
  hasn't learned **Penning** ("…have not learned Penning yet. Tame and keep herds to learn it.") / not
  domesticated / not the owner / already corralled / **no band is hunting it** (staff it first). Same
  gates as `validate_improvement`'s `Corral` arm, which every path shares.
- **The pen is a managed POPULATION** (`docs/plan_corral_managed_population.md`): its yield follows the
  animals you actually keep, those animals **eat** every turn, and underfeeding **shrinks** the herd. A
  one-off 25-turn build that then printed food forever is now a **sustained commitment with a running
  cost**. Corralled = fixed + place-local worker-tended + **fed** + escapes-if-untended:
  - *Fixed* — `advance_herds` skips a corralled herd's `advance_herd_roam` (it stays at `corralled_at`,
    no heading arrow); it still grazes its footprint + regrows toward the footprint's `K` (Grazing 2d).
    Since slice 3b this is **read off the `animal:pen` rung's `behavior.movement: fixed`**, not
    hard-coded on `is_corralled()` — see "Herd movement is a rung primitive".
  - *Place-local worker-tended* — a **Hunt assignment on a corralled herd** is herding/tending it, and
    the turn has two halves (the tend branch of `advance_labor_allocation`'s Hunt arm, which `continue`s
    before `hunt_take` — a corralled herd is never both hunt-drawn AND paid):
    1. **FEED (footprint-offset, Grazing 2d §2.3).** The pen grazes its fenced footprint
       (`advance_herd_grazing` → `footprint_intake`), and the larder pays only what the pasture can't
       cover: `demand = pen.upkeep_per_biomass × biomass × (1 − pasture_fraction)`,
       `pasture_fraction = clamp(footprint_intake / (fodder_per_biomass × biomass), 0, 1)`.
       `LocalStore::take` returns what it *actually* took; `pen_fed_fraction = pasture_fraction +
       (1 − pasture_fraction) × (paid / demand)` (the total fed share — pasture plus the paid part of
       the reduced larder bill). A lush footprint feeds the pen for free; a barren one pays the full
       bill — **the tether that gives "the pen pins the band" its teeth**, now cheap on good land.
    2. **HARVEST.** The keeper takes the **pen's MSY** (`fauna::pen_yield_biomass` →
       `managed_yield_biomass` under the herd's per-species pen ecology (`pen_ecology_for`), against its
       footprint `K` = `herd.carrying_capacity`), which **draws the herd
       down** — exactly what makes it sustainable (see "The husbandry yield ladder"). The credited yield
       is **gross**: the feed is a separate debit, so the player sees both halves of the trade rather
       than one netted number.
  - *Starves if underfed* — `advance_husbandry` reads last turn's `pen_fed_fraction` and, if the keeper
    could not pay, shrinks the herd by `pen.starve_shrink_rate × (1 − fed) × biomass`, floored at
    `pen.ecology.extinction_floor × K_pen`. **The pen's growth is what the feed buys**: `regrow_biomass`
    scales a penned herd's growth by `pen_fed_fraction`, so an unfed pen does **not** grow (without this
    the pen's own fast `r` out-runs the 10%/turn wasting several times over — an "unfed" herd would keep
    growing and quietly pay a yield for feed nobody bought). The herd **withers to a remnant and
    recovers when fed again**: it does **not** despawn (a penned herd is exempt from `advance_herds`'
    dispersal retention — dispersal is the *mechanism* of local extinction, and a confined herd cannot
    disperse) and it does **not** lose the pen. Deliberate: a recoverable famine the player can see and
    fix is better play than silently voiding a 25-turn investment. It is **never silent** — an
    edge-gated `CommandEventKind::Corral` feed line fires on the turn the famine *starts*
    (`"The <species> herd is starving — the pen has no feed"`, detail `status=starving fed=<f>
    action=corral herd=<id>`), not every turn it continues. **Starving your animals to feed your people
    becomes a *decision*, not an accident.**
  - *The decision this creates* — the pen stops being a strictly-dominant upgrade and becomes a **wager
    on staying**: it out-pays every other rung, but only while you feed it, every turn, forever — and
    its food cost lands **exactly when food is scarce**, so a bad winter forces a real choice (eat the
    seed corn and lose future yield, or go hungry).
  - *Sheds-if-under-contained, AFTER A GRACE (neglect-escape arc, `docs/plan_fauna_neglect_escape.md`)*
    — the binary escape is **retired**. In `advance_husbandry` (Logistics, before Population — on
    `Herd::corralled_tended_this_turn`, which gates the pen's **feed** and survives both webs' move
    onto the upkeep because feed is a separate account) an under-contained pen **sheds whole
    animals over its labor capacity** into the wild web at `pen_escape_fraction` (slower than pastoral
    — the fence buys time, and the pen rung's longer `upkeep.grace_turns` says the same thing on the
    turns axis),
    and an untended one is BOTH un-herded (sheds) and un-fed (`pen_fed_fraction = NOT_FED`, so it does
    not regrow — a fast breeder's growth would otherwise cancel the shed). A **fully-abandoned pen bleeds
    its whole flock out and DESPAWNS**: it keeps shedding until it can no longer shed a whole animal
    (`biomass < body_mass`), then the emptied entity is removed (`advance_husbandry` Phase 3) — the pen,
    fence and all, dies with it (no field reset needed — the entity is gone). `owner`/`corralled_at`
    are **never cleared at a floor** (that would stop the shed and strand a husk); the herd stays
    corralled and bleeds down. The flock is already in the wild web via the shed (at domestication 0),
    so nothing is lost but the empty pen — and losing it is what makes the tending obligation real (the
    "pins the band" mechanic), re-penning being a fresh herd's full 25-turn investment. Because loss
    **destroys a 25-turn investment**, it is **never silent**: `announce_pen_lost` pushes the same
    `CommandEventKind::Corral` feed line the pen's *completion* pushes (one kind for the pen's whole
    life), reading `"The <species> herd has drifted off — untended, the pen is lost"` with
    `status=escaped reason=untended action=corral herd=<id> x=<x> y=<y>` in the detail. `corral_at`
    grants a one-turn grace so a freshly-penned herd doesn't shed before its keeper takes up tending. A
    keeper who is present but *broke* **starves** the herd (above) — it produces no shed (a keeper holds
    the flock — its keeping is met), so it keeps its pen and recovers when fed; only animals *leaving*
    empty a herd.
- **Persistence** — the checkpoint clones the whole `HerdRegistry` (`SimState::herds`), so **every**
  `Herd` field rewinds with a rollback, `corralled_at` / `corral_progress` / `pen_radius` /
  `pen_extend_progress` / `pen_extending` (Grazing 2d) included: a half-built pen (or an in-flight fence
  extension) is restored rather than lost. The transient per-turn scratch —
  `corralled_tended_this_turn`, **`pen_fed_fraction`, `pen_starving`, `footprint_intake` and
  `pen_pasture_fraction`** — is transient *within a turn*, not across a checkpoint: it is captured and
  restored verbatim like everything else, so a restored pen resumes exactly as tended and as fed as it
  was, and a rollback can neither invent a famine nor destroy a standing pen. These fields are still
  **off the client wire**; "not persisted" is no longer true of any of them.
- **Config** (`fauna_config.json` `husbandry`): the **`pen`** block — `ecology` carries **phase bands
  only** now (its `regrowth_rate` is unused; the pen `r` is per-species — Grazing 2d),
  **`upkeep_per_biomass` (0.002 — the feed, now footprint-offset)** and `starve_shrink_rate` (**0.10** —
  a fully-unfed herd loses 10%/turn); `capacity_fraction` is **deleted** (`K_pen` is the fenced
  footprint's graze flow). Plus the **per-species growth gains** `pastoral_gain` (2.0) / `pen_gain`
  (4.0) / `husbandry_regrowth_cap` (1.0), **`pen_radius_max`** (2 — the `ExtendPen` fence cap, 2d-β,
  validated `>= 1`), the **`pastoral`** block (phase bands only). **The pen's
  investment cost and build rate moved to `intensification_ladder.json`'s `animal:pen` rung** — the old
  `corralling_yield_fraction` 0.50 became its `yield_fraction_while_building`, since **retired**
  outright with the dip; the old
  `corral_build_progress_per_turn` 0.04 its rate, since re-expressed as **`work_cost` 75** work units
  — 25 turns at a reference crew of 3, the pacing-neutral cost — and in **slice 4**
  the earned-knowledge levers `knowledge_progress_per_turn` (0.05) / `knowledge_completion_threshold`
  (1.0) moved to that file's ladder-level **`knowledge`** block at the same values, `labor_config`
  having duplicated them verbatim; the rate has since split into `learn_rate` 1.0 over a per-knowledge
  `lesson_costs` entry of 20, which is the same 0.05 (see "The Intensification Ladder").
  `claim_threshold` is **deleted** with the `domesticate` early-claim it gated (slice 3a — it let the
  player skip the investment). The retired flat rates
  `provisions_per_biomass` (0.01) / `corral_provisions_per_biomass` (0.012) and `fauna::corral_provisions`
  are **deleted**.
  - **Retuned once, against measurement** (a scripted 100-turn campaign on three pinned seeds — the
    default `map_seed` is `0`/entropy, so a probe *must* pin one): the first cut (`pastoral` 0.15,
    `pen` 0.60, dip 0.25) left a freshly-taming band at income **1.275** vs consumption **1.294** — a
    permanent one-day-of-food treadmill, no savings, no affordable expedition — and made the pen
    reachable only through a **~50% population crash** (the then-live build dip had to be paid out of a
    famine).
    The shipped values put the pastoral rung clearly *above* subsistence (a real surplus) and let the
    pen's build be paid from it. **`upkeep_per_biomass` was deliberately NOT touched** — the running cost
    is the point of the arc, and weakening it to fix balance would delete the mechanic.
  - **Every invariant above is enforced by `FaunaConfig::validate()`** — most importantly
    the pen's **best-case net-positive floor** (Grazing 2d §2.4 — `upkeep_per_biomass < r_pen · p /
    (2 + r_pen)` for the **fastest** species' `r_pen = min(husbandry_regrowth_cap, max_wild_r ×
    pen_gain)` = `min(1.0, 0.35 × 4.0) = 1.0`, so the bound is `1.0 × 0.02 / 3.0 ≈ 0.0067`; shipped
    0.002): derivation — at the operating point the
    pen yields `r·K/4 · p` and eats `u · K·(2 + r)/4`, so `net > 0 ⟺ u < r·p/(2 + r)`. **This inverts
    the old every-pen guarantee:** with per-species `r` and pasture-dependent feed, a slow breeder or a
    poor-pasture pen may run at a **loss by design** (a placement decision), so validate only guarantees
    the best pen (fastest breeder, fully larder-fed) still pays. See "The husbandry yield ladder".
- **The band's food ledger — `PopulationCohortState.penFeedUpkeep` (the per-band roll-up).** A pen's
  feed is taken straight off `cohort.stores` (`LocalStore::take`, the corral-tend branch), so it lands
  in **neither** `foodIncome` (Σ per-source `actual`) **nor** `foodConsumption` (the food the *people*
  actually ate — `PopulationCohort::last_food_consumption`, the real opening-brackets `stores` debit,
  the symmetric twin of this pen debit; **not** a post-turn `food_demand`, which the same turn's
  births would inflate). A band keeping a pen would therefore display a surplus **overstated by exactly the
  upkeep** — on a Red Deer pen a phantom **+1.74/turn** against a band that eats ~1.2 — and the player
  would watch the larder drain unexplained. `penFeedUpkeep` is **the food the band actually PAID** this
  turn (the summed `LocalStore::take` *return*, not the demand — a band that can only part-pay reports
  only what it handed over, and its herds starve for the rest), carried on
  `LaborAllocation::last_pen_feed_upkeep` (rebuilt per-turn, excluded from equality — same treatment
  as `last_yields`). It closes the identity
  ```text
  larder_delta == foodIncome − foodConsumption − penFeedUpkeep
  ```
  which `integration_tests/tests/pen_food_ledger.rs` pins against a **real turn** through the real
  systems and the real snapshot export, both fully-fed and part-fed. **It is deliberately NOT folded
  into `foodConsumption`**: "my people ate X" and "my animals ate Y" are separate lines, and that
  separation is the readout this arc exists to give. The sim answers the number so the **client does
  zero arithmetic** (it must not sum `penUpkeep` across herds itself) — the same discipline as the
  Pre-commit Yield Forecast.
- **Display snapshot (on the wire).** The corral state is exposed to the client stream on both
  `WorldSnapshot` and `WorldDelta` (`snapshot.fbs`, `sim_schema`, `snapshot.rs`
  `herd_snapshot_entries`): `HerdTelemetryState.corralled:bool` (= `Herd::is_corralled()`) and
  **`corralProgress:float`** (0..1, the pen-building meter — the animal twin of
  `ForagePatchState.cultivationProgress`), plus **`penUpkeep:float`** and **`penFedFraction:float`**.
  Both are **per-herd** (the herd drawer + the starving warning):
  - **`penUpkeep`** = the feed this pen **demands, or would demand once built**, at the herd's
    **current** biomass (`pen.upkeep_per_biomass × biomass`) — a *projection* for an unpenned herd, the
    *live* demand for a penned one. It is **always meaningful, never `0`-because-unpenned**, and is
    computed on the **same biomass basis** as `corralYield`, so the two are a **matched pair the client
    subtracts**. That is the point: the pre-commit `Corral` row is by definition looking at a herd that
    is *not yet penned*, so a `0` there would quote the payoff while hiding the running cost at the one
    moment the running cost should drive the decision — the same defect class as advertising the
    **gross** yield (a preview quoting a number the player will never bank). At or below `K/2` the
    projected `corralYield` is honestly `0` (escapement — the pen pays nothing until the herd
    rebuilds).
    > **`buildTurnsRemaining` is the same rule applied to TIME.** It publishes the turns a running
    > Tame/Corral still needs, and — with nothing being built — what the rung this herd would climb
    > next would take the crew currently working it. `-1` only where there is genuinely no answer
    > (already penned, a gate refuses, no crew, or a stalled build). `intensification.md` → "The build
    > on the wire" owns the seam, the gates it carries and which `*WorkCost` it belongs beside.
  - **`penFedFraction`** = last turn's fed fraction (`1.0` = fully fed, `< 1` = **starving** — the herd
    and its yield are shrinking, and it recovers when fed again).
  - **Demanded ≠ paid** (load-bearing): a starving pen demands more than it is paid, and
    `penFedFraction` is that ratio. The band's **actual** ledger debit is the per-band
    `PopulationCohortState.penFeedUpkeep` (the real `LocalStore::take` amount) — the food ledger draws
    **that**, never `penUpkeep`. So no consumer needs a "0 when unpenned" reading, and one field with
    one meaning beats two that must be kept in lockstep.
  - **`penLarderBill` / `penHayFood` (Flora Roster F3) — the render-ready feed split.** `penUpkeep` is
    the **gross** bill; these two are its **net** remainders, in FOOD units, after the footprint's
    pasture and any drawn hay pay their share — so the client draws *"Fed by pasture NN% · hay X.X ·
    larder Y.Y"* with **zero arithmetic** (the `penFeedUpkeep` precedent). `penLarderBill` is the
    corral-tend branch's own `demand` local (= `penUpkeep × (1 − (footprint + hay)/grass_demand)`), the
    exact number billed that turn, `0` when fully fed by pasture + hay; `penHayFood` is hay's
    food-equivalent (`penUpkeep × fodderDraw / grass_demand`) — `fodderDraw` is in grass units (~25× the
    food scale) and cannot share the row, this can. **The invariant the client relies on:**
    `pasture_food + penHayFood + penLarderBill == penUpkeep`, where `pasture_food = penUpkeep ×
    penPastureFraction` — three terms of one demand, no double-count (pinned by
    `core_sim/tests/grazing_f3_fodder.rs::the_three_pen_feed_terms_sum_to_the_gross_upkeep`). This
    replaces a pre-existing two-term (pasture + larder) split that **over-stated the larder** on any pen
    with pasture > 0 and had no honest place for F3's hay term. Both are transient per-turn `Herd`
    scratch (stamped beside `fodderDraw`/`penPastureFraction`, reset on escape), `0` for an unpenned
    herd.

  Plus the forecast pair `huntPolicyCeilings`' **corral** row / `corralYield` (see
  "Pre-commit Yield Forecast"). See "Intensification display snapshot" under Cultivation for the
  plant-side + faction-knowledge fields.
- **Follow-up (final Phase-1 slice):** the **client _rendering_ for both ladders** — cultivation +
  Cultivation-knowledge + tended-patch on the plant side, and domestication + Herding-knowledge +
  corral on the animal side — is the last remaining client-dev slice (the data is now all on the wire).
  **Phase 1b of the managed-population arc rides with it:** the pen's `penUpkeep` as a *negative* row in
  the band's food ledger, the `penFedFraction` starving warning, and the corrected policy hints.
  `docs/plan_corral_managed_population.md` §6 — **Phase 1a (the sim) must not ship to a player without
  1b**, only to `main`: without the readout the player watches their larder drain with no explanation.
  **Phase 2 (deferred):** the pen's upkeep is drawn *first* from the tile's `ForagePatch` biomass (the
  animals eat grass — a resource humans can't), and only the **shortfall** is hauled from the larder.

### A herd row is assembled from TWO frames, and it must describe ONE turn

`herd_snapshot_entries` walks the display **`HerdTelemetry`** entries and resolves each one's live
**`Herd`** out of the registry (`registry.find(&entry.id)`), then fills the row from *both*. The two
are not the same frame:

| source | written | what it is as of |
|---|---|---|
| `HerdTelemetry` entry | Startup, then Logistics — the end of `advance_herds`, and `repopulate_fauna` on an immigration | **the previous turn** for anything `advance_husbandry` or Population touches |
| live `Herd` | authoritative at all times | this turn |

Three stages of writes land **after** the last telemetry write and before the capture: the rest of
Logistics (`advance_predation`, then `advance_husbandry`'s shed / starve / despawn), the whole of
Population (`advance_labor_allocation` — the build accrual, the hunt take, `corral_at`), and every
stage between. So **every field a row takes from the entry is a turn behind every field it takes
live**, and a row that mixes the two lets the player *see* the contradiction — which is worse than a
row that is uniformly late.

**The governing rule: two fields describing one fact must agree in the frame they ship in.** What is
left on the entry, and why:

| field | source | why |
|---|---|---|
| `id` · `label` · `species` · `sizeClass` · `huntable` | entry | identity and shape; nothing writes them after spawn |
| `x` / `y` | entry | **kept as a pair with the fog gate**, which decided this row's visibility against `entry.position` — publishing a different tile beside it would describe a herd whose presence was judged somewhere else. They cannot differ in any case: the only Population-stage writer of `current_pos` is `Herd::corral_at`, and the tile it is handed is `herd.position()`, the hex the herd already stands on |
| everything else | **live `Herd`** | see below |

- **The build meters** — `domestication`, `corralled`, `corralProgress` — through
  `intensification::build_fraction`, because the build engine accrues in `advance_labor_allocation`
  at Population.
- **`biomass`**, because `advance_husbandry`'s shed/starve shrink and the Population hunt take both
  land after the entry. This one is **not cosmetic**: the client composes the escapement ceiling as
  `max(0, B − floor·K) × rate`, taking `B` from here and `K` from the live `carryingCapacity`, so a
  stale `B` quoted every yield preview from two different turns, every turn.
- **`ecologyPhase`**, **re-derived at capture** from the same stock, capacity and ecology the row
  publishes beside it (`classify_ecology_phase(biomass, herd_capacity, herd_ecology)` — the same
  three seams `Herd::refresh_ecology_phase` uses, so it restates the sim's own call rather than
  modelling it again). The entry's copy was classified in Logistics, while the cut points beside it
  (`collapseFraction` / `stressedFraction`) and `regrowthSamples` are read live off `herd_ecology`,
  which switches rung the instant a Tame or Corral completes — so on a completing turn the published
  word and the published cuts described **different rungs**. Re-deriving is safe because **nothing in
  the sim gates on `Herd::ecology_phase`**: the rung health gates were replaced by the floor's learn
  multiplier, and its only remaining readers are the analytics log line, the display mirror, and the
  Telling's `fauna.collapsing_group_count` / `most_collapsed_species`, which sample the stored word
  and are untouched. There is no behaviour for the wire to disagree with.
- **The heading** — `nextX` / `nextY`, and `routeLength` beside them — because `Herd::corral_at`
  clears `next_pos` in Population, so a herd penned this turn published the heading of the roam its
  pen had just ended and the map drew a migration arrow on an animal that cannot move. (`routeLength`
  is live for uniformity only; a herd's route is built at spawn and never rewritten.)

**The lag became a contradiction when the work pair joined it in the same sentence.** `tameWorkDone`
/ `tameWorkCost` are live, so a completed Tame published as *"Domesticating 50 / 50 work (99%)"* —
one row, one meter, two frames — and read Domesticated the turn after, with nothing in the sim having
reverted.

**The three tests that can see this class of defect all resolve a turn in STAGE ORDER**, which is the
only arrangement in which the two frames can disagree; the snapshot unit tests fabricate the telemetry
from the registry in the same instant and are structurally blind to it.
`core_sim/tests/build_turns_closed_form.rs` pins each build meter against its work pair on the turn
that rung completes. `core_sim/tests/ecology_bands_on_the_wire.rs` pins the phase word against its own
published cuts on the turn a Tame completes — **and it authors distinct pastoral bands to do it**,
because the shipped `husbandry.pastoral.ecology` block is `{}` and inherits the wild cuts, so on the
shipped numbers a rung transition moves the ecology object without moving its cut points and no
assertion could tell the two rungs apart. `core_sim/tests/herd_row_one_frame.rs` pins the stock
against a real post-telemetry writer (a starving pen) and the heading against the turn the pen
completes.

**`HerdTelemetry` is NOT rebuilt after Population, deliberately.** Per-field live reads were chosen
over a second rebuild because a rebuild's blast radius covers the herds `advance_husbandry` sheds or
despawns later in Logistics, which today publish with no registry herd and a zeroed forecast. The
`entry` copy survives on every live-read field as the fallback for the unreachable "in telemetry,
gone from the registry" case.

See Also: "Cultivation (Intensification Phase 1a)" under Depletable Forage — the plant twin of this
mechanic (the two are near-mechanical transposes).

> `FaunaPursuit` is **not** snapshot-persisted (unlike `HarvestAssignment`): a
> `rollback` mid-pursuit cleanly cancels the in-flight hunt (the rehydrated cohort
> simply lacks the component). Pursuits are short-lived; revisit if needed. Domestication
> state lives on the `Herd` (in `HerdRegistry`), alongside `biomass`.

> **The authoritative `HerdRegistry` *is* rollback-persisted** (as of the intensification
> arc's first slice, `docs/plan_intensification.md` §0-i). Each live `Herd` — identity,
> movement (`route`/`step_index`/`current_pos`/`dwell_remaining`/`roam`/`next_pos`/`corralled_at`),
> **and** its depletable-ecology subset (`biomass`/`carrying_capacity`/`ecology_phase`/
> `domestication_progress`/`owner`) — survives a rollback because the **checkpoint carries the whole
> registry** (`SimState::herds`). It reached that state by way of a serde `HerdState` mirror on
> `WorldSnapshot.herd_registry`; the checkpoint arc made the mirror redundant and it was deleted
> (`checkpoints.md`). This closes a **latent bug**: only the lossy display `HerdTelemetry`
> (`WorldSnapshot.herds`) used to be captured, so herd biomass/position silently kept their
> post-rollback values. Restore rebuilds the derived `HerdDensityMap` + `HerdTelemetry` (as
> `advance_herds` does post-loop) so nothing is stale for a turn. The FlatBuffers client stream is
> untouched (it keeps using the display telemetry). The serde `HerdState` / `HerdRoamState` /
> `EcologyState` records and the registry-side `HerdRegistry::{from_states, update_from_states}`
> constructors that read them are **deleted** — with the mirror gone there was no producer left, and a
> decoder with nothing to decode is a second, drifting definition of a restored herd.

Market hunting shipped as the third extractive rung, later renamed `Deplete`
(`docs/plan_hunt_yield_model.md` §2 — every policy sells, so the rung is named for its
pressure); `SedentarizationScore` shipped (see
"Sedentarization" under Campaign Loop); **corrals shipped** (Intensification Rung 1c — see "Corral"
below). Still deferred (`docs/plan_wildlife_hunting_overlay.md`): the `Camp` entity, and wiring the
sedentarization hard prompt to an actual `found_settlement`. The tile-based `HuntGame` handler stays
neutralized (its client button no longer surfaces).

---

