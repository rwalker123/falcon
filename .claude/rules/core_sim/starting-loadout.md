---
paths:
  - "core_sim/src/starting_loadout.rs"
  - "core_sim/src/systems/fission.rs"
  - "core_sim/tests/starting_loadout.rs"
  - "core_sim/tests/split_loadout.rs"
---

# The outfitting window — one per BAND, open until the turn is finalized

Nothing at the *spawn* grants a band anything: `equipment.json` ships `start_stock_fraction: 0.0`
(`.claude/rules/core_sim/equipment.md`) and no material declares a start stock
(`.claude/rules/core_sim/crafting.md`). **Everything a band owns comes through its own outfitting
window** — the sim commits that window's *default* the moment the band exists (see "A default is
applied, never suggested"), and the player revises it for the rest of the turn.

`starting_loadout.rs` holds all of it: the `StartingLoadout` resource, the systems that open, outfit
and shut a window, and `apply_starting_loadout`, which validates, resolves the band and moves or
mints.
`bin/server.rs`'s handler only translates the wire types and reports a refusal (see "A refusal is
said on the feed"). The client half is
`.claude/rules/client/starting-loadout.md`; the split that opens a splinter's window is
`.claude/rules/core_sim/fission.md`.

## Every band gets a window, and turn one is NOT a special case

Two things open a window and nothing else does:

| opener | when | the window it opens |
|---|---|---|
| `stamp_starting_loadout` | Startup, chained after the spawn | the spawned band's, carrying the campaign's **grant**. `outfit_opening_bands` is chained straight after it and commits the default against that grant |
| `split_band_from_parent` | every split, every turn | the splinter's, **carrying its accepted allocation** — the default take it was moved, or (on a grant) the split default it mints against its own carry |

