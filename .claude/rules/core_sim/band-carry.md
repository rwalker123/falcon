---
paths:
  - "core_sim/src/carry.rs"
  - "core_sim/src/expedition_config.rs"
  - "core_sim/src/starting_loadout.rs"
  - "core_sim/src/systems/fission.rs"
  - "core_sim/src/bin/server.rs"
  - "core_sim/src/data/expedition_config.json"
---

# A band's carry — what a band can take when it walks away (#732)

**Carry is a fact about the people.** A band carries `workers × per_worker_carry` load
(`carry::carry_capacity`). Workers carry; children and elders add nothing, so
splitting off elders buys no cargo space.

> ### ⛔ ONE WORKER COUNT FOR A BAND'S CARRY — the actual working-age value, never floored
>
> Carry is continuous, so a standing band's carry is priced on `cohort.working` as it is, through
> one helper. A split prices on `asked` (exactly the workers who cross) and the opening grant on
> `party_workers` (a budget struck once at spawn). **What this closed:** the long move floored
> `working` while the split did not, so one turn of demographic drift (4.0 → 3.99) cut a fresh
> splinter's carry from 32 to 24, its first long move dropped 2 of its 3 baskets, and the band
> starved on a third of its forage (AI bench seed 37, 2 hunger deaths).

## ONE currency, and it is the trade party's

**There is ONE carry model for every reason goods move** — a band's split, a long move, a trade
shipment, and any future carrier. Ray: *"We need one system regardless of what the reason is that the
material is moving"*, so that a cart or a truck benefits every mover through one seam. `carry.rs` is
its one home: `CarryConfig` (the `carry` block of `expedition_config.json`), `per_worker_carry(cfg)`
(the single seam a carrier-side model attaches to), `carry_capacity(workers, cfg)` (the one capacity
function — there is no separate band or shipment cap), and `CarryLoad`, the one load formula. Nobody
re-spells it; a shipment is priced through it with `items: 0`.

```text
load = food + fodder_carry_weight × fodder + item_carry_weight × item units + material_carry_weight × material units
```

A kit weighs the items it expands to (`expand_kits`): `big_game` is a spear and a sled, so 2.
`item_carry_weight` is the lever this arc added. Per-item weights are a strict refinement later,
defaulting to this flat number until someone authors otherwise. **Gear raising carry is
deliberately NOT modelled**: on turn one you would need the sled in hand to carry the sled.

## ⛔ IT CAPS THE TRANSFER, NEVER THE STORAGE

Ten people can haul `X` out the door, and nothing stops a band owning more than `X` afterwards.
Without this line the feature becomes an inventory-limit game. So the cap is evaluated only where
goods leave with people:

| where | what the cap bounds |
|---|---|
| the opening band's grant (turn one) | what it may **mint**: its carry **less its larder**. The larder is **fixed** — there is nowhere to leave it — so it does not yield to goods: on the shipped 30-person band 17.85 × 7.0 ≈ 125 of carry, ≈ 77.7 of it larder, ≈ 47 for kits and materials — the old system's opening total (17 kit slots + 30 material points) |
| a split | what the splinter walks out with. **Goods first, then food in the room left** — see below. `.claude/rules/core_sim/starting-loadout.md` owns the window arithmetic |
| a long move | what the band keeps. See below |

## A split loads GOODS first, and food fills the room left

The splinter's kits and materials load first, and food fills what is left, up to its full
proportional larder share `F`: food carried = `min(F, C − goods load)`. **Taking fewer tools brings
more food — that is the player's dial, and there is no food row.** The card reads *"Brings N of M
food"*. Every accepted order re-resolves the food and moves only the delta, booked as dowry on the
food ledger.

> **Food-first was tried and REVERSED.** With food loading first and no food dial, a turn-one
> splinter's share of the larder filled ~75% of its packs, it walked out with **0 kits**, and the AI
> bench's hunger deaths rose (seed 3: 3 → 16). The player had no way to trade food for tools, so the
> sim made the choice for them. The pack went from 6.0 to 8.0 for the same reason: at 6.0 a
> small splinter could carry its food *or* its tools, not both.

**A split's default is ONE proportional rule — kits, then food, then materials**
(`starting_loadout::split_default_outfit`, both arms):

