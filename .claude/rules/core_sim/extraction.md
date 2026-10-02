---
paths:
  - "core_sim/src/extraction.rs"
  - "core_sim/src/extraction_config.rs"
  - "core_sim/src/data/extraction.json"
  - "core_sim/src/snapshot/deposits.rs"
  - "core_sim/tests/extraction.rs"
---

# Wood and stone get a producer — the deposit, the source, the take

Design of record: `docs/plan_extraction.md` (issue #583). `wood` and `stone` shipped as materials
with real consumers and **no producer at all**: the `animal:pen` rung eats hurdles woven from wood,
`route:paved_road` eats stone, and the only way either reached a band was the turn-one outfitting
window. This arc gives both a producer, through **one** mechanism the minerals arc slots into
without re-design.

## The one idea

> **A deposit is a stock with a capacity and a regrowth rate, and rock's rate is zero.**

There is no `is_finite` flag, no finite-deposit branch and no special case anywhere. With
`regrowth_rate: 0.0` the growth term returns the stock unchanged, so *a quarry only ever goes down*
is **arithmetic**. That is the whole reason the arc has this shape: **a metal is a deposit and a
config row, not a second system.**

**Regrowth belongs to the DEPOSIT — the terrain — not to the material and not to the branch.** An
earlier draft split the two branches on renewable-versus-finite; the falsifier is that **surface
flint and a quarry are the same skill and only one of them runs out**. Both are `Extraction`, and it
is their *terrains* whose rates differ.

## What is state and what is not

| | where it lives |
|---|---|
| capacity | a pure function of the tile (`extraction::tile_deposit_capacity`) — never saved |
| regrowth rate | the same, per terrain (`tile_deposit_regrowth`) |
| characteristics | the same, per terrain (`tile_deposit_characteristics`) |
| **the stock** | `DepositSource::stock` — the **only** saved thing, and it opens at capacity |
| the ladder position | `DepositSource` too, like a patch's, with a stamped `standing` beside it |

`forage::tile_forage_capacity`'s discipline exactly: one function every caller goes through, so the
opening path, the site rule and any future wire path cannot drift.

**A working is opened LAZILY, the first turn a band puts a crew on it** (`DepositRegistry::open`). A
deposit nobody has worked stands at exactly its tile's capacity, which is a derivation — recording
one per land tile per material would be storing it twice over for the whole map. An untouched map
therefore checkpoints an empty registry.

**The WIRE does not follow the registry** — `deposit_states` publishes a derived row for every
discovered tile that holds a deposit and merges the registry in where a working stands, which is what
makes an unopened deposit reachable from the client at all. See "The wire" below.

> **It reads `tile.terrain`, never `resource_terrain()`.** A deposit is a thing on the ground of the
> hex, so a navigable river does not inherit the timber of the valley it cut — which is exactly where
> `tile_forage_capacity`'s underlay reading *does* belong, because a fishery is a property of the
> water standing over that valley.

## The two branches, and why they are two

`RungBranch` gains **`Forestry`** and **`Extraction`**, and `ALL_BRANCHES` grows to five — the
constant exists precisely so every sweep breaks at compile time rather than silently skipping a
ladder.

| branch | rungs | what a rung buys |
|---|---|---|
| `forestry` | `deadfall` (free floor) → `felling` → `coppice` | **regrowth**: conservationism is *more per turn for ever*, not more per turn |
| `extraction` | `gathering` (free floor) → `quarry` | **reach**: a finite deposit has no regrowth to raise, so the rung lowers the floor |

**Stone and metal share a branch** because quarrying and mining are one skill and only the material
in the deposit differs; that is what makes copper a deposit row rather than a sixth branch.
`extraction:mine` is the minerals arc's and is deliberately not on this ladder yet.

## ONE payoff block, both branches, and no branch check anywhere

`RungExtractionPayoff` — `yield_per_worker_turn`, `recovery_fraction`, `regrowth_multiplier` — is
`route_payoff`'s twin, required on every forestry and extraction rung and rejected on every other.
All three interpolate on the source's position (`plan_standing_upkeep.md` §2.8): each is a rate or a
share, and there is no classifier here whose cut points would have to step.

A **forestry** rung raises the multiplier and holds recovery at its ceiling; an **extraction** rung
raises recovery and holds the multiplier at `1.0`. **The multiplier scales the deposit's own rate**,
and rock's is `0`, so `0 × anything` is still `0` — *stone's rate is zero* survives as arithmetic
rather than as a rule someone has to remember. A branch-keyed payoff would have made it breakable by
a config edit.

## The turn: regrow, then take

```text
Logistics  — once per WORKING (`advance_deposits`, phase 4)
  stock    += regrowth(stock, capacity, regrowth_rate(terrain) × regrowth_multiplier(position))

Population — once per BAND ROW on it (the `Extract` arm)
  rung      = (1 − recovery_fraction(position)) × capacity
  floor     = if regrowth_rate(terrain) > 0 { max(rung, escapement × capacity) } else { rung }
  reachable = max(0, stock − floor)
  take      = min(workers × yield_per_worker_turn(position) + gear_take, reachable)
  stock    -= take
```

- **OVER-CUTTING IS POSSIBLE AND MUST STAY SO.** The take is not clamped to the sustainable rate —
  the whole point of the renewable half is that you can ruin a wood. The warning is a readout, not a
  guard: the row's `SourceYield::overdraws` ⚠, through this branch's one producer
  `extraction::deposit_take_overdraws`, beside the two food webs' own (`yield-forecast.md` → "THE ⚠
  IS INTENT **AND** ABILITY"). It is **intent and ability** like theirs — a *composed* floor below
  the food peak, and a crew whose throughput can actually get the stand down to it — and it is
  `false` on any working at `NEVER_RENEWS`, which warns with its runway instead (§7's fork). The
  `Extract` arm wrote the field on no row at all for a whole arc, so a working the tile card marked
  as over-cut sat green and unsorted in the band's own source list.
- **AND IT SAYS HOW MANY OF ITS CREW BROUGHT ANYTHING HOME**, through the *plant* web's own
  inversion. `SourceYield::workers_needed` is
  `systems::workers_needed_for_take(take, yield_per_worker_turn(position), workers)` — the very
  function the Forage arm and both Hunt arms invert their take with, so the deposit web cannot come
  to disagree with the plant web about one arithmetic. The throughput term is the crew's **mean**
  per-worker rate — the rung's interpolated `yield_per_worker_turn` plus the crew's `gear_take`
  spread over its head count, the plant web's reading of a partly-basketed crew — which is the rate
  the take's own labor cap is struck at, so
  a crew cutting a stand already drawn down to its composed floor reports the hands that carried the
  whole take and leaves the rest named as bringing nothing home. **`0` still means UNKNOWN** and the
  client prints no note at it: `validate` requires a positive, finite `yield_per_worker_turn` on
  every deposit rung, so the only `0` this arm can answer is a crew that genuinely took nothing, and
  nothing is clamped up to one. The field is a **worker count** and no part of the food identity —
  `actual` stays `SourceYield::ZERO` on this arm, and the take goes on paying only into
  `SourceYield::materials`.
- **These sources pay NO FOOD AND NO FODDER.** Not a zero-valued food term; no food term at all, and
  `seed_source_yield` returns rather than seeding a permanent `+0.00` line. What wood costs is **the
  food those hands did not bring home**: every hand on an `extract` row comes out of the same finite
  pool a Forage row spends.
- **The arrival rides the existing seam** — `LocalStore::deposit_material`, with the *ground's*
  characteristics, so a streambed pays knappable flint and a quarry pays building block out of one
  generic material, and the batch merge is the one every other arrival uses. It is reported through
  the row's `SourceYield::materials`, which is the producer the band's income map and the material
  shortfall Alert both read.
- **A far working's take walks home.** Past `band_work_range` the row posts a work party, the take
  is priced at the hands at the deposit, and what is deposited this turn is what the porters land —
  nothing while the party walks out. `outcome.taken` still draws the stock down the turn it is cut.
  See `work-party.md` → "The deposit web: the cargo is the material".
- **A shared working is drawn down SEQUENTIALLY** and needs no divider. Each band's row takes from
  the stock it actually finds, so the takes cannot sum past what the rung could reach — which is how
  `forage_take` divides a patch two bands gather.

> ### ⛔ THE GROWTH TERM RUNS ONCE PER WORKING, AND IT LIVED IN THE TAKE FOR ONE SLICE
>
> Renewal was the last step of `take_from_deposit`, on the reading that *"a deposit has no forecast
> riding a pre-regrowth reading, so there is nothing for the split to serve"*. **That is the wrong
> thing for the split to be about.** The plant web's split is not about forecasts, it is about **how
> often the growth term runs**: `advance_forage_regrowth` sweeps every patch once, and the gather is
> per band. `take_from_deposit` is called **once per band-row**, so renewal inside it failed in both
> directions at once —
>
> - **two bands on one wood ran the growth term twice in a turn** (`K` bands, `K`× renewal), which
>   undercut *"over-cutting is possible and must stay so"* in exact proportion to how many bands
>   shared a deposit; and
> - **a working no band held a row on never renewed at all**, so an abandoned over-cut wood was
>   frozen at its low-water mark for ever — against this file's own headline that a wood recovers
>   and rock does not, on exactly the ground the branch's move-or-stay pressure is made of.
>
> It is `advance_deposits`' phase 4 now, at the **post-decay** position, so a working that has
> slumped off its coppice rung renews at the rate it now stands on.
> `two_bands_on_one_working_renew_it_once` measures against a one-band control (asserting only that
> the stock rose would pass against the defect), and
> `an_abandoned_wood_still_recovers_and_an_abandoned_quarry_still_does_not` drives a world with no
> band in it at all.

## The escapement floor — the rung's and the crew's are ONE floor, taken as a MAXIMUM

An `extract` row carries a `floor` (`LaborTarget::Extract::floor`), the same fraction-of-capacity
dial `Forage` and `Hunt` have carried since `docs/plan_harvest_floor.md`. A deposit therefore has
**two** floors, and the one thing to get right about them is how they compose.

> ### ⛔ `max`, NEVER a sum and never two clamps
>
> ```text
> rung_floor      = deposit_floor(capacity, payoff)
> effective_floor = if regrowth_rate(terrain) > 0 { max(rung_floor, escapement × capacity) }
>                   else                          { rung_floor }
> reachable       = max(0, stock − effective_floor)
> ```
>
> They are **the same kind of quantity — an amount left standing.** `deposit_floor` is what the
> rung's reach cannot get at (`extraction:gathering` recovers 0.15, so it strands 85% of a rock
> body); `escapement × capacity` is what the player told the crew to leave. You stop at whichever is
> greater, so a player floor *below* the rung's changes nothing and one *above* it binds.
>
> **Summing them would double-count on every rung** — a gathering crew told to leave half a seam
> would be refused 135% of it, and a working nobody could cut would read as a config error. Clamping
> twice is the same arithmetic written out longer.
>
> `deposit_effective_floor` is the only place this is said, and `deposit_reachable` is the only seam
> that reads it — so the take, the runway, the row's `reachable` and the lesson's work predicate all
> move together. `extraction::tests::the_rungs_floor_and_the_crews_compose_as_a_maximum` asserts it
> from both sides *and* against the sum, on two payoffs chosen so one number binds each way.

### ⛔ The crew's half of that `max` participates ONLY WHERE THE DEPOSIT RENEWS

An escapement floor exists to protect **regrowth**. Stock left standing on a renewing deposit is next
year's harvest, so leaving it is a conservation choice with a return; stock left standing on a body at
`NEVER_RENEWS` is simply never taken, protecting a future that does not exist. So on a rate-0 deposit
the player's floor is not a conservation choice at all and does not bind — **the rung's own
unreachable remainder is the only floor a quarry has.**

The rate `deposit_effective_floor` forks on is the **ground's**, un-scaled by `regrowth_multiplier`,
which is `deposit_runway`'s own reading of *"is this working finite"*: a rung scales a rate, it does
not make the ground finite.

> **Without the condition a quarry crew stopped at HALF the rock body its own readouts promised it.**
> The client offers the dial only where `regrowthRate > 0` — correct, because rock does not grow
> back — so a finite working's row carries the omitted-token default `DEFAULT_ESCAPEMENT_FLOOR`
> (0.5). On `extraction:gathering` (recovery 0.15, rung floor 0.85) the `max` swallowed it and
> nothing showed; on **`extraction:quarry`** (recovery 0.85, rung floor 0.15) **0.50 bound above the
> rung**, against a verdict sheet promising 85% of the body for 250 work and 8 wood.
> `wire::a_quarry_at_the_default_floor_reaches_the_whole_of_what_its_rung_recovers` asserts the
> **sum** — what the crew has cut plus what it can still reach is the rung's whole recovery — because
> that is the promise the readout makes, and it read `0.50` against the defect.
> `extraction::tests::the_crews_floor_is_dropped_on_a_deposit_that_never_renews` pins the same
> condition beside the two arms it must not touch: the gathering rung, whose own floor was always
> higher, and a renewing body at the same order, which still stops the crew at half of it.

**The grammar still accepts a floor on a finite working, and the value is stored, published and
inert.** `deposit_effective_floor` is where the rule lives because **the sim is what knows the rate**:
a raw command line or a script can send `assign_labor … extract … 0.5 3` as readily as the client can,
and a client that sent `0` instead would be one producer of the verdict out of several. Refusing the
token at the command boundary is the other rejected shape — it would make the two branches' commands
differ in shape for a value that simply has no effect, and `DepositSource::last_floor` stays honest
about what the crews asked for either way.

### ⛔ An ABSENT floor token on a finite working means ZERO, not the shared default

`handle_assign_labor` resolves an omitted floor to `DEFAULT_ESCAPEMENT_FLOOR` (0.5) for every row in
the game **except** an `extract` row on ground at `NEVER_RENEWS`, where `server::unnamed_deposit_floor`
answers `STRIP_IT_BARE`. The client offers the dial only where a deposit regrows, so on a quarry it
sends no token at all, and reading that silence as the shared 0.5 wrote a conservation choice onto a
row where nobody made one — indistinguishable, in the field, from a player who chose 50%.

The rate it reads is the **ground's**, un-scaled by `regrowth_multiplier` — `deposit_effective_floor`'s
own reading verbatim, because two places forking on one fact fork on one reading of it.

**A `Forage` or `Hunt` row is untouched:** a patch and a herd always renew, so the shared line is
right for them. **A renewing deposit with no token still gets `DEFAULT_ESCAPEMENT_FLOOR`.** And an
**explicitly sent** floor is stored exactly as sent on a finite working — the grammar stays uniform
across the three webs (see the paragraph above), so the field can still be nonzero on a rock body.

⛔ **The four readers keep their own conditions, and this does not replace them.**
`deposit_effective_floor`, `deposit_lesson_floor` and the client's `composed_floor` / `floor_mark`
each ask *"does this deposit regrow?"* independently. Because an explicit token still reaches a rock
body's row, each must go on asking; the command boundary closing the *absence* case is defence in
depth. `server::tests::an_unnamed_floor_is_zero_on_a_finite_working_and_the_default_on_one_that_renews`
pins the fork (rolling hills carry renewing wood and rate-0 stone, so the two arms differ in the rate
and in nothing else) and `a_forage_row_with_no_floor_token_still_carries_the_shipped_default` pins the
shared line's survival.

**It changed no behaviour, and that is asserted rather than claimed.**
`extraction::a_rock_working_reads_the_same_at_the_shipped_default_and_at_the_new_zero` runs one
working at the old value and the new one and compares stock, take, runway and lesson — across both
extraction rungs, because `extraction:quarry` is where the take and runway would diverge (its own
floor sits below 0.5) and `extraction:gathering` is the rung that earns `quarrying`.

**Grammar:** `assign_labor <f> <b> extract <x> <y> <material> [floor] <workers>` — the `hunt` arm's
shape with a material where the herd id goes, disambiguated **by tail length** because the free-form
token is read positionally first and is never in the optional slot. It fails **closed** on the shared
`components::floor_is_valid` bound (`0.0..=1.0`, finite), struck once at the top of
`handle_assign_labor` for all three webs, and a retired stance word is refused **by name**.

**The working stamps the floor its crews worked to** — `DepositSource::last_floor`,
`last_take`'s twin, cleared once per turn by `advance_deposits` beside it. ⛔ **The aggregate across
bands is a MINIMUM where the take's is a sum**: a floor is not an amount to add up, two bands each
stop at their own, and the stock comes to rest at the lowest of them. `None` is *nobody cut this
working this turn* and reads as `STRIP_IT_BARE` — the **identity** of the `max` above, so an
unworked deposit publishes exactly the rung's own reach, as it did before the dial existed.

> ### The seed is the point the curve is READ AT, never a lift on the stock
> The logistic term is zero at a stock of zero, so a wood cut clean would stick there for ever — the
> trap `forage.reseed_floor_fraction` closes for a stripped patch. **It may not be solved the plant
> web's way:** `max(stock, fraction × K)` applied to the *stock* would raise a **quarry** off zero.
> The curve is read at `max(stock, seed_fraction × capacity)` and the **delta** is added to the real
> stock, so the seed is multiplied by the deposit's own rate and `NEVER_RENEWS` seeds exactly
> nothing. `max` and not `+`: an additive seed holds the zero-crossing *below* capacity, so a wood
> would stall a couple of percent short of full for ever.

## ⛔ NO RUNG MAY RAISE `capacity` — the §6 floor trap's guard

`plan_standing_upkeep.md` §6 records the trap: a rung raised a herd's ceiling, `floor_fraction × K`
climbed with it while the herd stayed the size it was, the build's own eligibility gate read the room
above that floor, and **a tame begun on its floor never completed at any crew size**.

Here the floor is `(1 − recovery_fraction) × capacity`, so the same trap is one config key away.
**Capacity comes from the terrain table and nowhere else** — `tile_deposit_capacity` takes no
position and no standing, so there is nowhere for a rung to reach it — and the payoff block has no
key for one. `tests/extraction.rs::no_rung_on_either_branch_may_raise_capacity` walks every rung of
both branches at three positions each and fails if it ever moves.

**And the build gate does not read the stock either**, which is the same trap by a different door: a
scatter already worked down to `extraction:gathering`'s floor has *nothing* reachable, and opening a
quarry is precisely how you reach deeper. `deposit_head_gate` asks the ground and the knowledge, and
nothing else.

## The site rule is the whole of "you cannot quarry just anywhere"

`RungSiteRequirement` gains **`min_deposit_capacity`**, checked against the tile's capacity for
*that source's material* through the **same** `forage::rung_site_refusal` seam a `sow` resolves
through — so `validate_deposit_verb`, the labor arm's gate and any future readout cannot drift.
`SiteRefusal::NoDeposit` is the new fault, and it supersedes the fertility and water readings for
`NotGatheringSite`'s reason.

`extraction:quarry` sets it at **100** — the extraction branch's half of the rule, `forestry:coppice`'s
**70** below being the other — and it sits in the gap the deposits table leaves between its
two populations: the smallest **finite** rock body is an ash plain at 120 and the largest
**renewing** scatter is a periglacial 70. So the split is a **capacity reading** rather than a list
of terrains anyone maintains, and the minerals arc's placed ore bodies fall on the right side for
free.

> ⛔ **IT SHIPPED AT 800, AND THAT WAS A MIS-READ OF THE TABLE RATHER THAN A TUNING CHOICE.** 800 is
> the gap between *rolling hills* (900) and the scatters — but three rate-0 rows sit below it:
> `AquiferCeiling` 600, `FumaroleBasin` 300, `AshPlain` 120. **A rate-0 row you cannot quarry is a
> dead work site that still accepts a crew**: the band works `gathering`, takes its 0.15 of the body
> once (90 / 45 / 18 units), and then reaches nothing for the rest of the game, because the stock
> never returns and the rung that would reach deeper is refused for ever. At 100 an ash-plain quarry
> yields ~102 against gathering's 18 — marginal, finite, and a real decision, which is the design
> working.
>
> **The invariant is now asserted rather than claimed.**
> `the_quarry_threshold_splits_the_finite_rows_from_the_renewing_ones` walks the shipped deposits
> table and fails if any `regrowth_rate == 0.0` row falls **below** the threshold, or any renewing
> row **above** it. That is what stops the split drifting again, and it is what this file said
> existed while it did not.

**Neither free floor carries a `site_requirement`, and that is not an omission.** A floor of ~0
admits every tile, which `validate_site_requirement` rejects outright as a placement rule that is
none. What refuses a crew on bare ground is that the terrain holds no deposit at all.

### `forestry:coppice` sets it at 70 — the forestry branch's own placement rule

A coppice costs **150 work** and buys `regrowth_multiplier` **2.0**, so what it adds to a stand is
**one more peak renewal a turn**, and it raises the take only 2.0 → 2.5. On the smallest stands that
doubling is worth almost nothing, and the rung was offered anyway: all three forestry rungs shipped
`site_requirement: null`.

**70 is a reading of the wood table rather than a chosen number.** Peak renewal on the shipped curve
is `capacity × regrowth_rate / 4` — `deposit_regrowth` is `r·S·(1 − S/K)`, which peaks at `S = K/2`,
and that is where `felling`'s own comment gets a mixed woodland's **4.5** from (600 × 0.03 / 4).
Ranked by what a coppice adds, the table splits either side of 70 with **a gap and no overlap**:

| side | the rows | gain a turn |
|---|---|---|
| refused | AlpineMountain 50, PeatHeath 60, KarstCavernMouth 60 | +0.125, +0.18, **+0.30** |
| admitted | AquiferCeiling 70 … MixedWoodland 600 | **+0.525** … +4.5 |

The three excluded are the three lowest by capacity **and** the three lowest by gain. Under the line
150 work buys under a third of a unit a turn — 500–1200 turns to repay — which is a rung with no
meaning.

⛔ **CAPACITY IS A PROXY HERE, AND THE WOOD TABLE'S IS A LOOSER ONE THAN THE STONE TABLE'S.** On
stone, capacity and rate are correlated by construction (the rate-0 bodies are the large ones), so
the quarry threshold ranks the rows exactly. On wood the two are independent, so capacity **mis-ranks**
some rows by gain — `KarstHighland` at 120 gains +0.60 where `FreshwaterMarsh` at 90 gains +1.24.
That mis-ranking does not reach the boundary and is not a defect to fix: the rule is the gap either
side of 70. `the_coppice_threshold_splits_the_wood_table_with_no_overlap_in_what_it_buys` asserts it
by walking the shipped table — `max(gain below) < min(gain at or above)` — and reads the gain off
`deposit_regrowth` itself, so a change to the curve's shape re-measures the gap rather than
mis-stating it. **It asserts the gap and never a list of terrain names**, which is the failure the
quarry's own threshold test exists to answer.

**The other two forestry rungs keep `site_requirement: null`.** `deadfall` is the free floor, and
`felling` is the rung whose whole point is that you *can* ruin a wood. Only the management rung asks
anything of the ground.

## You learn a rung by practising where that rung could be built

**A rung's lesson is credited only where the rung that lesson unlocks could be sited.** Picking loose
stone off a 40-unit scatter teaches nothing about quarrying, because no quarry could ever stand on a
40-unit scatter; picking it off a rock body teaches it.

`LadderConfig::rung_unlocked_by_lesson` is the ladder lookup — the rung on the **same branch** whose
`unlock_knowledge` names what this rung's `earns_knowledge` teaches — and the site question is
resolved through **`forage::rung_site_refusal`**, the one seam `deposit_head_gate` and the `quarry`
command's rejection go through. A second reader of `min_deposit_capacity` is exactly the drift that
seam exists to prevent: the ground a rung can be built on and the ground its lesson is worth learning
on are one reading, or they are two readings that will disagree.

**The gate is composed by the DEPOSIT arm's `eligible`** (`systems::labor`'s `Extract` arm, through
`ground_takes_the_rung_this_lesson_unlocks`), beside the crew and escapement-room terms it already
composes there — only the caller has the tile in hand. `RungDef::knowledge_accrual` and
`credit_rung_lesson` are untouched.

