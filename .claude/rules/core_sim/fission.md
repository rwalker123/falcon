---
paths:
  - "core_sim/src/systems/fission.rs"
  - "core_sim/tests/band_fission.rs"
---

# Band fission — a band splits in two where it stands

Design of record: `docs/plan_band_fission.md` (arc #508, this slice #511). The verb is
`split_band <faction> <band_id> <workers>`: a resident band divides on the tile it is standing on,
and the player moves the new one with the ordinary move order. Both halves are `ResidentBand` from
the moment the command resolves — they eat, age, forage, birth and starve on the same systems as
every other band, on their first turn.

## It is NOT an expedition that settles, and that is the load-bearing decision

An earlier design built this as an arrival verb: compose a scouting party, walk it somewhere, and let
it stop being an expedition on arrival. That shipped as `settle_expedition` (#510) and is **retired**.

**A scouting party is composed for scouting** — a there-and-back food budget, no dependants, a target
chosen for what it might *find*. Letting that party become a band means founding one from inputs
nobody chose for the purpose: the player never decided how big it should be or what it should carry,
because at the moment those were fixed they were answering a different question.

Deleted with that framing, and none of it should come back without reopening the decision: the
**reachability gate** and its BFS over the faction map (`founding_site_is_reachable`), the per-turn
`assess_foundings` sweep, `FoundingRefusal`/`FoundingRefusals`, `found_band_from_expedition`, and the
wire field `foundingRefusals`. There is now no gap between deciding and doing, so **every rule fires
once, at the split** — there is no *forecast* refusal distinct from a *blocking* one, and nothing is
re-checked on arrival.

What survived and was repointed rather than rebuilt: the runtime band-creation machinery (allocate a
`BandId`, insert `ResidentBand` + `DemographicFlowAccumulator`), the culture attach, and the
checkpoint path that re-attaches `ResidentBand` for a band worldgen never made.

## The worker count decides the SHARE — and the share is a starting point, not the answer

The player names a **worker count**. `share = asked ÷ parent.working`, applied to children, elders and
**every** store the band holds. The new band is a smaller copy of the one it came from, not a party
with a composition of its own.

> **The "one number decides everything" framing is RETIRED for the band's KIT.** It described the whole
> verb while a splinter's gear was a copy of the parent's, so there was nothing else to say. Since the
> per-band loadout arc a split also **opens an outfitting window on the new band**, standing at the
> proportional take it was just handed and revisable for the rest of the turn — so the player picks a
> worker count *and* a take. The share still divides the people, the larder and the material by itself,
> and it is still the only number a split needs; what changed is that the gear half is now a decision
> the player can revise rather than a consequence they only receive. See
> `.claude/rules/core_sim/starting-loadout.md`, and `docs/plan_band_fission.md` §Q3/§Q4.

**This is the model, not a simplification of it.** Per-bracket allocation lets a band that cannot feed
itself shed the people who cannot feed it — split off the elders, keep the workers, and the parent's
demand falls ~14% (or ~35% with the children) while its workforce is untouched. A proportional split
takes the same share of mouths as of hands, so the parent's `(children + elders) / working` comes out
**exactly** where it went in and that move does not exist.

**The exploit is closed by the shape of the model, not by a lever guarding against it** — which is why
no dependency-ratio ceiling appears anywhere in this arc. `band_fission::the_parents_dependency_ratio_does_not_move`
is the guard on that claim: if per-bracket allocation ever returns, it fails, and the deleted gate has
to come back with it.

**The share divides by the cohort's whole `working`, not by the assignable count.** The floors are
counted in `available_workers` (what the player is choosing from), but a cohort of 16.54 workers has a
fractional remainder that eats; dividing by the rounded figure would hand the new band a slightly
different slice of children than of workers.

## The exact fraction moves; whole bodies are a DISPLAY concern

`3.27` children is an ordinary `Scalar` and transferring it exactly is what keeps the two cohorts
conserved with no rounding rule and no leftover to reconcile. **The parent is debited by subtraction,
never by a second multiply**, so the halves sum to what the band held however the fixed point rounds.

The client renders whole bodies through `HudFormat.apportion_people_to`, and both halves are apportioned
in **one pass** — separate passes let both round the same way and show 31 people leaving a band of 30.
See `.claude/rules/client/band-city-panel.md`.

## Two floors, both counted in workers, both reported together

`split_refusals(asked, parent_workers, settle)` is the one rule set; the command and any forecast run
it rather than agreeing by habit.

- **`settle.min_founding_workers`** (4) — the **new** band's floor. Below it there is nobody to staff a
  single food role and it starves where it stands.
- **`settle.parent_min_workers`** (6) — the **parent's**. The guard against hollowing out the home band
  to crew a second one and killing both.

**Every applicable reason, never the first one.** A split that is both too small and leaves the parent
short has two things to fix; reporting one at a time teaches the rules one refusal at a time.

**A structural refusal stands ALONE.** `EmptySplit` and `NotEnoughWorkers` return early, because every
floor below them is a statement about a band that would exist — they would all fire at once and say
the same thing five ways.

**`parent_min_workers` is deliberately not bounded below.** `0` there is a real policy ("the parent may
give everything") and the way a playtest turns the floor off. Its sibling has no such reading: a band
of nobody is not a band, so `min_founding_workers ≥ 1` is validated.

## The dowry, and the two lines that are not in it

- **A proportional share of the larder** — not a reserve calculation and not a new number. The new band
  starts stocked because its people were already sitting on that food.
> ⛔ **THE TWO GOODS BULLETS BELOW ARE THE *TAKE* ARM — a split whose parent still holds an unspent
> grant moves NEITHER.** It partitions the grant instead, and moving goods on top of that charged the
> parent twice (`.claude/rules/core_sim/starting-loadout.md` → "A grant split pays ONCE"). The people,
> the larder and the culture below are unconditional.

- **A proportional share of the KIT, DENOMINATED IN KITS, moved and inherited WORN.**
  `fission::default_take_kits` resolves a kit allocation and `expand_kits` turns it into the item list
  that crosses, taken with `BandEquipment::take_units` and debited from the parent by exactly what
  left. A `BandEquipment::default()` would mint a fresh kit out of nothing on every split,
  permanently, and defeat the pull into the crafting economy that running your kit dry is supposed to
  be — so the splinter's units carry the parent's real condition. **The freshest units go**: see the
  callout below.
- **A proportional share of the MATERIAL BATCHES**, `floor(share × material_total)` **whole units**
  per material, moved with `LocalStore::take_material_batches` — the store's-own-order twin of
  `take_material`, because a split names no axis and each draw has to carry its source batch's exact
  reading across.
- **Grievance is inherited, not zeroed.** These are the same people who were unhappy a moment ago, and
  a split that reset it would make forming a band a way to launder discontent — the same class of move
  the proportional share exists to close.
- **Knowledge, the map and the culture** are copied whole. Culture is attached at the split from the
  **parent** (`CultureManager::attach_band_from_source`) rather than left to
  `reconcile_band_culture_layers`, whose no-layer case seeds from the province — identical today,
  since both halves are co-located, and wrong the moment the new band moves before the reconcile runs.

> ### ⛔ THE KIT WAS DUPLICATED, AND THE MATERIALS WERE NOT DIVIDED AT ALL
>
> Two defects lived in this section's own description of itself, and both shipped:
>
> - **The kit was CLONED.** `split_band_from_parent` copied the whole `BandEquipment` onto the splinter
>   and never debited the parent, so every split minted a second full kit. That was documented here as
>   intended — *"a copy of the parent's wear ledger"* — which is why it survived: the ledger really was
>   inherited worn, which is the property the sentence was defending, and the duplication rode along
>   underneath it. A splinter is exactly as worn out as the people it came from **and** it is a share of
>   what they held; the first claim alone is satisfied by a copy.
> - **Materials never crossed.** This section claimed *"the child inherits a share of every good"*, and
>   it was never true of materials: the child's store was rebuilt from `LocalStore::new()` plus the
>   parent's `iter()`, which walks the **commodity** account only. A splinter of a band sitting on
>   twenty hides opened with none of them and the parent kept the lot.
>
> The guard is `band_fission::gear_is_conserved_across_a_split` and its material twin, both asserting on
> the **pair** — a test that only looked at the new band passes against a clone.

> ### ⛔ THE FRESHEST UNITS LEAVE, AND THAT IS THE OPPOSITE OF THE WEAR ORDER
>
> `BandEquipment::wear_item` spends the **most worn** batch first: a band uses up the thing nearest the
> end of its life before opening a fresh one. `take_units` inverts it — **a new venture is outfitted
> properly**, so the splinter walks out with the best gear the band has and the parent keeps the worn
> stock. Ascending `wear`, ties broken by earliest insertion index, and the last batch **split** when
> the count falls inside it (the units that leave carry that batch's `tier`, `grade` and `wear`
> verbatim).
>
> They are two different questions — *which unit is being used* and *which unit is being handed over* —
> so they get two different answers rather than one shared "order" that serves neither.

> ### ⛔ THE MANIFEST IS DENOMINATED IN KITS, AND THAT IS WHAT THE CARD CAN SHOW
>
> The default take was a bare per-item `floor(share × count_of(item))`. It moved the right gear and it
> was **unpublishable**: the outfitting picker is kit-denominated (the player composes kits, never bare
> items), so no allocation could express it, the splinter's window opened with **empty** accepted rows,
> and the auto-popped card showed nothing while the band stood on its dowry. An apply is a
> *replacement*, so an untouched *"Set out"* ordered **take nothing** and handed the whole share
> straight back to the parent.
>
> **The card is not special-cased; the take is.** An empty tail stays a real order — *take nothing*,
> exactly as it is for a grant — and what changed is that the card is no longer empty when the take is
> not. `fission::default_take_kits` resolves the share into kits and the window publishes it, so
> re-sending it unchanged is an exact no-op
> (`split_loadout::re_sending_the_published_allocation_untouched_changes_nothing`).
>
> **The rule is proportional, floored, remainder unspent** — `clamped_kit_defaults`' rule, applied to a
> different budget. Two clamps, because `sled` is used by **both** `big_game` and `trapping` and so no
> kit's count can be resolved on its own: each kit's own ceiling is the minimum share across the items
> it uses, and where two kits' combined demand for an item exceeds that item's share they are both
> scaled by `budget ÷ demand` and floored. Not first-come, for the reason `clamped_kit_defaults` states:
> the roster has no author's order to consume in, so "declaration order" is really *id* order and makes
> `big_game` beat `trapping` because `b` sorts first.
>
> **The material half is floored to WHOLE units** for the same publishability reason — a card states
> `units:u32`, so a fractional take could not be shown and re-sending what it showed would hand the
> remainder back. The remainder stays with the parent, where the player can take it deliberately.

> ### ⛔ A BENCH TOOL DOES NOT WALK OUT WITH A SPLINTER
>
> `bone_awl`, `loom` and `tanning_frame` are the only three items **no kit `uses`**
> (`EquipmentConfig::item_is_kit_carried`; every other item is reachable — `warrior` grants `clubs`,
> `tillage` grants `hoes`). They are the knowledge-gated bench tools, and because the take is composed
> of kits they can never appear in one.
>
> **That is a design statement, not a limitation to apologise for.** They are shop equipment, not a
> pair of hands' gear, so they **stay with the workshop that built them**. The alternative is an item
> that moves invisibly with a band that cannot see it on the card, adjust it, or choose to keep it —
> and the fix for *that* would be widening the grammar to bare items, which is exactly the
> kit-denominated design being refused.
>
> They are also kept off the published cap (`parentItemSupply`), so no client is told they are
> claimable. Pinned by `split_loadout::a_bench_tool_does_not_walk_out_with_a_splinter`, which asserts
> the roster's un-kitted set **is** those three — a fourth means the rule needs restating rather than
> the list extending.

> ### The manifest divides on the RATIO, never on the rounded share
>
> `share` is a fixed-point quotient, so a third stores as `0.333333` and `0.333333 × 3` floors to **0**:
> a band splitting three ways would hand its splinter none of the three spears the player plainly asked
> for a third of. `fission::whole_share` computes `floor(held × asked ÷ workers)` from the ratio
> instead. The continuous halves — children, elders, the larder, the material batches — multiply by the
> share as before, because nothing there is quantised and there is no floor to fall through.
### The dowry is a transfer, and it is booked as one

The share of the larder that walks out with the new band is **food that crossed between two larders**,
so it is booked into the food ledger's transfer terms: a debit on the parent's
`last_food_transfers`, a credit on the child's, the same ledger `balance_supply_networks` and a trade
shipment write (`.claude/rules/core_sim/campaign.md` → the transfer callout, which owns the identity).

**The dowry takes the `TransferLink::Local` arm.** A splinter is camped where its parent is and
nothing carried the food anywhere — the same *standing together* crossing pooling is, and not the
`route` arm a party's pack takes.

**A split is a command, so it lands *between* two captures** — inside the interval a client's
`larder_delta` measures. Without the booking the parent publishes a frame whose Food line is short by
exactly the provisions and the child's opens at food it never grew, and the identity
`larder_delta == foodIncome − foodConsumption − raidForfeit + transferReceived −
transferSent` is simply false on the turn a band splits. The child receives it on the
`LaborAllocation` it is spawned with, because its first published frame is the one that has to
account for it.

**FOOD only.** The child takes a share of the material batches too (above), but materials deliberately
have no identity of their own — a material's account is the batch store itself.

Pinned by `transfer_food_ledger::the_food_ledger_reconciles_when_a_band_splits_mid_window`, which
splits after a published frame and asserts the identity on **both** halves.

**There is deliberately NO breeding stock**, and the reason is structural rather than a balance call:
a band's stores hold `provisions`, `fodder` and material batches and nothing else; a corralled `Herd` carries
`corralled_at` and a `pen_radius`, so it is fenced land and neither it nor a fraction of it travels;
and `Herd::owner` is a **`FactionId`**, so both halves of a same-faction split already co-own every
pen. **What a band loses by walking away is reach, not title** — labor assignments lapse with
`reason=out_of_range` once a source leaves the band's work range.

## The wire carries the FLOORS, never the verdict

`PopulationCohortState.foundingMinWorkers` / `foundingParentMinWorkers`, echoed onto every cohort (the
`bandMoveTilesPerTurn` idiom). The compose sheet moves a stepper, so a published verdict would need
one field per possible composition; what crosses is the pair of thresholds the sim owns, exactly as
the per-source forecast publishes rates rather than an answer per party size. The client composes the
refusal sentences and does no gate of its own.

> **`foundingRefusals:[string]` was DELETED from `snapshot.fbs`, not deprecated in place** — a
> deliberate exception to the rule stated in `expeditions.md` ("the wire slots are deprecated in
> place, a FlatBuffers vtable slot is positional"). It is safe here for one reason and only one: this
> repo has **no shipped clients or saves**, and both halves are built from the same tree, so no reader
> can hold the old vtable. The general rule stands; a slot removed after a client ships is a silent
> mis-read, not a compile error. Cross-reference: the `no-back-compat-yet` position in the root
> `CLAUDE.md` lineage.

## The splinter opens its own outfitting window

A split ends by opening a `starting_loadout` window on the new band, **carrying its accepted
allocation** — the kit and material rows the card draws itself from — and it stays open until the turn
advances. What the split does to get there is a fact about the **parent's** state rather than about
the turn:

- the parent still **grants** → the split **partitions the grant** and moves nothing; the parent is
  re-fitted to its reduced budget and what that takes off becomes the splinter's allocation;
- the parent **does not** → the split **moves** the default take and publishes it.

Doing both is what charged the parent twice and let its next revision mint material out of nothing. The whole model, the caps table and the onward-take refusal live
in `.claude/rules/core_sim/starting-loadout.md`; what belongs here is that **the split is one of the
two things in the game that opens a window**, and that the proportional manifest above is that
window's *starting point* and not its verdict.

**`StartingUnit` still rides onto the splinter, and it must.** It is not merely the marker the
outfitting window used to select a band by: `bin/server.rs`'s `resolve_starting_unit_entity` gates
**every band-addressed order** on it, and `visibility_systems::VisionCohorts` requires it to make a
band a vision source at all. A splinter without it is a band the player cannot order and that sees
nothing.

## What the new band silently joins

**`ResidentBand` is a membership switch, not a label.** Supply pooling, sedentarization, migration,
demographics, startup seeding, herd drift and the default-band command pickers all query
`With<ResidentBand>`, so the new band joins every one of them on the turn it is formed — with
`age_turns` at 0, stores that are a fraction of its parent's, and a position **identical** to its
parent's.

- `age_turns = 0` is asserted at the split rather than inherited: `migration_min_settled_turns` reads
  it, and a band formed this turn carrying the parent's settled duration would bleed people out on its
  first.
- The derived per-turn readings (`last_morale_*`, `last_food_consumption`, `discontent_fraction`, the
  migration counters) are **cleared**. They are recomputed next turn, but a split publishes a frame
  before then, and the new band would open by narrating somebody else's morale swing and meal.
- **Two bands on one tile** is the normal post-split state until the player moves one. Per-hex
  crowding, supply pooling and the work-range overlap all see it.
- A band formed by a split **can split again** — the command gates the source on `ResidentBand` — so
  the parent floor has to hold for a band that was itself formed fifteen turns ago.

## Config files

| File | Purpose |
|------|---------|
| `src/data/expedition_config.json` → `settle` block | `min_founding_workers` (**4**) and `parent_min_workers` (**6**). The block lives in the expedition config for historical reasons — the arc was an expedition verb once — and the loader/validator rationale stays in `expeditions.md` → Config files. Both dials ship as `/settle/` rows on the Workbench tuning page (`.claude/rules/client/workbench.md`): a gate that cannot be moved during a playtest cannot be judged during one. |