**Turn one is not special — only the PARENT's state differs.** On turn one the parent still holds an
unspent grant, so its splinter mints against a grant of its own (`carry_capacity(asked)`), its picks **mint**, and the split moves no
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
Grant  { carry_budget }                             → the picks MINT
Parent { parent, items, materials, carry_budget }   → the picks MOVE
```

**Both arms cap on ONE currency: load** (`.claude/rules/core_sim/band-carry.md` owns the carry model
and the load formula). An order's load is its expanded items × `item_carry_weight` plus its material
units × `material_carry_weight`, and `OverCarry { load, capacity }` refuses an order whose load
exceeds the window's `carry_budget`. It is still an enum because the arms differ in what ELSE caps
them: a grant mints, so nothing else does; a take moves, so it is also bounded by what another band is
standing on right now, per item and per material.

**What a band carries includes its food, on every window.** The opening band's carry is its own,
**read live** (`Grant { carry_budget: None }` → `band_carry_capacity(cohort)`, the unfloored working
value), so its card and its band panel state one number. Its spawned larder is **fixed**
(`LoadoutWindow::food_is_fixed`, published `foodFixed`): it counts against the carry and does not
yield to goods, so the goods it may mint are `carry − larder mass` (`goods_allowance`). Shipped:
≈ 125 − 77.7 ≈ 47, the old system's opening total (17 kit slots + 30 points).

> **The larder was once free, and that was wrong.** With the starting larder outside the budget, the
> opening band minted 136 of goods and then stood **over its own carry** on turn one (playtest: a
> parent after a split read `Carry 125 / 87`), so its first long move would shed ~38 — the outfit
> card let it pick goods its people could not walk away with, and a bigger band looked far richer
> per head than a splinter that paid for its food.
`opening_loadout.material_points` and the per-hand kit budget are **retired**: two allowances in two
incomparable currencies was the defect #732 closed, because on turn two a take was capped by nothing
that scaled with the workers leaving.

### What a SPLIT gives the splinter

| the parent's window | what the split does | the splinter's window |
|---|---|---|
| still **grants** (`LoadoutWindow::grants()` — open, and a `Grant`) | **recomputes the grant, and moves NOTHING physical** | a `Grant` of its own struck at `carry_capacity(asked)` — exactly the take arm's carry. The goods are minted from it; the food that fills the rest moves off the parent's larder. The parent's grant is **recomputed, not partitioned**: its own live carry less its own (fixed) larder, re-fitted by `rebalance_partitioned_grant` when its outfit no longer fits. The two carries add up to the parent's carry before the split by construction (both are linear in workers), which is the invariant the old ratio partition guarded (`split_loadout::a_turn_one_splits_two_carries_add_up_to_the_parents_before_it`). |
| does not (closed, or already a take) | **moves goods** — there is no grant left to partition | a `Parent` take whose `carry_budget` is the **whole carry**, `carry_capacity(asked)`. It is also capped by what the parent can supply (see below). |

**Goods load first on both arms, and food fills the room left.** The window records the splinter's
`SplitDowry` — its full proportional larder share `F` and what has crossed — and after every accepted
order `resolve_split_food` sets the food to `min(F, C − goods load)`, moving only the delta (both
directions) and booking it as `DowryOut`/`DowryIn`. The **default** is `split_default_outfit`:
the splinter's proportional share of the source's kits (over whole hands), then its food, then
materials in the room left — the rule and its reasons are in `band-carry.md`. Re-sending the
published allocation is still an exact no-op, food included. Why goods first: `band-carry.md`. Every cap is a number the split has just
resolved (`asked`, `share`), not a literal.

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

#### The re-fit: the parent is clamped to what the partition left it

`fission::rebalance_partitioned_grant` closes the loop, and it runs **only when the parent no longer
fits its reduced budget**:

1. the parent's standing allocation is re-fitted by `fit_to_carry`'s proportional-floored rule —
   the same rule the profile's default outfit is fitted by;
2. the parent is **re-materialized from the clamped allocation** through `apply_starting_loadout`, the
   path a player's own commit takes, so its ledger, its store and its meter state one thing and the
   meter can never read negative.

**Its standing allocation is never empty now**, which is what makes this fire on an *uncommanded*
parent: a band holds its applied default from creation, so there is always something for the clamp to
bite on. That is the `-6 / 22 left` case closed for a player who never opened a card
(`split_loadout::a_split_leaves_an_uncommanded_parent_inside_its_reduced_budget`).

> **What the clamp takes off is NOT handed to the splinter.** It used to be, because a grant split
> moves no goods and the parent's leftovers were the only thing there was to open the splinter's card
> on. The splinter mints its **own default** instead — a sensible opening outfit rather than whatever
> a heavily-committed parent happened to be over by. Nothing is destroyed by dropping the hand-off:
> on this arm every unit on either band is minted from a budget, and the two budgets still partition
> the one grant exactly (`split_loadout::the_splinters_outfit_is_its_own_default_rather_than_the_parents_leftovers`).

**A parent that still fits gives up nothing and the re-fit returns without touching it.**
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

**A splinter of a still-granting parent gets its rows from the APPLY that outfits it**: nothing was
moved, so it **mints its split default** (`split_default_outfit`) against its own carry, and the window's rows
are what that apply set. See the callout below, which is the rule for every band and not only a
splinter.

> ### ⛔ A DEFAULT IS APPLIED, NEVER SUGGESTED
>
> **A band holds its default outfit from the moment it is created, whether or not anybody ever opens
> its card.** Two seams, one per kind of band: the **opening band** gets the profile's defaults through
> `starting_loadout::outfit_band_with_defaults` (from `outfit_opening_bands`, a Startup system chained
> after `stamp_starting_loadout`), and a **splinter** gets `split_default_outfit` — through
> `fission::outfit_grant_splinter` on the grant arm, and as the moved default take on the take arm.
>
> **The defect this closes is a loss, not a blank screen.** A player composed an outfit for a
> splinter, never pressed *Set out*, ended the turn — and the band walked away with nothing. The
> server's record showed exactly one `set_starting_loadout` that game, for the parent. A default that
> exists only as a client-side seed **cannot survive a card nobody commits**; one the sim has applied
> cannot be lost. (The symptom that led here was narrower — a turn-one splinter published
> `kitBudget 5` / `materialBudget 8` with `kits: []` and `materials: []`, so the card read
> `5 / 5 left` and `8 / 8 left` with every row at zero — but a filled card is still a card, and an
> untouched *"Set out"* is a real order meaning **take nothing**.)
>
> **It goes through `apply_starting_loadout`**, the path a player's own accepted order takes, rather
> than writing the window's rows directly. So the band's ledger and store really hold the outfit, and
> the window's accepted rows say so **because an apply sets them** — the card and the band agree by
> construction, and a later revision replaces something real.
>
> **The budget is the BAND's.** `fit_to_carry` fits the kit and material defaults **together** to
> that band's own `carry_budget`: when their load exceeds it, **materials are cut before tools** —
> if the kits alone fit, every kit row is kept and the materials scale into what is left; otherwise
> the materials go to 0 and the kits scale. Proportional, floored, remainder unspent at each stage.
> Tools feed a band; materials can be gathered again. The defaults are not
> validated at load against any campaign number (there is none); they are clamped at runtime, and
> `stamp_starting_loadout` warns when the opening band's clamp binds.
>
> **Only a grant window is outfitted.** A take window mints nothing, and it already opens on the
> default take that physically crossed.
>
> ⛔ **MINTED, NEVER MOVED.** A grant split still takes nothing off the parent: the splinter's outfit
> is minted against its own carry, so filling the card cannot resurrect the double charge
> (`split_band_from_parent`'s `default_kits` / `default_materials` stay empty on that arm and are the
> only lists `expand_kits` walks).
>
> ⛔ **AN APPLY IS A REPLACEMENT, so the default REBUILDS the ledger from empty.** On shipped config
> that is invisible — `start_stock_fraction` is `0.0` and a spawning band owns nothing to overwrite.
> Under a config that *does* stock a band at spawn (`EquipmentConfig::for_a_stocked_fixture`, which
> only fixtures use) the default replaces that stock, so a fixture whose subject is some other item
> has to declare its gear again after the world is built.
>
> Pinned by `starting_loadout::a_band_is_created_already_holding_its_default_outfit` (the opening
> band, no command sent), `split_loadout::a_turn_one_splinter_is_created_already_holding_its_own_default`
> (the splinter, asserted on the **encoded envelope** *and* on the ledger and store behind it),
> `a_split_leaves_an_uncommanded_parent_inside_its_reduced_budget` (the `-6 / 22 left` case, now with
> nothing commanded) and `a_turn_two_splinters_card_is_still_the_take_it_was_handed` (the take arm,
> unchanged).

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
`fit_to_carry`'s stated reasoning about arbitrary first-come winners. So the thing that has to fit is the **expanded item list**, whole:
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
| `PartyCarries` | grant windows only: the order's expanded count of an item is below what the band's detached parties are carrying of it. A grant apply rebuilds from empty and mints `allocation − party_held_items`, so an order below the parties' holding would let their gear land on top of a re-spent outfit when they fold back |
| `OverCarry` | both arms: the whole order's **load** exceeds the window's `carry_budget`. On a take it is checked after `ParentCannotSupply` and `OnwardTakeStranded`, so those are reported first |
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
`whole_share_of` does the same for a `held` the parent stores in fixed point — a material total.
**Every whole-unit quantity a split derives goes through one of the two**: the item manifest and the
default take's materials. Continuous quantities — the larder, the material batches — multiply by the share as before.

The division is done in **exact integers**, never through `f32`, and both operands being fixed point
at the same scale is what makes that free: the scale cancels, so `held.raw() × asked ÷ workers.raw()`
*is* the floored quotient. An `f32` hop fails in both directions — a non-dyadic count rounds up
(a tenth of 101 at 10.1 workers answered 9, and the `min(held)` clamp cannot see a quotient that is
too *small*), and multiplying the rounded share rounds down (`30 × 0.333333 = 9.99999`, floor 9,
where a third of thirty is exactly 10).

## The default is fitted to WHICHEVER budget it is drawn against

`fit_to_carry` is the one fitting rule: **materials are cut before tools**, each stage proportional,
floored, remainder unspent, on one load. The long-move shed uses the same staging after its food
(`band-carry.md`). There are three readers of it:

| reader | the budget it fits to | what it does with the answer |
|---|---|---|
| `outfit_band_with_defaults` | **that band's own** goods allowance | **applies** it — the opening band at Startup (a splinter's default is `split_default_outfit`, not this) |
| `fission::rebalance_partitioned_grant` | the parent's reduced `carry_budget` | re-materializes the parent from it |
| `stamp_starting_loadout` | the opening band's | warns once per world when the config over-allocates, which is the only moment that fault is observable |

**A TAKE splinter is not outfitted from the defaults.** What it opens on is the **default take**, the
kit allocation the split just moved — already fitted to the goods allowance — which is a statement
about what the band already holds.

**The kit default is NOT published campaign-wide.** What a band holds comes from the apply, and the
applied, clamped rows arrive on that band's `loadoutWindow.kits` — pinned by
`starting_loadout::a_band_is_created_already_holding_its_default_outfit`, which sends no command at
all. The campaign-wide `materialDefaults` is a STATEMENT, not a second grant: its one reader is the AI
seat's grant-window material pre-fill (`sim_ai`'s `ConstantStance::prefill_over`), and no client
seeds from it.

## On the wire

The window is **per band**, so it rides the cohort beside the two things a picker draws with it:

- **`PopulationCohortState.loadoutWindow`** (`BandLoadoutWindowState`) — `open`, `carryCapacity`
  (the band's WHOLE carry, goods and food, on every window — the same number as the cohort's
  `carryCapacity`), `foodShare` / `foodCarried` and `foodFixed`: on a fixed-larder window (the
  opening band, a parent whose grant is open) both food fields are the larder's mass and the goods
  allowance is `carryCapacity − foodCarried`; on a splinter they are its full share F and what has
  crossed, and the goods allowance is the whole `carryCapacity`. `OverCarry`'s capacity is that goods
  allowance. Then the accepted `kits` / `materials` rows, `parentBandId` (`0` = a grant), and a
  take's caps as `parentItemSupply` / `parentMaterialSupply` (`id → units`, each already **holdings +
  this take's standing units**, so a client draws the cap the server refuses on). Absent, or `open ==
  false`, means there is nothing to outfit.
  **`kits` / `materials` are non-empty on EVERY fresh window** — the default take on a take, the
  applied default outfit on a grant — and on both arms they describe gear the band is **actually
  holding**, so re-sending them unchanged is an exact no-op and a card that renders them is never
  blank against a live budget. `parentItemSupply` lists only items some kit carries, so a bench tool
  never appears as a claimable cap.
- **`CampaignSection.openingLoadout`** keeps only the campaign-wide facts: `pickableMaterials`,
  `materialDefaults`, `craftableRecipeIds`, and the two weights a client prices an order with,
  `itemCarryWeight` / `materialCarryWeight` — so it computes an order's load exactly the way
  `OverCarry` refuses on. Whether a window is open, its carry and its kit rows are all per-band, on
  `loadoutWindow`.

**`loadoutWindow.open` is NOT the client's success signal** — it reads `true` after a refusal and after
a success alike, because a commit never closes a window. A success is read off the band's own
published state on the recapture the command triggers, which is exactly the allocation it sent. A
refusal is read off the event feed (below), because it moves no band row.

### A refusal is said on the feed

Populations ship as diffs, so a refused order — which leaves the band's row byte-identical —
publishes **nothing** about the band. The handler therefore keeps its `warn!` and pushes one event
feed line: kind `starting_loadout` (`CommandEventKind::StartingLoadout`), label `Outfit failed`,
detail = the `LoadoutRejection`'s Display text, filed under the commanding faction, and
`CommandEventState.band` = the refused band's `BandId` (`0` on every row not about one band — the
allocator never issues it). An accepted loadout pushes **no** line: the republished band row is its
confirmation. Pinned by `bin/server.rs`'s
`a_refused_starting_loadout_publishes_one_event_naming_its_band`, read off the encoded delta.

A `SimState` shape change has no migration path by design: `SAVE_FORMAT_VERSION` moves instead
(`save.rs`).

## Config files

| File | Key | Purpose |
|---|---|---|
| `src/data/start_profiles.json` | `opening_loadout.pickable_materials` | The grant's pick list, in the order it is drawn. Binds a grant window only |
| | `opening_loadout.material_defaults` / `kit_defaults` | The **opening band's** default outfit, applied after the world-build meal so it fits the allowance the card shows. Shipped: kits `big_game 5 / trapping 5 / gathering 7` (one per hand on the 30-person band, so an untouched proportional split hands a kit per splinter worker) and `bone 2 / fibre 12 / hide 6` — load 47 against ≈ 47.2, fitting whole. A splinter's default is `split_default_outfit`, not these |
| `src/data/expedition_config.json` | `carry.per_worker_carry` and the carry weights | The grant's size and every order's load — see `band-carry.md` → Config files. There is deliberately no loadout-specific dial: the budget is the band's own workers × one pack |