**It is the identity wherever the lesson opens no rung, or opens one with `site_requirement: null`.**
That is `forestry:deadfall` (`woodcraft` unlocks a `felling` that asks nothing of the ground), both
branch tops, and every rung on the plant, animal and route webs — which the deposit arm never reaches
anyway.

> ### ⛔ IT REACHES BOTH BRANCHES, AND THAT IS ONE RULE RATHER THAN TWO CASES
>
> `felling` earns `conservationism`, `conservationism` unlocks `coppice`, and `coppice` now carries a
> threshold — so **felling a stand under 70 teaches no conservationism**. The crew still fells, at
> the same rate, on the same ground; what it stops doing is learning. It is the stone sentence one
> branch over: *you learn to manage a wood on a wood worth managing.*
>
> `a_deposit_lesson_is_credited_only_where_the_rung_it_unlocks_could_stand` pins the stone pair **and**
> the inertness arm (a 50-unit alpine stand still teaches `woodcraft`, at the same rate the admitted
> stone arm earns — so the gate is the identity there rather than merely non-zero);
> `felling_a_stand_too_small_to_coppice_teaches_no_conservationism` pins the forestry pair, and
> `fauna_husbandry::the_deposit_lesson_site_rule_leaves_the_animal_web_alone` pins that the food webs
> did not move. Without that last one a gate that refused *everything* would pass the pairs.

