# Plan: Contact, Connections & Logistics

**Arc:** #527 · **Slices:** #537 (clear the ground) → #538 (the connection primitive) → #517 (logistics
and cargo) → #532 (the route ladder) → #232 (the overlay)

The substrate that two groups of people need before anything can pass between them: knowing the
other exists, holding a relationship that persists past the meeting, and building something over
which mass can actually move.

---

## Motivation

The manual promises a world of peoples who meet, trade, learn from and change each other. The sim
has solved that three times, in three unrelated ways, each covering one slice and none of them
aware of the others:

| What moves | How it moves today | Where |
|---|---|---|
| **Goods** | proximity — same-faction bands within `reach_tiles` pool their stores | `supply.rs:144` |
| **Knowledge** | tile-adjacency links carrying `openness`/`leak_timer` | `systems/trade.rs:142` |
| **Culture** | layer containment — global → regional → tile → band | `culture.rs` |

Nothing in that table asks *"do these two peoples know each other?"* — proximity does not require
acquaintance, containment does not require contact, and the knowledge path is dead code (§As-built).
So a splinter band that walks ten hexes from its parent is severed the moment it founds; a scouting
party that meets strangers comes home with nothing; and there is no object a second faction could
attach a relationship to when #513 arrives.

This arc builds one stack under all three, with a hard constraint: **the arrival of a second faction
must change almost nothing.** Faction is a property of the endpoints, never a branch in the code.

## The stack, in one line each

**Range** — the land a group can currently see.
**Contact** — the event of finding someone in it.
**Connection** — the relationship that outlives the finding.
**Riders** — the things that use a connection: logistics, culture, knowledge.
**Cargo** — the things that use logistics: anything with mass.

Everything below elaborates one of those five.

---

## Q1 — What is range?

**Range is not a number. A band's range is its currently-seen tile set.**

This is the design's first and most load-bearing decision, and it is a decision *not* to build
something. The fog-of-war ledger already computes, per faction, per turn, exactly the field this arc
needs (`visibility.rs`, `visibility_systems.rs`): `Active` (seen now), `Discovered` (seen once,
remembered, stale), `Unexplored`. That field is already:

