---
paths:
  - "core_sim/src/{forage,intensification}.rs"
  - "core_sim/tests/forage_*.rs"
---

<!-- Extracted verbatim from lines 2554-2825 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Cultivation and the `Sow` verb — the plant twin of the pen

## ⛔ THE PLANT WEB IS ONE POSITION — read this before any `cultivation_progress` reference below

**A patch has ONE number: `ForagePatch::ladder_position`, how far up the plant branch it has been
worked, in cumulative work units** (`docs/plan_standing_upkeep.md` §2.8). `plant:tended` runs `0 → 50`
and `plant:field` `50 → 125`, each rung's span being its own `build.work_cost`.
`cultivation_progress`, `field_progress` and their four stamped companions (`*_cost`,
`*_retain_bar`) are **gone**; prose below that names them is describing the retired shape, and the
seam it points at is `forage::patch_rung_work_done` — the position clamped into a rung's own span,
which is what the wire's two per-rung meters are still published from.

- **`ForagePatch::standing` is derived and re-stamped on every write**, and
  `ForagePatch::set_ladder_position(position, ladder)` is the **only** mutator — it writes both fields
  together, so the pair cannot drift. The position itself is private.
- **`is_cultivated()` / `is_field()` keep their signatures and their ~hundred call sites**; they read
  `standing.held`. A Field implies a tended patch **by construction** — the Field's range begins where
  the tended rung's ends — so the reported *"Field above 0% while Cultivation reads 99%"* is
  unrepresentable rather than forbidden.
- **Every rate on a patch interpolates on that position**: a Field 40% raised converts at a whole
  tended patch's rate plus 40% of the Field's extra, and owes its keeping on the same shape. **So does
  the BASKET** — `patch_composition` blends the held mix with the raising one per species at
  `standing.credit`, through `intensification::interpolate_composition`. A Field 40% raised is 40% of
  the way from its weeded basket to its sown one: the crop's share climbs and the volunteers' falls,
  turn by turn. See `flora.md` → "THE MIX INTERPOLATES TOO" for why the earlier *"a half-weeded basket
  is not a blend of two baskets"* ruling was overturned, and for the take-selection repair that rides
  the commitment.
- **A QUEUE ENTRY NAMES A DESTINATION, so `sow` on untended ground costs the WHOLE BRANCH — 125 work
  units, not 75.** It lays two legs (`plant:tended`, then `plant:field`), passes through Cultivated on
  the way — announced on Cultivate's own channel — and holds the head of the queue until it arrives.
  The same order on ground that is already tended is **one** leg owing 75, and on a patch 30 units
  into its Cultivate it owes **20** on that leg: a previous improvement is a receipt, not a discount.
  See `intensification.md` → "A QUEUE ENTRY NAMES A **DESTINATION**, NOT A RUNG", which owns the seam,
  the published legs and the per-web kit rule.

`intensification.md` → "ONE POSITION ON THE LADDER" is the authority for the primitives; this file
covers what the plant web does with them.

## ⛔ WHAT A FIELD BUYS — production, never draw

**A Field changed how you HARVEST, when its job is to change how much the tile GROWS.** Rung 3 used to
pay a flat `biomass × field_provisions_per_biomass` on a standing crop that was never drawn down —
no escapement floor, no overdraw, `sustainable == actual` by construction. Three things followed:

1. **The harvest floor did nothing at rung 3** — the one pressure lever the player holds was inert on
   the rung the whole ladder climbs toward.
2. **The payout could not interpolate.** A managed rate and an MSY draw-down are different *kinds* of
   harvest, not two values of one rate, so `tended ↔ Field` stayed a cliff while every other rung
   quantity had gone continuous.
3. **A Field could not be over-farmed**, so rung 3 was a strictly-better thing rather than a
   commitment with a failure mode.

> ### PRODUCTION AND DRAW ARE SEPARATE CONCERNS. A RUNG MAY CHANGE PRODUCTION; **NO RUNG CHANGES THE
> DRAW.**

**Every plant rung is foraged through one `forage_take` path** — floor-live, worker-capped, drawn
down. **A Field can be over-farmed and the ⚠ fires on it**: strip it every turn and it fails.

**What rung 3 buys instead is two production gains**, both interpolating on the ladder position
exactly as the upkeep does:

| gain | where it lands | why |
|---|---|---|
| `field_capacity_gain` | the one `carrying_capacity` write in `advance_forage_regrowth` | a sown field is planted densely with the competitors pulled out, so it **holds** more standing crop |
| `field_regrowth_gain` | `patch_ecology`, the seam that already existed for this | you sowed it and you replant it, so it **comes back** faster |

**The capacity gain is also what a Sow ADVERTISES.** `forage::patch_capacity_at` is the one
expression behind that write, and `patch_destination_capacity` calls it at
`RungStanding::arrived_at(build_destination)` — so a running Sow publishes the `K` the Field will
deliver (`buildDestinationCapacity`), struck through the same seam rather than a second formula.
That matters because **the escapement floor is a fraction of `K`**, so a Sow raises the floor under
the player every turn it runs; without the destination they see the take fall with nothing saying
where it is heading. `-1` means no build in flight and is deliberately not `0`, since a capacity of
zero is a real reading on barren ground.

