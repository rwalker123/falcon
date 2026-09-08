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

## What a working costs to HOLD: nothing, today

Neither branch declares an `upkeep`, so no rung has a `meter_decay`, no deposit claims a share of any
keeping pool, and a working's position only ever goes up. `route:trail` already ships built and
un-held on the same reasoning — *a rung that costs nothing to hold cannot be short, so what takes it
back is disuse rather than shortfall*. **The consequence worth knowing** is that a band which comes
back to a quarry it left finds the working it paid for; what it lost is every turn's production in
between.

**The quarry's wood is on the BUILD PILE and not on a rate** — 8 wood of props, ramps and sleds,
drawn as the meter climbs exactly as the paved road's 20 stone is. `plan_standing_upkeep.md` §2.7's
own distinction: a pile is spent once as the face is opened, a rate is owed every turn the rung
stands. It also puts the two branches in the right order for a faction with nothing: you must have
worked a wood before you can open a quarry.

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
| `src/data/extraction.json` | **THE DEPOSITS** (`extraction_config.rs`, env override **`EXTRACTION_CONFIG_PATH`**). `seed_fraction` **0.02** — what a deposit regrows from when it has been taken to nothing, evaluated *inside* the growth term so a rate of zero seeds nothing. Then one `deposits` row per material: its `branch`, and a `by_terrain` table of `{ capacity, regrowth_rate, characteristics }`. **A terrain absent from `by_terrain` holds none of that material** — absence is the answer, so there is no `enabled` flag and no parked `0.0` row, and every water terrain is absent from both tables deliberately. `DepositDef` and `DepositTerrain` are **`deny_unknown_fields`**, so the file's prose lives at file level in `_comment_*` keys, `materials.json`'s discipline. **Stone is two populations in one table**: rock bodies in the low thousands at rate `0.0` (alpine 4200, karst 3000, basalt 2600 … rolling hills 900) and loose-stone scatters in the tens at a small positive rate (periglacial 70 … mangrove 5). **Wood regrows on every row** — mixed woodland 600 at 0.03, boreal taiga 450 at 0.015, marsh withy 90 at 0.055 — because a forest that is worked out is not a forest. **The characteristic ratings are the provisional half of the file**: nothing reads either material's axes today (`docs/plan_extraction.md` §5b — a *recipe* will, when stone tools land), so they are authored for the shape the pair is meant to have — genuinely opposed, no best deposit — rather than tuned against a consumer that does not exist. Every number is a **playtest dial** |
| `src/data/intensification_ladder.json` | The five new rung records and the `extraction_payoff` block on each — see `intensification.md` for the ladder engine. `knowledge.lesson_costs` gains `woodcraft` / `conservationism` / `quarrying` at 20 apiece |
| `src/data/equipment.json` | `default_kits.extract: "none"`, the `none` kit's `jobs` gains `extract`, and `stone_dressing`'s flint tier gains its second `build_work` effect on `extraction:quarry` |

## Wire and client — what is NOT here

**Nothing about a deposit reaches the client yet.** There is no `DepositState` row, the `extract`
labor row publishes its **tile but not its material** (a wooded highland holds two workings and the
wire cannot yet tell them apart), and neither the build countdown nor the build kit lands anywhere.
`plan_extraction.md` §7 owns the readouts — the sustainable-versus-actual over-cut warning on a
renewable deposit and `turns_remaining = reachable / take rate` on a finite one, chosen by
`regrowth_rate > 0` rather than by branch — and they are a schema change.

## See also

- `docs/plan_extraction.md` — the arc: the gap, the one idea, the two branches, what is out of scope
- `.claude/rules/core_sim/intensification.md` — the ladder engine both branches sit on
- `.claude/rules/core_sim/routes.md` — the third branch, whose seams this one rides and whose
  camp-free improvement it deliberately does not copy
- `.claude/rules/core_sim/crafting.md` — a material is generic with characteristic axes; the batch
  merge every arrival here goes through
- `.claude/rules/core_sim/equipment.md` — the build axis, the rung bound, and the kit roster
