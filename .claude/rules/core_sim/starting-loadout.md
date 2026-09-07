---
paths:
  - "core_sim/src/starting_loadout.rs"
  - "core_sim/src/systems/fission.rs"
  - "core_sim/tests/starting_loadout.rs"
  - "core_sim/tests/split_loadout.rs"
---

# The outfitting window — one per BAND, open until the turn is finalized

A spawning band owns **nothing**: `equipment.json` ships `start_stock_fraction: 0.0`
(`.claude/rules/core_sim/equipment.md`) and no material declares a start stock
(`.claude/rules/core_sim/crafting.md`). What a band owns is what the **player** allocated through its
own outfitting window.

`starting_loadout.rs` holds all of it: the `StartingLoadout` resource, the two systems that open and
shut a window, and `apply_starting_loadout`, which validates, resolves the band and moves or mints.
`bin/server.rs`'s handler only translates the wire types and logs the refusal. The client half is
`.claude/rules/client/starting-loadout.md`; the split that opens a splinter's window is
`.claude/rules/core_sim/fission.md`.

## Every band gets a window, and turn one is NOT a special case

Two things open a window and nothing else does:

| opener | when | the window it opens |
|---|---|---|
| `stamp_starting_loadout` | Startup, chained after the spawn | the spawned band's, carrying the campaign's **grant** |
| `split_band_from_parent` | every split, every turn | the splinter's, **carrying its accepted allocation** — the default take it was moved, or the share of the grant re-fitted off the parent |

**Turn one is not special — only the PARENT's state differs.** On turn one the parent still holds an
unspent grant, so its splinter takes a slice of that grant, its picks **mint**, and the split moves no
gear at all. From turn two nobody holds a grant, so a splinter's window is a take on the parent, its
picks **move** gear out of the parent's own ledger, and the split moves the default take across. There is no `if turn == 1` anywhere in the arc, and there must not be: the
question the code asks is *"does this band's parent still have a grant"*, which is a fact about the
world rather than about the clock.

`StartingLoadout` is a `BTreeMap<BandId, LoadoutWindow>` and **checkpoint state** (`SimState`,
`SIM_STATE_RESOURCES`): nothing rebuilds it, so a rollback into a turn whose windows were open has to
land in a world whose windows are still open, budgets and standing takes intact.

> ### ⛔ A CLOSED WINDOW IS AN ABSENT ONE
>
> `close_opening_window` **removes** every row rather than clearing `open` on each. A closed window can
> never be revised, so keeping one would grow the checkpoint by a permanent entry per band ever split,
> to record a state an absent key already says. `Default` is *no windows*, which is what a world that
> never ran worldgen (a load, a bare test `App`) correctly reads as, and `StartingLoadout::is_open`
> answers `false` for a band it has never heard of.

## The two supplies — `LoadoutSupply`, and it decides EVERYTHING downstream

```
Grant  { kit_budget, material_budget }              → the picks MINT
Parent { parent, items, materials }                 → the picks MOVE
```

It is an enum rather than a pair of *"meaningful only when…"* fields because the two cases cap on
different currencies: a grant is bounded by two integers the world handed out, a take is bounded by
what another band is standing on right now.

### What a SPLIT gives the splinter

| the parent's window | what the split does | the splinter's window |
|---|---|---|
| still **grants** (`LoadoutWindow::grants()` — open, and a `Grant`) | **partitions the grant, and moves NOTHING physical** | a `Grant` of its own: `min(asked, the parent's remaining kit budget)` kit slots, and `floor(share × the parent's remaining material points)`. Both are deducted from the parent's, so no slot and no point is minted twice or lost. |
| does not (closed, or already a take) | **moves goods** — there is no grant left to partition | a `Parent` take: no budgets, and the cap is what the parent can supply — see below. |

Both caps are the numbers the split has just resolved (`asked`, `share`), not literals.