1. **Kits:** the splinter's proportional share of the source's kits,
   `floor(source kits × asked ÷ the source band's WHOLE working hands)`, spread across the rows by
   largest remainder in the source's mix. A parent with a kit for every hand gives a kit for every
   splinter hand (17 kits on 17 hands, split 6 → **6**); a short parent shares the shortage (12 on 17
   → **4**, and it keeps 8); a parent with spares shares those too (20 on 17 → **7**). The
   denominator is **whole** hands: dividing by the fractional working value (17.85) would floor the
   playtest's 6 to 5.
2. **Food:** the splinter's full larder share `F`, or what the carry leaves after the kits.
3. **Materials** fill the room left, in the source's mix, floored and capped by the source.

When the carry cannot hold it all, the cut runs in reverse: materials, then food, kits last. **The
parent keeps what is left** — on BOTH arms. On the take arm the goods physically move; on the grant
arm the splinter mints its share and **exactly those rows are deducted from the parent's standing
allocation** (`rebalance_partitioned_grant`, `MintedRows`), so the pair holds precisely the parent's
outfit (playtest: 5/5/7 split 6 → splinter 2/2/2, parent 3/3/5). The parent is then cut with
`fit_to_carry`'s staging only if it is still over its own goods allowance.

> ⛔ **A grant split that deducted nothing was a duplication**: the parent kept all 17 kits while the
> splinter minted 6 on top — 23 kits from a 17-kit outfit, each band "within its own carry". Pinned on
> the pair by `split_loadout::a_grant_split_conserves_the_parents_outfit_across_the_pair`.

> **Why one rule, not "one kit per worker".** The playtest ask was *"match the number of kits to
> workers"* for a fully kitted parent, and proportional gives exactly that there. A per-worker target
> instead hands the whole shortage to the parent when it is short of kits (a later split of a band
> that has worn tools out) — the bench showed it. The earlier default (scale goods and food together by
> `C ÷ (F + goods)`) left a 6-worker splinter 12 kits and 18 of its 26 food, and re-fitted the parent
> to 14 kits and no materials; both read wrong in play.

**A long move is still food first** (below): a band moving on its own keeps eating.

> ### ⛔ ON THE OUTFITTING CARD, A BAND'S DETACHED PARTIES ARE STILL PART OF THE BAND
>
> `starting_loadout::window_people` = the band plus every `Expedition` whose home band it is: the
> window's carry is `carry_capacity(band working + Σ party working)` and its fixed larder is the
> band's larder plus every party's carried provisions. So detaching or recalling a party leaves the
> card's carry, food and free room unchanged (`long_move_tests::a_detached_party_leaves_the_outfitting_window_unchanged`).
>
> **What it closes** (Ray, playtest): a scout takes one pack of carry AND `workers × distance ×
> provision_draw_per_worker_per_tile` of food, so a long trip FREED room on a turn-one grant card —
> send a scout, mint more kits, recall it — and a short trip pushed the band over.
>
> **The card only.** The cohort's own `carryCapacity` / `carryLoad`, the band panel and the long-move
> shed count the people PRESENT — a party does not walk with its band. A party's own kit is a fresh
> set issued at launch (`outfitted_party_equipment`), not debited from the band's ledger, so no goods
> term is needed for it.

> ### ⛔ A STATIONARY BAND IS NEVER WARNED THAT IT IS OVER ITS CARRY
>
> A band can end up holding more than it could carry without anybody ordering it: a splinter's
> revision hands food back to its turn-one parent (whose larder is fixed and counts), a scout party
> leaves and takes a pack of carry with it, the working value drifts. **Nothing re-fits or refuses
> that** (Ray, 2026-10-04: *"leave it"*), and — Ray, the same day — **"if a band isn't moving, it
> should never give the warning it is over its carry limit."** The cap limits what walks away, not
> what a band owns, so the ONE place over-carry is stated is the long-move targeting warning (the
> published `longMoveLeaves*`). The outfit card prints `0 / C carry left` in normal ink, the turn orb
> has no over-carry item, and the band panel's Carry row is never amber.
>
> **`OverCarry` bounds what is ADDED.** It refuses an order only if its goods load is above the
> allowance **and** above what the band already holds, so a band over its carry can still step its
> way back down one removal at a time — refusing every order that leaves it over would trap it.
> Silently trimming a band's gear, or refusing one band's order over another's choices, stays out.

