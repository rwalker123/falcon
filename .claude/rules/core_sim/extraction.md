---
paths:
  - "core_sim/src/extraction.rs"
  - "core_sim/src/extraction_config.rs"
  - "core_sim/src/data/extraction.json"
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

## The take

```text
floor      = (1 − recovery_fraction(position)) × capacity
reachable  = max(0, stock − floor)
take       = min(workers × yield_per_worker_turn(position), reachable)
stock     -= take
stock     += regrowth(stock, capacity, regrowth_rate(terrain) × regrowth_multiplier(position))
```

- **OVER-CUTTING IS POSSIBLE AND MUST STAY SO.** The take is not clamped to the sustainable rate —
  the whole point of the renewable half is that you can ruin a wood. The warning is a readout, not a
  guard.
- **These sources pay NO FOOD AND NO FODDER.** Not a zero-valued food term; no food term at all, and
  `seed_source_yield` returns rather than seeding a permanent `+0.00` line. What wood costs is **the
  food those hands did not bring home**: every hand on an `extract` row comes out of the same finite
  pool a Forage row spends.
- **The arrival rides the existing seam** — `LocalStore::deposit_material`, with the *ground's*
  characteristics, so a streambed pays knappable flint and a quarry pays building block out of one
  generic material, and the batch merge is the one every other arrival uses. It is reported through
  the row's `SourceYield::materials`, which is the producer the band's income map and the material
  shortfall Alert both read.
- **Renewal happens after the take**, unlike the food webs (which regrow in Logistics and gather in
  Population). A deposit has no ecology phase and no forecast riding a pre-regrowth reading, so there
  is nothing for the split to serve.

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

`extraction:quarry` sets it at **800**, which sits in the gap the deposits table deliberately leaves
between its two populations: the smallest rock body is rolling hills at 900 and the largest
loose-stone scatter is a periglacial 70. So the split is a **capacity reading** rather than a list of
terrains anyone maintains, and the minerals arc's placed ore bodies fall on the right side for free.

**Neither free floor carries a `site_requirement`, and that is not an omission.** A floor of ~0
admits every tile, which `validate_site_requirement` rejects outright as a placement rule that is
none. What refuses a crew on bare ground is that the terrain holds no deposit at all.

## The floor rungs must be workable BARE-HANDED

A felling kit wants a haft, a haft is wood, and wood comes from felling; each of the two shipped
quarry tools costs 3 wood. **So if either floor rung needed gear the material economy could never
start** — and a band spawns owning nothing (`equipment.json`'s `start_stock_fraction: 0.0`). It is
`materials.json`'s own argument for `hand_working`: a bare-handed rate refuses nothing, where a zero
would be a refusal branch the sim does not have.

**There is no kit term in the take at all.** The shipped roster declares no *take* gear on either
branch — forestry deliberately (§9: its natural tool is an axe, and a bone-hafted axe while stone
tools are out of scope is a roster question left open), extraction because its two tools declare
`build_work`, which lands on the pool that *raises* a working. `KitJob::Extraction` exists as the
seam the day a felling axe declares a take stat, with `default_kits.extract: "none"` — the same
opening `roadwork` has.

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
  with no kit is legal only where **nothing at all** serves its branch. `forestry` ships wholly
  kitless on purpose; the failure the test exists to catch is a **partial** roster, one rung's kit
  gone missing while its siblings keep theirs, after which every build there silently falls back to
  `none` for the rest of the game.

## The working belongs to a CAMP, like a patch — not to nobody, like a road

This is the one place the arc deliberately does **not** copy `RungBranch::Route`. A road follows no
one and is free to leave, which is why it belongs to no camp; **a quarry you walk away from is a
quarry you lost**. So `BuildSource::Deposit` *is* backed by a labor row (`LaborTarget::Extract`),
`holds_build_source` resolves it through that row like a patch's, and the row **lapses when the
deposit resolves out of the band's work range** — the Forage arm's abandonment rule, byte for byte,
and where this branch's move-or-stay pressure actually lives.

**A working raised above its free floor keeps its row through an unstaffing**
(`source_has_a_meter_at_risk`), with one difference from the two food webs: there is no rot to be at
risk *from*, so what the row protects is the position itself. A working still on its free floor is a
wild stand and its row goes.

## What a working costs to HOLD — the `quarrywork` pool

Every **built** rung on both branches owes work per turn, drawn from `LaborTarget::Quarrywork` — the
fourth keeping pool, and `Roadwork`'s twin two branches over. Without it a working's position never
falls and **a quarry is free to hold for ever**, which contradicts the arc this one sits on: an
improvement that costs nothing to hold cannot weigh on move-or-stay.

**The two FREE FLOORS owe nothing**, and that is what makes them free: `forestry:deadfall` and
`extraction:gathering` declare no `upkeep` at all, exactly as `plant:wild` and `route:path` do.
Nobody built them, so there is nothing to hold.

### ONE role for BOTH branches

The two food webs get a keeping pool each because they are separate *ladders a crew builds with
tools*. Forestry and extraction split on **knowledge** and on nothing a keeper does — the roster
declares no gear for either, and *hold the face open, clear what has fallen* is one job. A second
pool would be a distinction nothing in the game can express, which is the argument
`plan_standing_upkeep.md` §6 already makes for not splitting the two it has.

**It is not the `extract` take row.** The take crew stands *on the working* and is paid in material;
the keepers are a **band pool** that holds every working the band has, worked or idle — the same
split `Agriculture` draws from `Forage`. A working with no cutters is still held and still owes.

`KitJob::Quarrywork` is split from `KitJob::Extraction` even though both ship bare, on
`KitJob::Agriculture`'s stated reason: **gear covers people**, so sharing a job with the take row
would divide whatever a future felling axe arms among hands that are not cutting. The split is free
to make while both defaults are the empty `none` kit and would be a migration afterwards.

### The claims are the ROUTE shape, and the reason is the index

`keeping_claims` sets `KeepingClaim::index` to an **assignment index**, because the plant and animal
shares are written straight back into `maintenance_shares`' per-assignment award vector. A working's
share is not: it lands on the **working**, in the `DepositRegistry`, exactly as a road's lands on the
road — so `extraction_keeping_claims` indexes its own `(tile, material)` key vector, which is
`route_keeping_claims`' arrangement. Everything downstream (`keeping_rates`,
`KeepingRate::worker_need`, `distribute_upkeep_pool`) is the identical seam either way: **a working's
keeper is funded exactly as a road, a field or a flock keeper is.**