> #### ⛔ A GRANT SPLIT PAYS **ONCE** — IT PARTITIONS OR IT MOVES, NEVER BOTH
>
> A split used to walk the proportional manifest out of the parent's ledger **and** deduct the
> splinter's slots and points from the parent's budget. Those are two ways of paying for one
> splinter, and doing both charges the parent twice.
>
> **What a player saw** (reported from a live run): a band of 17 hands with 30 material points, the
> shipped pre-fill committed as `bone 3 / fibre 17 / hide 8` = 28 units, split 5 workers off on turn
> one. Its resources meter read **`-6 / 22 left`**. The kit meter escaped only because the 4/4/4
> pre-fill happened to sum to exactly the reduced budget; a player who had spent all 17 slots would
> have seen that go negative too.
>
> **The negative meter was the visible edge of a duplication bug.** The parent's standing allocation
> was never re-fitted, so it still claimed 28 units against a 22-point budget — and an apply is a
> *replacement built from empty*, so the parent's next revision **re-minted all 28** while the units
> that had walked stayed with the splinter. Material out of nothing, on every turn-one split.
>
> **On turn one the budget is the currency and a ledger is a draft against it**, which is why the
> grant arm moves nothing: the splinter mints from slots carved out of the parent's grant, and that
> *is* the payment. The people and the larder dowry are untouched by this — they are not loadout
> goods, and the food-ledger transfer booking is unchanged.

#### The re-fit: what the parent gives up reaches the splinter

`fission::rebalance_partitioned_grant` closes the loop, and it runs **only when the parent no longer
fits its reduced budget**:

1. the parent's standing allocation is re-fitted by `clamp_allocation`'s proportional-floored rule —
   the same rule the profile's kit pre-fill is fitted by;
2. the parent is **re-materialized from the clamped allocation** through `apply_starting_loadout`, the
   path a player's own commit takes, so its ledger, its store and its meter state one thing and the
   meter can never read negative;
3. **what the clamp took off is offered to the splinter**, bounded by the splinter's own budget by the
   same rule, and materialized the same way. Those units are not deleted — they are taken away from
   the main band and given to the new one, which is the model.

**A parent that still fits gives up nothing and the re-fit returns without touching either band.**
That is load-bearing rather than an optimisation: re-materializing rebuilds a ledger from *empty*, so
running it on a parent with no standing allocation would destroy gear that never came from one.

The invariant is over the **grant**, not over the ledgers: `held + unspent` across both bands comes
back to what the parent alone had. Pinned by
`split_loadout::a_grant_split_conserves_the_material_grant`, which also re-applies the parent's own
clamped allocation afterwards — the exact move the duplication rode in on.

### ⛔ THE WINDOW OPENS AT THE ALLOCATION, NOT AT ZERO

A window's `kits` / `materials` rows are the **accepted allocation** — what the last accepted order
described, and what a client's card draws itself from. On a splinter they were **empty**, because the
split's default take was a bare per-item manifest that no kit allocation could express. The card
therefore showed nothing while the band stood on its dowry, and since an apply is a *replacement*, an
untouched commit ordered **take nothing** and handed the whole share back to the parent.

The default take is now **denominated in kits** (`fission::default_take_kits`) and published, so
re-sending it unchanged is an exact no-op. `items` and `kits` are two readings of one move —
`items == expand_kits(kits)` by construction — which is what makes the round trip exact rather than
approximately right.

**A splinter of a still-granting parent gets its rows the other way round**: nothing was moved, so its
allocation is what the parent's re-fit shed, materialized on it by the same `apply_starting_loadout`
call. Either way the rows describe the band standing beside them, which is the whole property.

> **An empty tail is still a real order** — *take nothing* — on a take exactly as it is on a grant.
> Nothing special-cases the commit; the card is simply no longer empty when the take is not. A
> `set_starting_loadout` with no `kit` and no `material` lines means what it says.

**The material half is floored to whole units** for the same reason: a card states `units:u32`, so a
fractional take is one it cannot show and re-sending what it showed would hand the remainder back.