- **Dynamic** — it grows with local scouts (a band's `scoutRevealRadius` is its vantage distance),
  with hunting expeditions, and with scouting expeditions, and it shrinks back to `Discovered` when
  the crew comes home.
- **Earned** — you see far because you *spent people on seeing far*, which is what scouts are for.
- **Terrain-shaped**, and about to get more so. Movement today draws a straight line and reveals
  along it; once it follows a navigable path (**#282**) the revealed field bends around the mountain
  instead of through it, and range stops being a circle **with no work in this arc**.

### Why not a radius

A radius would have been the obvious build, and every version of it is wrong here:

- It re-answers a question the map already answers. A band across a range and a band four hexes down
  a river valley are not equally close, and the one system where the land should decide who you can
  know is the last place to flatten it to Euclidean distance.
- It would be the **fifth** radius in the sim, next to `band_work_range` (2), `reach_tiles` (3),
  `migration.base_reach` (4) and the scout vantage — four numbers that already answer four different
  questions, and a fifth would have no way to explain itself to a player.
- A hard cutoff is a cliff the player cannot see. At `reach_tiles` = 3 today, a band three hexes away
  is a full economic partner and one four hexes away is a stranger.

### Why not a separate "presence" stat

An earlier draft gave every group two numbers — *presence* (how far I am felt) and *awareness* (how
far I notice) — to capture the asymmetry between a large settled band and a three-person scouting
party. The asymmetry is real; the second number is not the way to get it. **A scout party is unseen
because it is choosing to be**, not because it is small, and a party that *wants* to be noticed
should be. That is a scouting mechanism and it belongs to the scouting arc; here it is enough that
**scouting is the verb that temporarily extends a band's range**, which is already true.

> **Open:** whether a large group is detectable *beyond* anyone's range — smoke on the horizon. It
> would be a per-target sight bonus rather than a per-group stat, and nothing in this arc needs it.

---

## Q2 — What is contact, and what does it leave behind?

**Contact is an event: a group is standing in a tile you can see.** There is no radius test, no
roll, and no config lever. If it is in your range, you have found it.

What contact *creates* is a **connection**, and the connection is the thing this arc actually adds
to the world.

### A connection is a raw primitive

A connection knows **nothing** about goods, culture, or knowledge. It is not a trade agreement and
not a treaty. It is: *these two groups know each other, this much, right now.* Everything else in the
game builds on top of it and defines its own use of it — the same way `LocalStore` knows nothing
about food.

This is the discipline that keeps the arc honest. The moment a connection carries a `tariff` or an
`openness_to_ideas`, it has stopped being a primitive and has started being one rider's opinion
wearing the primitive's clothes. That is precisely how `TradeLink` ended up carrying
`from_faction`/`to_faction`/`tariff`/`leak_timer` on what should have been an edge (§As-built).

### Connections are directional

A connects to B without B connecting to A. This is not a refinement — it is the scout on the ridge,
watching a settlement that has no idea anyone is there. The direction is *who found whom*, and it is
what lets influence be asymmetric without a second mechanic. A mutual relationship is simply both
edges existing.

Whether a given rider *requires* both directions is the rider's business, not the connection's:
culture plausibly needs a two-way tie where a one-way observation is enough to carry knowledge home.
That is a per-rider property (§Q5).

---

## Q3 — The keystone

> ### Only presence makes something **Seen**. A connection can only ever grant **Discovered**.

Meet a band, exchange maps, and their land becomes `Discovered` — frozen at the moment they told
you. To actually *watch* anything you need presence there, and presence has to be maintained.

Three consequences, all of which the arc leans on:

**A remembered band behaves exactly like a remembered herd.** You know where they *were*. They may
have moved, split, starved, or been killed, and you will not know until you look again. This is not a
new mechanic to build or teach — it is the fauna memory model applied to people, and the player
already understands it because they have already been surprised by a herd that wasn't where they
left it.

**It decides the question the old FOW spec left open.** `plan_trade_fow_integration.md` (deleted with
the demolition — `git log -- docs/plan_trade_fow_integration.md`) planned to mark trade-route tiles
`Active`; its own "Future Enhancements" list floated `Discovered` as a possible balance tweak. It is
not a tweak, it is the rule — with one carve-out below.

**The carve-out is honest.** A *maintained logistics route* does keep its tiles `Seen`, for exactly
as long as it is held (§Q4). That is not an exception to the keystone so much as an instance of it:
those tiles are seen because there are people on them.

---

## Q4 — Logistics: what holds a link open, and what it costs

A **logistics link** is a rider on a connection: a maintained physical path between two groups over
which mass can move. You cannot have one without a connection; having a connection does not give you
one.

### The supply network is already a logistics network

`balance_supply_networks` (`supply.rs:144`) connects same-faction bands within `reach_tiles` and
moves every commodity toward a population-weighted per-capita balance, throughput-limited and
friction-lossy. That is a logistics network whose links are **implicit** (derived from proximity),
**free**, and **equalizing rather than directed**.

So it is not replaced — it is **re-founded on the primitive**: proximity produces a connection,
and over a short distance a logistics link is cheap enough to hold itself. Two bands standing near
each other behave exactly as they do today. **There is no early-game regression, by construction**,
and the same object that carries a splinter's lifeline is the one a foreign trade route will need.

### The route ladder

What holds a link open is a **route**, and routes climb a ladder: **game trail → trail → dirt road →
paved road**, and later the things that succeed them. Each rung is unlocked by knowledge earned
working the rung below — walking a trail is how you learn to build one — and each rung is **cheaper
to travel and dearer to keep**.

That two-sided movement is the whole point. The ladder is *not* a straight upgrade path: you pave
where the traffic pays for the upkeep, and everywhere else a trail is the right answer forever. It is
the same economy as the pen — a rung you climb only where the land justifies it.

The bottom rung already has an issue: **#215**, *"herd/game trails follow hex centers and become the
basis of roads."* The first roads being the ones the animals made is the origin this ladder wants.

### It is the intensification ladder's shape

`intensification_ladder.json` is a config-driven rung engine, and its fields map onto routes almost
without translation:

| Ladder field | On a patch or herd | On a route |
|---|---|---|
| `branch` | `plant` / `animal` | `route` — a third branch |
| `requires_rung` | can't pen what you haven't tamed | can't pave what isn't a road |
| `unlock_knowledge` / `earns_knowledge` | working a rung teaches the next | walking a trail teaches road-building |
| `build.progress_per_turn` | a crew clears ground | traffic wears the route in |
| `build.decay_per_turn` | an unworked patch goes feral | an unused road reverts |
| `build.grace_turns` | *"a weeded patch reverts in a season, a fence stands for years"* | a trail reverts in a season, a paved road stands for years |
| `build.crew_needed` | the crew the build wants | the crew the road wants |

The knowledge pacing is already shared across branches in one file, deliberately — *"every knowledge
pace in the game is tuned in ONE file"* — so a route branch is paced against the same
~20-turns-per-lesson yardstick as cultivation and husbandry, with no new tuning vocabulary.

**Two known gaps, to be closed in the slice that builds this:**

1. **A route is an edge, not a source.** Every shipped rung sits on a *thing* — a patch, a herd —
   with a `site_requirement` about the tile it stands on. A route spans many tiles and belongs to a
   connection. The meter machinery ports; the site and behavior primitives are source-shaped and
   need route-shaped siblings.
2. **Upkeep is a standing cost, and the ladder only has a build cost.** `crew_needed` is the crew
   that *builds*; a paved road costing more to hold than a trail is a crew that *stays*. The closest
   shipped precedent is herding, which the campaign rule already calls standing labor.

### The terrain cost table is already written

`TerrainDefinition` (`terrain.rs:150`) carries, for all 37 terrain types:

| Field | Read by |
|---|---|
| `movement` profile | fauna, the visibility sweep |
| `logistics_penalty` | the dead logistics sim; morale hardness |
| `attrition_rate` | the dead logistics sim; morale |
| **`detection_modifier`** | **nobody** |
| **`infrastructure_cost`** | **nobody** |

The last two have never been read by any system. `detection_modifier` is *how well you see, and are
seen, in this terrain* — the range question, already answered per biome. `infrastructure_cost` is
*what it costs to hold a route through here* — the route-upkeep question. **This arc's cost model is
substantially already authored**; it has simply never been wired to anything.

---

## Q5 — The riders

Each rider defines its own use of a connection. The connection does not know they exist.

| Rider | Sits on | Its own behaviour |
|---|---|---|
| **Logistics** | a connection | holds a route; its tiles stay `Seen` while held; climbs the ladder |
| **Culture** | a connection | **open — §Open items** |
| **Knowledge** | a connection | **open** — the `openness → leak_timer → partial fragment` model is worth keeping (§As-built) |
| **Cargo** (food, fodder, materials) | a **logistics link** | mass moves, throughput-limited, friction-lossy |

Two structural properties belong to the rider, not the connection:

- **Directionality requirement** — how many directions the rider needs. A one-way observation is
  enough to carry knowledge home; culture plausibly needs a two-way tie. Expressed as a column on
  the rider table, not as five special cases.
- **What it does to the world beyond moving its payload** — logistics keeps tiles `Seen`; the others
  are open.

### The cargo is food, fodder and MATERIALS — and the trade scalar was retired rather than given a sink

**An earlier draft of this section had it the other way round**, and the correction is worth keeping
because it is the arc's own reasoning turned against its first premise. That draft argued: the sim
already pays a `TRADE_GOODS` yield nobody spends — there is no `take(TRADE_GOODS)` anywhere in the
workspace — so *"cargo over a logistics link is the sink that has been missing"*.

Both halves of that were true and the conclusion did not follow. **A written-and-never-read account
is not a resource waiting for a use; it is a duplicate.** Beside every one of those credit sites sat
a `credit_material_yield` banking the *same* take's concrete hide, bone and fibre as `MaterialBatch`
es keyed by quality axes — and materials are the real resource model. The trade scalar was the
flattened copy that model made redundant, and it collapsed exactly the distinction the crafting arc
exists to preserve: a mammoth hide and a hare pelt are both `hide`, and they are not the same thing.
Giving it a sink would have built a market on the one representation that cannot tell them apart.

So `TRADE_GOODS`, `trade_goods_per_biomass` and every field that carried them are **retired**
(arc #527, same PR as the demolition above), and **the cargo this arc moves is food, fodder and
materials**. Five flora species paid the trade scalar and nothing else; cotton and flax already
carried fibre rows, and tobacco, tea and grapevine now carry materials of their own.

**What that leaves for #517 to build is a market over batches, not over a number** — a shipment
carries a *rating*, `balance_supply_networks` already pools per `(material id, band key)`, and
`LocalStore::drain_materials_into` already moves a haul batch by batch without averaging. The
substrate is in better shape for it than the scalar ever was.

> **The one surface the retirement really cost, and how it came back.** The crop picker's cash-crop
> row read `sowTradePayoff` / `cultivateTradePayoff`, so for a while a cotton row showed only its
> small rung-2 calories — a cash crop the player could not evaluate is a cash crop nobody sows.
> `forage::commit_material_payoff` replaced it with a **per-material** quote
> (`FloraShareInfo.sowMaterialPayoff` / `cultivateMaterialPayoff`, `[{ materialId, amount }]`), which
> is the same lesson as the retirement itself: the answer is *"0.29 fibre"*, not a number a market
> could total. Nothing may sum those rows back into one figure. See
> `.claude/rules/core_sim/flora.md` → "The crop picker's cash quote is PER MATERIAL".
>
> **The animal web had the identical hole, and it closed the same way.** A wolf paid the trade scalar
> and nothing else, so with it gone its compose sheet quoted no rate at all while the pelts still
> landed in the band's store. `HerdTelemetryState.materialPerBiomass` / `perWorkerMaterial` are the
> per-material rates that replaced it, and `LaborAssignment.materialYield` is what a resolved row
> actually credited — the same `MaterialPayoff` rows, the same three contracts (never summed, empty
> means no row, key always present). See `.claude/rules/core_sim/fauna.md` → "What a hunt is MADE OF
> is on the wire, per material".

> **The retired `FactionInventory` grant was a different thing that shared the name.** The shipped
> start profile used to hand the faction 40 `trade_goods`, which `apply_trade_goods_bonus` then
> drained into an openness field on a `TradeLink` that never existed — so the grant was deleted at
> startup, every game, for no effect. The grant went with that system; the band-local store went with
> the axis above.

---

## Q6 — Decay: three clocks, not one

Meeting someone is nearly irreversible, but nothing about the meeting stays fresh. Three different
things decay at three genuinely different speeds, and they are three separate levers:

| What decays | Speed | What it means |
|---|---|---|
| **Their location** | immediately on losing sight | you know where they *were*. Same as a herd. |
| **The connection's strength** | over turns without contact, down to zero | trust, route knowledge, currency of what you know. **At zero, nothing flows.** This is the clock that gates gameplay. |
| **The fact of them** | very slowly, but not never | eventually you have simply forgotten there was such a people |

The middle clock is the one to tune first; the outer two are a turn count and a very large turn
count.

---

## As-built — what exists, and what is being demolished (verified 2026-08-10)

### `TradeLink` is never inserted anywhere at runtime

Not once, in the whole workspace. Worldgen stopped attaching it (`plan_trade_route_data_model.md`
Step 1) and nothing ever replaced it. Every consumer therefore runs every turn over an empty set:

| Dead | Where |
|---|---|
| `trade_knowledge_diffusion` | `systems/trade.rs:142` — the openness/tariff/leak model, never executes a body |
| `publish_trade_telemetry` | `systems/trade.rs:239` — counters that count nothing |
| `apply_trade_route_visibility` | `visibility_systems.rs:898` — the FOW stub, waiting on a component nobody creates |
| trade counters | `metrics.rs:48` — always 0 |
| `TradeLinkState` / `tradeLinks` | `snapshot.fbs:2660` — always an empty wire section |
| `SECTION_TRADE_LINKS` | client native cache, for a section that never arrives |
| the map trade overlay | `map_preview.gd` — drawn only from a canned fixture |

A full vertical slice — sim, schema, wire, client cache, map overlay — inert for the whole life of
the band game.

### `LogisticsLink` is live but vestigial

Worldgen spawns one per adjacent tile pair — **~8,200 entities on an 80×52 map**
(`systems/worldgen.rs:454`) — and `simulate_logistics` (`systems/trade.rs:69`) sorts and walks all of
them every turn to move `tile.mass` down gradients. Nothing reads `mass` but the metrics total and
the published snapshot. It is the pre-band material-flow economy; band-local `stores` replaced it
without anyone deleting it.

### What is kept

- **The per-terrain cost table** (`terrain.rs`), all five columns — see §Q4.
- **The knowledge leak model** — a timer that fires more slowly the more closed you are, delivering a
  partial `KnowledgeFragment` with a fidelity. A real mechanism, mounted on the wrong object. The
  `KnowledgeFragment` type itself stays live regardless: migration already carries fragments between
  bands (`systems/population.rs`), which is the one knowledge-diffusion path that was never dead.
- **`TradeTelemetry` / `TradeDiffusionRecord` / `TradeDiffusionEvent`** — misleadingly named, but
  **not** dead: the migration path writes them with `via_migration: true`. Only the link-driven
  producer dies.

> **The client's link-drawing code is deleted, not kept.** An earlier draft proposed keeping it —
> the cache section, delta handling and the map overlay can already draw a line between two places,
> which is the surface a connection eventually wants. But it is fed by a wire section that will be
> empty for several slices yet, and dead client code is still dead code. What is worth keeping is the
> knowledge that the client *has* drawn links and can again; #232 rebuilds it against a network that
> exists.

### What is deleted

- The tile-pair topology (`LogisticsLink`, its ~8,200 entities, `simulate_logistics`).
- `TradeLink` and every consumer listed above, including the wire sections and the client cache
  section.
- The `tile.mass` economy.
- **The stale specs**: `docs/plan_trade_route_data_model.md` and `docs/plan_trade_fow_integration.md`.
  Both describe a world of settlements joined by explicit tile-adjacency routes; the game moved to
  bands with local stores and no founding step, and neither doc's mechanism ports. This document
  replaces them.

### Not touched

`simulate_materials` (`systems/trade.rs:44`) shares the module and owns `tile.temperature`, which
population cold-morale, sites and power genuinely read. **Its temperature half stays; its mass half
goes with the rest of the mass economy** — `Tile.mass` is written by that system and by
`simulate_logistics`, and read by nothing but the metrics total. Its wire slot
(`snapshot.fbs` `TileState.mass`) was already retired from the client stream in #386 and stays
`(deprecated)` rather than being freed: this repo is worked by concurrent sessions that append to
these tables, and a freed field id is exactly how two branches collide.

---

## Sequencing

1. **Design doc (this document).**
2. **Clear the ground (#537)** — remove the dead trade/logistics slice end to end, delete the two
   stale specs. Separate and first, so the substrate is not built beside its own ruins.
   **It grew a second half this document did not foresee**: the demolition exposed that the
   **`trade_goods` yield axis** was in the same condition one layer up — written on every harvest,
   read by nothing — while the `materials` list beside it already carried the same yield with the
   quality axes that make a mammoth hide and a hare pelt different things. Retiring it belonged with
   the demolition rather than after it, for the reason the step exists: a substrate built over a
   second dead currency would have had to be unbuilt later. See §Q5.
3. **The primitive (#538)** — connections: formation on contact, direction, strength, the three
   clocks, and the map-exchange grant of `Discovered`. **Its open item is connection strength** —
   what raises it, the curve, and what zero means to each rider — which is that slice's to settle
   rather than a question deferred elsewhere.
4. **Logistics + cargo (#517)** — logistics links on connections, the supply network re-founded on
   them, and goods moving between a splinter and its parent across distance. This is the slice that
   proves the substrate by consuming it.
5. **The route ladder (#532)** — the `route` branch, its standing-upkeep term, and #215's game trails.
6. **The overlay (#232)** — the logistics network drawn on the map.
7. **The remaining riders** — #530 (culture), #531 (knowledge).
8. **Blocked on #513, then:** the cross-faction riders — #458 (proximity trade), #512 (defection).
   By construction these should be small.

**#231 (Early Diplomacy)** was narrowed to match: the route-network half moved here, and what remains
there — treaties (#233) and cultural-reach victory metrics (#234) — is a *policy* layer that governs
connections this arc owns.

## Cross-cutting touchpoints

- **Fog of war** is now load-bearing for gameplay, not just presentation. Range *is* the visibility
  field, so anything that changes how visibility is computed changes who can meet whom.
- **#282 (movement ignores impassable terrain)** is what makes range terrain-shaped. Until it lands,
  contact is shaped by straight-line reveals.
- **`reach_tiles`** stops being a first-class lever and becomes "the distance at which a logistics
  link holds itself for free."
- **Checkpoints** — connections are persisted state with their own clocks; they are `SimState`, not
  derived. Contrast `SupplyNetworkMembership`, which is correctly derived and rebuilt each turn.
- **The event feed and the Telling** — meeting a people for the first time is a beat.

## Open items

- **How culture uses a connection** — #530. The question is not whether culture gates a connection;
  it does not, connections have no vocabulary about culture. It is what culture *does* with one it
  has: rate, direction, what it changes, and whether it needs the tie to be mutual.
- **How knowledge uses a connection** — #531, beyond the decision to keep the leak-timer model.
- **Standing upkeep on the route ladder** — #532, which owns the `route` branch and the standing-cost
  term the intensification engine does not yet have.
- **Whether a large group is detectable beyond anyone's range** — #533 (§Q1).
- **The exact behaviour of connection strength** — what raises it, what the curve is, and what
  reading zero means for each rider. Not filed separately: it is decided by the slice that builds the
  primitive (**#538**).

## See Also

- `docs/plan_band_fission.md` — the arc that makes this necessary: a splinter that walks away is
  severed the turn it founds.
- `docs/plan_settlement_population.md` — §Migration and the supply network §Q4 re-founds.
- `.claude/rules/core_sim/campaign.md` — the supply network as built.
- `.claude/rules/core_sim/intensification.md` — the ladder engine §Q4 borrows.
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` — the peoples who meet and change each
  other.