**The catchment is the ROW**, not a keeper — a working is held by a labor row, which is the arc's one
deliberate departure from the route branch — and **the take crew's size is not part of it**: a source
row survives losing its take crew, and a felling working nobody is cutting this season is still a
face somebody has to hold. That is `Agriculture`'s *"keeping a patch does not require gathering it"*
on a fourth pool.

> #### ⛔ `KeepingClaim` GAINED A `branch`, AND THE COVERAGE IS WHY
>
> `keeping_rates` used to take one `branch` for a whole call. **One pool can now hold sites on two
> ladders** — a band may keep a coppice and a quarry — so a single argument would have to lie about
> half of them. Calling it once per branch is *not* the alternative: coverage answers *"how many of
> these hands does the band own gear for"*, a fact about the **ledger**, so two calls would arm two
> prefixes off one stock. That is `keeping_rates`' own *"the rung is not part of the key"* note, one
> axis over. The branch rides the claim; the parameter is gone from `keeping_rates` and
> `keeping_worker_need`, and every construction site states its own.

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

`extraction::advance_deposits` (Logistics) is the deposit branches' `routes::advance_roads`, in the
same three phases: how short → the bleed at the at-risk rung's own rate past its own grace → clear
the payment and **re-stamp the bill at the post-decay position**.

**The slide shrinks its own penalty.** The position falls, the interpolated demand falls with it, and
an abandoned working decays toward costing nothing rather than bleeding a band's roster for ever
(§2.7).

⛔ **IT RUNS ON EVERY WORKING, HELD OR NOT** — `bill_and_stock_roads`' lesson. A pass that billed only
the workings some band still has a row on would leave an **abandoned** working reading as kept for
ever: never arming its counter, never decaying. A working whose band walked out of range is precisely
what this branch's move-or-stay pressure is made of, so it is precisely the case that must decay.

**The payment is a whole stage later** — `systems::settle_bands_extraction`, called from inside
`advance_labor_allocation` at `settle_bands_roadwork`'s own seat: **after the shed** (the head count
it divides is the one that survived) and **above the band's `continue`s** (a band whose whole
allocation was shed still owes what its workings cost). Paying any later is the defect
`settle_bands_roadwork`'s note records — every billed road quoting its rot at a work shortfall of
`1.0` whatever its keepers had done.

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
it: `felling` is the first rung on either branch at which over-cutting is possible.

**A deposit crew learns at the plain rate.** `learn_multiplier` prices *calories given up against
lessons gained* and is the player's own escapement dial on a food-web row; a deposit pays no calories
and its floor is the **rung's**, not the row's, so there is nothing to trade.
`intensification::PRACTICE_AT_THE_PLAIN_RATE` is the named fixed point the earn site passes.

## Config files