### A BENCH TOOL DOES NOT WALK OUT — the denomination is what enforces it

`bone_awl`, `loom` and `tanning_frame` are the only three items no kit `uses`
(`EquipmentConfig::item_is_kit_carried`), and they are the knowledge-gated bench tools. A take is
composed of kits, so they can never appear in one — **shop equipment stays with the workshop that
built it**, which is a design statement rather than a gap. They are also filtered out of the published
`parentItemSupply` cap, so no client is told they are claimable.

Reaching for `item <id> <n>` to move one is the thing this design refuses: the picker is
kit-denominated, and a bare-item grammar would make every take a second composition surface with a
second set of caps.

### The standing take is RECORDED, and that is what makes a revision exact

`LoadoutSupply::Parent` carries the expanded `items` (whole units) and `materials` (fixed point) this
band has taken off its parent. It is recorded rather than re-derived, because **a band's ledger is the
take minus whatever its own splinters have since taken off it** — so "reduce this band's take from 5
spears to 3" cannot be read off the ledger once the band has itself split. Before this arc only the
materialized items survived and the question was unanswerable.

## `apply_starting_loadout` — one composition against one supply

The command is `SetStartingLoadout` (proto field 69, text
`set_starting_loadout <faction_id> <band_id> [kit …]... [material …]...`), replayable like every other
world-mutating verb. **The band is positional and required**: every band has a window of its own, so
there is no "the faction's band" to default to.

> ### ⛔ AN ALLOCATION IS A REPLACEMENT, NOT A PURCHASE
>
> `apply_starting_loadout` never clears `open`; only `close_opening_window` does, on the turn advance,
> and unspent budget is forfeited there. The whole turn is a **working surface** — the player tries a
> pick, sees what it buys, and revises it — which is the entire reason the pick happens after worldgen
> rather than before it.
>
> So after applying `A`, the band holds exactly what `A` describes, however many drafts preceded it.
> `6 big_game` revised to `4 big_game` leaves **four** kits' worth of gear, not ten.

**A grant window rebuilds both halves from EMPTY** — the ledger from `BandEquipment::default()`, the
store through `LocalStore::clear_materials`. The material reset is account-aware and the *store* is
what makes it so: a band's `LocalStore` holds its opening food reserve beside its material batches, so
a wholesale `LocalStore::new()` would starve it, and the distinction lives in the store rather than in
a call site that would have to name `FOOD` and `FODDER` to spare them.

**A take window moves the DELTA**, in whichever direction each line points. `plan_take` resolves the
new standing take and the signed per-item and per-material deltas; `move_take` then applies them, and
nothing between the two can fail.

> ### The delta is the "return it all, then take the new order" the spec describes — without the transient
>
> Pricing a raise from 3 to 5 against the parent's holdings *alone* refuses it for the 3 that already
> moved, so the order is priced against **the parent's holdings plus the take already standing**. That
> is algebraically `new − old ≤ holdings`, which is exactly what moving only the delta does — and it
> never puts the world through an intermediate state a refusal would have to undo. **Everything is
> checked before anything is written, so a refusal leaves the world byte-identical**, and that
> invariant is what the delta form buys structurally rather than by discipline.
>
> **Both directions go through `BandEquipment::take_units`**, so the freshest units move whichever way
> they are going — one rule about which unit is being handed over, not a second one for the return leg.

### A kit ROW cannot be capped on its own

`equipment.json` maps kits to items almost one-to-one, and the single exception is **`sled`, used by
both `big_game` and `trapping`**. It is why `default_take_kits` needs its second clamp too: a kit's
share cannot be resolved independently, so where two kits' combined demand for an item exceeds that
item's share both are scaled by `budget ÷ demand` and floored — proportional, remainder unspent, on
`clamped_kit_defaults`' stated reasoning about arbitrary first-come winners. So the thing that has to fit is the **expanded item list**, whole:
`expand_kits` sums the order into `item → units` and every cap is checked against that. A client
drawing a take's cap must expand the same way, or it will draw a cap the server does not refuse on.