⛔ **ON STONE, THE ESCAPEMENT DIAL NOW PACES NO LESSON AT ALL.** Every renewing row in the stone table
is below the quarry's threshold by construction, and every row at or above it is at `NEVER_RENEWS`,
where `deposit_lesson_floor` drops the dial. So `quarrying` is learnable only on rock bodies and only
at `PRACTICE_AT_THE_PLAIN_RATE`. That is what moved
`a_finite_working_learns_at_the_plain_rate_and_a_renewing_one_rides_the_dial`'s renewing arm onto
`forestry:deadfall`: there is no stone ground left on which both readings are live.

## The floor rungs must be workable BARE-HANDED

A felling kit wants a haft, a haft is wood, and wood comes from felling; each of the two shipped
quarry tools costs 3 wood. **So if either floor rung needed gear the material economy could never
start** — and a band spawns owning nothing (`equipment.json`'s `start_stock_fraction: 0.0`). It is
`materials.json`'s own argument for `hand_working`: a bare-handed rate refuses nothing, where a zero
would be a refusal branch the sim does not have.

**The take's kit term is an ADDITION above that bare rate, and it is bound off both floors.**
`EquipmentStat::DepositTake` (`deposit_take`) is the extra units one **equipped** worker cuts per
turn, `build_work`'s shape exactly: summed over the crew through the row's own coverage
(`EquipmentConfig::deposit_gear` → `gear_take`) and added to `workers × yield_per_worker_turn` in
`extraction::deposit_take` — never a replacement rate, because the bare rate is interpolated per
rung and an absolute gear rate would erase the climb, and never a change to the reach. It carries a
**required** `branch` and an optional `rung` (`EquipmentStat::is_rung_scoped`, filtered on
`EquipmentEffect::serves_build`), and the shipped tools bind to the rungs above the floors only:

| item | tier | `deposit_take` on | also |
|---|---|---|---|
| `axe` | `flint` only (no bone tier — §9's *"sits oddly"*) | `forestry:felling`, `forestry:coppice` — **not** `deadfall` | `build_work` 0.5 on `forestry`, so the builders (through `pool_toe`) and a forestry working's own keeping hands (through its keeping claim) are geared by it |
| `wedges` | `flint` only | `extraction:quarry` — **not** `gathering` | — |
| `sled` | `plain` (the hunt's own sled) | **+0.3** on `forestry:deadfall`, **+0.4** on `extraction:gathering` — the floors' only tool, costing hide and fibre and no wood | its hunt `hunt_carry`, unchanged; shared with `big_game`/`trapping`/`ranging` and the three take kits through the per-item ration |

**Three take kits — `sledding` (sled), `woodcutting` (sled + axe) and `stonework` (sled + wedges) —
each claimed whole, like every job's kit.** The sled's `deposit_take` names the floors (+0.3 on
`deadfall`, +0.4 on `gathering`), costing no wood, so the floors stay bare-workable; the axe's and
the wedges' name the rungs above. **The floors take the Sled kit because deadfall is picked up and
loose stone is gathered**: nobody cuts on them, so an axe or wedges would go out with the crew and
lift nothing.

- **A working OFFERS every kit whose items each declare a `deposit_take` on its branch**
  (`extraction::working_offered_kits` → `EquipmentConfig::deposit_kits_for`): a wood offers
  `sledding` and `woodcutting`, a stone working `sledding` and `stonework`. A kit carrying a tool
  that does nothing on the branch is not offered.
- **The DEFAULT follows the held rung**, the way a herd's default follows its quarry
  (`extraction::working_default_kit` → `EquipmentConfig::deposit_kit_for`): of the offered kits
  with an item serving the held rung, the one carrying the fewest items that do not. So `sledding`
  on `deadfall` and `gathering`, `woodcutting` on `felling` and `coppice`, `stonework` on `quarry`.
  A tie between two kits falls back to `default_kits.extract`.
- **The row keeps the kit it was sent with.** The kit is stored on the row at assignment, so a
  working that climbs does not re-kit its crew; the player re-sends the row to change it.

A row claims every item its kit uses wherever it works — through the one claim / coverage / budget
path a hunt row's kit goes through (`LaborAllocation::item_budget`, `kitWorkersHolding`) — and each
tool's `deposit_take` applies on the rungs it names, which is ordinary effect resolution.
`extraction::working_default_kit` is the one function behind the kit `assign_labor extract` stores
when the command names none, the seed, and `DepositState.defaultKitId`; `working_offered_kits` is
behind `DepositState.offeredKitIds`. `default_kits.extract` and `default_kits.quarrywork` are both
`none`. See `equipment.md` → "The take axis".

**The useful-crew cap is on the row** — `LaborAssignment.usefulCutters` (`extraction::useful_cutters`),
and the row's `workersNeeded` is the same count struck by the turn: **the crew whose capacity
reaches the room above the row's floor** (`extraction::deposit_crew_needed`), the forage and hunt
rule (`yield-forecast.md` → "`workers_needed` IS THE ROW'S WHOLE CREW"). Take hands are walked
best-equipped first over the units the row's claim settles plus the spare ones on the shelf, each at
its cut (the rung's bare rate plus its tools' `deposit_take` on the held rung) capped by its carry;
the keep hands at the row's crew are folded in and the sum rounded once. Crew-independent, so a crew
short of the room reads above itself; `1` where nothing is reachable; `0` off an extract row. A band
with axes still stops before the bare quotient, because its armed hands cut more each. Pinned by
`server::tests::useful_cutters_is_the_sheet_curves_plateau_and_gear_lowers_it` and
`::a_deadfall_above_its_floor_tells_one_cutter_a_second_would_cut_more`.

> ⛔ **IT WAS THE CURVE'S PLATEAU OVER THE BAND'S OWN POOL, AND THE POOL IS WHAT CAPPED IT.** With no
> idle hand the pool was the crew, so the plateau was the crew whatever another cutter would cut.
> Played on Firbrook — deadfall 585.6 of 600, floored at half, one sledded cutter at `+0.6` a turn
> and one sled in the band — the row read `usefulCutters 1` (`2` on the save with one idle hand)
> and greyed the `+` while the compose sheet said a second forester would draw it down. It reads
> **958** now: the one sledded hand at `0.6` and the rest bare at `0.3` against `287` of room. The
> `DepositCrewTake` curve rises one hand at a time up to it, which is the agreement. The next rung's
geared cut is the crew curve's `next_rung_take` alone; no committed-row field carries it.

**One seam, three readers.** `take_from_deposit` (the turn), `server::seed_source_yield`'s `Extract`
arm and `deposit_take_overdraws`' ability half all read the crew's lift — the tools' `deposit_take`
through `deposit_gear` at the rung the working **holds** (`standing.held`), and the crew's carry —
as one `extraction::CrewLift`, so the seed, the take and the ⚠ agree. Pinned by
`server::tests::a_deposit_crew_with_axes_is_seeded_the_cut_the_turn_pays` and, through the shipped
turn, `extraction::a_crew_with_axes_cuts_more_off_a_felling_working_and_wears_only_the_axes`. **The
turn reads the rung before this turn's build**: a working raised mid-walk is cut at its old rung's
tool effects that turn and its new rung's the next.

**THE CUT IS CAPPED BY WHAT THE CREW CAN CARRY, LIKE A HUNT.** `extraction::deposit_crew_throughput`
is `min(workers × yield_per_worker_turn + tools, carry)`, where `carry` is the crew's
coverage-weighted haul — the hunt's own `hunt_carry` resolution (bare
`labor.hunt.per_worker_biomass_capacity` 12, the sled 40) over the same coverage a hunt row's is —
times its head count, over the material's `weight` (`extraction::DepositCarry`). A hunt's kill is
bounded by what its hunters can haul; a cut is bounded the same way, near or far, so a sledded crew
cuts more of a heavy material than a bare one. The turn reads the carry its hunt arm already
computed for the row (`hunt_per_worker_biomass`), the seed, the crew curve and the work-party query
read it off the same coverage through `DepositCarry::lift`. Pinned by
`work_party_caravan::a_local_extract_take_is_capped_by_carry_over_weight`. **The sled is charged
`biomass_hauled` on every extract take** (units × weight), against the row's kit, the hunt's own
haul quantum.

### The compose sheet asks a crew curve BEFORE the commit — `DepositCrewTakeQuery`

Every figure on the deposit compose sheet is read before the player presses Cut, so it cannot come
off the committed row's `materialYield` / `kitWorkersHolding` alone.
`QueryPayload::DepositCrewTake` (proto `QueryCommand.deposit_crew_take = 8`, answered on
`QueryReplyEnvelope.deposit_crew_take = 11`) asks it the hunt curve's way: band, `(x, y, material)`,
the sheet's `kit_id` (`none` included; a kit not listing `extract` is `kit_wrong_job`), `floor` and
`max_workers`, and answers one `DepositCrewTakeRow` per crew size — `take` (this turn's cut at the
held rung, geared, carry-capped and reach-capped), `armed_workers` (the crew's whole-kit count),
`next_rung_take` (the cut once raised) and `keep_hands` (proto 5, the hands the crew keeps the
working with — the split `take` is struck on, `DepositCrewQuote::keep_hands`) — plus the `held_rung`
/ `next_rung` it priced and `in_range`.

**One model, not a second.** Each row is `extraction::deposit_crew_quote`: the crew is a
**prospective row** (`extraction::prospective_deposit_gear` — the whole kit, the band's other rows
beside it, less the pools' issue, and the crew's carry off the same coverage), the take runs through
`deposit_take` at the working as
the next turn finds it (renewed on a clone, the seed's rule; an unopened working derived from
`DepositSource::opening`), and the next rung through `next_rung_take_for`. So committing crew `n`
with kit `k` pays row
`n`'s `take`: `server::tests::the_deposit_crew_curve_is_what_the_turn_pays` (felling with axes and
idle sleds, an unopened deadfall with sleds, and `none`) and
`::the_deposit_crew_curves_next_rung_is_what_the_turn_pays_once_raised`. It is seat-gated like the
hunt curve (`querying_faction`), refuses a bad band, kit, floor, crew or material by token
(`::a_deposit_crew_ask_is_refused_field_by_field`; ground holding none of the material is
`unknown_deposit`). **Past the band's work range it quotes the take AT THE SOURCE**, exactly as
inside it: a far working posts a work party and its crew cuts the deposit as any crew does, so row
`n` is the cut `n` hands make standing there. `in_range` is a plain fact (`true` inside the apron)
and zeroes nothing — the hunt crew-take curve's rule, which carries no range gate at all. What
reaches home, delayed by the walk, is the work-party query's `rate_home` on an `extract` source
(`work-party.md`); pinned by `work_party_caravan::a_far_workings_crew_curve_quotes_the_cut_the_turn_makes_at_the_source`.
**`armed_workers` IS the committed row's `kitWorkersHolding`**, counted the same way: the crew's
whole-kit count (`KitCoverage::workers_holding_whole_kit`, the scarcest of the kit's items) off the
prospective row's share of the band's stock, which for a committed crew is arithmetically the turn's
own budget. So the sheet's *"N of M kits available"* before the commit is the row's count after it,
exactly as a hunt sheet's committed arm reads `kitWorkersHolding`; `none` reads the whole crew
(*"nothing to be short of"*). The per-tool share that lifts the cut stays inside the take math
(`extraction::CrewLift`). Pinned by
`server::tests::the_deposit_curves_kit_count_is_the_committed_rows_kit_workers_holding`.

**Wear is `WearQuantum::DepositTaken`, per unit cut BY THE HOLDERS.** `extraction::deposit_geared_units`
bills `taken × equipped_labor / labor` — the equipped workers' share of the crew's throughput,
applied to what was actually taken — against the items whose `deposit_take` served that rung
(`DepositGear::wear_kit`, effect-scoped like a build tool's wear), so a bare hand in a half-axed crew cuts for free, a take capped by the
reach bills the smaller number, and the wedges wear nothing on a felling take.

> ⛔ **THE AXE IS ON A TAKE ROW'S KIT, ON THE BUILDERS' REQUIREMENT AND ON THE WORKING'S OWN
> KEEPING**, and one axe arms one person per turn across all three. The builders and each working's
> keeping are issued theirs first (`settle_scarce_tools`, by priority — the keeping at its row's,
> the builders at the head entry's mark), and the `extract` rows split what is left
> (`BandItemBudget::reserving`, by row Priority, less `LaborAllocation::last_keeping_issued`) —
> `equipment.md` → "AND THE POOLS LEFT THE PRO-RATA ITEM BUDGET" owns the rule and why the claims
> win. `extraction::prospective_deposit_gear` reserves the keeping's issue the same way, so the seed
> quotes the cut the turn pays
> (`server::tests::a_seed_beside_a_kept_working_holding_axes_quotes_the_cut_the_turn_pays`).

**A far working's porters carry on the row's kit.** Past `band_work_range` the take walks home a
pack at a time (`work-party.md`), and one pack is the row's haul carry over the material's `weight`
(`materials.json`) — the same carry that caps the cut. Every take kit carries the sled, so every
kitted crew hauls on its sleds on every rung, felling and quarry included, because the kit is
claimed whole.

## The two road tools are WIDENED, not duplicated

`stone_dressing` — the maul, wedges and dressing hammer — declares a **second** `build_work` effect
on `extraction:quarry` beside its `route:paved_road` one. A pick is a pick, and minting a second item
that was the same three tools under another name would have made the roster longer without making a
decision. `earthmoving` is deliberately *not* widened: one rung wants one serving kit
(`build_kit_for_branch` takes the earliest match), and two kits answering for `extraction:quarry`
would make which one a builder carries an alphabetical accident.

**That forced two shipped invariants open, and both were narrowed rather than deleted:**

- `validate_effect_layer`'s *"a stat declared twice in one layer is a silently dead line"* now
  exempts `build_work` entries bound to **different** rungs. It held for every stat resolved through
  `LiveItem::effect_entry` (first match wins) — and `build_work` is no longer one of them:
  `LiveItem::build_work_entries` sweeps the whole layer and every consumer filters on
  `serves_build`. Two entries that could both serve one build are still a dead line and are still
  refused, which is what protects the per-worker **sum** the `rung` bound exists for.
- `every_crew_built_rung_has_a_builders_kit_that_serves_no_other_web` cross-checks *the rungs a kit
  did not declare*, not *every rung off its branch* — and its missing-kit arm is now a fork: a rung
  with no **builders kit** is legal only where no builders kit serves its branch at all. `forestry`
  has none: its build tool, the axe, rides `woodcutting` (an `extract` kit) and reaches the builders
  through the rung requirement (`pool_toe`), which asks items rather than kits. The failure the
  test exists to catch is a **partial** roster, one rung's kit
  gone missing while its siblings keep theirs, after which every build there silently falls back to
  `none` for the rest of the game.

## The working belongs to a CAMP, like a patch — not to nobody, like a road

This is the one place the arc deliberately does **not** copy `RungBranch::Route`. A road follows no
one and is free to leave, which is why it belongs to no camp. So `BuildSource::Deposit` *is* backed
by a labor row (`LaborTarget::Extract`), and `holds_build_source` resolves it through that row like a
patch's.

**A working past the band's work range posts a WORK PARTY, the Forage arm's rule byte for byte, and
nothing lapses for distance.** The row keeps the working; its crew walks each full pack home and back
(`work-party.md` → "The deposit web: the cargo is the material"). **The move-or-stay pressure is paid
in walking**: a band that moves away from its quarry keeps it, and pays the round trip on every pack
— the take arriving home falls with the distance, and a road between camp and quarry wins it back.
The retired rule lapsed the row the turn the band stepped past `band_work_range`
(`status=lapsed reason=out_of_range`), which made a working the one source a band could not keep
from a new camp.

**A working raised above its free floor keeps its row through an unstaffing**
(`source_has_a_meter_at_risk`), with one difference from the two food webs: there is no rot to be at
risk *from*, so what the row protects is the position itself. A working still on its free floor is a
wild stand and its row goes.

## The three verbs — `fell`, `coppice`, `quarry`

`fell <faction> <x> <y> <material>`, and the same shape for the other two. Each **declares**: an
entry on the build queue of every band of the faction that already has an `extract` row on
`(tile, material)`, raised by that band's `builders` pool at the head of the queue — so none of them
names workers, exactly as no rung verb has since `plan_standing_upkeep.md` §2.5.

**It is `cultivate`/`sow`'s grammar and not `grade`/`pave`'s, and the section above is the reason.** A
road belongs to nobody until a band is named, so the route verbs carry a band token and stamp a
`RoadKeeper`; a working belongs to a camp, so its keeper is already known and there is nothing for the
command to name. `handle_deposit_verb` therefore goes through the same
`queue_build_on_working_bands` the plant and animal verbs do, and inherits its *"no band is working
this"* rejection — which is also why several bands can quote one working's countdown, the reason
`publish_entry`'s deposit arm needs a `BuildEstimateClaims` where the road's needs nothing.

⛔ **A MATERIAL TOKEN, WHICH NO OTHER TILE VERB CARRIES.** The working's key is `(tile, material)`
because one hex can hold two — rolling hills carry timber *and* rock — so a line naming only the tile
names **neither**, and the plausible shape of that hole is one that raises whichever the registry
answered with first. It rides a trailing positional token in `assign_labor extract`'s own position,
after the tile, so the two ways of addressing one working read alike, and the tail is **closed**: the
material is the last token, so an extra one would be silently dropped on exactly the verb where a
second material name is the plausible typo.

**The gates are `validate_improvement`'s `Extract` arm and `validate_deposit_verb`, run once.** Those
were written a slice before the verbs existed and were reachable from nothing; the command runs them
rather than a second copy, which is what keeps the command's refusal and `deposit_head_gate`'s the
same two terms.

**"There is no timber on this ground" is answered one command upstream**, and deliberately not here: a
free floor's `site_requirement` is `null`, so no capacity term in `validate_deposit_verb` can refuse
a `fell`. What refuses it is `validate_labor_policy`'s `Extract` arm at the moment a crew is
assigned — absence in `by_terrain` *is* the answer — so a tile holding no wood has nobody on its wood
for the verb to declare for, and the verb's own rejection names the crew.

> ### ⛔ THE VERBS SHIPPED AS `Improvement` VARIANTS WITH NO COMMAND AT ALL, FOR A WHOLE ARC
>
> `Improvement::Fell` / `Coppice` / `Quarry`, a `RungKey` apiece, `RungKey::built_by` mapping them,
> `valid_for_extract` guarding them and `validate_deposit_verb` gating them — and **no grammar, no
> proto message, no `Command` variant and no dispatch arm** (issue #650). Nothing failed to compile,
> because a verb nobody sends is a verb nothing calls; the client's rung ladder was built and pressing
> it could send nothing. A player could cultivate a patch, sow a field, grade a road and pave one, and
> could not build a quarry by any means.
>
> The guard is `server::tests::every_build_verb_has_a_command_line`, which walks **`Improvement::ALL`**
> — a new variant fails its exhaustive match until somebody states the line that declares it — and
> drives each line through the *encoded* envelope: the grammar, the proto round trip and
> `command_from_payload` are three separate crates that each compile perfectly without the next, so a
> test that handed the parsed payload straight to the handler would prove only the first and the last.
> The `Command` it comes out as is compared against `commanding_faction`'s own label rather than a
> hand-written expectation, so a `fell` line that decoded into `Command::Coppice` cannot pass.

## The fourth verb — `abandon_working`, and why a bare-tile `abandon` never reaches a working

`abandon_working <faction> <x> <y> <material>`, on the three rung verbs' grammar exactly: no band
token, the material as the closed trailing token, and the same *"no band of yours works this"*
rejection. It drops the band's **holding** — the `extract` row and its build-queue entry — on every
band of the faction working `(tile, material)`, through the same `drop_holding_and_cancel_ring` a
patch's `abandon` goes through, and **leaves the working's meter alone** to slide back at the rung's
own rate. Nothing is destroyed on the spot, so it needs no confirmation, and there is no `validate_*`
to run: putting a thing down asks nothing of the ground, the knowledge or the rung.

**THE ROW OUTLIVES ITS CREW, AND AN EMPTY ROW KEEPS NOTHING.** A working raised above its free floor
is a holding (`source_has_a_meter_at_risk`), so `assign_labor … extract … 0` is *"stop working
this face"* and keeps the row — but the working's keeping is that row's own crew
(`docs/plan_site_crews.md` §2.1), so a row at zero is unkept and the working slides. **Measured** on
a seated `extraction:quarry` at `AlpineMountain` with nobody on it: 4 turns of grace at **2.10**
work a turn, then a linear slide to **0.06** by turn 103, at which point the position reaches zero
and the row prunes itself — **104 turns**. `forestry:felling` is the same shape at **1.0 → 0** over
103 turns; `forestry:coppice` slides through `felling` on the way and takes **202**.
`abandon_working` drops the row and its build entry at once.

> ### ⛔ A BARE-TILE `abandon` NEVER REACHES A WORKING; ITS MATERIAL FORM *IS* THIS VERB
>
> `abandon <faction> <x> <y>` names a **place**: it drops the forage row on that tile *and*
> releases the faction's road keeping there — which its own tile card already warns about in a
> second line. A deposit verb names a tile **and** a material, because one hex holds two workings,
> so a bare tile reaching the workings too would make an already-destructive verb quietly more
> destructive on exactly the hexes where the player meant one of two things.
>
> **`abandon <faction> <x> <y> <material>` is `abandon_working`, not a second definition of it**
> (`docs/plan_site_crews.md` §2.4): `handle_abandon` calls `handle_abandon_working`, so the two
> spellings cannot disagree. `abandon_working` keeps its own proto field (**74**), payload and
> command, and rides `command_text.rs`'s `fell | coppice | quarry` arm so the material's position
> and the closed tail cannot drift from the rung verbs'; `abandon`'s material form carries the same
> token in the same place (`command_text::parse_site_source`). `unqueue <faction> <x> <y>
> <material>` withdraws a queued `fell` / `coppice` / `quarry` and leaves the row.
>
> `server::tests::abandon_working_drops_one_workings_holding_and_leaves_its_neighbour` drives the
> line through the **encoded** envelope on a hex carrying timber and rock at once, both staffed and
> both declared, so a verb resolving the working by tile alone fails there rather than passing a
> single-deposit fixture. `extraction::putting_a_working_down_stops_its_bill_and_its_neighbour_stops_sliding`
> pins the gameplay claim against the measurement above: the leak arm and the fix arm one drive
> apart, plus that the put-down working's meter still slides — untouched, not destroyed.
> `server::tests::abandon_and_unqueue_reach_a_working_and_a_road` pins the material forms of
> `abandon` and `unqueue`.

## What a working costs to HOLD — its own crew keeps it

Every **built** rung on both branches owes work per turn, and the `extract` row on the working pays
it **first**, before it cuts (`docs/plan_site_crews.md` §2.1): `keep_hands = min(crew, bill ÷
keep_rate)` and the cut runs on the rest. The take kit is spread over those cutters alone
(`equipment.md` → "A ROW CLAIMS ONLY THE TAKE HANDS"), so their tools and carry are theirs whole.
Without a bill a working's position never falls and **a quarry is free to hold for ever**, which
contradicts the arc this one sits on: an improvement that costs nothing to hold cannot weigh on
move-or-stay.

**The two FREE FLOORS owe nothing**, and that is what makes them free: `forestry:deadfall` and
`extraction:gathering` declare no `upkeep` at all, exactly as `plant:wild` and `route:path` do.
Nobody built them, so there is nothing to hold.

- **Paid in the `Extract` arm**, right after `deposits.open`, through the same site-keeping seam
  the patch and herd arms use (`systems::labor::site_keeping`): the claim is the working's pro-rata
  share of its stamped `upkeep_demanded` by crew across bands, the tools are the rung's requirement
  (the axe on felling and coppice, `stone_dressing` on the quarry) claimed per planned keeping
  hand at the row's priority, and `DepositSource::upkeep_supplied` / `upkeep_hands` /
  `upkeep_toe` (per people, read off for the viewer's `upkeepToolsShort`) are stamped for the capture.
- **The quote reads the same split** — `extraction::crew_keep_hands` / `crew_keeping_issue` and
  `deposit_crew_quote` strike the cut on the hands the keeping leaves, and
  `prospective_deposit_gear` reserves the keeping's tools first.
- **Retired**: `LaborTarget::Quarrywork`, `settle_bands_extraction`, `extraction_keeping_claims` and
  the per-assignment award vector they fed, and the band-level `quarrywork*` triple on
  `PopulationCohortState`; `KitJob::Quarrywork` and `default_kits.quarrywork` stay in the equipment
  vocabulary.

### The measure: keeper-loads off the deposit's own capacity

`UpkeepScale::SourceLoad`'s **fourth** branch reading (`extraction::deposit_keeper_loads`) is the
tile's capacity for this material over the material's own `capacity_per_keeper`.

**All three shipped branches measure the PLACE, never the activity** — a tile's `K` on the plant web,
a herd's head count on the animal one, a tile's `infrastructure_cost` on the route one — and this is
the same statement: a great wood takes more holding than a few stands along a draw, whatever crew is
on it that turn.

> ⛔ **IT IS POSITION-FREE, AND THAT IS `capacity_per_tender`'s TRAP AVOIDED RATHER THAN A
> SIMPLIFICATION.** The obvious alternative — *how much of the deposit this rung can reach*,
> `recovery_fraction × capacity` — interpolates on the ladder position, and `upkeep.work_per_turn`
> interpolates on it too, so the two would **compound**: a quarry would be billed for its climb
> twice over, which is exactly the ~10× a Field landed at when the plant measure briefly read the
> boosted `carrying_capacity` instead of the tile's own `K`. Capacity is the terrain's and no rung
> may raise it, so this measure provably cannot compound with the rate that rides it.

**The ratio belongs to the material and the rate to the rung** — `animals_per_herder`'s division —
because 600 units of wood and 600 units of stone are not the same size of job, and one global divisor
would make the two branches' bills incomparable for a reason that is about units rather than about
workings. Each `capacity_per_keeper` is **anchored on a reference terrain's own capacity**, exactly
as `capacity_per_tender` is `AlluvialPlain`'s `K`.

### The decay is what makes neglect self-limiting

`extraction::advance_deposits` (Logistics) is the deposit branches' `routes::advance_roads`: how
short → the bleed at the at-risk rung's own rate past its own grace → clear the payment and
**re-stamp the bill at the post-decay position** → **renew the stock**, at that same position.
The grace counter (`DepositSource::neglect_turns`) resets the turn a rung completes
(`DepositSource::set_ladder_position`), so a working raised by a crew short of its bill keeps its
full grace — `intensification.md` → "A COMPLETED RUNG RESETS THE COUNTER".
A working's slip and loss are announced on the `Extraction` channel to every people with an
`extract` row on it (a working has no owner) — `event-feed.md` → "A rung's decay is two edges".

**The slide shrinks its own penalty.** The position falls, the interpolated demand falls with it, and
an abandoned working decays toward costing nothing rather than bleeding a band's roster for ever
(§2.7).

⛔ **IT RUNS ON EVERY WORKING, HELD OR NOT** — `bill_and_stock_roads`' lesson. A pass that billed only
the workings some band still has a row on would leave an **abandoned** working reading as kept for
ever: never arming its counter, never decaying. A working its band has abandoned is precisely
what this branch's move-or-stay pressure is made of, so it is precisely the case that must decay.

**The payment is a whole stage later**, in the `Extract` arm of `advance_labor_allocation` — the
bill `advance_deposits` stamped in Logistics is what the crew's keeping hands are paid against.

### The build countdown nets the LIVE rot

`extraction::deposit_meter_rot` is what the `Extract` arm's `BuildQuote::balance` subtracts, resolved
through the same seams the decay pass bleeds through — so a quote cannot promise a rung will finish
while the next pass takes more off it than the builders put on.

⛔ **It passed `NO_UPKEEP_DECAY` for one slice**, on a comment that was true when slice 1 shipped
(*"neither deposit branch declares an `upkeep`"*) and that this slice falsified without updating.
**The blast radius is the whole queue, not the working**: `publish_build_chain` accumulates the
head's turns into `cumulative`, so an understated deposit head carried its error onto **every entry
behind it** — including the patch, herd and road entries that do reach the wire.
`a_working_whose_keeping_is_short_quotes_a_rotting_meter` pins both halves, because *"it said
Rotting"* also passes against a rot that is simply always larger than the crew.

### ⛔ Decay must not resurrect the §6 floor trap, and it does not

A falling position lowers `recovery_fraction`, which **raises** the floor and so reduces `reachable`.
That is correct and intended — you reach less of the deposit as the face slumps — and it is bounded
two ways: `deposit_reachable` floors at zero, and **the build gate reads the ground and the knowledge
and never the stock** (`deposit_head_gate`), so a slumped working is always re-cuttable at some crew
size. `a_slumped_working_can_be_cut_back_open` drives a quarry all the way back to its free floor and
then cuts it open again; the day somebody adds a stock term to that gate, it fails.

### No standing material rate, and `validate` refuses one

**The quarry's 8 wood stays a BUILD PILE.** Props, ramps and sleds timbered once as the face is
opened go *into* the working and stay there — §2.7's own pile-versus-rate distinction, against a road
whose stone rate is real because *re-dressing a road is not re-laying it*. It also puts the two
branches in the right order for a faction with nothing: you must have worked a wood before you can
open a quarry.

**On nine of the fourteen quarryable terrains that wood has to be CARRIED IN, and the pile is drawn
from the band's own store rather than from the ground it stands on.** Basalt, volcano slope, rocky
reg, fumarole and ash plain have never held timber; the #650 deletions add canyon badlands, high
plateau, the crater fields and the sinkhole field, whose wood rows were 15–45 and could bootstrap the
8-unit pile only over dozens of bare-handed turns. **No rung became unbuildable** — a band carries its
`LocalStore` when it moves, and the supply network pools between linked camps — so what changed is
that four more rock bodies now ask for the trip the other five always asked for, which is the same
ordering this section already states.

**Neither branch settles a standing material**, so `validate_upkeep` **rejects an `upkeep.materials`
on a deposit rung outright**. A rate there would parse, validate, publish a demand and be paid by
nobody — the *"looks live but isn't"* failure, and exactly what `route:paved_road` shipped for one
slice when its rot term could not see the second currency. Giving one branch a rate means building the
settle pass first and then deleting that check deliberately, rather than discovering it was never
enforced.

## Knowledge

Three lessons, on the ladder's own *practise rung N to unlock rung N+1* shape. Discovery ids 2014 /
2015 / 2016 (2011 is the retired `trailcraft` and is not reused).

| earned by | lesson | gates |
|---|---|---|
| `forestry:deadfall` | `woodcraft` (2014) | `fell` |
| `forestry:felling` | `conservationism` (2015) | `coppice` |
| `extraction:gathering` | `quarrying` (2016) | `quarry` — and the minerals arc's `mine` above it |

**Conservationism is learned by being in a position to ruin a wood**, which is why `felling` teaches
it: `felling` is the first rung on either branch at which over-cutting is possible. **And each lesson
is credited only on ground that could carry the rung it gates** — see *"you learn a rung by
practising where that rung could be built"*, which is why `woodcraft` is earned everywhere timber
grows while `conservationism` and `quarrying` are not.

**A deposit crew's lesson rides its own floor WHERE THE DIAL PARTICIPATES, through the seam both
food webs go through.** `intensification::learn_multiplier` is `floor / MSY_BIOMASS_FRACTION` and
belongs to no web: it prices *what you left standing* against *what you learned*, and an `extract`
row on renewing ground carries the same dial a Forage row does. So a crew told to leave more of a
wood standing learns conservationism faster, in proportion; one told to strip it learns nothing
(`learn_multiplier(0)` is `0`), and one at the top of the dial has no escapement room, so the work
predicate is false and watching teaches nothing either.

### ⛔ A working whose dial is not offered learns at the PLAIN RATE

`intensification::PRACTICE_AT_THE_PLAIN_RATE` is `learn_multiplier`'s fixed point, and what a working
at `NEVER_RENEWS` passes. The escapement is offered only where a deposit renews — the same condition
`deposit_effective_floor` carries — so on a rock body the row's floor is **stored, published and
inert**, and pricing the lesson off it would pay a crew a learning bonus calibrated to a choice
nobody made: the client omits the token there and `server::unnamed_deposit_floor` resolves the
omission to `STRIP_IT_BARE` on ground at `NEVER_RENEWS` — **not** to the `DEFAULT_ESCAPEMENT_FLOOR`
(0.5) every other row's omission resolves to — and that lands on the row (§ above, on
`handle_assign_labor`). Where it bites is
`extraction:gathering`, which earns `quarrying` and is a live teaching rung on a body that is finite.

`extraction::deposit_lesson_floor` is the only place the fork lives — the crew's own dial where the
escapement participates, the fixed point where it does not — and **both earn sites go through it**:
the live credit in the `Extract` arm and `source_is_still_teaching` in the shedding order, which
answer one turn apart and would otherwise let a working teach at one rate and report another.
`resolve_shed_facts` takes a `deposit_renewal_of` closure for that, `tile_capacity_of`'s twin one
branch over, because the tile query and the deposits config are the caller's to hand.

> ⛔ **AND IT IS NOT THE COMPOSED FLOOR.** The other option was to price the lesson off whatever the
> take actually stopped at, and it is wrong on the renewing side rather than the finite one: on a
> renewing scatter `extraction:gathering`'s rung floor is `0.85`, so composing would hand every
> gathering crew a permanent **×1.7** regardless of its dial — which makes the dial irrelevant to
> learning on the one rung that teaches `quarrying`. A rung's unreachable remainder is not a
> conservation choice and may not be paid for as one. **The renewing branch keeps pricing the lesson
> off the player's own dial, unchanged.**
>
> `extraction::a_finite_working_learns_at_the_plain_rate_and_a_renewing_one_rides_the_dial` pins both
> sides, the finite one on `extraction:gathering` and the renewing one on `forestry:deadfall` — see
> *"you learn a rung by practising where that rung could be built"* for why no stone ground carries
> the renewing reading any more. It asserts the plain rate as a **value** — a rock crew
> earns exactly what a renewing crew at the fixed point earns — since equal-to-each-other alone would
> pass against a lesson that had stopped being credited at all, and it asserts that the two lessons
> cost the same, since across two branches that equality would otherwise be a claim about
> `lesson_costs`.

The other *"this source has no dial"* reading is `systems::labor::credit_managed_rung_lesson`'s, and
it belongs to **rung 3**, which is a claim about a take that draws nothing down rather than about a
branch.

## Config files

| File | Purpose |
|---|---|
| `src/data/extraction.json` | **THE DEPOSITS** (`extraction_config.rs`, env override **`EXTRACTION_CONFIG_PATH`**). `seed_fraction` **0.02** — what a deposit regrows from when it has been taken to nothing, evaluated *inside* the growth term so a rate of zero seeds nothing. Then one `deposits` row per material: its `branch`, and a `by_terrain` table of `{ capacity, regrowth_rate, characteristics }`. **A terrain absent from `by_terrain` holds none of that material** — absence is the answer, so there is no `enabled` flag and no parked `0.0` row, and every water terrain is absent from both tables deliberately. `DepositDef` and `DepositTerrain` are **`deny_unknown_fields`**, so the file's prose lives at file level in `_comment_*` keys, `materials.json`'s discipline. **Stone is two populations in one table**: rock bodies in the low thousands at rate `0.0` (alpine 4200, karst 3000, basalt 2600 … rolling hills 900) and loose-stone scatters in the tens at a small positive rate (periglacial 70 … mangrove 5). **Wood regrows on every row** — mixed woodland 600 at 0.03, boreal taiga 450 at 0.015, marsh withy 90 at 0.055 — because a forest that is worked out is not a forest. **The wood table stops at 50 and the floor is a design line, not a tuning one** (issue #650): eight rows under it were **deleted** rather than tuned down — canyon badlands and tundra 15, periglacial steppe 20, prairie steppe 25, crater fields 30, high plateau and semi-arid scrub 40, sinkhole field 45 — because a wood a bare-handed crew works out in a few dozen turns costs the player a decision and pays them nothing. **Absence is the config's own mechanism**, so no code carries a threshold; the terrains simply hold no timber, and each of the eight keeps its stone row, so **nothing is left holding neither material** (the only rows absent from both tables are the six water terrains and `Glacier`, deliberately). **⛔ THE STONE TABLE'S SMALL ROWS ARE THE OPPOSITE CASE AND WERE LEFT ALONE** — the low-capacity **positive-rate** scatters are *loose stone the ground keeps turning up*, which is what makes knapping flint available nearly anywhere, and a scatter that regrows is never worked out the way a 15-unit copse is. **The characteristic ratings are LIVE on both materials since issue #736**, and the shape they were authored for is the shape their readers wanted: wood's `hardness` / `pliancy` are read by the two road tools (`earthmoving` / `stone_dressing`), and stone's `hardness` / `workability` by the three knapped kit recipes — `spears_flint` and `clubs_flint` want a hard stone for a point that holds, `hoes_flint` a workable one for a blade that flakes predictably. So a deposit's ratings now decide what grade the things made out of it come out at, which is what *"genuinely opposed, no best deposit"* was written in advance of rather than instead of. They are still **playtest dials** — nothing about a rating was retuned when its reader landed. **`capacity_per_keeper`** is the divisor that turns a tile's capacity into the keeper-loads the rungs quote their `work_per_turn` per — wood **600** (`MixedWoodland`'s own capacity, so a felling working on closed woodland is exactly one load) and stone **3000** (`KarstHighland`'s — limestone, the classic quarry stone, and the middle of the rock bodies, so rolling hills reads 0.3 and an alpine mountain 1.4). Every number is a **playtest dial** |
| `src/data/intensification_ladder.json` | The five new rung records and the `extraction_payoff` block on each — see `intensification.md` for the ladder engine. `knowledge.lesson_costs` gains `woodcraft` / `conservationism` / `quarrying` at 20 apiece. **The three BUILT rungs each declare an `upkeep`** (`scaled_by: source_load`): `forestry:felling` **1.0** work a turn per keeper-load, rot **0.6**, grace **3**; `forestry:coppice` **2.0** / **1.5** / **2**; `extraction:quarry` **1.5** / **2.5** / **4**. The rates read as *keepers on the reference ground*, because `capacity_per_keeper` is anchored there — a felling working on closed mixed woodland is exactly one keeper, against the plant web's 2.0 for a tended patch on *its* reference tile. Each `meter_decay` is the pacing-neutral inversion of the plant web's rule of thumb (a wholly unmaintained rung lapses over ~100 bleeding turns), so it tracks each rung's own `work_cost`. The graces say how forgiving each rung is of a crew re-tasked for a season: a quarry face is the most forgiving at 4 because the rock does the holding, and a **coppice** the least at 2 because a managed wood is the most perishable thing on either branch — the same direction `plant:field` runs in against `plant:tended`. **The two free floors declare none.** |
| `src/data/equipment.json` | three `extract` kits, **`sledding`** (`sled`), **`woodcutting`** (`sled` + `axe`) and **`stonework`** (`sled` + `wedges`) (#663); the `sled`'s plain tier declares `deposit_take` **0.3** on `forestry:deadfall` and **0.4** on `extraction:gathering` and wears `deposit_taken` **1.0**; `default_kits.extract` and `default_kits.quarrywork` are both `"none"` — a working's offered kits are derived per branch (`working_offered_kits`) and its default per held rung (`working_default_kit`); the `none` kit's `jobs` carries both; `stone_dressing`'s `plain` tier (its only one) carries its second `build_work` effect on `extraction:quarry`; the `axe` (`flint`, 70 durability) declares `deposit_take` **1.0** on `forestry:felling` and `forestry:coppice` plus `build_work` **0.5** on `forestry`, wearing `deposit_taken` **1.0** / `build_progress` **0.16** / `upkeep_work` **0.16**; the `wedges` (`flint`, 70) declare `deposit_take` **1.0** on `extraction:quarry`, wearing `deposit_taken` **1.0** |
| `src/data/recipes.json` | `axe` — knapping, work 6, stone 1 (`reads: hardness`) + wood 2 + fibre 1 → `axe` at `flint`; `wedges` — knapping, work 6, stone 2 (`reads: hardness`) + wood 1 → `wedges` at `flint` |

## The wire — one row per DEPOSIT-BEARING TILE, and the rate picks the readout

`DepositState` is the deposit's row: keyed `(tile, material)` because one tile can hold two, built by
`snapshot::deposits::deposit_states` and diffed as a whole vector on `foragePatches`' rule (no
`removedDeposits` twin — a row that leaves the frame leaves by being absent). The `extract`
labor row carries `material` beside its tile. The working's keeping is on its own row —
`upkeepDemand` / `upkeepSupplied` / `upkeepShortfall`, and `upkeepHands` / `upkeepToolsShort`
(`docs/plan_site_crews.md` §4); `PopulationCohortState` carries no band-level `quarrywork*` triple.

**A ROW DESCRIBES THE GROUND, AND THE WORKING IS ITS STATE — the FORAGE PATCH's shape.** A row is
published for **every discovered tile that holds a deposit** — every `(tile, material)` pair whose
`tile_deposit_capacity` is above `NO_DEPOSIT` — and the registry's live `DepositSource` is merged in
where one exists. A patch row stands on every food-bearing tile whether or not anybody has touched
it, for exactly the reason this row now does: what it is *about* is the land.

⛔ **THE REGISTRY IS NOT SEEDED TO ACHIEVE IT.** An unopened deposit's row is derived at capture from
`DepositSource::opening` — full stock at the tile's capacity, the branch's free floor, no upkeep, no
neglect, no take — and is saved nowhere, so retuning `extraction.json` still reaches it and a
checkpoint does not grow a row per land tile per material. `DepositRegistry` stays exactly what it
was: the lazily opened set of workings a band has put a crew on.

> **PUBLISHING ONLY THE REGISTRY MADE THE WHOLE FEATURE UNREACHABLE** (issue #650). The client builds
> its tile-card `Workings ▸` affordance off these rows, and `DepositRegistry::open` is reached from
> one place — the labour pass, when a crew is assigned. So a fresh world published **zero** rows, the
> action appeared only on a tile that already carried a working, and nothing in the client could
> create the first one. **Absence of a row now means *there is no deposit on this ground*** (or the
> faction has not explored it), never *nobody has worked it*.
> `wire::ground_nobody_has_worked_still_publishes_what_it_holds` is the regression test, and
> `wire::a_live_working_wins_over_the_derived_opening_state_on_one_tile` pins the merge's direction
> on a hex carrying a seated quarry beside untouched timber.

**On an unopened row the two §7 readouts fall out of the existing seams with no special case.** A
renewing deposit standing at capacity quotes its **MSY** — an untouched wood is the one that can
best afford a crew, not the one with nothing to give; a finite one nobody is cutting answers
`DEPOSIT_RUNWAY_NO_TAKE` off a `last_take` of
nothing, and a renewing one still answers `DEPOSIT_RUNWAY_NOT_APPLICABLE`. `isQueued` is keyed
`(tile, material)` and answers `false` for a key it does not hold.

**The fog gate is the ROAD's `Discovered`, not the herd's `Active`, and the whole row passes or none
of it does.** A quarry does not wander off, so remembering one is remembering something true; and a
row carries a `ladderPosition` a band earned, so it takes the road's **row-level filtering** rather
than the two food webs' publish-and-let-the-client-redact. That is what keeps `ladderPosition`,
`buildFraction` and `rung` off ground the faction has never seen, with no per-field rule to get
wrong.

**Order is `(y, x, material)`** — `snapshot_forage_patches`' own sort, because the rows are built off
the capture's tile sweep (`extraction::tile_holds_a_deposit` picks the ground out of the one full
walk) rather than off the registry's key order.

**Measured on the shipped 80×52 map at full reveal: 3,245 rows and ~435 KB, against `foragePatches`'
2,113 rows and ~1.82 MB on the same frame** — 1.5× the rows at under a quarter of the bytes, because
a deposit row is far narrower than a patch's. Both sections are diffed as whole vectors, so the cost
is per frame.

**Every derived number is read LIVE off the tile at capture** — capacity, the ground's rate,
`deposit_reachable`, the payoff — because that is `DepositSource`'s own doc comment: it carries the
stock and the position and nothing that could be derived. A tile the capture's sweep never saw
publishes **nothing rather than a row at zero**, since every number on the row is a function of that
ground.

### The floor and the chart the client draws it on

Three fields ride the row for the escapement instrument, two of them **named after their
`ForagePatchState` twins** so the client's chart builder is reused rather than forked:

| field | what it is |
|---|---|
| `rungFloorFraction` | the **rung's own** floor in the same units, `1 − recovery_fraction`. ⛔ **Compose the two as a MAXIMUM, and only where `regrowthRate > 0`** — a chart that added them would draw a gathering crew stopping 85% of a seam short of where it really stops, and one that took the maximum on a quarry would draw it stopping at the dial the sim ignores there |
| `defaultKitId` | the `extract` kit a crew on this working is sent with when the command names none — `extraction::working_default_kit`, the same function `assign_labor` stores a no-kit row's kit with. It follows the held rung: `"sledding"` on `deadfall` and `gathering`, `"woodcutting"` on `felling` and `coppice`, `"stonework"` on `quarry`; `"none"` only where no single kit fits the rung most tightly |
| `offeredKitIds` | the `extract` kits a crew on this working may be sent with, in roster order — `extraction::working_offered_kits`, every kit whose items all declare a `deposit_take` on the working's branch. `["sledding", "woodcutting"]` on wood, `["sledding", "stonework"]` on stone. `none` is not in it: it is the job's own fall-back, which a picker offers beside these |
| `perWorkerBiomass` | what ONE **bare** cutter moves per turn at the standing rung, in the material's own units. No seasonal weight, and it is a SOURCE row, so no band's take kit is in it — a row's axes add `deposit_take` per equipped worker on top (see "The floor rungs must be workable BARE-HANDED"). It is the rung's rate flat and is never `0` on a live rung |
| `regrowthSamples` | the deposit's own growth curve, sampled on the **same implicit x-axis** as the patch and herd curves (`snapshot::subsistence::regrowth_sample_fraction`), through `deposit_regrowth` — the seam `renew_deposit` advances the stock with, at the rung's scaled rate |

⛔ **THE PLAYER'S HALF OF THAT MAXIMUM IS NOT ON THIS ROW, AND A SOURCE-LEVEL ONE WAS DELETED.** A
`floor` field published `DepositSource::last_floor` — where this turn's crews stopped, deepest-first
across the bands cutting the working — and **no surface ever read it**: a compose sheet states what
*one* band is asking for, which is `LaborAssignment.floor` on that band's own `extract` row, and
seeding a dial from a source-level minimum would silently adopt another band's deeper floor. Its
only consequence, `reachable`, is already published at the composed floor. `DepositSource::last_floor`
itself **stays** — it is what `reachable` is composed at.

⛔ **A QUARRY'S CURVE IS ALL ZEROS AND IS STILL PUBLISHED.** Rock's rate is `NEVER_RENEWS`, so the
delta is exactly `0` at every reading point — *this does not grow*. An **empty** vector is the
different claim *no curve was sent*, which is what a client blanks its chart on, so the codec's
`is_empty → None` rule and this all-zero reading are two distinct states and must stay so. No sample
is ever negative: a deposit has no Allee term, so the curve's shape is the plant one's.

**The client has no `ecologyPhase` / `collapseFraction` / `stressedFraction` to draw zones from,
because a deposit has no ecology phase in the sim.** Nothing classifies a working as thriving or
collapsing, and inventing a band ladder to fill a chart would be a mechanism with no owner. The
chart's phase zones are simply absent there.

**Nor a `buildDestinationCapacity` twin**, and that absence is provable rather than pending: no rung
on either branch may raise `capacity` (`no_rung_on_either_branch_may_raise_capacity`), so `floor ×
capacity` cannot climb under a build the way a gentling herd's does. There is nothing for a
*"the floor is moving"* flag to mark.

**Every repeated field on this row must be seeded in `xtask/src/decode_fixture.rs`** — its
`assert_no_empty_arrays` gate is what stops an appended vector reaching the client as nothing at all,
and `regrowthSamples` is seeded at `REGROWTH_CURVE_SAMPLES` beside the patch and herd curves.

### A freshly-assigned deposit crew is seeded, and it is seeded in MATERIALS

`server::seed_source_yield` writes the touched source's pre-commit forecast into
`LaborAllocation.last_yields` right after `set_assignment`, which is the only reason a fresh forage
crew reads a real number instead of `+0.00`. The `Extract` arm used to `return` outright, on the
reading that *a deposit pays no food, so there is no row to seed*.

**That was right about the food and wrong about the row.** `actualYield` is unconditionally on the
wire at `0.0` whether or not the seed runs, so declining to seed did not remove a `+0.00` food line —
it removed the **material figure beside it**, and a working the player had just staffed published
nothing at all until the turn resolved.

The arm now seeds `SourceYield::materials` alone, through the take's own seams: **regrow first, then
take** (`renew_deposit` on a clone, then `deposit_take` at the row's floor), which is
`yield-forecast.md`'s *"a forecast regrows first"* rule — the seed is read between turns, so the live
stock is the one this turn's take already drew down. A working nobody has opened is **derived** from
`DepositSource::opening`, `snapshot::deposits`' own rule, which is what gives the commonest case of
all — a crew put on fresh ground — a figure at all.

⛔ **`actual` stays `SourceYield::ZERO`.** `PopulationCohortState::food_income` is `Σ actual` and one
side of the pinned larder identity, so a working must contribute nothing to it, seeded or resolved.
The turn's own `Extract` arm writes `row.materials` and touches no other field, and the seed mirrors
it exactly — which is what keeps `forecast == actual` true per component here.

> #### `DepositState.actualTake` is NOT seeded, and that is a decision
>
> It is `DepositSource::last_take`: a **source-level accumulator**, `+=` across every band cutting the
> working and cleared once per turn by `advance_deposits`. Seeding it at assign time cannot be
> reconciled with either of the two things commands actually do — a second `assign_labor` on the same
> row would double it under `+=` and a second band's would clobber it under `=`, and both fail
> silently. It is also the denominator of `turnsRemaining` and one half of the over-cut pair, so a
> seeded value would put a projection on both.
>
> So `actualTake` keeps meaning **what was cut**, and `DEPOSIT_RUNWAY_NO_TAKE` keeps meaning *nobody
> is cutting it* — both of which are true of a just-assigned working. What the client needs to say
> *"this crew will cut X a turn"* is on the wire already: the crew is on the assignment row
> (`workers`), the rate it will cut at is the seeded `materialYield` beside it, and the runway is
> `reachable ÷ that rate` — linear and exact, which is the side of the boundary rule where the sim
> ships terms and the client evaluates them.

### `regrowthRate` is the fork, and `branch` is not

`deposit_runway` answers `sim_schema::DEPOSIT_RUNWAY_NOT_APPLICABLE` (`-1`) the moment
`tile_deposit_regrowth > 0`, so a renewing working publishes the **over-cut pair** —
`deposit_sustainable_take` against `DepositSource::last_take` — and
a finite one publishes the **runway**, `floor(reachable / last_take)`. Surface flint and a quarry are
both `extraction` and land on opposite sides of it, which is §3's callout restated where it is
observable; `a_renewing_working_quotes_the_pair_and_a_finite_one_quotes_the_runway` asserts the two
against each other in one run.

#### ⛔ THE SUSTAINABLE HALF IS THE **MSY**, NOT THE GROWTH AT TODAY'S STOCK

`deposit_sustainable_take` reads the growth term at `min(stock, MSY_BIOMASS_FRACTION × capacity)` —
`fauna::sustainable_yield`'s own expression (`net_biomass_delta(min(B, K/2), …)`) with the deposit's
curve substituted for the food web's. That is what §7's *"the existing sustainable-versus-actual
income breakdown pointed at a new source"* actually resolves to: a **full forage patch quotes its
MSY** (`systems::labor`'s forage arm: *"one turn's MSY of the patch at its pre-take biomass"*), so a
deposit quoting anything else would be two answers to one question.

**It shipped as the instantaneous growth term, and that fired the ⚠ on the single most ordinary
action in the feature** (issue #650). A mature wood stands at `K`, where `(1 − S/K)` is zero, so
`actualTake > sustainableTake` was true of the *first* cut. Measured on mixed woodland (`K` 600,
`r` 0.03) with one cutter at `forestry:deadfall`'s 0.3 a turn: sustainable read `0.0090` on turn 1
and `0.0646` by turn 8, against an actual of `0.3000` throughout — against a true MSY of
`r·K/4 = 4.5`, fifteen times what the crew was taking. **And it never cleared**: the stock converges
on the point where growth equals the take *from above* (≈589.8 wood here), an asymptote, so the
strict inequality holds for every finite turn. A warning that fires on correct play for ever teaches
players to ignore it. `wire::a_full_wood_sustains_an_ordinary_crew_rather_than_warning_on_the_first_cut`
pins the quiet case over eight turns and
`wire::a_crew_that_out_cuts_the_msy_reads_as_over_cutting` pins that the ⚠ still has teeth — three
fellers taking 6.0 against the same wood's 4.5.

**The rung is in it, because the published `regrowthRate` is.** The MSY is taken on the ground's rate
*already scaled by* `regrowth_multiplier`, so a coppice sustains twice what a felling working does —
which is exactly what the forestry branch is for.

**`sustainableTake` reads `0` on a quarry by arithmetic, not by a branch**: `deposit_regrowth` at a
rate of `NEVER_RENEWS` returns its argument unchanged *wherever the curve is read*, so the difference
is exactly zero and the MSY reading changes nothing about stone. **The runway
is a FORWARD projection** (the arrivals rule): its denominator is `last_take`, an accumulator the
band rows add into and `advance_deposits` clears once per turn, so the count moves the turn the crew
does. A finite working nobody is cutting answers `DEPOSIT_RUNWAY_NO_TAKE` (`-2`) — it *will* run out,
just not while it stands idle — which is deliberately a different sentinel from `-1`.

### The countdown goes through a claims set, where the road's does not

`DepositSource` gained the road's four scratch fields — `last_take`, `build_blocked_reason`,
`build_turns_remaining`, `build_queue_position` — all cleared by `advance_deposits` phase 3 on the
one-turn cycle, which is what makes *"live-queued and still cleared"* mean *"queued since the last
pass"*.

`publish_entry`'s deposit arm publishes through `BuildEstimateClaims<(UVec2, String)>` — the road's
own reason read the other way. **A road has one keeper per tile**, so at most one band can hold an
entry for it and there is nothing to arbitrate; **a deposit verb enqueues on every band of the
faction working the source**, so several bands really can quote one working and the sooner answer
must win. It uses `publish_countdown` rather than the six-field `publish_running`: `DepositState`
carries the date, its cause and the place in the line and nothing else, and giving `DepositSource`
gear/destination/leg fields so it could pass a `BuildEstimateSlots` would be writing state no capture
reads.

The unqueued tail takes the **food webs'** arm rather than the road's silence — a working the band
cuts but has not queued is dated at the back of the line, which is where a build ordered now would
actually go.

### The capture indexes carry the working's membership

`QueuedBuildSources` gains `deposits: HashSet<(UVec2, String)>`, keyed both ways because one tile can
hold two workings and a `fell` queued on the timber is not a `quarry` queued on the rock beneath it.
Its presence is the row's `isQueued` **and** the `queued_live` term of the build countdown — one
index answering both rather than two that could disagree.

> **It was a MAP from a working to a kit id**, whose value was this row's `buildKitId`, and
> `UpkeepKitIds` carried the same key for `upkeepKitId` / `upkeepKitNamed` — resolving the working's
> branch off the **source's own rung** rather than off the row, because one row kind serves both
> ladders. `docs/plan_pool_toe.md` §4 retires both ids: a site's tools follow from its rung, are
> settled band-wide by priority, and are published per pool as `PopulationCohortState.poolToe`. The
> values and the wire fields went, and only the membership stayed. **A worked-deposit set went with them** — the deposit row gates its build scratch on the
> queue alone, so `WorkedSources` indexes the two food webs and nothing else.

### The rung catalog — what a wood or a rock body MAY become, once per world

`SubsistenceSection.depositRungs` is a `DepositRungState` per rung of the **forestry** and
**extraction** branches, grouped by branch and climbing within it, built by
`snapshot::subsistence::snapshot_deposit_rungs` off `extraction::deposit_rungs_in_climb_order`. It is
a per-world constant on `routeRungs`' own seam — a `Whole<…>` baseline, diffed whole like `kits`, and
re-sent only on a world rebuild.

**`DepositState` says where a working stands; the catalog says what stands above it.** Without it no
readout could state what a quarry costs, what it reaches, or why the ground refuses one until a
working already sat on that rung — which is the same gap `routeRungs` was added to close for roads.

⛔ **IT FOLLOWS THE ROUTE PRECEDENT AND NOT THE PLANT ONE, AND THE DIFFERENCE IS WHO OWNS THE LIST.**
The plant and animal ladders are drawn from **hardcoded client-side rung arrays**, a second authority
that goes stale the day a rung is added. Every field here is derived from
`intensification_ladder.json` through the sim's own rung types — the prices are the record's `build` /
`upkeep`, the payoff is its `extraction_payoff`, the placement rule is its `site_requirement`, the
chain is its `requires_rung` — so a rung added to that config reaches the wire and the client's ladder
with no Rust edit and no client edit. That is not hypothetical on this branch: `extraction:gathering`'s
own config comment reserves the minerals arc's `mine` above the quarry, on this same ladder.
`wire::a_rung_added_to_the_config_is_published_with_no_code_change` appends a fourth forestry rung to
the shipped config's JSON, swaps the `LadderConfigHandle`, and asserts the row appears.

**`branch` on every row is the one field `RouteRungState` has no need of.** One vector carries both
ladders because they share the payoff block, so a reader groups by it;
`extraction::DEPOSIT_BRANCHES` fixes the order the groups come in, which a set-walk could not.

⛔ **`recoveryFraction` IS PUBLISHED AND MUST NEVER BE DERIVED FROM `reachable / capacity`.**
`deposit_reachable` clamps to the **stock**, so that ratio stops being the rung's recovery the moment
a seam is drawn down: a payoff sub-row computing it would begin quoting a number that *falls as the
rock is worked*, on a rung whose reach never moved. It is the field the whole stone branch turns on —
0.15 at the surface, 0.85 at the quarry — and it rides the catalog for that reason.

**`minDepositCapacity` is on the row so a client can say WHY a rung is refused**, not merely that it
is: the `capacity` a working already publishes is the other half of that sentence, and a threshold
transcribed client-side would be a second authority over a placement rule the config owns.

**`buildWorkPerWorkerTurn` is the one field that is the SIM's and not the rung's** — the bare
`PER_WORKER_OUTPUT`, read through `intensification::build_work_per_worker_turn` at `NO_BUILD_GEAR`,
because worker output is written as a *sum of terms* and a transcribed constant goes stale in silence
the day a second term lands. It is the same figure for every rung, which is why it rides the catalog
rather than the working's row.

## See also

- `docs/plan_extraction.md` — the arc: the gap, the one idea, the two branches, what is out of scope
- `.claude/rules/core_sim/intensification.md` — the ladder engine both branches sit on
- `.claude/rules/core_sim/routes.md` — the third branch, whose seams this one rides and whose
  camp-free improvement it deliberately does not copy
- `.claude/rules/core_sim/crafting.md` — a material is generic with characteristic axes; the batch
  merge every arrival here goes through
- `.claude/rules/core_sim/equipment.md` — the build axis, the rung bound, and the kit roster
