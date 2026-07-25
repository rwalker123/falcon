---
paths:
  - "core_sim/src/{fauna,fauna_config,creatures_config}.rs"
  - "core_sim/src/data/{fauna_config,creatures}.json"
  - "core_sim/tests/fauna_*.rs"
---

<!-- Extracted verbatim from lines 44-44;55-56;1099-1588 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Fauna & wild game

## Config files

| File | Purpose |
|------|---------|
| `src/data/fauna_config.json` | Wild-game species table (display, size class, migratory flag, route length = anchor count, biomass, host biomes, + movement cadence `dwell_turns` / migratory `loiter_turns [min,max]` / `loiter_radius`, + **`fodder_per_biomass`** (Grazing 2b-i — graze the herd eats per unit biomass/turn; cached on `Herd` at spawn) + **`regrowth_rate`** (Grazing 2b-ii — per-species WILD breeding rate, `Option`, cached on `Herd`; rabbit/fowl 0.35, deer/boar 0.10, migratory 0.04 — replaces the single global `ecology.regrowth_rate` for wild herds; see "Phase 2b-ii") + **`taming_rate`** (intensification ladder slice 3c — a **per-species multiplier on the `animal:pastoral` rung's BUILD**, default **1.0**; the rung owns the taming mechanic, the species scales it (the `regrowth_rate`/`pastoral_gain` split again). It scales **`progress_per_turn` AND `decay_per_turn`** — a whole **timescale**, so the rung's 4:1 ratio is invariant: *slow to tame, slow to forget*. Roster: rabbit/fowl/crag_goat 1.0 (25 turns), boar 0.8 (~31), aurochs 0.5 (50), steppe_runner/marsh_grazer 0.2 (125); deer/mammoth omit it (`wild` ceiling — never tame). **Playtest dials.** Validated finite & `> 0`; resolved live by display name (`FaunaConfig::taming_rate_for`), *not* cached on `Herd`, so a retune reaches herds already on the map. See "The `Tame` verb") + **`husbandry_ceiling`** (Grazing 2d-δ — `wild`|`pastoral`|`pen`, default `pen`; how far up the ladder the species climbs — mammoth/deer `wild`, steppe_runner/marsh_grazer `pastoral`, boar/rabbit/fowl `pen`; cached on `Herd`, gates domestication + corral/extend; see "Phase 2d") + **`pastoral_density` / `pen_density`** (the per-species husbandry DENSITY (K) multiplier per rung, default **1.0** = neutral; domestication makes the LAND hold more animals, non-linearly by species — DISTINCT from the global r-gains, which scale the breeding rate not the ceiling. Roster: crag_goat/aurochs 2.0/5.0, boar 1.5/4.0, rabbit/fowl 1.1/1.5, steppe_runner/marsh_grazer 1.5/1.0 (pastoral only — pen inert), deer/mammoth omit both (wild → ×1). Applied at the one K seam `ecological_carrying_capacity` via `fauna::herd_density_gain`, resolved live by display name (`FaunaConfig::pen_density_for`/`pastoral_density_for`), *not* cached on `Herd`. **Playtest dials.** Validated finite & `>= 1.0` (a gain below 1 would make domestication reduce capacity). See "The husbandry yield ladder") + **`adjacent_water`** (the **shore predicate**, `none`\|`any`\|`salt`\|`fresh`, default **`none`** so every other species is byte-identical — a species that sets it may only spawn on a land tile that **borders open water of that kind** on one of its six hex sides (`fauna::adjacent_water_kinds`), the site rule filtering the short-range spawn's candidate list *before* the pick. **The kind is load-bearing:** `salt` = `WATER` **without** `FRESHWATER` (the ocean — the same rule `hydrology.rs`'s `TileWorld::is_ocean` states, in the same tag vocabulary), `fresh` = `WATER` **with** it (a lake, an `InlandSea`, a `NavigableRiver`), `any` = either. A blanket any-`WATER` test let a **Grey Seal colony haul out beside a one-hex freshwater lake** — seals are marine, so `seal` carries **`salt`**; the freshwater **`river_fish`** (Silt Catfish) carries **`any`**, which is byte-identical to its pre-split behaviour. Shipped on those two rows only; the seal pairs it with `host_biomes: ["boreal_arctic", "coastal_littoral"]` — **the cold half comes from `host_biomes`, NOT from a climate gate**: `climate::climate_band_for_temperature` is the single climate authority and a second one here would be a parallel authority that drifts from it. It **READS** the coastline geometry the worldgen stamped and never edits terrain. **Validated: `migratory: true` + any non-`none` value is REJECTED** — the migratory placement path (`suitable_tiles_for`/`build_migratory_route`) does not apply site rules, so the combination would be *silently ignored*; the unhandled state is made unrepresentable and loud instead. Measured on 6 seeds of the standard map: seals **2 → 14 colonies over the sweep** (0–1 → 0–4 per map), against 44–94 water-adjacent `boreal_arctic` tiles per map — see `core_sim/tests/fauna_coastal_habitat.rs`. **The guard is now habitat tiles PLUS colonies, because they catch different regressions**: the water-adjacent `boreal_arctic` count (floor 350 against a measured 419) is a raw terrain+adjacency reading, so it is causal and near-deterministic and is what a climate/moisture change actually moves, while the colony count (floor 4 against a measured 7) is a probabilistic roll under a map-wide cap and catches only a roster/site-rule regression. The colony floor was lowered 8 → 4 after the #332 crest-released rain shadow left habitat flat-or-up on all six seeds (413 → 419) while the placement roll reshuffled — it had been sitting on its own floor with zero headroom, so it fired on noise. **The seal pairs it with `route_len: [1, 1]`, and that is load-bearing, not incidental:** the site rule filters *placement* only — nothing in it stops `advance_herds` walking a colony inland on turn 1, and with the shipped `[1, 2]` it did (measured: a colony drifted `(24,21) → (23,22)`). A single anchor **is** the spawn tile, so `step_index` cycles `(0+1)%1 = 0` and `step_herd_toward` is handed the herd's own position — the colony is a fixed **haul-out**, which is what makes the shore invariant *structural* rather than placement-time. A rookery is a site the animals swim out from, not a herd that wanders overland. **Do not restore a multi-anchor route to a species carrying a site rule** without making roam site-aware, or the rule silently degrades to placement-only)) + per-biome spawn abundance + `hunt` / `follow` / `ecology` (regrowth + depensation collapse thresholds) / `immigration` (respawn) / `husbandry` (**the flow-based yield ladder**: **per-species managed `r`** (Grazing 2d — `pastoral_gain` 2.0 / `pen_gain` 4.0 scale each species' own wild `r`, capped at `husbandry_regrowth_cap` 1.0, retiring the flat `pastoral.ecology.r` 0.25 / `pen.ecology.r` 0.90 which now carry phase bands only) and `pen` (**`upkeep_per_biomass`** — the pen's **gross** feed rate; `× biomass` is the `penUpkeep` wire field, the SAME basis `corralYield` uses. The footprint's pasture and any hay **offset** it into the separate net `larder_upkeep` the keeper actually pays (exported render-ready as `penLarderBill`/`penHayFood`) — the *lever itself stays gross* — / `starve_shrink_rate`; `capacity_fraction` is **deleted** — a penned herd's `K` is its fenced-footprint graze flow) + the **neglect-escape shed rates** (`docs/plan_fauna_neglect_escape.md`) `pastoral_escape_fraction` **0.25** / `pen_escape_fraction` **0.10** (fraction of an under-contained herd's labor-capacity *overage* that sheds to the wild web per turn — pen slower, the fence buys time) / `escape_fraction_jitter` **0.25** (the ±band the seeded RNG applies; validated finite, `>= 0`, `pen < pastoral`), the **`Corral` policy**'s investment levers having **moved to `intensification_ladder.json`'s `animal:pen` rung** (the old `corralling_yield_fraction` → `yield_fraction_while_building` 0.50, `corral_build_progress_per_turn` → `progress_per_turn` 0.04); every rung pays MSY against its own ecology, see "The husbandry yield ladder" / "Phase 2d") + **`hunt_yield`** (the per-species HUNT-YIELD VECTOR, `docs/plan_hunt_yield_model.md` §3 — `{provisions_per_biomass?, trade_goods_per_biomass?}`, what a take of this species PAYS per unit of biomass. The **product** half of *yield = product × intensity*; how MUCH biomass is the policy's job (`hunt_policy_rate`), and the two axes are orthogonal. Each component omitted ⇒ the `hunt.*` global, so every species but the wolf is byte-identical; an explicit **`0.0` is a real value**, not 'unset' — it is how a wolf says *you do not eat me* (which is why the fields are `Option`). `edible`/`tradeable`/`yields_nothing` are **DERIVED** from the vector, never stored. Roster: only `wolf` declares it — `{0.0, 0.02}`, and **the 0.02 is ANCHORED**: it is exactly the retired commercial rate (`hunt.trade_goods_per_biomass` 0.005 × the retired `market.trade_goods_multiplier` 4.0), so a unit of wolf biomass is worth what a unit of commercially-hunted biomass was worth before the arc. Validated per PRESENT component: finite & `>= 0`. Resolved live by display name (`FaunaConfig::hunt_yield_for` — THE single seam; no call site may read the `hunt.*` globals for a take). Mirrors `flora_config`'s per-species `yield`, so the two food webs are the same shape. **Playtest dials.**) tuning (**the `market` block is RETIRED** — its 4× `trade_goods_multiplier` paid one rung a product bonus, re-welding product to policy; `Deplete` still out-earns `Sustain` on trade because it *takes* 2.5× more biomass) + **`graze`** (the pasture layer, Grazing Phase 2a — `capacity_by_biome` a **total** per-biome table (one row per `TerrainType`), `ecology` (`regrowth_rate` **0.40**, the fastest vegetal stock in the model), `reseed_floor_fraction` 0.02, **`overgraze_escapement_fraction` 0.25** (Grazing 2b-ii — grazing can't draw a patch below this, the constant-escapement floor that keeps the herd↔graze loop convergent); see "The Graze (Pasture) Layer" / "Phase 2b-ii"). **Validated** — `FaunaConfig::validate()` runs inside `from_json_str` (every load path), rejecting a pen that eats more than it yields, an inverted ladder, a dead ecology, or a **partial / all-zero / negative graze table** (a missing biome would silently read as an invisible zero-graze dead zone); a broken invariant is logged at **error** level (`fauna_config.invalid_rejected`) and the builtin is used |
| `src/data/creatures.json` | **The creatures roster** (Predators Phase 0; loader `creatures_config.rs`, env override `CREATURES_CONFIG_PATH`). Intrinsic `CombatStats` for **non-fauna** units — today one row, `"person"` (`attack 1, defense 1, range melee`). A human is not wildlife (not `fauna_config`) and its stats are not resolver tuning (not `combat_config`) — a combatant is *creature ⊕ equipment*, and this holds the base human creature. **Validated** inside `from_json_str` (the `"person"` row must exist; every row's `attack ≥ 0` finite, `defense > 0` finite); rejected at **error** level (`creatures_config.invalid_rejected`) → builtin. See "Combat & Casualties" |

## Fauna & Wild Game

Mobile animal **groups** (not individuals) graze-wander / migrate across the map
independent of the gather layer (see "Movement" below). One entity = one
band/warren/herd; `biomass` = group size.

**Species table** (`src/data/fauna_config.json`, loader `fauna_config.rs`): the
former hard-coded `HerdSpecies` enum is now a data-driven table. Each row has a
`display_name` (also the snapshot `species` string — it embeds the client icon
keyword, e.g. "Red Deer" → 🦌), `size_class` (`migratory`/`big`/`small`),
`migratory` flag, `route_len` `[min,max]` (= roaming range), `biomass` `[min,max]`
(group size), and `host_biomes` (a list of **`FoodModule` keys**, reusing
`classify_food_module`). Shipped roster (19 rows): **migratory** mammoth/steppe_runner/
marsh_grazer/reindeer/wild_horse (long routes); **big game** deer/boar/aurochs/seal/wild_elk
(2–3 tiles); **small game** rabbit/fowl/crag_goat/wild_sheep/alpine_ibex/gazelle/forest_grouse/
**river_fish**/**snow_hare** (~1 tile, stationary). The `pen`-ceiling **livestock** are
aurochs (🦬, wild r 0.09 → slow ranch cattle) on grass + woodland edge, **Crag Goats** (🐐,
wild r 0.22 → fast hardy hill stock) on highland/dry-upland, plus boar, rabbit, fowl,
wild_sheep and snow_hare.

**Regional signature — every biome offers distinct game, and every land biome offers a pen.**
Three roster rows close the last gaps:
- **`river_fish` ("Silt Catfish")** — the wet biomes' own game, hosting
  `riverine_delta`/`wetland_swamp`/`coastal_littoral`. Structurally the `seal` row: a
  non-grazing, non-migratory colony with `route_len [1,1]`, pinned to the shore by
  **`adjacent_water: "any"`** — freshwater game, so unlike the seal it wants a shore of *either* kind
  — so it needs no new Rust. A catfish inland is a bug.
- **`snow_hare` ("Snow Hare Warren")** — hosts **`boreal_arctic` alone** at a **`pen`** ceiling.
  Before it, `boreal_arctic` was the **only land biome with no `pen`-ceiling species at all**: the
  mammoth and elk there are `wild`, the reindeer only `pastoral`, so the intensification ladder's
  pen rung was flat unreachable from a northern start. The hare is what makes it reachable.

> **`host_biomes` names a MODULE, not a terrain — and `montane_highland` is the leaky one.**
> `CanyonBadlands` (an arid desert canyon) carries `ARID | HIGHLAND` and reaches the **fallback**
> arm of `classify_food_module_from_traits`, where the `HIGHLAND` test runs **before** the `ARID`
> test — so arid badlands classify as `montane_highland`. A live playtest duly found snow hares
> warrening in a desert canyon. There is no way to target one exact `TerrainType`, so the fix is to
> drop the module: `boreal_arctic` is an **explicit** arm (BorealTaiga | Tundra | PeriglacialSteppe
> | SeasonalSnowfield) and is exactly the hare's range. Nothing is lost but the occasional alpine
> warren, and no gameplay gap opens — `montane_highland` already has `crag_goat` at a `pen` ceiling.
> **The next cold-climate species pointed at `montane_highland` will hit this same trap.** Guarded
> by `fauna_wet_biome_roster::snow_hares_never_warren_the_highlands`, which asserts on the *spawned
> tile's module* — a count floor would not catch it, since re-adding the host raises the count.
- **`boar` gained `riverine_delta`**, giving the delta a big-game row beside the migratory marsh
  grazer and the small fowl/catfish.

Measured over `SWEEP_SEEDS` on the standard 80×52 earthlike map
(`core_sim/tests/fauna_wet_biome_roster.rs`, the guard against the silent never-spawn of an
unmatched `host_biomes` key): **20 catfish colonies** (1–6 per map, all water-adjacent), **37
snow-hare warrens** (4–8), **54 boar groups on delta tiles** (5–15) — each **0** before the
change. **The map-wide game cap is saturated** (122 herds per map against
`abundance.max_total_game` 120 + 2 migratory, identical pre- and post-change), so these three are
**displacing** other short-range game rather than adding to it — the roster shifts composition,
never density. Raise `max_total_game` if the intent is more game, not different game.

**Spawning** (`spawn_initial_herds`, `fauna.rs`): two passes into one
`HerdRegistry`.
1. **Migratory** — a few long-route walkers (`determine_herd_count`,
   `build_migratory_route`), species drawn from the config's `migratory` rows. **`host_biomes` is
   LIVE here** (it was previously ignored): a herd's loiter **anchors** are placed on tiles suitable
   for its species (`module_at ∈ host_biomes`), drawn from a regional home range
   (`MIGRATORY_HOME_RANGE_RADIUS`, spaced by `MIGRATORY_ANCHOR_MIN_SPACING`) around a random suitable
   seed tile and ordered into a walkable circuit by nearest-neighbour chaining — so the migration
   legs cross the less-suitable ground *between* the patches and the herd lives in its biome range
   across the map rather than clustered at the player start. A species whose host biomes the map
   lacks (empty `suitable`) **falls back to the start-anchored spiral** (`build_route`), so it still
   spawns somewhere.
2. **Short-range game** — iterate land tiles, classify each via
   `classify_food_module`, roll `abundance.per_biome[module]`; the map-wide winners
   are shuffled then greedily placed respecting `min_spacing` up to `max_total_game`
   (bounded entity count, spread across the map rather than clustered by scan
   order). Route via `build_short_route` (`route_len == 1` → single stationary
   tile → no client trail).
   **The species candidate list is SITE-FILTERED before the pick** (`spawn_game_group_at`): a
   candidate carrying a site rule — today only **`adjacent_water`** — is dropped unless the winner
   tile satisfies **its own** kind (the neighbour scan runs once per tile and yields
   `(has_salt, has_fresh)`, against which each candidate is tested separately), and only then is the
   single `rng.gen_range` draw made (the draw count is
   unchanged; only its bound moves). **An empty filtered list spawns nothing on that tile** — a cold
   *inland* tile whose only candidate is a marine forager correctly stays empty rather than seating a
   seal on the tundra. The immigration path (`repopulate_fauna`) shares the helper, so a respawn
   obeys the same rule. **The migratory pass does NOT apply site rules** — which is why
   `migratory + adjacent_water` is validate-rejected rather than silently ignored.

**Movement — graze-wander + loiter-then-migrate** (`advance_herds`, `docs/plan_wildlife_hunting_overlay.md`
"Herd Movement"). A `Herd` carries a **live `current_pos`** (walked ≤1 hex/turn, land-clamped,
wrap-aware — `position()` returns it) over its sparse `route` (now **anchors**, not a per-turn path),
plus a `RoamState` + `dwell_remaining`. One primitive — **graze-wander** (dwell `dwell_turns`, then
step ≤1 hex) — split by `size_class`:
- **Wild game** (`Big`/`Small`): permanent `GrazeWander` toward the current cluster anchor (cycling);
  ≈ half speed (a `route_len==1` group stays put). Catchable by an equal-speed party during a graze
  turn.
- **Migratory**: a `Loiter { turns_left }` ↔ `Migrate` state machine over the anchors. **Loiter** —
  graze-wander within `loiter_radius` of the current anchor for `loiter_turns` (sampled). **Migrate** —
  1 hex/turn toward the next anchor, **no dwell**, then loiter at the new anchor. Fixes the old bug
  where `Herd::advance()` teleported 4–12 tiles/turn along the sparse route.

**Herd movement is a rung primitive** (intensification ladder slice 3b — the **first** behavior
primitive the engine reads). `advance_herds` resolves the herd's rung (`fauna::herd_rung`: penned →
`animal:pen`, tamed → `animal:pastoral`, else `animal:wild`) and dispatches on its
`behavior.movement`, so §3's proximity spine **far → near → fixed** is *config*, not a branch on
`is_domesticated()`:
- **`roam`** (wild) — the graze-wander / loiter-migrate machine below, over its own full range.
- **`drift_to_owner`** (pastoral) — each turn the herd first tries **one step toward the nearest band
  of its owning faction** (`ResidentBand` only — a camp, not a passing expedition party). It
  **composes with, never replaces, the 2b-i graze-aware roam**: the candidates are exactly the roam's
  own acceptable steps (`acceptable_steps` — land, and not barren), ordered by **(hex distance to the
  nearest camp ASC, graze capacity DESC, y ASC, x ASC)** — a *preference ordering*, so there is **no
  drift-strength lever** to tune. Only a step that genuinely closes the distance counts as a drift;
  once the herd is at the camp or hemmed in, the turn **falls through to the normal roam**, so a tamed
  herd grazes *around* its people instead of freezing on their tile. The species' own `dwell_turns`
  cadence still applies (taming makes an animal *near*, not fast), and the herd never crosses barren
  ground to reach its owner. An unowned herd, or an owner with **no bands**, roams normally. The last
  two sort keys are load-bearing: two candidates can tie on distance *and* capacity, and a tie broken
  by anything incidental is the ~20% flake `GrazeRegistry::richest_patch` already cost us.
  - *Emergent tension, deliberately unsolved (playtest):* a herd that prefers proximity will settle for
    adequate-but-poorer pasture near camp, which lowers its range-derived `K` and shrinks it — real
    pastoral overgrazing. It cannot **strip** the range: 2b-ii's `overgraze_escapement_fraction` floor
    still binds, so the pasture recovers and the herd stabilizes smaller.
- **`fixed`** (pen) — pinned at `corralled_at`, no roam, no heading arrow.

Movement is **deterministic under rollback** — a per-herd/​per-turn `SmallRng` seeded from `map_seed ^
tick ^ HERD_MOVEMENT_SEED_SALT ^ fnv(herd.id)` (mirrors `repopulate_fauna`). Cadence levers are
per-species on `SpeciesDef` (`fauna_config.json`): `dwell_turns` (~1), `loiter_turns [min,max]`
(migratory, e.g. [12,24]), `loiter_radius` (~2), all `#[serde(default)]`. `advance_herds` resolves a
herd's levers via `FaunaConfig::species_by_display`. Movement is **independent of** `regrow_biomass`
(a loitering herd still grazes/regrows — ecology unchanged). Telemetry `next_position` is the next
`Migrate` hex (client heading arrow), `None` while loitering/grazing.

Abundance is a **tuning value, high to start** (design: game plentiful early,
thins under overhunting in later phases). Herds
flow to telemetry, the `HerdDensityMap`, and the snapshot (`HerdTelemetryState`,
which now also carries `size_class` + `huntable` so the client can offer the right
verbs — a free-form `species` string means new species need no schema change).

> ### Herd display telemetry is FOG-FILTERED; the herd registry is not
>
> **`WorldSnapshot.herds` publishes a herd only if the viewer faction's tile is `Active` for it this
> turn, or the viewer owns it** (`snapshot/subsistence.rs`, `HerdSnapshotInputs::herd_is_visible`).
> Before this the list went out unfiltered, so the client received the position, species, biomass,
> heading and full hunt-estimate table of every animal on the map — *wire-level fog was decorative
> for fauna*, and no client-side masking could fix a leak that had already crossed the socket.
>
> - **`Active`, not `Discovered`.** Ground you saw two hundred turns ago says nothing about where a
>   herd stands today, so `Discovered` would leak live positions across the whole explored map.
>   Remembering the **last seen** herd is a separate, deliberate feature (issue #214) layered on top
>   of this — never a weaker filter.
> - **Ownership is not a leak.** A tamed or penned herd is your property. Without that clause a
>   pastoral herd drifting a hex out of sight would take its `corralProgress` /
>   `penFedFraction` starving warning with it, and a pen alert that vanishes because of fog is a bug.
> - **The heading arrow is filtered separately** — `next_x`/`next_y` name a *second* tile, so a herd
>   visible at the edge of your sight would otherwise hand you a free look at where it is walking.
>   Withheld as the existing `-1` "no heading" sentinel.
> - **It fails CLOSED.** With no faction map for the viewer (before the first `calculate_visibility`,
>   or the turn after a rollback clears the ledger) **every** herd is hidden — which is exactly what
>   `visibility_raster_from_ledger` does in the same state (an all-unexplored, black raster). The two
>   read the same ledger for the same faction, so they cannot disagree about a herd on dark ground.
> - **Only the VIEW is filtered.** `WorldSnapshot.herd_registry` — the authoritative rollback record,
>   and `export_map`'s ground truth — carries every live herd. Restore rebuilds `HerdTelemetry` from
>   the registry (never from `snapshot.herds`), so rollback is untouched. **Consequence:** an
>   `export_map` JSON's `snapshot.herds` is now the *player's view*; read `snapshot.herd_registry`
>   for the full roster.
> - **A hunted herd stays visible for free** — `calculate_visibility` reveals `worked_source_sight_range`
>   around each worked Hunt herd's tile, so a herd your band is working is always `Active`.
> - **Known gap:** a hunting **expedition**'s target herd is *not* revealed (an expedition is
>   `Without<Expedition>`-excluded from live faction reveal; its discoveries are comm-range gated), so
>   a distant target is not published. The in-flight readout is unaffected — `expeditionEtaTurns` /
>   `expeditionProjectedDelivery` ride the *cohort*, not the herd.
> - **Per-faction snapshots are still a future arc.** The capture has ONE `ViewerFaction`, so this
>   closes the leak for the single-viewer stream the game ships today; true competitive MP needs a
>   per-faction capture.
>
> Guarded by `core_sim/src/snapshot/mod.rs` unit tests (unseen / owned-in-the-dark / empty-ledger /
> heading suppression) and `integration_tests/tests/fauna_fog.rs`, which asserts on the **encoded
> FlatBuffers bytes** the client actually receives, decoded through the client's own accessor chain.

**Hunt (one-shot)** — the `hunt_fauna <faction> <herd_id> [band_entity_bits]`
command (`handle_hunt_fauna`, `server.rs`; full plumbing in `command.proto` /
`commands.rs` / `command_text.rs`) attaches a `FaunaPursuit` component (`components.rs`)
to a band (auto-picked when no band id is given). Each turn `advance_fauna_pursuits`
(`systems.rs`, `TurnStage::Population`) re-reads the herd's **live** position (herds
already moved in the earlier `Logistics` stage), steps the band up to
`hunt.pursuit_tiles_per_turn` toward it, and on closing to `hunt.pursuit_radius`
(=1, Chebyshev) resolves a one-shot take: `hunt.take_from(biomass)` biomass →
provisions/trade (`hunt.*_per_biomass`), drawn from the group and added to
`FactionInventory`, then removes the component. An elusive herd is abandoned after
`hunt.max_pursuit_turns`. Config lives in the `hunt` block of `fauna_config.json`.

**Follow (persistent, per policy)** — `follow_herd <faction> <herd_id> [policy]
[band_entity_bits]` attaches a `FaunaPursuit { mode: Follow { policy } }`
(`FollowPolicy` ∈ Sustain | Surplus | Deplete | Eradicate). The same `advance_fauna_pursuits`
system keeps the band within `pursuit_radius` of the moving group and, once adjacent,
**auto-hunts each turn per policy** instead of removing the component. The policy is a
free string parsed via `FollowPolicy::from_str`, so a new policy needs no schema/proto change. Each
turn it also grants a small non-food benefit — a `FogRevealLedger` tracking pulse
(`follow.reveal_radius`/`reveal_duration_turns`) + `follow.morale_gain`. The old one-shot teleport
follow (and its `apply_herd_rewards`/`apply_herd_knowledge` helpers) is retired.

> #### The hunt policy axis: FOUR ASCENDING MULTIPLES OF MSY + a kill-credit bank (slice 8b)
>
> `fauna::hunt_policy_rate` (the per-turn take **rate**) + `hunt_credit_ceiling` (what the herd's banked
> credit can afford this turn) are the one source. Each policy earns a multiple of the sustainable yield
> (`MSY = r·K/4`, `peak_regrowth`), banked into `Herd::hunt_credit`, and a whole animal is killed only
> once the bank clears one `body_mass`:
>
> | policy | rate | herd |
> |---|---|---|
> | **Sustain** | `sustainable_yield` = `min(MSY, regen(B))` | stable, settles at `K/2` |
> | **Surplus** | `hunt.surplus_multiplier × MSY` (**1.5**) | slowly declines (reversible) |
> | **Deplete** | `hunt.deplete_multiplier × MSY` (**2.5**) | declines to **extinction** |
> | **Eradicate** | the whole standing stock (bypasses the bank) | gone |
> | **Tame / Corral** | Sustain's rate × the rung's `yield_fraction_while_building` | a dip on a sustainable draw |
>
> **The third rung was `Market`; it is now `Deplete`** (wire key `"deplete"`,
> `hunt.deplete_multiplier`). *Every* harvesting policy sells the source's trade goods, so naming one
> rung for the market described nothing that distinguished it — the axis is a **harvest-pressure
> ladder** (Sustain → Surplus → Deplete → Eradicate) and the rung is named for its pressure, not its
> product. Behaviour-preserving rename; see `docs/plan_hunt_yield_model.md` §2. `FollowPolicy` is
> shared with the Forage arm, so the plant web's third rung renamed with it. The `fauna_config.json`
> `market` block has since been **deleted** (see the intensity note below); `labor_config.json`'s
> `forage.market` keeps its old key name pending a plant-side pass.
>
> **Monotone in take BY CONSTRUCTION.** Surplus/Deplete are multiples of the *same* MSY base, so
> `Sustain ≤ 1× < 1.5× < 2.5× ≤ B` at every biomass and every species — *"each option takes more than
> the previous."* `FaunaConfig::validate` pins `1 < surplus_multiplier < deplete_multiplier` (one
> rejection test per bound: `validate_rejects_a_{surplus_multiplier_at_or_below_one,
> deplete_multiplier_at_or_below_surplus}`), and
> `fauna_deplete::hunt_policy_takes_are_strictly_ordered_at_every_biomass` sweeps the ordering across
> B × {fast, slow}. **The regression guard against reintroducing a skim** — do not weaken it.
>
> **The kill-credit bank is what makes multiples-of-MSY produce whole lumpy animals.** For 7 of 9
> species MSY < `body_mass`, so a *per-turn ceiling* of that rate would `floor` to **zero forever** (the
> flow trap that made those species unhuntable under the old `r·K/4` Sustain and `1.6 × MSY` Surplus).
> Banked, the fractional rate **accumulates** until a body is affordable — a mammoth is a wait-then-one
> pulse (`body/MSY` turns per kill), a rabbit takes *several per turn* (the credit ceiling never clamps
> it to one). The bank **carries across policy changes** (earned regrowth toward the next animal;
> switching Sustain↔Deplete must not reset it) and is **capped at the standing biomass** (never bank
> credit for animals that do not exist — that would release a burst on recovery). Measured
> (`fauna_deplete::the_kill_credit_pays_multiples_for_fast_game_and_a_pulse_for_big_game`): a full **Rabbit**
> (MSY 350, body 2) Sustain-takes ~200 rabbits/turn tapering to `K/2`; a full **Mammoth** (MSY 120, body
> 800) waits ~7 turns then takes one under Sustain, ~4 under Surplus, ~3 under Deplete.
>
> **Sustain's rate is sized against the PRE-regrowth biomass** (`Herd::biomass_before_regrowth`, captured
> at the top of `regrow_biomass`). The take runs *after* Logistics regrowth, so evaluating
> `sustainable_yield` at the grown stock takes slightly more than the herd grew (`regen(B_post) >
> regen(B_pre)`) and slowly **leaks a below-`K/2` herd down**. Reading the pre-regrowth biomass makes
> Sustain take exactly one turn's growth below `K/2` (the herd **holds/recovers** — pinned by
> `fauna_deplete::a_below_half_k_herd_under_sustain_recovers_never_declines`) and a full MSY above it (a
> **gentle** decline to `K/2`, no escapement burst).
>
> **Extinction is REAL and on-map.** Constant catch above MSY has no equilibrium: Surplus declines a
> herd (reversible if switched back), **Deplete drives it extinct** (`deplete_hunt_drives_collapse`). The
> resident band (`systems::hunt_take`) and the hunting expedition (`expedition_take_biomass`) share the
> same rate + bank, so a herd hunted by either reads one accumulator.
>
> **Shared herd (chosen handling, reported):** credit advances **per hunt resolution** — once per
> resident `hunt_take` and once per expedition take. The intended invariant is **one hunter per herd**
> (a resident band leashes to a nearby herd; expeditions target distant migratory ones), where it is
> exactly correct. Two *concurrent* hunters on one herd would each bank their rate (more pressure, a
> faster harvest — realistic and non-crashing, since the bank is capped at the stock and kills at the
> animal count). Not a per-worker-share split — that would be overbuilding for a case the labor system
> does not normally produce.
>
> **THE AXIS IS PURE INTENSITY — the product comes from the SPECIES** (`docs/plan_hunt_yield_model.md`
> §3, issue #337). A rung decides *how much biomass* comes home; `SpeciesDef::hunt_yield` (resolved by
> `FaunaConfig::hunt_yield_for`) decides *what that biomass is worth*, and **every** rung is paid the
> same vector through `HuntYield::apply` — one call, both products, so no site can convert the meat
> and forget the pelt. Two consequences:
> - **`market.trade_goods_multiplier` is RETIRED** with its whole block. A 4× trade bonus on the third
>   rung alone re-welded product to policy. `Deplete` still out-earns `Sustain` on trade *because it
>   takes 2.5× more biomass* — the ladder doing the work. A deliberate rebalance: a Deplete hunt's
>   trade/biomass drops 4×, and Sustain/Surplus/Eradicate gain a trade component they never had.
> - **Eradicate pays a WINDFALL**, and `FollowPolicy::delivers_food` is **retired** (not adjusted).
>   Its premise — *"denial carries nothing home"* — is what the arc reverses: denial is the END STATE
>   (the species is gone, for you and everyone else), not a promise the carcasses were thrown away.
>   Its readers now ask the **species** (`HuntYield::edible`); the two *intensity* facts it smuggled
>   (Eradicate ignores the pack's carry cap, and it has no escapement floor to spend) are stated as
>   `matches!(policy, Eradicate)` at their two sites in `systems::expeditions`.
>
> **Quantisation never divides by a food number it has not established is positive.** The old
> "flooring in provisions and in biomass agree, a positive linear factor cancels" note is **false** for
> an inedible species: `provisions_per_biomass == 0` makes `floor(food_ceiling / food_per_animal)` a
> `0/0`. Operationally, the animal count is a **ratio**, so it is taken on
> `SourceYieldForecast::ratio_axis()` — the first component with a positive rate (`Provisions` for
> every edible species, bit-identical to the pre-arc arithmetic; `TradeGoods` for a wolf) — and
> `YieldPair::rescaled_to` carries that one count into the other currency. Correspondingly **"does this
> source quantise?" is `!body_mass_yield.is_zero()`**, not `body_mass_yield.provisions > 0`: whole
> animals are a property of the animal, not of what it is worth to you.
>
> **The forecast is a `YieldPair` per rung, and `forecast == actual` holds PER COMPONENT** — see
> `.claude/rules/core_sim/yield-forecast.md` for the wire fields, the plant side's
> `trade_goods = 0.0` gap, and why `huntPerWorkerProvisions` must not clamp a per-herd preview.
>
> **The picker.** The flags gate the yield *components*, not the buttons: a wolf shows the full ladder
> and is paid in pelts, because each rung is a meaningful *rate* at which to collect pelts. The only
> pruning rule is `fauna::hunt_policies_for` — the ONE seam the `assign_labor` validator and the
> snapshot's `huntPolicyCeilings`/`huntTripEstimates` export share — and it prunes only a
> `yields_nothing` species (worth neither meat nor pelt) down to `Eradicate` alone. No shipped species
> hits that branch; it is pinned on a synthetic config
> (`core_sim/tests/hunt_yield_vector.rs`).
>
> **Retired levers** (all stay retired): `follow.surplus_multiplier`, `surplus.take_fraction`,
> `market.take_fraction`, `hunt.take_fraction` / `min_take` / `take_from`; `ecology.
> surplus_escapement_fraction` and `fauna::hunt_policy_floor` were deleted with the ordered-floors cut.
> `ecology.collapse_fraction` is once again **only** the Allee/depensation threshold (it briefly doubled
> as Deplete's floor). The whole **`market` block** joined them (above), and `fauna::hunt_provisions`
> — the single *global* biomass→provisions conversion — is retired in favour of `HuntYield::apply`.
> Config: `hunt.{surplus_multiplier, deplete_multiplier}` + the per-species `hunt_yield` vector.

> #### Herding is standing labor, and it scales with the HERD (slice 8)
>
> `fauna::herders_needed` — `ceil((biomass / body_mass) / animals_per_herder)` — is owed **every turn**
> by a pastoral or penned herd, **including wait turns** when it cannot spare an animal. *Just because
> you aren't killing an animal doesn't mean you aren't tending them, keeping them from running off,
> repairing fences.* Before this a pen of 2 and a pen of 200 needed the same single keeper; only the
> feed scaled.
>
> - **Downward hysteresis — staff it once and it holds.** The bare `ceil` is stateless, so a
>   Sustain-hunted slow herd sitting near an `animals_per_herder` multiple (a Wild Aurochs near 12
>   head) **flickers 1↔2 every turn** as the lumpy whole-animal kill breathes its biomass ±1 animal
>   across the boundary — and because the `herded_fraction` decay lags a turn, the player is told
>   "staff all 1", then "staff all 2", satisfies neither, and slips the tameness. So the requirement is
>   now a **persisted, deadband-stabilized `Herd::herders_needed`** (round-tripped through `HerdState`
>   like `corral_progress`), updated every turn by `Herd::stabilize_herders_needed` in
>   `advance_husbandry`: **up immediately** when the raw need rises (under-herding is harmful), **down
>   only once the herd falls below `(current − 1)·animals_per_herder − band`** where `band =
>   animals_per_herder × husbandry.herders_hysteresis_fraction` (**0.25**, `fauna_config.json`, a
>   playtest dial; `0` restores the raw flicker). A herd bumped to 2 stays at 2 through a one-animal dip
>   and drops only on a genuine multi-band fall — wild = 0 unchanged (a wild herd isn't yours to
>   maintain). `herd_herders_needed` reads this stabilized field (falling back to the raw ceil only for
>   a not-yet-stabilized managed herd — the turn it is tamed, or a test fixture), so **every** consumer
>   (`herded_fraction` decay, `managed_crew_needed`, the `herdersNeeded` snapshot field) is steady; the
>   wire field is unchanged, just no longer churning.
> - **Heads, not tonnes.** The denominator is per-**animal** (`SpeciesDef::animals_per_herder`,
>   per-species: fowl/rabbit 200, crag_goat 80, boar 15, steppe_runner/marsh_grazer 15, aurochs 12;
>   deer/mammoth are `wild`-ceiling and omit it). A shepherd minds ~300 sheep, a cowherd ~80 cattle —
>   you watch individuals, and a heavier beast is not proportionally more work. A per-*biomass* dial
>   says "one herder per 100 fowl but one per 2 boar" and invents a 45-herder steppe megaherd that is a
>   pure artifact of the unit (4,560 biomass of Steppe Runner is **38 animals** ⇒ ~3 herders).
> - **ONE need, not two — but "one need" means one CREW, not one formula.** The herders mind the herd
>   *and* butcher it, so a managed rung reports **one** number and staffs **one** team
>   (`systems::labor::managed_crew_needed`) — but that team must be big enough for **both** jobs, which
>   scale on **different units**: herding is per **head** (one herder minds 12 aurochs), hauling is per
>   **biomass** (one hauler carries 40). A shepherd minds ~300 sheep and could not carry three. So
>   `workersNeeded = max(herders_needed, hunt_haul_workers)` — `+` would be two teams; `max` is
>   one crew covering its busiest job. **Neither term dominates across the roster** (measured, settled
>   radius-1 pens): small-bodied species are **herder-bound** (Wild Fowl 9 herders vs 5 haulers; Rabbit
>   5 vs 4), big-bodied ones are **haul-bound** (Crag Goats 2 vs 7; Boar 1 vs 3; Aurochs 2 vs 3). Do not
>   "simplify" the `max()` away.
>
> - **An INVESTMENT policy (Tame/Corral) sizes the herder term ownership-INDEPENDENTLY**
>   (`fauna::would_be_herders_needed`, the taming-startup-lag fix). `herd_herders_needed` is
>   ownership-gated to `0` until Population's `accrue_domestication` records `owner`, so on the turn a
>   Tame assignment *starts* the crew used to collapse to the tiny Tame-dip haul count — "1 of N working"
>   on a full crew. `would_be_herders_needed` returns the biomass-derived crew for a species that *can* be
>   tamed regardless of recorded ownership (`0` only for a `wild` ceiling), preferring the stabilized
>   `herders_needed` so an already-managed herd is identical to `herd_herders_needed` (no re-flicker). The
>   labor arm's `herded_fraction`/`workers_needed` **and** the assign-time seed (`forecast_source_yield`,
>   which now folds the herder term into a hunt row's `workers_needed`) both apply it for an investment
>   policy, `herd_herders_needed` for an extractive one — a wild Sustain-hunted herd must stay
>   ownership-gated to `0` or it would read `herded_fraction < 1` and falsely shed. One definition, shared
>   with the `herdersNeededIfManaged` wire field. Pinned by
>   `labor::a_wild_herd_being_tamed_reports_its_full_crew_without_the_ownership_lag`.
>
> - **The haul term is the STEADY carry crew, not this turn's `carried`** (`fauna::hunt_haul_workers`).
>   `workers_needed`'s hauling component is the crew that carries home the **peak per-turn animal drop**
>   — `ceil((floor(rate/body) + 1)·body / per_worker)`, off the policy's **steady** `hunt_policy_rate`
>   (not the credit-inclusive `hunt_credit_ceiling` burst) — the **same** count the client's compose
>   panel `_max_useful_workers` caps at. It is deliberately **not** `workers_needed_for_take(take.carried,
>   …)`: a slow breeder whose MSY < `body_mass` (a Wild Aurochs) drops **zero** animals on a wait turn
>   while its kill-credit banks, so inverting `carried` collapses `workers_needed` to `0` — and, for a
>   managed herd, to the bare herder count via `max()`. That made the panel contradict itself:
>   `workersNeeded: 1` beside a 50%-`wastedYield` at one worker — *drop workers* and *add workers* on the
>   same row, with half an aurochs rotting. Sizing the crew off the steady rate makes it **stable across
>   wait and kill turns** (it can't flicker with the pulse) and **equal to `wasted_yield`'s answer**:
>   `workers > workers_needed` ⇒ overstaffed, `wasted_yield > 0` ⇒ understaffed, and the two never
>   disagree. Both hunt sites (wild/pastoral and pen) and the assign-time forecast seed
>   (`fauna::forecast_source_yield`, off `SourceYieldForecast::ceiling_for`) read this one helper. **Wild
>   hunting** gets the same steady haul crew (`herders_needed == 0`, so `max()` collapses to it) — so a
>   wild herd's `workers_needed` is the client max-useful too. **Forage is untouched** — a gather is
>   continuous (`body_mass_yield == 0`, no lumpiness), so it keeps the ordinary `workers_needed_for_take`
>   overstaffing inversion.
> - **Wild hunting is untouched, deliberately.** No maintenance (the herd isn't yours), but it keeps
>   its carry cap. **The models differ because the products differ: hunt = reach + carry; harvest =
>   maintain + take.**
> - **Understaffing SHEDS ANIMALS — it does not touch tameness (neglect-escape arc,
>   `docs/plan_fauna_neglect_escape.md`).** The tameness-bleed (`decay_under_herded`, and with it
>   `decay_domestication`) is **DELETED**: `domestication_progress` is now permanent stock capital,
>   monotone-up (earned via `Tame`), never bled by a neglected turn. Instead an under-contained managed
>   herd (`is_corralled() || owner.is_some()`) **sheds whole animals over its labor capacity** into a
>   nearby wild herd of the same species. The overage is reconstructed from the **real staffing** —
>   `capacity_animals = herded_fraction × herders_needed × animals_per_herder` (the product recovers
>   `assigned` exactly), `overage = max(0, current − capacity)` — **not** the `(1 − herded_fraction) ×
>   current` shorthand, which over-estimates hard at a `ceil` boundary (101 @ aph 50 staffed at 2: true
>   overage 1, shorthand 33.7 — a PR #329 review fix); `herders_needed` reads through
>   `herd_herders_needed` so a not-yet-stabilized `0` can't collapse capacity. It sheds at the per-rung
>   `husbandry.{pastoral,pen}_escape_fraction × (1 + seeded jitter)`, whole animals with a min-1 floor
>   (`shed_uncontained_animals` → `place_shed_animals`, `advance_husbandry`). It is
>   **self-limiting** (a fraction of the *overage*, so the herd converges to its labor capacity and stops)
>   and **visible** (biomass, not an invisible stat). The binary corral escape is gone — total
>   abandonment is just the `herded_fraction == 0` limit of the same shed. **Total abandonment BLEEDS
>   OUT and DESPAWNS, symmetric between the webs.** A herd with **zero** herders last turn has its
>   regrowth **suppressed** — `regrow_biomass` reads the same one-turn-lag `herded_fraction == NOT_HERDED`
>   signal and zeroes growth, the pastoral twin of the untended pen's `pen_fed_fraction = NOT_FED`
>   (and `advance_herds`' dispersal-despawn exempts owned herds too, so it survives to bleed out) — so it
>   keeps shedding until it can no longer shed a whole animal (`biomass < body_mass`), at which point the
>   **emptied managed entity is despawned** (`advance_husbandry` Phase 3). Its flock is already in the wild
>   web via the shed; **tameness is never reset** — it leaves *with* the animals (each shed batch is a wild
>   herd at domestication 0), so there is **no ownerless-but-tame husk**. `owner`/pen state are **never
>   cleared at a floor** (clearing `owner` would drop the herd out of the managed set and stop the shed,
>   stranding the very husk this removes); the herd stays owned/corralled and bleeds all the way down. A
>   pen is announced lost (`announce_pen_lost` — the feed line only; the fence dies with the entity, no
>   reset). **PARTIAL** neglect (`herded_fraction > 0`) keeps normal regrowth and settles at a stable
>   smaller **tame** herd, owner intact — a **binary abandoned/not gate, never a scaling**. The pen's
>   **feed** path (`starve_underfed_pen`) floors a *fed* pen and keeps it, unchanged (a starving pen has a
>   keeper, so it never reaches the bleed-out); `herders_needed`/hysteresis unchanged. Levers
>   `husbandry.{pastoral_escape_fraction 0.25, pen_escape_fraction 0.10, escape_fraction_jitter 0.25}`
>   (validated finite, `>= 0`, `pen < pastoral`). Pinned by
>   `fauna_husbandry`'s `an_over_stocked_managed_herd_converges_to_its_labor_capacity`,
>   `a_partially_herded_pastoral_herd_stays_tame_with_regrowth`,
>   `a_fully_abandoned_pastoral_herd_goes_feral_without_decaying_its_taming`, `neglect_never_un_tames_a_herd`,
>   `a_pen_sheds_slower_than_a_pastoral_herd`, `total_abandonment_sheds_the_flock_and_loses_the_pen`,
>   `shed_animals_appear_in_the_wild_web`, `the_shed_is_deterministic`.
> - **The under-herded EDGE NOTICE (slice 2).** When a managed herd *becomes* under-contained (a shed
>   occurs — its herders can't hold all its animals), `advance_husbandry` pushes a
>   **`CommandEventKind::HerdUnderHerded`** (`"herd_under_herded"`) feed line to the owner, naming the
>   species — *"The Rabbit Warren has too few herders — animals are drifting off"* — with detail
>   `status=under_herded herded=<f> needed=<n> herd=<id> x=<x> y=<y>`. **Edge-gated on the persisted
>   `Herd::under_herded` bool**: it fires **once** on the `false → true` transition (not every turn it
>   stays under-contained), clears the turn the herd recovers (fully staffed / within capacity), and
>   re-fires on a later relapse. Distinct from the pen-*lost* (`announce_pen_lost`, Corral) and
>   pen-*starving* (`starve_underfed_pen`, Corral) edges — this is the herder-shortfall edge and it
>   fires for pastoral herds too. Unlike the transient `pen_starving`, `under_herded` is
>   **snapshot-persisted** (rewinds with rollback) so a restore does not spuriously re-announce. Pinned
>   by `fauna_husbandry`'s `the_under_herded_notice_{fires_once_on_becoming_under_contained,
>   re_fires_after_recovery_then_relapse}` + `the_persisted_under_herded_flag_suppresses_a_re_fire`,
>   `snapshot::mod`'s herd-state identity round-trip, and
>   `integration_tests/fauna_rollback::under_herded_edge_state_rewinds_on_rollback`. **Slice 3 (client,
>   not built):** rendering this line + the panel warning icon.

**Retired: single-task model → labor allocation (Early-Game Labor slice 3a).** The
one-task-per-band model (`reassign_band` + `HarvestAssignment`/`ScoutAssignment`/`FaunaPursuit`
and their systems `advance_harvest_assignments`/`advance_scout_assignments`/`advance_fauna_pursuits`,
plus the `scout`/`forage`/`hunt_fauna`/`follow_herd` command handlers) is **removed**. A band is now a
**labor pool**: a `LaborAllocation` component (`components.rs`) partitions its whole working-age workers
(`available_workers(working)` = `floor`) across `LaborTarget`s — `Forage { tile, policy }`, `Hunt { fauna_id,
policy }`, `Scout`, `Warrior` — with the invariant `Σ workers ≤ available`. `advance_labor_allocation`
(`systems.rs`, Population stage, replacing the three retired systems) resolves per-worker yields each
turn: Forage = `workers × per_worker_yield × seasonal_weight` from an in-range `FoodModuleTag` tile;
Hunt take = `min(workers × per_worker_biomass_capacity, policy_ceiling)` (reusing the per-policy ecology
ceilings — Sustain under-hunting lets a herd grow), tracking a roaming herd out to `band_work_range +
hunt_leash_tiles` before the assignment lapses (feed entry). Scout extends the band's live sight range
in `calculate_visibility` by posting forward-observer vantages (`scout.vantage_distance(scouts)` out
in all 6 hex directions, LOS revealed from each — re-marked Active every turn while scouts are
staffed, scaling with head-count); Warrior is inert until the predator slice. `move_band <faction> <band> <x> <y>` sets a `BandTravel` component that
`advance_band_movement` steps at `band_move_tiles_per_turn`/turn. `assign_labor` sets one target's
worker count (0 unassigns; clamps to free headroom); **`cancel_order <faction_id> [band_entity_bits]
[all|work|roles]`** clears the assignments its **scope** names — `all` (the default when the token is
omitted, and the historical behaviour) clears every assignment **and** stops movement (fully idle),
`work` unassigns only the worked Forage/Hunt sources, `roles` clears only the Scout/Warrior standing
roles. **The two narrow scopes never touch `BandTravel`** — moving is not working, so unassigning a
band's sources must not strand it mid-journey — and the rejection is **scope-aware**: a band with
sources but no roles accepts `work` and refuses `roles` ("…has no standing roles to clear."), rather
than claiming it is idle. The scope rides the wire as `CancelOrderCommand.scope` (absent == `all`);
an **unrecognised scope token is a hard text-parse error** (failing closed — silently defaulting to
`all` would mass-unassign a band that asked for `work`), while an unrecognised *proto* string decodes
to `all`. The Band panel's per-section clears are what the scope exists for. The snapshot exports `laborAssignments`/`idleWorkers`/`workingAge`, and still
summarizes `activity` (target-kind with most workers) + `huntMode` (largest Hunt's policy) for the
pre-3b client. Husbandry re-homes here — but **Sustain no longer tames** (slice 3a): a **`Tame`** Hunt
fills the meter, while any stewardship policy on a Thriving source earns the knowledge that source's
current **rung** teaches (slice 4 — see "The knowledge pattern"). The
**investment policies** `Cultivate` (Forage-only) / `Corral` (Hunt-only) also resolve here — a reduced
take while the improvement is prepared, then the managed yield; see "Cultivation" / "Corral". Config:
`labor_config.json`. Client allocation panel is PR 3b.

**Ecology — critical-depensation collapse (Phase D)** — `advance_herds` applies one
turn of `net_biomass_delta` (`fauna.rs`) toward each group's per-species carrying
capacity (`Herd.carrying_capacity` = the species' `biomass[1]`). The curve is **not**
plain logistic: above the Allee threshold (`ecology.collapse_fraction * cap`) the group
regrows logistically at `ecology.regrowth_rate`; **below** it the group is non-viable and
declines by `ecology.collapse_rate` per turn — an **irreversible crash to local
extinction even if hunting stops** (the overhunting point of no return). `advance_herds`
**despawns** any group below the viability floor (`ecology.extinction_floor * cap`), so a
collapse reaches zero in finite turns. So a hunt/follow draws a group down in
`Population`; it regrows (or, past the threshold, collapses) in the next turn's
`Logistics`; sustained overhunting drives it extinct permanently.

**Ecology phase + domestication hook** — each `Herd` carries a coarse `EcologyPhase`
(`Thriving` / `Stressed` / `Collapsing`), recomputed every turn from biomass vs
`ecology.stressed_fraction`/`collapse_fraction` (`classify_ecology_phase`) and exported in
the snapshot (`HerdTelemetryState.ecologyPhase`) so the client warns the player before a
group is doomed. This derived state also **gates domestication** (below): husbandry
progress accrues only while a `Thriving` herd is Sustain-hunted (a Sustain Hunt assignment).

**Immigration** — `repopulate_fauna` (`fauna.rs`, `TurnStage::Logistics` right after
`advance_herds`) gives a low per-turn chance (`immigration.chance_per_turn`) to respawn one
short-range game group up to `abundance.max_total_game`, sampling up to
`immigration.max_attempts` random land tiles that host game and respect `min_spacing`. This
keeps an overhunted map slowly replenishing (early forager play stays game-rich) without
undoing a local extinction (the crashed group is gone; a *new* group may immigrate
elsewhere). Seeded per-turn from `map_seed ^ tick ^ salt` (deterministic under rollback).

**Domestication / husbandry (Phase E)** — the pastoral counter-force to depletion. A
`Herd` carries `domestication_progress` (0–1, `1.0` = domesticated) and `owner:
Option<FactionId>`, exported as `HerdTelemetryState.domestication`.
- *Accrual — the **`Tame`** verb, not a side effect of hunting*: in `advance_labor_allocation`
  (Population), a Hunt assignment carrying **`FollowPolicy::Tame`** on a **Thriving** herd adds the
  `animal:pastoral` rung's `progress_per_turn` × the species' `taming_rate` for the acting faction
  (sets `owner` on first accrual; only the owner accrues; gated on **Herding** + the species'
  husbandry ceiling). At `1.0` the herd domesticates. **A `Sustain` hunt tames nothing** — it only
  *teaches* the faction Herding. That de-conflation is slice 3a; see "The `Tame` verb".
- *Decay*: `advance_husbandry` (`fauna.rs`, `TurnStage::Logistics` after `advance_herds` — runs
  *before* the same turn's accrual, so a `Tame`-worked herd nets `progress_per_turn − decay_per_turn`
  and an abandoned one only decays by the `animal:pastoral` rung's `decay_per_turn`, clearing `owner`
  at 0).
- *Yield*: **none here — passive-free pastoral is RETIRED** (intensification ladder slice 3b, §3:
  every rung is worker-driven). A tamed herd used to pay its owner the pastoral MSY **with no worker
  at all**, split evenly across the owner's bands; `advance_husbandry` now pays **nothing** and a
  pastoral herd yields **only** through a normal `Hunt` assignment, exactly like a wild one. The
  taming payoff is **yield per worker**: `herd_ecology` puts a tamed herd on the pastoral ecology
  (`r` = wild × `pastoral_gain` 2.0), so the *same* hunters take ~2× the sustainable food from the
  same `K` (measured: `fauna_husbandry::the_husbandry_ladder_is_a_per_species_growth_rate_ladder`).
  That also deletes the "you are not paid twice" hazard structurally — with no second payment to
  stack, the `Corral` dip is a real cost again — so the `Herd::worked_this_turn` flag that guarded it
  is **gone**.
- *Collapse immunity*: `regrow_biomass` uses plain `logistic_regrowth` (never the collapse
  branch) for a domesticated herd — a managed group recovers and never crashes.
- *No early claim*: the `domesticate <faction_id> <herd_id>` command, its `husbandry.claim_threshold`
  lever and `Herd::claim_domestication` are **deleted** (slice 3a). Snapping progress to `1.0` let the
  player skip the investment, which is the entire decision — the plant side removed its twin for the
  same reason. **Proto field 30 is reserved and must never be reused.** The `tame` command that
  replaced it *sets the `Tame` policy*; it claims nothing.
- *Practice teaches the next rung*: working this herd under a **stewardship** policy earns the faction
  the knowledge its **current rung** teaches — **Herding** while it is wild, **Penning** once it is
  pastoral (slice 4, §4). See "The knowledge pattern".
- `HerdRegistry::domesticated_count(faction)` is the seam the future `SedentarizationScore`
  reads for its "domestication progress" input.