Within one allocation two kits that share an item still **add**: 3 `big_game` + 3 `trapping` is 3
spears, 3 traps and **6** sleds. Across allocations they do not — the later one replaces the earlier.

### It fails CLOSED and WHOLE

A loadout is one composition against one supply, so honouring the lines that happened to be legal
would spend the player's points on something they did not choose. `LoadoutRejection` refuses the
**entire** command — changing no ledger, no store and **not the window**:

| variant | when |
|---|---|
| `WindowClosed` | the named band has no open window. **A band with no entry reads as closed**, which is what a turn-two band without a splinter's window is |
| `UnknownKit` / `KitBuysNothing` | a kit the roster does not carry, or one that carries nothing — the roster's `none`, refused by its **empty `uses`** rather than by its id, so the rule stays true of any future empty entry |
| `UnpickableMaterial` | **grant windows only** — see below |
| `OverKitBudget` / `OverMaterialBudget` | grant windows only, checked on the **sum**, not per line |
| `DuplicateAllocation` | a repeated kit or material line |
| `NoStartingBand` | no band of that id in that faction (or its parent has vanished) |
| `ParentCannotSupply` | take windows only: a line the parent cannot cover, naming the id and the shortfall |
| `OnwardTakeStranded` | take windows only: a revision that would leave a band that split off this one holding a take its stock can no longer justify |

**The pick list binds the GRANT and deliberately not a take.** `pickable_materials` says which
materials the *world* hands out at the start, so it binds exactly the window that mints. A take's only
question is whether the parent has the stuff — refusing a material off the pick list would make a
material a band actually crafted untransferable to its own splinter.

> ### ⛔ THE ONWARD TAKE IS A FLOOR, AND IT IS REFUSED RATHER THAN CLAMPED
>
> Ray's case: split 12, pick, split 6 off the splinter, pick — then revise the **first** take downward.
> The middle band has already handed part of its stock onward, so lowering its take under what it
> passed on would strand the third band's units.
>
> `StartingLoadout::onward_items` / `onward_materials` sum every open window whose supply names this
> band as its parent; the new take must clear that floor per item and per material. **A silent clamp
> would move a number the player did not name**, which is the same failure the whole-order refusal
> exists to prevent one level down.

## `BandEquipment::take_units` — freshest first, and that is the opposite of the wear order

The removal API the ledger did not have. `wear_item` spends the **most worn** batch first — a band
uses up the thing nearest the end of its life before opening a fresh one — and `take_units` inverts
it: **a new venture is outfitted properly**, so the splinter walks out with the best gear and the
parent keeps the worn stock. Ascending `wear`, ties broken by earliest insertion index so the rule is
deterministic; the last batch is **split** when the count falls inside it, and the units that leave
carry that batch's `tier`, `grade` and `wear` verbatim (a batch is a quantity of interchangeable units
and half of it is still those units at exactly that condition). Emptied batches are pruned and the
item key with them, matching `restore_batches`' convention.

It returns **what actually left**, short of the count when the ledger is short: the availability
question is the caller's, asked with `count_of`. `place_batches` is the receiving half and **appends**
— a batch carries one wear number, so merging an arriving batch into a standing one would re-condition
both.

## The manifest divides on the RATIO, never on the rounded share

`share` is a fixed-point quotient: a third stores as `0.333333`, and `0.333333 × 3` floors to **0**.
`fission::whole_share` computes `floor(held × asked ÷ workers)` instead, and its sibling
`whole_share_of` does the same for a `held` the parent stores in fixed point — a material total, or a
grant's material points. **Every whole-unit quantity a split derives goes through one of the two**:
the item manifest, the default take's materials, and the splinter's slice of the parent's grant.
Continuous quantities — the larder, the material batches — multiply by the share as before, because
nothing there is quantised.