> #### ⛔ THE UPKEEP SCALE READS THE **TILE's** K, NEVER `ForagePatch::carrying_capacity`
>
> `forage::patch_tender_loads` takes `forage::tile_forage_capacity` — the land's own K — and never
> the patch's stored `carrying_capacity`, which is that K **already multiplied** by the interpolated
> `field_capacity_gain`. The demand interpolates on the *same* position the gain does, so a measure
> reading the boosted number multiplies the keeping bill by **2.53 on top of** the rate's own climb
> from 2.0 to 4.0 — a Field landing near **10×** a tended patch's upkeep, compounded out of two terms
> nobody chose to multiply.
>
> **⛔ AND IT IS ONE FUNCTION, `forage::patch_land_capacity`, AT ALL FOUR READINGS.** The quote, the
> claim gate, the published bill and the rot all ask the land the same question, so they cannot answer
> it differently. That matters most where the tile is **absent from `TileRegistry`** — the synthetic
> off-map patch a harness builds. `advance_forage_regrowth` deliberately keeps such a patch's *seeded*
> capacity (`if let Some(tile)`), and `advance_cultivation` resolved the same absence to
> `NO_FORAGE_CAPACITY` while its comment claimed it matched — so an off-map patch presented
> `NO_TENDER_LOAD`, owed nothing, never bled, and **kept a finished Cultivate or Field for ever with
> nobody on it**. Fixing only the one pass would have been worse than leaving it: the patch would have
> started bleeding while its claim and its published bill still read zero. All four go through the one
> seam, and `an_off_map_patch_owes_its_keeping_and_reverts_like_any_other` is the pin.
>
> **The separation is the model, not a guard.** The tile's K is the size of the *place* and it is what
> the rung is billed against; the gain is the rung's *payout*. `labor_config.json` already states the
> half of it that was written first — **the land owns K and no rung may lower it** (issue #433) — and
> this is the other half: no rung may be billed for the K it raised.
>
> Pinned by `climbing_to_field_does_not_compound_the_capacity_gain`, which asserts the demand is
> `4.0 × tile K / capacity_per_tender` **and** the precondition that `carrying_capacity` really did
> rise by the gain — without that second assertion the test passes whenever the gain silently stops
> applying.

**This is the animal web's shape, which is the argument for it** — a herd already gets a regrowth
multiplier and a density multiplier on the land's capacity at pastoral and again at pen. Plants were
the odd web out.

- **A rung may RAISE `K` and may never LOWER it** (#433). The capacity write applies the gain to the
  *tile's* capacity, so it stays idempotent and a lapsed Field hands the capacity straight back; a
  gain below `1.0` is a **config rejection**, because the retired concentration term shrank capacity
  and threw the remainder away, which made a commitment cost production.
- **The two gains MULTIPLY** through `r × K / 4`, and **the product is what was held**: they were
  chosen so the measured Field yield on the reference basket lands where the retired managed rate put
  it. **The split between them is provisional** and a feel dial. `tests/field_reference_basket.rs`
  pins the product and will fail if either moves alone.
- **Measured, on the reference basket** (`AlluvialPlain`, `K = 195`, tile `(0,0)` under seed
  `0xF10A_5EED_C011_0010`, committed to `wild_emmer`): wild **0.703** → tended **1.328** → Field
  **6.240** before, and **0.703 → 1.328 → 6.241** after. A re-expression, not a rebalance.
- **A per-rung QUOTE re-bases the land onto the rung it is asked about** (`rung_msy_take`), because
  `fieldYield` is published for every patch including a tended one. It is a *ratio*, so a patch
  already standing on that rung re-bases by exactly `1.0` — which is what keeps the live reading and
  the quote one number. `composition_for_rung`'s rule, applied to the land instead of the basket.
- **A Field now needs HANDS.** It holds far more standing crop, so one gatherer brings home a
  gatherer's load off any rung and realizing rung 3 genuinely takes a crew. That is a real new cost
  and it is intended.
- **Composition and conversion BOTH moved, and neither is the capacity story.** A Field's basket is
  `1.0 − Σ(protected)` for the crop, not a flat 100% — the members that do not stand in the worked
  ground survive the reweight (see the `stands_in_worked_ground` callout below). And rung 3 carried
  **no conversion gain at all** until `field_conversion_gain`: `favored_conversion_gain` returned the
  tended gain at `plant:tended` and the identity everywhere else, so a Field converted each unit of
  biomass at half what the tended patch beneath it did. Those two are what the tile is *made of* and
  how well it *converts* — still separate axes from how much it grows, which is what the two gains
  above buy.

**Retired with the model**: `field_provisions`, `field_fodder`, `field_harvest_production`,
`field_harvest_biomass`, `field_fodder_per_biomass`, `patch_species_quality`,
`managed_per_worker_yield`, `managed_per_worker_fodder`, `MANAGED_HARVEST_SEASON`,
`settled_biomass_fraction`, the plant side of `SourceYieldForecast::managed`, and the
`cultivation.field_provisions_per_biomass` dial they all read. **The animal web's `managed` is
untouched** — the pen is its own slice.

## Cultivation (Intensification Phase 1a)

The **plant analog of animal husbandry** (`docs/plan_intensification.md` §3), evolved past the
mechanical husbandry transpose into **Rung 1a — the worker-tended, place-local tended patch**, and now
into an **explicit policy with an investment cost**. A patch carries `ladder_position` (in **work units**, with the tended rung
complete at the top of its span) + `owner: Option<FactionId>` on `ForagePatch` — and a `Herd` carries
the exact twin, one `ladder_position` beside a stamped `standing`; the checkpoint clones the whole `ForageRegistry`
(`SimState::forage`), so both rewind with a rollback. A completed patch is a **tended patch**:
**worker-tended + place-local + higher-output + feral-if-abandoned**. *Sim-only — the client readout is a follow-up.*

> **The free path is gone (design fix).** Cultivation used to accrue **silently and for free** under
> Sustain: same labor, same tile, no cost ⇒ cultivating was always correct and there was **no
> decision**. It is now the **`Cultivate` improvement** (`Improvement::Cultivate`, plant-only — a
> harvest-stance variant until issue #442 split the pressure axis from the build verb) with a real
> up-front cost, and the **early-claim `claim_threshold` is removed** (it would let the player skip the
> investment — the whole point). Gathering still *teaches* the faction Cultivation knowledge; it just
> never tames a patch. The animal twin is the **`Corral` policy** — see "Corral".
- **Rung 1b — the earned-knowledge gate (`docs/plan_intensification.md` §4b).** Cultivation is a
  faction-level knowledge *learned by doing*, **never start-granted**: **any** forage that actually
  draws a wild patch down accrues faction **Cultivation** knowledge (discovery
  `CULTIVATION_DISCOVERY_ID` = 2003, `forage.rs`) in the per-faction `DiscoveryProgressLedger`, at the
  ladder's `knowledge.learn_rate` **scaled by the assignment's floor, over Cultivation's own
  `lesson_cost`** (`intensification::learn_multiplier` — a crew that leaves more standing learns
  faster, the food peak is ×1.0; `add_progress`, clamped to `1.0`). **It does NOT scale with the
  crew**: a lesson is credited once per source per turn, in **practice units**, which is exactly what
  keeps it apart from the build's work units (`intensification.md` → "A LESSON COSTS PRACTICE — and
  practice is NOT work"). The old `Sustain && Thriving` pair of gates is gone —
  see "The knowledge pattern" in `intensification.md`. **A patch cannot accrue
  `cultivation_progress` until the faction *knows* Cultivation** — `advance_labor_allocation` only calls
  `accrue_cultivation` once `ledger.get_progress(faction, 2003) >= knowledge_completion_threshold`.
  Knowledge is all a plain gather earns — it **never** accrues `cultivation_progress`. The `cultivation` tag →
  discovery 2003 mapping is declared in `start_profile_knowledge_tags.json` purely so it is mappable;
  **no start profile lists it**, so no faction begins knowing Cultivation.
- **The `Cultivate` improvement — the investment.** In `advance_labor_allocation`'s **Forage** arm
  (Population), a patch worked with `Improvement::Cultivate` in flight:
  - **Costs the hands it is staffed with, and nothing else.** `yield_fraction_while_building` is
    **retired** (`docs/plan_standing_upkeep.md` §2.2): `cultivate <faction> <x> <y>` **queues** the
    build and `assign_labor <f> <b> builders <n>` staffs the pool that raises it, so what a Cultivate
    costs is *the people who are
    clearing instead of gathering*, and the gatherers beside them carry exactly what they carried
    before. It is the same statement at every staffing, where the dip's price depended on whether the
    patch's standing stock was binding the crew — a regime the player cannot see.
  - **Accrues the band's BUILDERS POOL whole output** — `builders × PER_WORKER_OUTPUT`, and only
    while this patch is the **head** of that band's build queue
    (`docs/plan_standing_upkeep.md` §2.5) — **no floor
    term** (a build crew is not pulling on the patch — see "THE FLOOR CAME OFF THE BUILD RATE" in
    `intensification.md`),
    in **work units**, toward the rung's `work_cost` (sets `owner` on first accrual;
    only the owner accrues), **gated** on the faction *knowing Cultivation* and on there being
    something standing above the floor to work (`systems::labor::crew_is_working_the_source`).
    **That term reads the WHOLE stand, never the gatherers' take selection** — a selective gather
    leaves the rest standing by definition, and the builders are a band pool that is not gathering at
    all, so threading the selection in stalls a queued build the moment a player narrows a work row.
    `flora.md` → "THE BUILD IS GATED ON THE GROUND, NOT ON THE SELECTION" owns that split. See
    "An improvement costs WORK, not turns" in `intensification.md`: the cost is fixed, **turns are
    the output**, and there is no crew cap. **The crew's KIT is not in that expression** — a tool
    takes work off the **job**, never off the crew's output, which is what makes a hoe fade on a
    farm; see "The build axis" in `equipment.md`. No plant item declares the stat today.
  - **THERE IS NO HEALTH GATE, on the verb or on the accrual** (`docs/plan_harvest_floor.md` §3.2).
    `Cultivate` used to demand `EcologyPhase::Thriving` as a **start** gate, with an exemption for a
    build already underway (`ForagePatch::cultivation_underway`) and a whole start-vs-continue ruling
    to make the mid-build lapse survivable. All of it is deleted — the gate, the exemption, the helper
    and the ruling — because the floor replaced the cliff with a **rate**: a crew pulling hard on the
    ground it is clearing builds slowly, in proportion, and nothing lapses. The rejections that
    survive are the knowledge gate, the already-cultivated rejection, the other-faction-owner rule and
    the species gate; `server::tests::cultivate_is_accepted_on_a_stressed_patch` pins the absence
    positively, so a re-added phase check cannot regress silently.
  - **The other three rungs never had one**, which is why this is a *uniformity* rather than a
    relaxation: `validate_tame` carried no phase gate at all (a herd's `ecology_phase` swings as it is
    hunted), `validate_sow` gates on the rung's **`site_requirement`** — static land that cannot lapse
    — and `Corral` gates on ownership and the species ceiling. Rung 2 of the plant web was the only
    verb reading a value that moved under its own build.
  - **Accrues AFTER the turn's take**, so the turn pays exactly what the pre-commit forecast promised
    (forecast == actual). The turn progress reaches the job's cost is the last preparing take; the full tended
    yield starts the next turn.
  - **THE KEEPING POOL PAYS THE RATE, AND THE BUILDERS PAY NONE OF IT**
    (`docs/plan_standing_upkeep.md` §4.6a). The meter under a running `Cultivate` is at risk like any
    other and is owed the same rate — to the band's `agriculture` pool, at any fullness — so the
    pace is `work_cost / crew` and staffing the pool is what stops the ground rotting under the
    builders. Pinned by
    `forage_cultivation::a_kept_cultivate_finishes_in_its_stated_turns_and_an_unkept_one_is_slower`
    and `every_staffed_build_crew_climbs_when_the_keeping_is_met`.
  - **Break-even, and it is now a CROP choice** (`work_cost` 50). The figures below were measured
    under the retired 0.25 dip and describe **the shape, not today's numbers**: what a build costs is
    now the hands on it, so the forgone yield is whatever those hands would have gathered.
    Measured on the reference basket below: the dip forgave **0.527 prov/turn × 25 = 13.19 prov**
    across the build; favoring the basket's **best** staple then out-pays wild Sustain by
    **+0.625/turn** and recoups in **21.1 turns**, while favoring a **marginal** member of the same
    basket pays **+0.213/turn** and takes **61.8** — nearly 3× as long on the same tile for the same
    25 turns of work. *That spread is the decision the rung exists to make.* Cultivating is still
    correct only if you intend to stay — the decision the free auto-accrual erased.
  - `ForagePatch` methods: `is_cultivated`/`accrue_cultivation`/`decay_cultivation` (the early-claim
    `claim_cultivation` is **removed**).
- **Tended yield — a WILD STAND, gathered place-local** (slice 7 — the rung-2 correction). A tended
  patch is **worked, not passive**, and it is **still wild**: it rides a curve
  (`cultivation.tended_regrowth_gain`, folded in by **`forage::patch_ecology`** — the plant twin of
  `fauna::herd_ecology`, and the one seam every consumer resolves a patch's ecology through). **Flora
  Roster S2 retired the regrowth boost to a NEUTRAL 1.0** (`docs/plan_flora_roster.md` §4.3): once S1
  made competitor-removal explicit, a growth boost double-counted it, so tending pays through the
  **composition + conversion** of a committed crop, not the curve. **#433 fixed what "composition"
  means**: rung 2 **weeds** the tile's basket (the favored share rises to `min(1, share ×
  tended_weeding_gain)`, taken from the least abundant first) and **never touches `K`** — the retired
  concentration term cut a committed tile's capacity and discarded the remainder — and it pays
  `tended_conversion_gain` on the **favored species' term only**, which is the debt S2 recorded and
  left unpaid when it retired the regrowth boost with nothing in its place. It is gathered by the
  **ordinary `forage_take` path**, exactly like rung 1: **floor-live** (the assignment's own
  escapement floor — there is no policy axis left to be live on), **worker-capped**, and **drawn down** — so a tended patch **can
  be over-farmed** and the overdraw ⚠ fires on it. This is the exact shape a **pastoral** herd already
  had; the plant web used to collapse a rung *earlier* than the animal web, and that asymmetry was the
  bug. **A committed crop still out-yields the same patch's wild Sustain** on good ground — the
  intensification incentive, now carried by composition + conversion (guaranteed by the roster's bar,
  `core_sim/tests/flora_roster.rs`) rather than the retired boost. A *bare* tended patch (no crop)
  still pays **exactly** wild — no commitment means no weeding and no conversion gain, and
  `tended_regrowth_gain` is neutral, so every term is the identity.
  > **THE REFERENCE BASKET the measured figures in this file are taken on** — `AlluvialPlain`,
  > `K = 195`, the realization of tile `(0,0)` under seed `0xF10A_5EED_C011_0010` (the one the shipped
  > `sweep_tiles` fixtures use): `wild_emmer` 0.375 / `wild_tubers` 0.292 / `tobacco` 0.208 /
  > `wild_rice` 0.125, giving a wild basket rate of **0.0577** food and **0.0415** trade per biomass,
  > MSY 12.19 biomass/turn. Best staple `wild_emmer`, marginal member `wild_rice`. **Quote the basket
  > whenever you quote a number** — since #433 a per-tile figure means nothing without the
  > realization it came from.
  Working a completed improvement is **just a harvest** — it does not hold the rung. What the decay
  pass reads is `ForagePatch::upkeep_supplied`, this patch's **share of the band's `agriculture`
  pool** (§2.5), carried across the turn boundary by the Population→Logistics lag. The old
  even-split-across-all-the-owner's-bands payment in `advance_cultivation` is **retired**, as is the
  flat `tended_provisions_per_biomass` managed rate.
  - **Completion RETIRES THE QUEUE ENTRY — a completed patch is never left building.** A queued
    build means "the band's builders are raising this"; the moment the meter fills that stops being
    true *and can never become true again on this ground*, so `Cultivate` is a dead rung there.
    `advance_labor_allocation` therefore removes the entry from `LaborAllocation::build_queue`,
    which hands the **whole pool** to whatever the player put next — the row itself is untouched, so
    the tile, the **committed species**, the take crew and the stance all simply stay as they are.
    It is announced, so the player sees the head move. **Nobody is freed**: the builders never stood
    on the source, and the completion hand-off onto the keeping is retired (§2.3 — the keeping bill
    starts at the first work banked, so the failure it guarded cannot happen).
    **The completing turn still pays the build's whole price** (the
    accrue-after-take ordering). The completion event's detail is
    `status=complete action=cultivate x=… y=…`; the `retired_policy=` token went with the constant
    (issue #442 — the pass used to rewrite `policy` to `HARVEST_POLICY_AFTER_BUILD`). **This is the
    one seam for all four build verbs** — `Sow`, `Tame` and `Corral` clear identically, from the same
    post-loop pass; **every accrue helper on both webs now returns a "this call finished it" `bool`**
    (`ForagePatch::accrue_cultivation` / `accrue_field`, `Herd::accrue_domestication`), mirroring
    `Herd::accrue_corral`, so the feed line rides the *transition* rather than a post-hoc
    `is_cultivated()` that is true for every crew once anyone has finished.
    **Clearing the verb is a separate question from announcing it**, because `cultivate`/`sow` set the
    improvement on every band working the tile — see "Completion CLEARS the improvement" in
    `intensification.md` for the once-per-source "nothing left to build" test that hands a
    non-finishing crew's verb back, and why it has to sit ahead of the Field arm's early return.
    **There is no lapsing gate left to worry about** (`docs/plan_harvest_floor.md` §3.2): a patch that
    is drawn down slows its own meter rather than stalling it, so the only way a build stops is the
    crew taking nothing at all — and then nothing is finished, so there is nothing to hand off.
- **THE DECAY IS PROPORTIONAL TO THE SHORTFALL, AT THE RUNG'S OWN RATE — AFTER A GRACE, NEWEST RUNG
  FIRST, and never silently.** `advance_cultivation` (`forage.rs`, `TurnStage::Logistics` alongside
  `advance_forage_regrowth`) is the **decay/feral** pass only, and it asks exactly one question:
  **how short of the hands this meter needs was it this turn?** The at-risk rung's `upkeep_demand` is
  what holding it costs, whoever owes it supplies `upkeep_supplied` against it, and

  ```text
  decay_this_turn = (shortfall / demand) × the rung's own meter_decay.per_turn
  ```

  **The demand and the rot rate are separate dials** (`docs/plan_standing_upkeep.md` §2.4). Shortfall
  used to *be* the decay, which welded them: raising a demand made the improvement rot faster in
  exact proportion, so neither could be retuned. Splitting them is what let the plant demands become
  whole numbers a player can staff exactly while the rot rates stayed precisely where they were.
  - **THE BAND'S KEEPING POOL OWES IT, AT ANY METER FULLNESS** (`forage::patch_upkeep_supply`,
    `docs/plan_standing_upkeep.md` §4.6a) — from the **first work banked** until the last. A
    mid-`Cultivate` patch is billed exactly as a finished one is, and to the same hands.

    **The meter's FULLNESS used to decide who paid** — the build crew below its cost, the pool at it
    (`forage::patch_is_maintaining`, deleted). Two states reported from ordinary play were wrong under
    it: a **half-built** meter whose builders had left could not be held at all, bleeding its full
    rate with keepers idle in the `agriculture` role and no command that could aim them at it; and a
    completed rung eroded to 99% flipped into *building*, so it stopped being the pool's business at
    the moment it began needing it. *"You cannot be billed to hold something you have not finished
    building"* is deleted with it: **you can** — what you cannot be billed for is ground with nothing
    on it at all.
  - **AND THE RATE IS NOT A TAX ON BUILDING, so a Cultivate's pace is `work_cost / crew`.** A build
    crew supplies none of the rate, so its whole output is progress and **a lone builder banks a whole
    worker-turn** on a rung that costs two hands to hold. What can still eat a build is the **rot** —
    the ground going backwards while the keeping is short — which is a *countdown* term
    (`RungDef::build_balance`, published as `meterRotPerTurn`) and never an accrual one.
  - **The builders' identity no longer changes the supply, but the meter's still does.** A crew
    mid-`Cultivate` on a patch whose half-sown Field is the at-risk meter is not sowing — and the pool
    holds the Field anyway, because holding is not building.
  - **THE VERB NAMES THE METER, AND THAT IS WHAT SURVIVES THE ONE-TURN CARRY.** The supply is stamped
    in Population and read by the *next* Logistics pass, so it has to describe the meter that pass
    will judge — not the one that was at risk when it was written. `patch_upkeep_supply` therefore
    takes the **newest** of two readings, the meter with progress on it and the meter the crew's verb
    is filling: a crew starting a `Sow` on a finished tended patch answers for the **Field** from its
    first turn, before that meter has any progress at all. Reading progress alone credited their work
    to the tended rung underneath, and the next pass — seeing a Field that now *did* have progress —
    bled `0.75` off the very meter they had just started, on turn two of every Sow.
  - **THE SUPPLY ACCUMULATES ACROSS THE BANDS WORKING THE SOURCE** (`+=`, zeroed by this pass at the
    top of each turn). The demand is per-**source**, so two bands each put a fraction of it on the
    ground; assigning would let whichever band the loop visited last speak for all of them — a crew
    *gathering* a patch a second crew is sowing would overwrite the sowers' supply with its own zero.
  - **The completion hand-off is RETIRED** (§2.3). It moved a finished build's crew onto the band's
    `agriculture` role so a brand-new rung did not decay before anybody noticed it cost something —
    which cannot happen now that the bill starts at the first work banked. Staffing keepers *during* a
    build is no longer merely harmless: it is what stops the meter rotting under its own builders.
  > #### GATHERING A PATCH NO LONGER HOLDS IT — the behavioural headline
  >
  > The retired `ForagePatch::tended_this_turn` was set by **any** crew on the tile, so a patch
  > somebody was *harvesting* never decayed: holding an improvement was free for exactly as long as
  > you were taking from it. Holding and taking are separate allocations
  > (`docs/plan_standing_upkeep.md` §2.2), so a band that gathers and staffs no keeper watches the
  > ground it improved revert underneath it. **The cost is TWO hands per tended patch and FOUR per
  > Field**, forever — whole numbers a player can staff exactly, where the retired sub-worker demands
  > rounded up to one hand and threw the rest of that hand away. They are staffed from the band's
  > `agriculture` pool rather than per tile (§2.5), so a band holding several patches pays the sum and
  > wastes nothing. Pinned by
  > `forage_cultivation::gathering_a_patch_does_not_hold_it_but_one_keeper_does`.
  >
  > **AND THE MIRROR HOLDS: keeping a patch does not require gathering it.** A band that finishes a
  > Cultivate and moves its foragers to a richer stand still *holds* that ground, so its row survives
  > at zero gatherers and goes on drawing from the pool — `assign_labor forage <x> <y> 0` is *"stop
  > gathering"*, not *"this band has nothing here"*. Pinned by
  > `forage_cultivation::a_patch_with_no_gatherers_is_still_kept_by_the_bands_pool`;
  > `intensification.md` → "A SOURCE ROW IS THE BAND'S HOLDING" owns the seam and the retirement rule
  > that bounds it.
  - **The grace.** `ForagePatch::neglect_turns` counts **consecutive turns of shortfall** (a single
    turn whose demand was met wipes it — it is not a lifetime budget), and the bleed applies only
    while it **exceeds** the at-risk rung's `upkeep.grace_turns` (`RungDef::upkeep_grace_turns`; the
    build's own grace is `null` on both plant rungs, because this branch no longer counts un-worked
    turns at all). A crew re-tasked for a turn or two, a band that walked to answer a raid: none of
    those cost the investment. The animal twin is the same counter gating the shed in
    `fauna::advance_husbandry` — **one trigger, two penalties**, though the animal branch still
    counts *un-worked* turns until its own slice lands.
  - **IT IS CONTINUOUS IN THE SHORTFALL, AT ANY METER FULLNESS.** Half the hands a meter needs is
    half a shortfall and bleeds at half the rung's rate, on a meter being raised exactly as on a held
    one — one pool, one arithmetic. The binary flag made a crew of one and a crew of ten equally
    sufficient, so under-crewing cost exactly nothing until it reached zero. **A half is reachable on
    the SHIPPED ladder now**, which is most of what the whole-number retune bought: pinned by
    `forage_cultivation::a_half_staffed_keeping_bleeds_at_half_the_rungs_rate` on the shipped
    demands, and at the seam by
    `forage::tests::a_half_staffed_keeping_is_half_short_on_the_meter_it_is_raising`.
  - **The bleed is in absolute WORK UNITS** — the rung's `upkeep.meter_decay.per_turn`, `0.5` on
    `plant:tended` and `0.75` on `plant:field`. Both are **the pacing-neutral inversion of the
    retired `0.01 × work_cost`** and were held there through the demand retune, so a wholly
    unmaintained improvement decays at exactly the rate it always did (~100 bleeding turns to lapse
    fully); they are *not* a considered spread. Pinned as arithmetic by
    `forage::tests::the_plant_rot_rates_are_exactly_what_the_retired_decay_fraction_bled`. **A meter
    that reaches zero forgets its stored cost too**, so the ground reads as unstarted rather than as a
    wild patch quoting a price nobody is paying.
  - **A COMPLETED RUNG *IS* LOST ON THE FIRST BLEEDING TURN AGAIN, AND THAT IS NOW A ROUNDING.** A
    finished meter sits *exactly* at its own cost, so a `progress >= cost` predicate made the first
    `0.5` flip `is_cultivated()` — the reported bug, patched for one slice by a **stamped retention
    bar** at `retain_fraction` `0.75`. **The one-position ladder deletes that bar because it removes
    the cliff it was patching** (`docs/plan_standing_upkeep.md` §2.8): the payout and the keeping both
    interpolate on the position **at both rung boundaries**, so a patch at `49.99` of a 50-unit rung
    pays and owes 99.98% of a tended patch and losing the rung costs a fraction of a percent. It held
    at `wild ↔ tended` only until the Field's own model was fixed — see "What a Field buys" below.
    `intensification.md` → "`retain_fraction` AND THE RETENTION BAR ARE DELETED" owns the seam and the
    guard.
  - **THE UNWIND IS ARITHMETIC NOW, not a rule the pass honours.** There is one meter —
    `ForagePatch::ladder_position`, cumulative work units up the branch — and the Field's range sits
    **above** the tended rung's, so a decay eats the Field first and reaches the ground beneath only
    once the Field is wholly gone. *"Cultivation cannot move while the Field has progress"* is a
    property of the number rather than an ordering `advance_cultivation` has to get right, and the
    state below is unrepresentable rather than merely forbidden.
    > **The state this makes unreachable.** Bleeding two independent meters together knocked a
    > *completed* tended patch to `0.99` during a gap in the Sow work; once the crew returned, the
    > running `Sow` marked the patch worked every turn, so rung 2 could neither decay further nor
    > re-accrue. The patch was stranded one hundredth below a rung it had already paid for,
    > **permanently**. A single position cannot express it.
  - **A lost rung is ANNOUNCED**, on the edge where the position falls out of a rung's span —
    `ForagePatch::decay_ladder` returns **which rung this call took the patch out of**, the exact
    mirror of the accrue helpers' "did this call finish it", and `forage::announce_rung_lost` pushes
    the verb's **own** feed kind (`Cultivate`/`Sow`, detail `status=feral reason=untended action=…`).
    Once, not every turn of the bleed that follows: the 25-turn payoff has already been destroyed. The
    animal twin is `fauna::announce_pen_lost`.
  - **On the wire:** `upkeepDemand` / `upkeepSupplied` / `upkeepShortfall` / `upkeepWorkersNeeded`
    — what holding this rung costs, what the crew that owes it paid, what went unmet (i.e. what it is
    losing) and how many hands would stop it. **All four are published on BOTH sides of completion**,
    because the rate is owed either way: over a patch mid-Cultivate `upkeepWorkersNeeded` reads as
    the hands that hold a half-built meter, exactly as over a held one it is the keepers that hold a
    finished rung. One arithmetic, one sentence: *hands to meet the demand*. It is **not** a minimum
    viable build crew — a build crew supplies none of the rate (§4.6a) — and it used to read `0`
    mid-build, on the since-retired premise that an unfinished meter owed no keeping. Plus
    `hasNeglectGrace` / `neglectGraceRemaining` — the **countdown**,
    not the counter (`0` = reverting now; a worked patch reads `grace + 1`, the honest *"walk away and
    you have this long"*), published through the *same* `patch_unwinding_rung` seam the pass bleeds
    through so the wire cannot count down against a rung the sim is not touching. **That seam is the
    claim's own question asked with NO VERB** (`NOTHING_IN_FLIGHT`) — the same question
    `patch_claims_keeping` asks *with* one, so the claim gate and the decay pass cannot drift the way
    they did when a build's first turn drew a share of zero on a staffed role
    (`intensification.md` → "The band's demand is the SUM"). The retired `patch_keeping_meter` used
    to be that shared function; its two jobs are now `patch_claims_keeping` (the gate) and
    `patch_keeping_basis` (the bill), and what holds *those* together is the **stamp**
    (`upkeep_demanded`), which every reader takes. The
    decay pass and the wire are byte-identical across that change. `hasNeglectGrace =
    false` = a wild patch with nothing at risk, which is most of them — read the bool first, as with
    `owner`/`hasOwner`.
  - **Stage-ordering** is unchanged: Logistics runs *before* Population, so the `upkeep_supplied`
    this pass reads was written by the labor arm **last** turn (a deliberate one-turn-lag
    carry-across-turns signal, exactly as the flag it replaced was; it is cleared here and re-stamped
    next Population stage). Net: a patch whose keepers meet the demand every turn never decays; a
    patch whose keepers leave starts counting toward its rung's grace one turn later.
  - **THE SHORTFALL IS DERIVED, NOT STORED, and `upkeep_supplied` is the only stored fact.** The labor
    arm visits only sources some band is assigned to, so a stored shortfall would read a tidy `0` on
    exactly the abandoned patches that are reverting — and the wire row would say *"demand 0.75,
    supplied 0, shortfall 0"* while the sim bled the ground underneath it. `forage::patch_upkeep_demand`
    / `patch_upkeep_shortfall` / `patch_upkeep_workers_needed` are the one definition, reached by the
    decay pass and by the snapshot alike — so the demand the sim bleeds against and the demand the
    wire bills for can never be two different rungs' answers.
    > **TWO OF THE THREE READ THE STAMPED BILL, NOT THE LIVE DEMAND**, since the position moves
    > within a turn: `patch_upkeep_shortfall` and `patch_upkeep_workers_needed` resolve through
    > `forage::patch_keeping_basis` (what the keepers were actually handed), and only
    > `patch_upkeep_demand` is the live interpolated cost. That is what keeps the published trio
    > satisfying `demand − supplied == shortfall`. **The head count must follow the bill too** — it
    > briefly did not, and published *"wants 3, you have 2"* beside a zero shortfall on a patch
    > mid-`Sow`, because the bill is stamped before the accrual and the live demand had already
    > risen.
- **The loop (the settle pull).** Sustain-forage a thriving patch → *learn* Cultivation → **choose** to
  staff a Cultivate crew for ~25 turns at two hands → the patch becomes tended → a band tending it collects the
  higher tended yield **place-locally** → move the band away and it goes feral, reverting to wild.
  Place-locality + feral + a sunk investment = the band is **pinned near its farm**: intensifying
  raises output *and* deepens the anchor.
- **`cultivate` command (repurposed)** — `cultivate <faction> <x> <y>` (`handle_cultivate`; unchanged
  proto/runtime/text plumbing, `CommandEventKind::Cultivate`) **sets the `Cultivate` improvement** on
  the band(s) already foraging that tile (`queue_build_on_working_bands`) — the command form of
  what the client's checkbox does. It **claims nothing**, and since issue #442 it touches the
  improvement slot only, so the band's stance and its committed crop survive by construction (the
  `merge_target` helper that used to carry the crop across a whole-target rewrite is deleted). Gates
  (`validate_improvement`'s `Cultivate` arm): faction knows Cultivation, not already cultivated, not
  another faction's; plus a rejection when **no band is foraging** the tile (staff it first). **No
  health gate** — see "THERE IS NO HEALTH GATE" above.
- **THE BUILD VERB IS DERIVED FROM THE METER** (`forage::patch_build_verb`,
  `docs/plan_standing_upkeep.md` §2.4). A patch with progress on a meter is building that rung; a
  meter at its cost is maintaining; **only a meter at zero needs the player to say which rung this
  ground climbs**, which is what the four verb commands are for. So a tended patch that has slipped
  below its cost **names its own rung** without the player working out which job a repair is.
  - **⛔ BUT NOTHING RE-ADOPTS IT** (`docs/plan_standing_upkeep.md` §2.4). Deriving the verb says
    *what* a repair would be; it does not put the source back in the band's **build queue**.
    Repairing an eroded rung is a **fresh decision**, made by re-queueing — which is what keeps a
    one-percent-eroded Field from displacing the build the player actually ordered off the head of a
    pool funded all-hands-on-one, being topped up, slipping again, and oscillating there while the
    real build stands still. Pinned by
    `forage_cultivation::a_rung_completes_erodes_and_is_repaired_only_by_re_queueing_it`.
  - **`abandon_improvement` is RETIRED** with the stored authority it used to clear (proto field 46
    reserved, never reused), because a command that cleared a *derived* value would either do nothing
    or fight the derivation. **What came back in its place is disposal, not arbitration** (§2.5):
    `unqueue <faction> <x> <y>` drops the declaration and leaves the row, its take crew, its kit and
    the meter alone, and `abandon <faction> <x> <y>` puts the whole **holding** down — row and entry
    together — leaving the meter to rot back at the rung's own rate. Together they are the undo a
    declaration never had: `cultivate <faction> <x> <y> 0` used to *set* the verb with no builders
    and clear nothing, so an unwanted declaration was stuck for the life of the band.
  - **A fully feral patch clears owner, species, cost and rung together** — `reconcile_owner`'s
    "nothing is left of either improvement" and the derivation's "a meter at zero needs a
    declaration" are one notion of empty, pinned by
    `forage_cultivation::a_fully_feral_patch_clears_its_owner_species_and_rung_together`.
- **Improvement validation** — `Improvement::valid_for_forage` / `valid_for_hunt`: `Cultivate`/`Sow`
  are plant-only and `Tame`/`Corral` animal-only, both stated as **exhaustive** matches so a new verb
  fails to compile until someone says which web it belongs to. `validate_improvement` rejects a
  cross-web verb (and a failed gate) with a clear failure event before touching the allocation.
  `assign_labor` validates the **stance** only; unassigning (`workers == 0`) is always allowed, so a
  player can always abandon an investment.
- **Sedentarization (folded)** — `sedentarization_tick` reads `herds.domesticated_count(faction) +
  forage.cultivated_count(faction)` for its **domestication** input: plant + animal domestication
  share the one driver (no new weight, no re-balance).
- **`crew_needed` IS RETIRED, and so is the `workers_needed` floor it fed.** It was a staffing
  floor under the source's published `workers_needed`, needed only because that count was inverted
  out of a **dipped** take: committing to a 25-turn improvement asked for *one* forager where the
  same wild patch asks for two, so doing more work required fewer people. With each role staffed on
  its own row there is no blended count for a floor to raise — `workers_needed` is the **take**'s own
  count, `upkeepWorkersNeeded` is the **keeping**'s, and the builders are the band's own pool.
  `RungDef::build_crew_needed`, `LadderConfig::build_crew`, `source_crew_needed`
  and the `cultivateCrewNeeded` / `sowCrewNeeded` wire slots went with it (the slots stay
  `(deprecated)` — FlatBuffers field ids are positional). **The crew is still the throughput**: a
  Cultivate run by a pool of one takes 50 turns, by two 25, by ten **5**, with no cap beyond the
  band's own head count (`docs/plan_unit_costed_work.md` §1.2). The declared cost was priced against two hands, which
  is why 25 turns is still the reference reading.
- **Config.** The plant rung-2 **build dials moved to `intensification_ladder.json`**'s `plant:tended`
  rung (`build`: **`work_cost` 50** work units → 25 turns to prepare **at the rung's crew of 2, at the
  food peak, with no gear**, `upkeep.work_per_turn` **2.0** what holding the rung costs per turn
  **per tender-load**, out of the band's `agriculture` pool — a whole number on the reference tile
  and only there, since the load scales it with the ground (2.0 on `AlluvialPlain`, 0.718 on
  `PrairieSteppe`, 2.154 on `RiverDelta`) —
  `upkeep.meter_decay` **`{ per_turn 0.5 }`**, what an *unkept* patch loses per turn (its
  `retain_fraction` companion is **deleted** — see "A COMPLETED RUNG *IS* LOST ON THE FIRST BLEEDING
  TURN AGAIN"),
  `upkeep.scaled_by` **`source_load`** (× the patch's **tender-load**, `tile forage capacity /
  capacity_per_tender` — one tile is what the load measures, and the tiles are not the same size),
  **`upkeep.grace_turns` 2** — cleared, weeded ground keeps its clearing a couple of
  turns after the crew stops; **`crew_needed` and `yield_fraction_while_building` are both retired**
  — the builders are the band's own pool and the gatherers beside them are untouched, so neither a
  staffing floor nor a dip has anything left to say), so
  the plant and animal ladders can only be tuned together (see "The Intensification Ladder"). What stays
  in `labor_config.json` `forage.cultivation` (`CultivationConfig`): **`tended_regrowth_gain`** (1.0 —
  NEUTRAL since Flora Roster S2: a tended patch's stock regrows exactly as fast as wild. It began as
  the plant twin of `husbandry.pastoral_gain`, but S1 made competitor-removal explicit, so a growth
  boost double-counted it; tending pays through composition + conversion and the rung-2 "wild <
  tended" guarantee moved to the roster. Kept as a playtest dial; only a gain *below* 1.0 is
  rejected), **`tended_weeding_gain`** (1.5 — how far rung 2 can push the favored species' share,
  `min(1, share × gain)`; the renamed `tended_concentration_gain`, which used to multiply `K` and now
  moves *composition*) and **`tended_conversion_gain`** (2.0 — rung 2's conversion multiplier on the
  **favored species' whole yield vector**, #433; the term that makes a 25-turn Cultivate pay back in
  the teens of turns instead of the eighties, and the reason a marginal favorite barely moves the
  number while a dominant one pays twice). Both validated finite and `>= 1.0`.

  **`field_conversion_gain`** (2.0 — the **rung-3 twin**, the same multiplier on the favored crop's
  whole yield vector once the patch is a sown Field. It exists because **rung 3 had no conversion
  gain at all**: `forage::favored_conversion_gain` returned the tended gain at `plant:tended` and the
  identity at every other rung, Field included, so a Field converted each unit of biomass at *half*
  what the tended patch beneath it did. Reported from play: a completed tended patch paid 2.00
  food/turn and the same tile sown to a Field paid 1.33 at the same two tenders — a rung paying
  **less** than the rung it was built on, because rung 3's own rate (`field_provisions_per_biomass`)
  retired with the managed-harvest model and nothing replaced it. It ships **equal** to the tended
  rung's, and that is deliberately the minimum: equality restores the invariant and nothing more.
  Validated finite and `>= tended_conversion_gain`, which makes **a rung may never pay less per unit
  than the rung beneath it** a load-time rejection rather than a number someone has to remember —
  the failure it guards is silent, because the Field's capacity and regrowth gains still read like a
  better rung right up until you count the food. **PLAYTEST DIAL**, §4.14.)
  > **⛔ NEITHER REWEIGHT MAY TOUCH A MEMBER THAT IS NOT STANDING IN THE WORKED GROUND.**
  > `FloraDef::stands_in_worked_ground` (default **true**, `false` on `kelp` / `shellfish_beds` /
  > `river_fish`) names the physical fact, not the mechanic: *does this member stand in the soil the
  > crew is turning over.* **Weeding** pays its gain only out of the clearable members —
  > `target = share + min(asked, Σ clearable)` — so the favoured share rises by what was clearable and
  > **no further**, rather than reaching into the protected ones for the shortfall. **A Field** gives
  > the crop `1.0 − Σ(protected)` instead of `1.0`, so a Sow beside a channel publishes **two** entries
  > where it used to publish one.
  >
  > **`cultivation_ceiling` IS NOT THE PREDICATE, and that is the whole reason the field exists.** Ten
  > of the thirty-three species are `ceiling: wild`, and they split two ways: you genuinely *can* clear
  > oak mast, pine nut, cloudberry, mesquite, rock tripe and arctic greens off ground you are working —
  > you simply cannot farm them. Gating on the ceiling would have shielded all six and quietly made a
  > Cultivate much weaker on woodland and scrub. A test asserts that pair — `ceiling: wild` **and**
  > standing in worked ground — so the two questions cannot be re-merged. `sea_kale` is the judgement
  > call and is left **clearable**: samphire is a salt-marsh plant rooted in ground, unlike the mussels
  > beside it.
  >
  > **The remainder cannot go negative, by proof rather than by clamp:** a crop must be clearable to be
  > committable (a load-time rejection), so the crop's own share is never inside the protected sum.
  >
  > **Playtest consequence worth knowing:** a Field beside a channel now pays a little food from the
  > fishery even when the crop is a zero-provision cash crop. That is correct — the fish were always
  > there — but it is a visible change on navigable tiles.

  **`capacity_per_tender`** (195.0 — **HOW MUCH STANDING CROP ONE TENDER LOOKS AFTER**, the divisor in
  `forage::patch_tender_loads` and the plant twin of `fauna_config`'s per-species `animals_per_herder`.
  **One global ratio, deliberately not one per flora species**: a patch's basket is several species
  and a Field forces it to one, so a per-crop divisor would swing as a patch climbs the ladder —
  the same compounding the tile-K rule exists to prevent. 195.0 is the **reference tile's own K**
  (`AlluvialPlain`), which is what makes the move onto the scale provably pacing-neutral there:
  a tended patch on that ground still owes exactly 2.0 work/turn and a Field 4.0. Validated finite
  and `> 0` — a zero divisor is a division by zero and a negative one an inverted load, and both
  read as live dials. **PLAYTEST DIAL**, and the number moves in `plan_standing_upkeep.md` §4.14.)
  **`field_reference_crop_share`** (0.5625), **`field_share_cost_floor`** (0.25) and
  **`field_share_cost_ceiling`** (2.0) — **WHAT A SOW COSTS BY HOW MUCH OF THE TILE IT REPLACES**, the
  `plant:field` rung's own per-source price multiplier (`forage::field_cost_multiplier_at_share`; see
  "A Sow is priced by what it replaces" below). The anchor is the reference basket's own **weeded**
  share of `wild_emmer` on `AlluvialPlain` (`0.375 × tended_weeding_gain`), which is what makes the
  shipped 75-work-unit price pacing-neutral there, exactly as `capacity_per_tender` is that tile's own
  `K`. Validated: the anchor finite and in `0.0..1.0` **exclusive of 1.0** (it is a `1 − share`
  denominator), the floor finite and `> 0` (a free Sow still collects both Field gains), the ceiling
  finite and `>= floor`. **PLAYTEST DIALS**, and the numbers move in `plan_standing_upkeep.md` §4.14.
  **`field_concentration_gain` is RETIRED** — a Field's favored share is set by the reweight
  (`1.0 − Σ(protected)`), not by a gain, so there is no gain left to tune. Plus the
  **Rung 1b earned-knowledge** levers `knowledge_progress_per_turn` (0.05 — faction Cultivation earned
  per gathered turn *at the food peak*, ~20 turns to know; the floor scales it; since split into the
  ladder's `learn_rate` 1.0 over a `lesson_costs` entry of 20, which is the same number) and
  `knowledge_completion_threshold` (1.0 = the ledger's completion value). The early-claim `claim_threshold` is **removed**. The build dials'
  invariants (`0 < work_cost`, `0 < upkeep.work_per_turn` and `0 < upkeep.meter_decay.per_turn`
  **when present**
  — `null` is how a rung says its meter does not bleed, and a parked `0` is rejected because it would
  mean the same thing while reading like a live dial — `grace_turns < work_cost / reference_output` (a
  grace that outlasts its own build makes walking away free)) are now **enforced on every load path**
  by
  `LadderConfig::validate()`, which owns them — as are the **knowledge** invariants
  (`learn_rate > 0`, every `lesson_cost > 0`, **every knowledge the sim teaches priced at all**,
  `0 < completion_threshold <= 1`), which moved to the ladder with those dials in slice 4. **The levers homed here are now validated on every load path**
  (slice 7 — the old "asserted over the *builtin* only, so a `LABOR_CONFIG_PATH` override that breaks it
  is accepted silently" gap is **closed**): `LaborConfig::validate()` enforces the **plant ladder's
  monotonicity** — `field_regrowth_gain × field_capacity_gain > tended_regrowth_gain` (tended <
  field, the payoff twin of `FaunaConfig::validate`'s `pen_gain > pastoral_gain > 1`). **Both rungs
  are drawn down through the same MSY skim**, so every other term — the basket, the conversion, the
  tile's `K`, the shared `r/4` curve — is common to both sides and cancels, and the check is
  scale-free in the biome *and* free of which species it is asked about. The retired form compared a
  flat `field_provisions_per_biomass` at tending's saturated best case, which is what kept the check
  scale-free in `K` *and* independent of which species it is asked about. The `tended_regrowth_gain` check is now a **coherence floor only** — `>= 1.0`,
  not `> 1.0` — since S2 retired the "wild < tended" guarantee to the roster (`flora_roster.rs`); it
  forbids only the incoherent case of tending growing a stand *slower* than wild.
- **Intensification display snapshot (on the wire, consumed by the client-dev rendering slice next).**
  The intensification-ladder state is now exported to the FlatBuffers client stream (append-only per
  the schema discipline; `snapshot.fbs`, `sim_schema`, `snapshot.rs`), on both `WorldSnapshot` and
  `WorldDelta`:
  - **Forage patch cultivation** — a new per-tile `foragePatches:[ForagePatchState]` list
    (`snapshot_forage_patches`, from the `ForageRegistry`, stable `(y, x)` order). Per patch: tile
    `(x, y)`, `cultivationProgress:float` (0..1), `isCultivated:bool` (tended = progress ≥ 1.0),
    `owner`/`hasOwner` (tending faction; `hasOwner = false` = wild), plus `biomass`/`carryingCapacity`/
    `ecologyPhase` for optional patch-health. This is the client's first per-tile forage-patch payload
    (previously forage was visible only via `laborAssignments`).
  - **Faction ladder knowledge** — a per-faction
    `intensificationKnowledge:[IntensificationKnowledgeState{ faction, cultivation, herding,
    seedSelection, penning }]` list (`snapshot_intensification_knowledge`, from the
    `DiscoveryProgressLedger`), mirroring `sedentarization[]`. **One field per rung-transition**, so it
    reads as the ladder itself — `wild --cultivation--> tended --seedSelection--> field` and
    `wild --herding--> pastoral --penning--> pen` — each the 0..1 progress on discoveries 2003 / 2004 /
    **2005** / **2006** (the last two appended in slice 4, **append-only**: `cultivation`/`herding`
    keep their shipped slots). A faction is emitted only once it has begun learning *something* (all
    zero → skipped). Client renders these as learning/known meters like the sedentarization meter;
    the **two-meter split** (faction knowledge vs per-source build progress, §4.1 — the root UX fix)
    is the client slice, and both meters are already distinctly on the wire.
  - **Herd corral** — `HerdTelemetryState.corralled` (see the corral section above).
- **Follow-ups:** **Rung 1c — corral** (the fauna-side pen behind a `herding` gate) **shipped** — see
  "Corral (Intensification Rung 1c)" under Fauna & Wild Game. The **client _rendering_ for both ladders**
  (tile-card cultivation N% / tended-patch + Cultivation/Herding knowledge meters + herd corral
  indicator) is the **final Phase-1 slice** and remains a client-dev follow-up; the sim/schema data is
  now all on the wire (fields above).

## Gathering is SITE-BOUND — the plant branch's rung-1 site rule (issue #464)

**A crew may only be put on a curated gathering site**, and until #464 that rule lived nowhere in the
sim. `plant:wild` now carries a `site_requirement` of its own (`requires_gathering_site: true`),
enforced in `validate_labor_policy`'s `Forage` arm through `server::plant_rung_site_refusal` — the one
seam `validate_cultivate`, `validate_sow`, the labor arm's `Sow` placement gate and the wire's
`sowSiteRefusal` all resolve, so no two of them can disagree about which ground may be worked.

**Why it is not "the tile has a `FoodModuleTag`".** `classify_food_module` tags essentially every land
biome and `spawn_initial_forage` seeds a patch on every tagged tile with capacity, so a module test
admits ~2,300 of 4,160 tiles and the rule would be vacuous. The **gathering sites** are the curated
`FoodSiteRegistry` — a latitude-band + spatial-bucket quota with minimum spacing,
sized as a **share of land** (~8%, 130–134 per standard map) and biased toward fresh water since #466,
fixed for the life of a world. That scarcity is what makes *which
site a band can reach* the early game's real decision, and it is the pillar the whole design rests on.

**It was a CLIENT-SIDE rule.** The tile card simply declined to offer the compose sheet off-site
(`DrawerComposeController._forage_compose_available`), so the sim accepted a command no player could
send, and any other path — a script, a future AI, `cargo xtask command` — bypassed the game's central
scarcity entirely. The card meanwhile rendered a full stand with a named basket on that same ground,
which is the contradiction #464 was filed against.

**`FoodSiteRegistry` gained a positional `is_site`** (a `HashSet` rebuilt with the vec by both writers,
so it cannot drift from the list it indexes) — it was a bare `Vec` read only by the snapshot capture
before this, i.e. map decoration rather than a live rule.

**Test fixtures must now STATE their sites.** A patch seeded on bare nothing describes a world the sim
cannot produce, and a fixture that omits the site makes its one worked tile unworkable and silently
zeroes every yield under measurement. `server::tests::seed_gathering_site` and the `labor.rs` /
`forage_field.rs` / `forage_cultivation.rs` harnesses each declare theirs explicitly; an empty
registry is a *valid* map (all barren), so no fallback could tell "no sites here" from "the fixture
forgot".

**A fixture that writes its `LaborAllocation` directly can stay siteless for a long time**, which is
how `forage_cultivation.rs` did: the site rule was read only by the `assign_labor` / `cultivate`
command path those fixtures bypass, and the Cultivate *accrual* carries no site term of its own. The
`buildTurnsRemaining` **projection** reads it — a quote for a rung the command would refuse is the
defect that field exists to avoid — so a build fixture without a stated site now measures a refusal
rather than the rung. On a real map the term is inert for Cultivate, because rung 1 already demands a
gathering site and a crew could not have been put on the tile otherwise; it bites only for `Sow`,
whose fresh-water rule rung 1 does not carry.

## The `Sow` verb + the Field (Intensification rung 3) — the plant twin of the pen

**Rung 3 places a food source where you want it** (`docs/plan_intensification_ladder.md` §2, slice 5).
Once a faction knows **Seed Selection** (`SEED_SELECTION_DISCOVERY_ID` = 2005 — earned by *working
tended patches*, slice 4's `plant:tended` `earns_knowledge`; earned then, spent here), a crew working
a tile with **`Improvement::Sow`** in flight builds a **Field** on it. A Field is not a new entity: it is a
`ForagePatch` **at rung 3** — its `ladder_position` carried up into that rung's span, exactly as a
`Herd`'s position carries up into `animal:pen`'s. There is **no "extend
the field"**: each tile is its own patch, so you sow another field (the pen extends only because one
herd has one appetite).

- **Sown on ground your people already work — and SCARCITY IS THE POINT.** Rung 3 is *"I know how to
  take seed from a plant and put it somewhere else — but I do not know fertilization or water-carrying,
  so I sow ground we already gather, near fresh water"*. That rule is the rung's **`site_requirement`**
  on the ladder record (`RungSiteRequirement` — the plant twin of `ceiling_required`, keyed on the
  **land** instead of the species), and every dial is a lever:
  - **`requires_gathering_site: true`** — the tile must be a curated `FoodSiteRegistry` entry
    (`FoodSiteRegistry::is_site`). **Inherited from rungs 1–2, not invented here**: gathering itself is
    site-bound, so rung 3 narrows an already-scarce set rather than starting a second scarcity.
  - **`requires_fresh_water: true`** — the tile must be on or beside **fresh** water
    (`forage::tile_is_fresh_watered`): `TerrainTags::FRESHWATER` on the tile, **or** a river along one
    of its six sides (`Tile::has_any_river_edge` — the hydrology edge primitive, set on *both* flanking
    hexes, so the riverbank needs no neighbour lookup), **or** a fresh-water hex next door (odd-r
    `hex_neighbors_wrapped`). A **salt coast is not water** for this — you do not farm sea spray.
  - **`min_forage_capacity: 0`** — parked. It was **195** (admitting exactly the river-deposit class:
    RiverDelta 210, Floodplain 205, AlluvialPlain 195) while this rung had no site rule, and stacking
    the two demanded a curated site that ALSO landed on one of three biomes AND had water — scarcity
    three times over on a set the marker list had already made small. **The dial stays live because
    rung 4 needs it**: Farm has no site rule, so fertility is the only thing between it and planting a
    glacier.
  - **Measured on the standard map** (earthlike 80×52, seed 119304647, through the **real Startup
    chain**): **174 tiles clear the fertility+water rule of 4160 (4.2%)**, against **2113** that merely
    bear food; over six seeds the mean is **197**. **The measurement only means anything with
    `generate_hydrology` run**: the rule wants fresh water, and rivers/deltas are hydrology's, so a
    fixture that skips it measures 0 at every grid size and every seed. Of the tiles clearing the
    fertility floor, the water rule cuts about **40%**.
    > **The "49 sowable tiles (1.2%)" this line carried until #466 was wrong, and the way it was wrong
    > is the lesson.** Both counters that produced it — `relief_sweep::sowable_and_deltas` and
    > `forage_field`'s own `spawn_world_on` — stop after `spawn_initial_world` + `generate_hydrology`,
    > skipping `apply_tag_budget_solver`, `apply_biome_palette_clamp`, `reconcile_coastal_shelf` and
    > `reconcile_food_modules`. That is an **intermediate** map: four later stages repaint terrain, and
    > they add sowable ground. Measured on the same seed the short harness reads 136 and the real chain
    > 174. **Count worldgen outcomes through `build_headless_app`**, never through a partial chain — the
    > figure was quoted in two rule files and used to argue that sowable ground was desperately scarce,
    > which it is not.
  - **THE SCARCITY LIVES IN THE MARKER LIST, NOT THE TILE COUNT — and since #464 the sim says so.**
    A band may only work a curated `FoodSiteRegistry` marker, so the ground rung 3 can be built on is
    `markers ∩ watered`: **130–134 markers** per map (8% of land since #466; a flat 90 before), of which
    **73.8** clear the water rule once curation is biased toward fresh water, against **33.8** on
    pre-#466 main. See "Gathering markers follow the fresh water" in `worldgen.md`.
    > **That rule used to exist only in the client.** This bullet read *"a player can only Forage where
    > there is a marker (the client's `_forage_compose_available` reads `food_module`)"* — an accurate
    > description of a scarcity the **sim did not implement**: `assign_labor forage` was accepted on any
    > patch, so the single client refused commands the server would have honoured. #464 made it
    > `requires_gathering_site` on plant rungs 1–3, enforced through `rung_site_refusal`. The player-facing
    > arithmetic above is unchanged; what changed is that the sim now owns it. See "Gathering is
    > SITE-BOUND" above.
  - **A GATHERING SITE ADMITS BASKETS RUNG 3 CANNOT COMMIT TO — the site and the CROP are now two
    questions, and both already answer.** The 195 floor used to imply a rich basket (the river-deposit
    class is full of `field`-ceiling staples), so "the ground takes seed" implied "something here can be
    sown" and the two never came apart. They do now: an open-water fishery or an alpine shelf is a
    perfectly good gathering site whose whole basket is `wild`-ceiling — which `flora_config.json`'s
    own `cultivation_ceiling` note calls *"the ruling working, not a gap"*. **Neither half needed
    fixing, only pinning:** `validate_sow` refuses it through `SpeciesRefusal::NothingClimbsHere`
    (`sow` names no species, so `resolve_committed_species` asks the rung for its default and finds
    none), and the client **withholds the rung outright rather than gating it**
    (`RungGates.any_crop_allows` — greying it would imply a prerequisite that could be lifted). Guards:
    `server::tests::sow_rejected_where_nothing_in_the_basket_can_climb`, whose fixture *finds* such a
    site on the pinned map through the same `tile_flora_composition` + `default_species_for_rung`
    seams the command judges with, and `ui_preview`'s "a wild-ceiling species is offered nothing,
    gated or otherwise".
  - **The refusal names the fault** (`SiteRefusal::{NotGatheringSite, TooPoor, TooDry,
    TooPoorAndTooDry}` — the rung judges, the caller phrases through the shared
    `server::site_refusal_message`). **`NotGatheringSite` supersedes the ground readings** rather than
    joining them: whether such a tile is also thin or dry is moot while there is no way to work it, and
    a refusal naming three faults teaches two the player cannot act on.
  - **Rung 4 (Farm) IS A LOOSER COPY OF THIS RECORD and nothing else** —
    `requires_gathering_site: false` plus a fertility floor put back. That is the arc's config-driven
    thesis paying out: a rung whose *placement rule* differs is a config edit (pinned by
    `a_looser_site_requirement_is_a_pure_config_edit`).
- **"IT NEEDS NO SOURCE BELOW IT" IS RETIRED — that was §2's claim and issue #464 reversed it.** The
  rule read *"seed travels, so qualifying ground carrying no forage site at all is a legal — indeed the
  interesting — target"*, and `Sow` on bare ground still **creates** the patch (`ForagePatch::sown` —
  the tile's own biome capacity, biomass at the reseed floor, normal logistic regrowth). What it missed
  is that a band could never *reach* such ground: gathering is site-bound, so the only tiles a crew
  works are gathering sites, and a rung that could leap off them existed on paper only. **"Seed
  travels" moves up to rung 4 (Farm)**, where dropping `requires_gathering_site` is the whole of what
  the rung unlocks. `Corral` needing a herd you already tamed is therefore no longer the asymmetry it
  was — both webs' rung 3 now stands on something rung 2 established. *(The create-from-nothing branch
  survives in code but is near-dead: a gathering site is curated onto a tile carrying a food module,
  which is exactly the tile `spawn_initial_forage` seeds a patch on. The one gap left is a site curated
  onto a **zero-capacity** biome — `SaltFlat`, `HydrothermalVentField` — which `spawn_initial_forage`
  skips; that is a worldgen question, filed rather than fixed.)*
- **Never gated on the ground's health** — load-bearing, not a relaxation: sown ground starts at the
  reseed floor, i.e. *Collapsing* by construction, so a health gate would forbid the case the rung
  exists for. Rung 2 has since lost its own (`docs/plan_harvest_floor.md` §3.2), so this is now true
  of every rung on both webs rather than being rung 3's exception.
- **Its accrual carries NO work predicate either**, and that is the same fact stated on the other
  axis: `accrue_field`'s gate is Seed Selection and nothing else. The work predicate is what replaced
  each rung's `Thriving` gate, rung 3 never had one, and requiring it would make the
  create-from-nothing case impossible — **bare ground stands below every floor**, by construction. The
  floor still paces the build, so a crew stripping the ground it is sowing still builds nothing.
- **The investment is the band's builders pool** — `sow <faction> <x> <y>` queues the job and the
  hands staffed on `assign_labor <f> <b> builders <n>` raise it when it reaches the head,
  which are hands not gathering (`docs/plan_standing_upkeep.md` §2.2; the rung's
  `yield_fraction_while_building` is retired). On **bare** ground there is nothing for the gatherers
  beside them to carry either, so a bare-ground sow is near-pure investment. `forage_field.rs` pins
  that as a
  **relation, not a pair of literals** — the whole build is a trickle beside the Field it buys
  (`while_building_per_turn < BUILD_TRICKLE_FRACTION × field_yield`) — because since #433 the Field's
  payout scales with the committed crop's own rate, so any literal would be true of one crop only.
- **The payout — rung 3 out-yields rung 2, or the rung is pointless.** A completed Field is gathered
  through the **ordinary drawn-down path** and out-yields the tended rung by holding more standing
  crop and growing it back faster (`field_capacity_gain` × `field_regrowth_gain`, `labor_config.json`
  — see "What a Field buys"). **`sustainable != actual` is reachable and the ⚠ fires**: a Field can be
  over-farmed. **The collection cap binds harder than it used to**, because there is more to carry:
  the actual take is
  `min(what the floor offers, workers × per-worker throughput)`, `workers_needed` is derived, and the crop the crew
  could not carry is reported as `wasted`. **Measured production/turn on the reference basket** (see
  "Cultivation" → the callout; `AlluvialPlain`, K 195), committed to `wild_emmer`: wild Sustain
  **0.703** → tended **1.328** → Field **6.240**, needing **2 / 2 / 10** gatherers. The rung-2 crew is
  unchanged from rung 1 and that is not a typo — the gather cap is in **biomass**, and rung 2 changes
  the *rate* a take converts at, not the size of the take, so the same two people carry home nearly
  twice the food. Per-worker throughput is now **crop-dependent** (`8.0 biomass × the basket's rate`),
  so it is no longer the single 0.40 prov/worker figure this line used to quote.
- **Feral if abandoned — one rule for the whole plant web, and rung 3 goes FIRST.**
  `advance_cultivation` bleeds **one** meter per untended turn, the highest rung that still has
  progress on it, each at its own rung's `upkeep.meter_decay.per_turn` and past its own
  `upkeep.grace_turns` (`plant:field`: `work_cost` **75**, upkeep demand **4.0**/turn per
  **tender-load**, rot
  **0.75**/turn held to **0.75** of its cost, grace **1** — a standing crop is the most perishable
  thing on the ladder and wants hands every turn; the 75-unit cost is 25 turns at three hands, sowing
  *placing* a source rather than tidying one).
  So an abandoned Field stays a Field for **27** turns, then reverts to a **wild** gather patch and
  lapses to zero over ~100 turns in all, and
  only then does the tended ground beneath it begin to go. It does **not** step down to a tended patch
  on the way — that would pay the deserter rung 2's managed yield for free — but it no longer drags
  rung 2 down with it either: *the least-established improvement is the most fragile*. Ownership
  clears only once nothing is left of either meter. See "Feral if unworked" above for the grace, the
  ordering's own bug, and the feed line.
- **`sow <faction> <x> <y>` command** (`handle_sow`; `SowCommand` proto field **41**, its `workers`
  field `reserved`,
  `CommandEventKind::Sow`) — **sets the `Sow` improvement** on the bands already foraging that tile,
  the command form of the client's checkbox (issue #442: the stance beside it is left alone). It sows nothing outright; the seed goes in when the crew
  works the ground, so the improvement need only be checked once. Rejections, each
  distinct (`validate_sow`, shared with the `assign_labor` path): no such tile / **the land will not
  take seed** — *nobody gathers here* or *too dry*, each naming the fault and what to do about it / faction
  hasn't learned **Seed Selection** ("Work tended patches to learn it") / already a Field / another
  people's ground / **no band is foraging it**. The site rule gates the **labor arm** too (both the
  seed placement and the build accrual), so the labor arm cannot farm ground the command refuses.
- **`cultivated_count` counts Fields** (`ForagePatch::is_managed`), so the sedentarization
  domestication signal cannot read rung 3 as *less* domesticated than rung 2 (a bare-ground Field
  carries no cultivation meter at all).
- **Persistence** — a half-sown Field is a `ladder_position` inside the Field's span, and it rides the
  checkpoint's whole-registry clone like every other patch field, so a rollback rewinds it. **The
  derived `standing` rides with it**, which is safe for the reason it is stamped at all: it is written
  only by `set_ladder_position`, so a restored pair is the pair that was captured.
- **On the wire (slice 6a — append-only, slots 36–44):** `ForagePatchState` carries
  `fieldProgress:float` + `isField:bool` (the rung-3 meter and the completed rung — read the *bool*,
  never infer a rung from the float) beside the already-shipped `cultivationProgress`/`isCultivated`,
  so the client has **both** plant meters for the §4.1 two-meter split; `ceilingSow:float` +
  `fieldYield:float` (Sow's payoff, the twin of `tendedYield`; the `sowBuildFraction` that briefly
  carried its **dip** is `(deprecated)` with the dip itself — a building crew takes nothing, so there
  is no factor left to publish — and `ceilingSow`/`ceilingCultivate`
  are retired `(deprecated)` slots — two rungs still keep two numbers, so a retune of one cannot move
  the other); and **`sowSiteRefusal:string`** — `""` when the ground takes seed, else
  **`"not_gathering_site"`** / `"too_dry"` / `"too_poor"` / `"too_poor_and_too_dry"`
  ([`SiteRefusal::as_str`], free-form per the `species`/`ecologyPhase` convention). That last one
  ships **the answer, not a bool**: sowable ground is scarce by design, so *"why can't I sow here?"*
  is the live question, and the client can re-derive nothing (it holds neither the site list's
  reasoning nor the hydrology). The capture resolves it through the **same**
  `RungSiteRequirement::refusal` seam the command and the labor arm gate on — pinned by
  `the_exported_sow_site_refusal_is_the_verdict_the_command_acts_on`, so the wire cannot disagree with
  the gate.
  > **`not_gathering_site` is the newest key and became the COMMONEST answer** (issue #464): it is
  > shipped for every patch tile that is not one of the 130–134 curated sites, i.e. the large majority
  > of them. **Two things follow.** `"too_poor"` is currently **unreachable** — every shipped rung's
  > `min_forage_capacity` is `0` — but the key stays, because the floor is rung 4's dial and the
  > variant is what will carry it. And a reader extending the client from this list must take **all
  > four**: the client's `HudFloraVocab.SOW_REFUSAL_REASONS` is this table transcribed, and a missing
  > key there does not fail loudly — `RungGates.sow_site_refusal_reason` renders
  > `SOW_REFUSAL_FALLBACK`, copy written for an *unrecognized* verdict, in place of the real reason.
- **On the client (slice 6):** the native reader — now
  `clients/godot_thin_client/native/src/dict/subsistence.rs::forage_patches_to_array`, not the old
  `lib.rs` home — surfaces all five as dict keys: `field_progress` / `is_field` /
  `field_yield` / `sow_site_refusal` (the last optional), beside the already-shipped
  `cultivation_progress` / `is_cultivated`. **Two spellings reach GDScript and they are not
  interchangeable:** `HudBandLaborState.forage_patch_lookup()` holds the keys **bare**, while the
  `tile_info` dict the tile card and compose sheet read carries a `patch_` prefix — except for the
  cultivation pair, which `MapView` stamps bare there too. Read whichever the caller's dict uses.

## A Sow is priced by what it replaces

**`plant:field`'s build cost is the rung's declared `work_cost` times a per-patch multiplier** set by
how much of the tile the chosen crop still has to displace (`docs/plan_standing_upkeep.md` §4.15).
Sowing a crop that already holds most of the ground is tidying; sowing one that holds a tenth is
replacing the tile. **`plant:tended` is not scaled** — clearing wild ground is clearing wild ground —
and neither is the **upkeep**, which is `scaled_by: source_load` off the tile's `K`: holding a field is
about how big the place is, never about what used to grow there.

```text
replacement = 1 - crop_share
share_load  = replacement / (1 - field_reference_crop_share)
multiplier  = clamp(share_load, field_share_cost_floor, field_share_cost_ceiling)
```

- **It is the LADDER'S OWN per-source price hook, not a mechanism beside it.** `RungStanding::at`
  takes a `cost_at` resolver — *this source's price for a rung* — which the animal web spends on a
  species' `taming_cost_multiplier`. The plant web passed `RUNG_COST_UNSCALED` at four call sites with
  a comment saying a plant has no species; `forage::plant_rung_cost` now answers the Field rung at the
  patch's own multiplier and every other rung at `RUNG_COST_UNSCALED`, which is `Herd::standing_at`'s
  shape exactly.
- **The multiplier lives on the PATCH** (`ForagePatch::field_cost_multiplier`, private, read through
  `quoted_field_cost_multiplier`), the twin of `Herd::taming_cost_multiplier`, because a patch's
  `standing` is derived from its own rung spans: a price the source could not see would put the
  position's meaning and the job's price in two places. `patch_rung_span` is that price list;
  `plant_rung_span` remains the **reference** span and is what a fixture seats a position with.
- **⛔ THE SHARE IS MEASURED ONCE, WHEN THE LEG STARTS, AND HELD FOR THAT LEG.**
  `ForagePatch::price_field_rung` is idempotent — the Sow arm calls it on the turn the Field leg first
  takes work, above the accrual and below the stalled-quote return, so a leg that banked nothing is
  not priced. `None` means the leg has not started, and while it is `None` the Field rung's width
  provably changes nothing the patch derives: the position is at or below the rung's base. It lapses
  again in `set_ladder_position` whenever the position falls back to that base, so a Field bled away
  and re-sown is re-quoted.
- **⛔ IT IS NOT LIVE, and the reason is that the mix interpolates.** `patch_composition` blends across
  the rung being raised, so a Sow raises its own crop's share continuously as it proceeds. A live price
  would shrink the remaining work as the work was done — a job that accelerates itself — and it would
  turn the queue's chained finish date (`plan_standing_upkeep.md` §4.6b) from an exact construction
  into a drifting estimate. Guarded by
  `forage::tests::a_running_sows_price_holds_while_the_mix_moves_under_it`, which asserts the crop's
  share climbing as a **precondition** before asserting the price standing still.
- **⛔ AND IT READS THE BASKET OF THE RUNG BELOW, NOT THE PATCH'S LIVE MIX** (`field_replaced_share` =
  the crop's share of `weeded`). A turn's accrual routinely **overshoots** the rung boundary, so a live
  reading taken when the leg starts is taken after the build has already moved it — the build pricing
  itself, which is `capacity_per_tender`'s trap one account over (*the measure reads the TILE's `K` and
  never the patch's `carrying_capacity`, which has already been multiplied*). The rung below's basket
  is free of it **and is exact**: a Field leg can only begin from a full tended rung, and a full tended
  rung's mix is `weeded` by construction — so a two-leg `Sow` on untended ground is quoted the same
  number at declaration that its Field leg is stamped with once the Cultivate leg has weeded the
  ground. The re-quote is a discrete event at a leg boundary, not a drift.
- **The crop is the patch's commitment where it has one, and `default_species_for_rung`'s auto-pick
  where it has not** — the same plant a Sow ordered here would commit to, so a patch nobody has worked
  quotes the price it would really be sold at. Ground where nothing climbs to a Field prices at
  `RUNG_COST_UNSCALED`: the rung is unbuildable there, and the ladder's declared figure is the honest
  thing to publish.
- **Every surface that states what a Sow will cost resolves through one seam**,
  `forage::patch_field_cost_multiplier` — the arm that charges it, `patch_build_legs`' work figures and
  their chained dates, the pre-commit `projected_build_quote` the `⌃` mark and the compose sheet read,
  and the published `fieldWorkCost`. **No wire field was added**: `fieldWorkCost` already carried the
  price and now carries the scaled one. That seam answers the patch's **own** price list whenever the
  position stands above the Field rung's base, quoted or not, so the published cost and the published
  `fieldWorkDone` can never divide by two different denominators.

See Also: "Cultivation (Intensification Phase 1a)" (the rung below), "Corral (Intensification Rung 1c)"
(the animal rung 3 this mirrors), "The Intensification Ladder" (the engine + the config).

---