## A long move leaves behind what the band cannot carry

`handle_move_band`, for a resident band (never an `Expedition`, whose packs have their own rules):

- **Within `carry::move_ferry_reach_tiles` the band keeps everything.** It can ferry its goods
  across in trips. The base is `supply_network_config.json` `reach_tiles` (3), the radius within
  which same-faction bands already pool for free, so it needs no lever of its own.
- **Past it, the band sheds down to its carry the moment the order is accepted**, and what it drops
  is **lost**: there is no storage object to leave it in. When storage exists, that becomes where the
  leftover goes.
- **Food first, then tools before materials.** If the food tier alone overfills the packs, food and
  hay scale down to fit and every item and material is left. Otherwise, in the room left: if the
  items alone fit, every item is kept and the materials are cut (proportional, floored); if not, the
  materials are dropped and the items scale down, floored to whole units. It is `fit_to_carry`'s
  staging — **one rule for every fit that has to cut: tools feed a band, materials can be gathered
  again.** The units left behind are the **most worn** (`BandEquipment::shed_units`), so the band
  carries its best gear. Bench tools weigh like anything else.
- **An overage smaller than one whole unit comes off the food.** A band at exactly full carry can
  drift a hair over it (a fresh splinter at 4.0 workers reads 3.99 a turn later: 27.93 of carry
  against a 28 load), and dropping a whole tool for 0.07 is a punishment for rounding, not a choice.
  So a sub-unit overage is taken from the continuous food tier and every item and material stays; a
  real overage keeps the order above, whole units apportioned by largest remainder.
- **Distance is measured per order.** A band that crosses the map in reach-sized hops keeps
  everything. That reads as ferrying in relays and is accepted, not an exploit to close.

> ### ⛔ `move_ferry_reach_tiles` IS A FUNCTION SEAM — NOBODY READS `reach_tiles` FOR IT
>
> `routes::road_keeping_range`'s discipline: the config holds a base, every caller (the move and the
> published forecast) asks the function, so a reach that later grows with roads, pack animals or a
> cart is one body changing and no call site moving.

**One function plans the shed** (`carry::plan_long_move_shed`). The move applies its plan and
the snapshot publishes the same plan as the band's long-move forecast
(`PopulationCohortState.longMoveLeavesFood` / `longMoveLeavesItems` / `longMoveLeavesMaterials`).
So the warning the client shows before a move and what the move then drops cannot disagree, and the
client mirrors none of the rule.

**The dropped food is a ledger term**, `foodLeftBehind` (`campaign.md` → the food identity), with a
`status=left_behind` feed line naming what was dropped. Dropped hay is simply gone, because the fodder
ledger has no spoil term.

## On the wire

`PopulationCohortState`: `carryCapacity`, `carryPerWorker` / `carryMaterialWeight` / `carryFodderWeight` (the pack echo, renamed from `expeditionTrade*`),, `carryLoad`, `moveFerryReachTiles` (echoed per cohort, the
`bandMoveTilesPerTurn` idiom), the three `longMoveLeaves*` fields (all 0 when the band fits) and
`foodLeftBehind`. The outfitting window's own cap and the weights a client prices an order with are
in `starting-loadout.md` → On the wire.

## Config files

| File | Key | Purpose |
|---|---|---|
| `src/data/expedition_config.json` | `carry.per_worker_carry` (**7.0**) | One worker's pack, in food units — THE pack for every carrier (was `trade.per_worker_carry` 6.0). History: 6.0 left a splinter its food *or* its tools; 8.0 gave the opening band ≈ 65 of goods room against the old ≈ 47, and with one currency all of it could become kits (playtest: 17 spare baskets on 17 workers); 7.0 puts the opening room back at the old total |
| | `carry.item_carry_weight` (**1.0**) | Load of one item unit relative to one food unit |
| | `carry.material_carry_weight` (1.0), `carry.fodder_carry_weight` (0.5) | Load of a material unit and a unit of hay |
| `src/data/supply_network_config.json` | `reach_tiles` (3) | The base of `move_ferry_reach_tiles` |