The division is done in **exact integers**, never through `f32`, and both operands being fixed point
at the same scale is what makes that free: the scale cancels, so `held.raw() × asked ÷ workers.raw()`
*is* the floored quotient. An `f32` hop fails in both directions — a non-dyadic count rounds up
(a tenth of 101 at 10.1 workers answered 9, and the `min(held)` clamp cannot see a quotient that is
too *small*), and multiplying the rounded share rounds down (15 hands splitting 5 against 30 points
gives `30 × 0.333333 = 9.99999`, floor 9, where a third of thirty is exactly 10). The second is
reachable on shipped config, so `split_loadout::a_grant_split_divides_the_material_points_on_the_ratio`
pins those numbers rather than deriving them from the `earthlike` fixture, whose own worker count
never lands on a non-terminating share.

## The pre-fill is the SPAWNED band's, and a splinter has a DEFAULT TAKE instead

`clamped_kit_defaults` keeps its rule unchanged: proportional, floored, remainder unspent. What moved
is which budget it is fitted to — `snapshot::campaign::opening_kit_budget`, the lowest-id band still
holding a grant, because the pre-fill is the *opening* suggestion. A splinter's window has no pre-fill: what it opens
on is the **default take**, the kit allocation the split just moved, which is a statement about what
the band already holds rather than a suggestion about what it might.

**A pre-fill remains a client seed and must never become a back-door spawn stock.** A band that never
receives a `SetStartingLoadout` owns nothing, forever — pinned by
`starting_loadout::the_published_defaults_grant_the_band_nothing`.

## On the wire

The window is **per band**, so it rides the cohort beside the two things a picker draws with it:

- **`PopulationCohortState.loadoutWindow`** (`BandLoadoutWindowState`) — `open`, `kitBudget`,
  `materialBudget`, the accepted `kits` / `materials` rows, `parentBandId` (`0` = a grant), and a
  take's caps as `parentItemSupply` / `parentMaterialSupply` (`id → units`, each already **holdings +
  this take's standing units**, so a client draws the cap the server refuses on). Absent, or `open ==
  false`, means there is nothing to outfit.
  **`kits` / `materials` are non-empty on a fresh splinter** — they carry the default take, so a card
  that renders them and re-sends them unchanged commits a no-op. `parentItemSupply` lists only items
  some kit carries, so a bench tool never appears as a claimable cap.
- **`CampaignSection.openingLoadout`** keeps only the campaign-wide facts: `pickableMaterials`,
  `materialDefaults`, `kitDefaults`, `craftableRecipeIds`.

> **`open`, `kitBudget` and `materialBudget` were DELETED from `OpeningLoadoutState`, not deprecated in
> place** — the same narrow exception `foundingRefusals` took (`fission.md`), and safe for the same one
> reason: this repo has no shipped saves or clients and both halves build from one tree, so no reader
> can hold the old vtable. The general append-only rule stands.

**`loadoutWindow.open` is NOT the client's success signal** — it reads `true` after a refusal and after
a success alike, because a commit never closes a window. What a client reads is the band's own
published state on the recapture the command triggers: after a success that is exactly the allocation
it sent, and after a refusal whatever stood before.

**`SAVE_FORMAT_VERSION` went to 5** with the map (`save.rs`); a `SimState` shape change has no
migration path by design.

## Config files

| File | Key | Purpose |
|---|---|---|
| `src/data/start_profiles.json` | `opening_loadout.material_points` (**30**) | The **grant's** material budget, one point per unit. There is deliberately no kit dial: the kit budget is the band's own working-age head count, and a dial would be a second statement of how many people the band has |
| | `opening_loadout.pickable_materials` | The grant's pick list, in the order it is drawn. Binds a grant window only |
| | `opening_loadout.material_defaults` / `kit_defaults` | The spawned band's pre-fills — suggestions, never grants |