| File | Purpose |
|---|---|
| `src/data/extraction.json` | **THE DEPOSITS** (`extraction_config.rs`, env override **`EXTRACTION_CONFIG_PATH`**). `seed_fraction` **0.02** — what a deposit regrows from when it has been taken to nothing, evaluated *inside* the growth term so a rate of zero seeds nothing. Then one `deposits` row per material: its `branch`, and a `by_terrain` table of `{ capacity, regrowth_rate, characteristics }`. **A terrain absent from `by_terrain` holds none of that material** — absence is the answer, so there is no `enabled` flag and no parked `0.0` row, and every water terrain is absent from both tables deliberately. `DepositDef` and `DepositTerrain` are **`deny_unknown_fields`**, so the file's prose lives at file level in `_comment_*` keys, `materials.json`'s discipline. **Stone is two populations in one table**: rock bodies in the low thousands at rate `0.0` (alpine 4200, karst 3000, basalt 2600 … rolling hills 900) and loose-stone scatters in the tens at a small positive rate (periglacial 70 … mangrove 5). **Wood regrows on every row** — mixed woodland 600 at 0.03, boreal taiga 450 at 0.015, marsh withy 90 at 0.055 — because a forest that is worked out is not a forest. **The characteristic ratings are the provisional half of the file**: nothing reads either material's axes today (`docs/plan_extraction.md` §5b — a *recipe* will, when stone tools land), so they are authored for the shape the pair is meant to have — genuinely opposed, no best deposit — rather than tuned against a consumer that does not exist. **`capacity_per_keeper`** is the divisor that turns a tile's capacity into the keeper-loads the rungs quote their `work_per_turn` per — wood **600** (`MixedWoodland`'s own capacity, so a felling working on closed woodland is exactly one load) and stone **3000** (`KarstHighland`'s — limestone, the classic quarry stone, and the middle of the rock bodies, so rolling hills reads 0.3 and an alpine mountain 1.4). Every number is a **playtest dial** |
| `src/data/intensification_ladder.json` | The five new rung records and the `extraction_payoff` block on each — see `intensification.md` for the ladder engine. `knowledge.lesson_costs` gains `woodcraft` / `conservationism` / `quarrying` at 20 apiece. **The three BUILT rungs each declare an `upkeep`** (`scaled_by: source_load`): `forestry:felling` **1.0** work a turn per keeper-load, rot **0.6**, grace **3**; `forestry:coppice` **2.0** / **1.5** / **2**; `extraction:quarry` **1.5** / **2.5** / **4**. The rates read as *keepers on the reference ground*, because `capacity_per_keeper` is anchored there — a felling working on closed mixed woodland is exactly one keeper, against the plant web's 2.0 for a tended patch on *its* reference tile. Each `meter_decay` is the pacing-neutral inversion of the plant web's rule of thumb (a wholly unmaintained rung lapses over ~100 bleeding turns), so it tracks each rung's own `work_cost`. The graces say how forgiving each rung is of a crew re-tasked for a season: a quarry face is the most forgiving at 4 because the rock does the holding, and a **coppice** the least at 2 because a managed wood is the most perishable thing on either branch — the same direction `plant:field` runs in against `plant:tended`. **The two free floors declare none.** |
| `src/data/equipment.json` | `default_kits.extract` and `default_kits.quarrywork` both `"none"`, the `none` kit's `jobs` gains both, and `stone_dressing`'s flint tier gains its second `build_work` effect on `extraction:quarry` |

## Wire and client — what is NOT here

**Nothing about a deposit reaches the client yet.** There is no `DepositState` row, the `extract`
labor row publishes its **tile but not its material** (a wooded highland holds two workings and the
wire cannot yet tell them apart), and neither the build countdown nor the build kit lands anywhere.
`plan_extraction.md` §7 owns the readouts — the sustainable-versus-actual over-cut warning on a
renewable deposit and `turns_remaining = reachable / take rate` on a finite one, chosen by
`regrowth_rate > 0` rather than by branch — and they are a schema change.

**The keeping ledger is in the same position.** `LaborAllocation::last_quarrywork_demand` and its
supplied twin are summed per band exactly as the roadwork pair is, and for the same reason the sim
does the summing — but `roadwork_demand` has a `PopulationCohortState` field and this pair does not,
so a Work board cannot yet show a band the bill it is failing to pay. That is the field the
`quarrywork` role wants most.

## See also

- `docs/plan_extraction.md` — the arc: the gap, the one idea, the two branches, what is out of scope
- `.claude/rules/core_sim/intensification.md` — the ladder engine both branches sit on
- `.claude/rules/core_sim/routes.md` — the third branch, whose seams this one rides and whose
  camp-free improvement it deliberately does not copy
- `.claude/rules/core_sim/crafting.md` — a material is generic with characteristic axes; the batch
  merge every arrival here goes through
- `.claude/rules/core_sim/equipment.md` — the build axis, the rung bound, and the kit roster
