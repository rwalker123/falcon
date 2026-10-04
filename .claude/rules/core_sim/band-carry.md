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
| the opening band's grant (turn one) | what it may **mint**: its carry **less its larder**. The larder is **fixed** — there is nowhere to leave it — so it does not yield to goods: on the shipped 30-person band 17.85 × 8.0 ≈ 142.8 of carry, ≈ 77.7 of it larder, ≈ 65 for kits and materials |
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
> sim made the choice for them. The pack also went from 6.0 to 8.0 for the same reason: at 6.0 a
> small splinter could carry its food *or* its tools, not both.

**The default is proportional**: an untouched split takes the default goods and the full food share
if they fit; otherwise both are scaled by `C ÷ (F + goods load)`, goods floored, and the food absorbs
the rounding slack. On shipped numbers an untouched 4-worker turn-one splinter carries 3 kits
(Stalking, Trapping, Gathering), about 10 material units and ~15.5 of its 17.4 food.

**A long move is still food first** (below): a band moving on its own keeps eating.

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
| `src/data/expedition_config.json` | `carry.per_worker_carry` (**8.0**) | One worker's pack, in food units — THE pack for every carrier (was `trade.per_worker_carry` 6.0) |
| | `carry.item_carry_weight` (**1.0**) | Load of one item unit relative to one food unit |
| | `carry.material_carry_weight` (1.0), `carry.fodder_carry_weight` (0.5) | Load of a material unit and a unit of hay |
| `src/data/supply_network_config.json` | `reach_tiles` (3) | The base of `move_ferry_reach_tiles` |
