# core_sim - Simulation Engine

Bevy-based ECS headless simulation that resolves turns via `run_turn`. Systems execute in order: materials → logistics → population → power → tick increment → snapshot capture.

## Quick Reference

```bash
# Build
cargo build -p core_sim

# Test
cargo test -p core_sim

# Benchmark
cargo bench -p core_sim --bench turn_bench

# Run server
cargo run -p core_sim --bin server
```

## Configuration Files

| File | Purpose |
|------|---------|
| `src/data/simulation_config.json` | Grid size, environmental tuning, trade/power/corruption multipliers, TCP bind addresses (see `SIM_PORT_BASE` under Environment Overrides for per-checkout port shifting) |
| `src/data/map_presets.json` | World generation tuning parameters |
| `src/data/start_profiles.json` | Campaign initialization (units, inventory, knowledge tags) |
| `src/data/victory_config.json` | Victory mode thresholds and `continue_after_win` flag |
| `src/data/turn_pipeline_config.json` | Per-phase clamps for logistics, trade, population, power |
| `src/data/knowledge_ledger_config.json` | Leak timers, suspicion decay, countermeasure scaling |
| `src/data/espionage_agents.json` | Agent archetypes and generator templates |
| `src/data/espionage_missions.json` | Mission templates with success/fidelity bands |
| `src/data/espionage_config.json` | Security posture penalties, probe resolution tuning |
| `src/data/crisis_archetypes.json` | Plague, Replicator, AI Sovereign definitions |
| `src/data/crisis_modifiers.json` | Shared modifier definitions with decay models |
| `src/data/crisis_telemetry_config.json` | Gauge thresholds, EMA alpha, trend windows |
| `src/data/great_discovery_definitions.json` | First-wave constellation catalog |
| `src/data/culture_corruption_config.json` | Culture propagation, divergence thresholds, corruption penalties |
| `src/data/influencer_config.json` | Roster caps, decay factors, scope thresholds |
| `src/data/snapshot_overlays_config.json` | Overlay normalization weights |
| `src/data/visibility_config.json` | Fog of War sight ranges, decay, terrain modifiers |
| `src/data/labor_config.json` | Early-Game Labor allocation: `band_work_range` (true odd-r **hex-distance** radius of in-range sources — `grid_utils::hex_distance_wrapped`, wrap-aware), `worked_source_sight_range` (fog reveal range around each worked Forage tile / Hunt herd tile in `calculate_visibility`), `hunt_leash_tiles` (extra leashed-follow reach for Hunt), `band_move_tiles_per_turn` (`move_band` speed), `forage.per_worker_yield`, `hunt.per_worker_biomass_capacity` (per-hunter take cap; biomass→provisions/trade reuses `fauna_config.hunt.*_per_biomass`), `scout.vantage_distance_base`/`vantage_distance_per_scout`/`vantage_distance_max`/`vantage_range` (staffed scouts post forward-observer vantages in all 6 hex directions and reveal LOS from each in `calculate_visibility`, so they see *around* obstacles) |
| `src/data/fauna_config.json` | Wild-game species table (display, size class, migratory flag, route length = anchor count, biomass, host biomes, + movement cadence `dwell_turns` / migratory `loiter_turns [min,max]` / `loiter_radius`) + per-biome spawn abundance + `hunt` / `follow` / `ecology` (regrowth + depensation collapse thresholds) / `immigration` (respawn) / `husbandry` (domestication accrual/decay/claim/yield) / `market` (commercial-hunt take + trade multiplier) tuning |
| `src/data/sedentarization_config.json` | Sedentarization Score tuning: soft/hard prompt thresholds, EMA `smoothing`, input `weights` (domestication/surplus/resource_density/population), and saturation `references` |
| `src/data/demographics_config.json` | Demographic population tuning: `initial_distribution` (children/working/elders split), `consumption` (per-capita food draw + per-bracket factors), `startup` (`food_reserve_days` seeded into each band's larder + `well_fed_morale_bonus`), `births` (rate/surplus_bonus; morale-independent), `maturation_rate`/`aging_rate`/`elder_mortality_rate`, `scarcity` (starvation + per-bracket vulnerability, deficit-capped), `cold` (temperature-death) |
| `src/data/supply_network_config.json` | Supply-network tuning: `reach_tiles` (connection radius), `throughput_per_turn` (max goods moved per node/turn), `friction` (fraction lost in transit), `min_transfer` (dead-band) |
| `src/data/wellbeing_config.json` | Civilization Wellbeing tuning: `discontent` (`content_morale`/`floor_morale` productivity curve, `grievance_gain`/`grievance_decay`/`trapped_multiplier`), `productivity` (`floor_mult`, `discontent_weight`), `migration` (own morale-scaled onset: `morale_threshold`, `max_rate`, `base_reach`, `attractive_morale`, `min_morale_gap`, `dependent_weight`) |
| `src/data/sites_config.json` | Wondrous Sites catalog (`catalog`: per-`site_id` `category`/`display_name`/`glyph`/`placement_rule`/`discovery_reward.morale_bonus`) + `placement` rules (per-rule `max_sites`, `min_spacing`, and the union of rule inputs: `min_relief`, `max_habitability_pressure`, `min_food_weight`). Loader `sites_config.rs`, env override `SITES_CONFIG_PATH`. Not wired into the `reload_config` hot-reload path (mirrors `fauna_config.json`) |
| `src/data/expedition_config.json` | Expedition tuning. Scout: `max_party_size`, `comm_range_tiles` (discovery-report range), `comm_range_tech_factor` (stubbed 1.0 tech hook), `observe_sight_range` (per-turn LOS radius, matches band base sight), `provision_draw_per_worker_per_tile` (launch larder draw = party × distance × this), `provision_upkeep_per_worker` (per-turn drain = party × this, scouts only). Hunt (PR 2) `hunt` block: `per_worker_carry` (carry cap = party × this), `reach_tiles` (how close to the herd to take), `drop_off_within_tiles` (herd-near-band delivery gate), `sustain_floor_fraction` (Sustain takes down only to this × carrying_capacity), `min_deliver_fraction` (herd-near-band early delivery needs carried ≥ this × cap). The take **policy** is **not** a config lever — it is chosen at launch via the optional trailing arg of `send_hunt_expedition` (default `FollowPolicy::Sustain`). Scout replenish `replenish` block: `low_turns` (top up below party × upkeep × this), `reach_tiles`. Loader `expedition_config.rs`, env override `EXPEDITION_CONFIG_PATH`. Not on the `reload_config` hot-reload path (mirrors `sites_config.json`) |

Hot reload: `reload_config [path]` or `reload_config turn|overlay|crisis_archetypes|crisis_modifiers|visibility [path]`

### Environment Overrides

| Var | Effect |
|-----|--------|
| `SIM_CONFIG_PATH` | Load an alternate `simulation_config.json` instead of the baked-in default. |
| `SIM_PORT_BASE` | Shift all four TCP listen ports to a fresh block so multiple checkouts/worktrees don't collide. The base maps to `snapshot=base+0`, `command=base+1`, `snapshot_flat=base+2`, `log=base+3`; `base=41000` reproduces the historical fixed ports (41000–41003). Applied in `load_simulation_config_from_env` (`resources.rs`) over whatever the config JSON specifies, preserving each bind's host. A non-numeric or out-of-range value (needs `1 ≤ base` and `base+3 ≤ 65535`) is warned and ignored rather than fatal. `scripts/run_stack.sh` derives a per-checkout base automatically and forwards the matching `STREAM_PORT`/`COMMAND_PORT`/`LOG_PORT` to the Godot client; `cargo xtask command …` still defaults to `127.0.0.1:41001`, so pass `--port <base+1>` when targeting a shifted server. |

Each `*_CONFIG_PATH` var in the tables above overrides its specific config file; those are noted per-row.

---

## World Generation Pipeline

Implements the procedural map pipeline producing terrain, coasts, rivers/lakes, climate bands, resources, and wildlife spawners. Player-facing framing: manual §3a World Bootstrapping, §3b Terrain Palette.

### Pipeline Stages
1. **Macro landmask** - Continent seeds via weighted BFS to reach `target_land_pct`
2. **Tectonics** - Drift vectors, collision belts, fault seams, volcanic arcs, dome plateaus → mountain mask
3. **Polar microplates** - Subdivide polar tiles, converging vectors raise fold strength
4. **Heightfield** - Multi-octave height raster with erosion smoothing → `elevation_m`
5. **Coastal smoothing** - Blend shoreline tiles via 3×3 blur
6. **Ocean/coasts** - Distance-transform bands: Shelf → Slope → Deep Ocean; inland seas. See "Continental shelf width" below — the shelf is a size-scaled, sub-tile-capable band, not a fixed ring.
7. **Climate** - Assign `climate_band` using latitude + elevation + moisture
8. **Hydrology** - D8 flow direction, river polylines, `Floodplain`/`FreshwaterMarsh` marking. `RiverDelta` is stamped **only here**, at the last land tile of each river that ends in a standing water body — the ocean *or* an inland sea/lake (lacustrine deltas). The mouth tile must border that water; the biome picker and tag solver never create deltas (those would scatter them with no river attached). Delta tiles are protected from the tag solver's reduction passes so genuine river mouths survive.
9. **Biomes** - Stamp `TerrainType` via `terrain_for_position` with micro-variant jitters
10. **Moisture transport** - Humidity blending with wind-driven rain-shadow pass
11. **Resources** - Surface deposits biased by `TerrainDefinition.resource_bias`
12. **Wildlife** - Seed herd spawners, migratory paths, `game_density` raster
13. **Starting areas** - Place candidates respecting World Viability Contract

### Data Shapes
- **Rasters**: `elevation_m: i16`, `climate_band: u8`, `flow_dir: u8`, `flow_accum: u16`, `game_density: u8`
- **Vectors**: `rivers: [RiverSegment]` with polylines and edge tracking
- **Tiles**: `hydrology_id`, `substrate_material`, `terrain_type`, `TerrainTags`

### Tile Temperature — latitude + elevation climate model
`Tile.temperature` is a real climate, **not** the old `(x+y)%4` element checkerboard. The single
source is `systems::climate_temperature(y, grid_height, above_sea_normalized, element, &ClimateConfig)`:

```
temperature = latitude_base(y, H) − elevation_lapse(elev) + element_jitter(element)
```

- **`latitude_base`** — equator-in-the-**middle**: `lat_frac = |y − (H−1)/2| / ((H−1)/2)` ∈ [0,1]
  (0 = center/equator, 1 = top *or* bottom edge/pole), `equator_temp − lat_frac·(equator_temp −
  polar_temp)`. Symmetric: the top and bottom edges are equally cold; the temperate band (~18°)
  lands at mid-latitudes (lat_frac ≈ 0.34).
- **`elevation_lapse`** — `ElevationField::above_sea_normalized` (height above sea remapped to [0,1])
  × `elevation_lapse_span`; higher ground is colder.
- **`element_jitter`** — the element's `thermal_bias` × `element_jitter_scale`, kept small (~±1.5°)
  so it is local texture, not the driver.

Config lives in the `climate` block of `simulation_config.json` (`equator_temp` 30.0, `polar_temp`
-5.0, `elevation_lapse_span` 12.0, `element_jitter_scale` 0.25). Worldgen seeds each tile at exactly
this value **after** elevation exists (a `climate_elevation` field with sea level attached), and
`simulate_materials` relaxes each turn toward the *same* recomputed climate temperature (no longer
the element target), so turn 1 has no jump. On an 80×52 map: equator ≈ 29–30°, mid-latitude ≈ 18°,
pole = −5° at sea level (mountains up to 12° colder).

### Map Presets (`map_presets.json`)
Presets control: `seed_policy`, `dimensions`, `sea_level`, `continent_scale`, `mountain_scale`, `moisture_scale`, `river_density`, `terrain_tag_targets`, `locked_terrain_tags`, `biome_weights`.

The active preset's `sea_level` is carried on the `ElevationField` resource (`heightfield.rs`, via `with_sea_level`; falls back to `DEFAULT_SEA_LEVEL` = 0.6) and exported in the snapshot as `ElevationOverlay.seaLevel` — **pre-normalized to the overlay's [minValue, maxValue] sample scale** (`snapshot.rs` `elevation_overlay_from_field`) so the Godot client can compare it directly against decoded samples for its relative-height / LOS readout.

**Continental shelf width** (`classify_bands` + `effective_shelf_width`, `mapgen.rs`; `ShelfConfig`, `map_preset.rs`): `ContinentalShelf` is the ocean band within a computed distance of the coast (slope collapses to `DeepOcean` downstream, so only the shelf boundary affects ocean composition). The width is a knob, not a fixed ring:
- `width_tiles` (default 2) — legacy absolute band width. Used only when `width_frac` is unset (e.g. `polar_contrast`), preserving historical behavior.
- `width_frac` + `width_exp` (earthlike) — the width scales with map size as `width_frac * min(w, h)^width_exp` and is **not floored to a whole tile**. A sub-1.0 width is rendered as a *partial* coastal ring: whole rings up to `floor(width)` are all shelf, and the next ring is shelf on only `frac` of its tiles (deterministic per-tile hash in `classify_bands`). This matters because at coarse resolution Earth's shelf is thinner than one tile. `width_exp < 1` counteracts the extra coastline that larger maps accumulate, keeping the shelf a **size-invariant fraction** of the ocean (earthlike targets ~5-8% of open ocean, verified flat from 80×52 to 256×192 by `integration_tests/tests/shelf_ratio.rs`). This is a pure ocean-tile reclassification — it does **not** touch the land mask, so mountains/rivers/land ratio are unchanged.

  Deferred / future options (not implemented): a true *depth-based* shelf would need real offshore bathymetry (today ocean elevation is fractal noise with no coast-relative deepening); and if the narrower shelf's reduced `CoastalUpwelling` forage frontage matters for gameplay, lock the `Coastal` tag to stamp compensating `TidalFlat` (the tag solver's coastal pass). Neither shipped preset locks `Coastal` today.

**Elevation ↔ biome coupling** (`restamp_elevation`, `mapgen.rs`): mountain biomes come from the tectonic mountain mask + relief, so the elevation field is tied to that same signal to keep them consistent (mountains genuinely tall — see the `mountain_tiles_out_top_lowland_tiles` regression test). Every mountain-mask tile is floored into `[elevation_base, 1.0]`, ordered by relief and scaled by per-type prominence; non-mountain land is compressed into `[sea_level, elevation_base]`. Tunables live in each preset's `mountains` block: `elevation_base`, `fold_prominence`, `fault_prominence`, `volcanic_prominence`, `dome_prominence`, `belt_texture` (small spine-vs-edge elevation texture added on top of the relief floor; bounded so it never reorders relief bands). The non-mountain `elev ≥ high_dry_elevation → CanyonBadlands` / `elev ≥ high_wet_elevation → RollingHills` cutoffs (`terrain.rs`) live in `terrain_classifier` and default to the top of the compressed lowland band.

**Highland biomes are mask-driven, never noise-driven.** `classify_terrain` (the base climate classifier) does NOT pick AlpineMountain/HighPlateau/CanyonBadlands/etc. — it has no real elevation, so it used to invent them from a tile hash and scatter flat "mountains." Mountain biomes now come only from the tectonic mask (`select_mountain_terrain`) + the real-elevation `terrain.rs` branches. `apply_belt_relief` (`mapgen.rs`) scales belt-tile relief by belt strength (`mountains.relief_belt_gain`, default 1.2) so belt cores clear the AlpineMountain relief threshold (`terrain_classifier.alpine_relief_threshold`, default 1.45) and taper to plateaus/hills — genuine Alpine spines that are also tall. Polar rows are skipped (they keep their low-relief-basin tuning). Regression guards: `mountain_tiles_out_top_lowland_tiles`, `alpine_biome_tiles_are_tall`.

**Number of ranges** is emergent tectonics: land connected-components → plates (area buckets, ≤4/continent) → fold belts form only where two plates' drift *converges* (`dot <= mountains.belt_convergence`, `derive_mountain_mask`). Drift is radial-outward so most boundaries diverge; raising `belt_convergence` toward 0 (earthlike default **0.25**; polar_contrast keeps the tighter **−0.1** to preserve its low-relief-basin contrast) lets more boundaries become ranges. Range count also scales strongly with **map size** — a full 256×192 map has 30+ ranges, an 80×52 "Standard" ~4–13, a 56×36 "Tiny" ~2–6.

**Tag Budget Solver**: After biome stamping, iterates locked tag families (water → wetlands → fertile → coastal → highland → polar → arid → volcanic → hazardous) nudging tiles until coverage falls within `tolerance`.

---

## Ecosystem Food Modules

Pre-agricultural survival modules mapping to worldgen tags, snapshot payloads, and client affordances.

| Module | Primary Inputs | Storage Hooks |
|--------|----------------|---------------|
| Coastal Littoral | Shellfish, tidal fish, kelp | Fish racks, shell middens |
| Riverine / Delta | Freshwater fish, cattail gardens | Smokehouses, tuber pits |
| Savanna Grassland | Herd shadowing, wild yams | Jerky racks, nut caches |
| Temperate Forest | Oak/chestnut groves, berries | Clay-lined nut pits |
| Boreal / Arctic | River/ice fishing, seals | Permafrost pits, pemmican |
| Montane / Highland | Alpine tubers, marmots | Sun-dried meat, stone caches |
| Wetland / Swamp | Cattail rhizomes, amphibians | Mud storage, smoke curing |
| Semi-Arid Scrub | Drought tubers, cactus fruits | Roasting pits, seed cakes |

**Implementation**: `FoodModuleTag` components with tile entity, module id, seasonal weight. `ForageSiteLedger` tracks capacity. Commands: `gather_roots`, `harvest_shellfish`, `dry_fish`, `follow_herd`.

> **Wild game is an overlay, not a tile flag.** Game used to overwrite a food
> tile's gather kind with `FoodSiteKind::GameTrail` (×0.75 weight), but food-site
> curation sorts by weight **descending** so game trails never survived (0 on live
> maps). That upgrade + the `wild_game_*` config + `GameTrail` are **retired**;
> wild game now lives in the fauna herd layer (below), so a tile offers **both**
> gathering and hunting. See "Fauna & Wild Game" and
> `docs/plan_wildlife_hunting_overlay.md`.

---

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
`classify_food_module`). Shipped species: migratory mammoth/steppe_runner/
marsh_grazer (long routes); big game deer/boar (2–3 tiles); small game rabbit/fowl
(~1 tile, stationary).

**Spawning** (`spawn_initial_herds`, `fauna.rs`): two passes into one
`HerdRegistry`.
1. **Migratory** — a few start-anchored long-route walkers (`determine_herd_count`,
   `build_route`), species drawn from the config's `migratory` rows.
2. **Short-range game** — iterate land tiles, classify each via
   `classify_food_module`, roll `abundance.per_biome[module]`; the map-wide winners
   are shuffled then greedily placed respecting `min_spacing` up to `max_total_game`
   (bounded entity count, spread across the map rather than clustered by scan
   order). Route via `build_short_route` (`route_len == 1` → single stationary
   tile → no client trail).

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
(`FollowPolicy` ∈ Sustain | Surplus | Market | Eradicate). The same `advance_fauna_pursuits`
system keeps the band within `pursuit_radius` of the moving group and, once adjacent,
**auto-hunts each turn per policy** instead of removing the component — a commercial
spectrum: Sustain takes one turn's net regrowth (`net_biomass_delta(..).max(0.0)`, group
~stable; a collapsing group yields nothing), Surplus takes that × `follow.surplus_multiplier`
(slow decline), **Market** takes `market.take_fraction × biomass` (a large commercial share →
fast decline into the Phase D collapse) and sells it at `market.trade_goods_multiplier`× the
normal trade-goods rate, Eradicate takes `hunt.take_from` (drives extinction). The policy is a
free string parsed via `FollowPolicy::from_str`, so Market needs no schema/proto change. Each
turn it also grants a small non-food benefit — a `FogRevealLedger` tracking pulse
(`follow.reveal_radius`/`reveal_duration_turns`) + `follow.morale_gain`. Config lives in the
`follow` and `market` blocks of `fauna_config.json`. The old one-shot teleport follow (and its
`apply_herd_rewards`/`apply_herd_knowledge` helpers) is retired.

**Retired: single-task model → labor allocation (Early-Game Labor slice 3a).** The
one-task-per-band model (`reassign_band` + `HarvestAssignment`/`ScoutAssignment`/`FaunaPursuit`
and their systems `advance_harvest_assignments`/`advance_scout_assignments`/`advance_fauna_pursuits`,
plus the `scout`/`forage`/`hunt_fauna`/`follow_herd` command handlers) is **removed**. A band is now a
**labor pool**: a `LaborAllocation` component (`components.rs`) partitions its whole working-age workers
(`available_workers(working)` = `floor`) across `LaborTarget`s — `Forage { tile }`, `Hunt { fauna_id,
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
worker count (0 unassigns; clamps to free headroom); `cancel_order` clears all assignments + stops
movement (fully idle). The snapshot exports `laborAssignments`/`idleWorkers`/`workingAge`, and still
summarizes `activity` (target-kind with most workers) + `huntMode` (largest Hunt's policy) for the
pre-3b client. Husbandry re-homes here: a Sustain Hunt on a Thriving herd accrues domestication. Config:
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
- *Emergent accrual*: in `advance_labor_allocation` (Population), a **Sustain** Hunt assignment on a
  **Thriving** herd adds `husbandry.progress_per_turn` for the acting faction (sets
  `owner` on first accrual; only the owner accrues). At `1.0` the herd auto-domesticates.
- *Decay + yield*: `advance_husbandry` (`fauna.rs`, `TurnStage::Logistics` after
  `advance_herds` — runs *before* the same turn's accrual, so a Sustain-followed herd nets
  `progress_per_turn − decay_per_turn` and an untended one only decays by
  `husbandry.decay_per_turn`, clearing `owner` at 0). A **domesticated** herd pays its owner
  `biomass × husbandry.provisions_per_biomass` provisions each turn (via `add_stockpile`,
  **without** depleting biomass — sustainable managed harvest).
- *Collapse immunity*: `regrow_biomass` uses plain `logistic_regrowth` (never the collapse
  branch) for a domesticated herd — a managed group recovers and never crashes.
- *Explicit claim*: the `domesticate <faction_id> <herd_id>` command (`handle_domesticate`,
  full proto/runtime/text/server plumbing) lets the owner claim a herd **early** once
  `domestication_progress ≥ husbandry.claim_threshold` (snaps progress to 1.0); rejected for a
  non-owner or an under-threshold herd. The emergent Sustain-follow is how progress is built.
- `HerdRegistry::domesticated_count(faction)` is the seam the future `SedentarizationScore`
  (`TASKS.md`) reads for its "domestication progress" input.

Ecology/husbandry tunables live in the `ecology` (`regrowth_rate`, `collapse_fraction`,
`collapse_rate`, `stressed_fraction`, `extinction_floor`), `immigration`, and `husbandry`
(`progress_per_turn`, `decay_per_turn`, `claim_threshold`, `provisions_per_biomass`) blocks
of `fauna_config.json`.

> `FaunaPursuit` is **not** snapshot-persisted (unlike `HarvestAssignment`): a
> `rollback` mid-pursuit cleanly cancels the in-flight hunt (the rehydrated cohort
> simply lacks the component). Pursuits are short-lived; revisit if needed. Domestication
> state lives on the `Herd` (in `HerdRegistry`), alongside `biomass`.

> **The authoritative `HerdRegistry` *is* rollback-persisted** (as of the intensification
> arc's first slice, `docs/plan_intensification.md` §0-i). Each live `Herd` — identity,
> movement (`route`/`step_index`/`current_pos`/`dwell_remaining`/`roam`/`next_pos`), **and** its
> depletable-ecology subset (`biomass`/`carrying_capacity`/`ecology_phase`/
> `domestication_progress`/`owner`) — round-trips through a serde `HerdState` (the ecology subset
> embedded as a shared `EcologyState`) captured into `WorldSnapshot.herd_registry` and rebuilt on
> restore via `HerdRegistry::update_from_states`, following the `GenerationRegistry` round-trip
> convention. This closes a **latent bug**: only the lossy display `HerdTelemetry`
> (`WorldSnapshot.herds`) used to be captured, so herd biomass/position silently kept their
> post-rollback values. Restore rebuilds the derived `HerdDensityMap` + `HerdTelemetry` (as
> `advance_herds` does post-loop) so nothing is stale for a turn. `HerdState` is the sim side; the
> FlatBuffers client stream is untouched (it keeps using the display telemetry). **`EcologyState`
> is the shared depletable-ecology record** the forage-depletion slice (§0-ii) reuses for its
> per-tile `ForageState`.

Market hunting shipped as the `Market` follow policy; `SedentarizationScore` shipped (see
"Sedentarization" under Campaign Loop). Still deferred (`docs/plan_wildlife_hunting_overlay.md`):
the `Camp` entity + corrals, and wiring the sedentarization hard prompt to an actual
`found_settlement`. The tile-based `HuntGame` handler stays neutralized (its client button no
longer surfaces).

---

## Wondrous Sites

Data-driven catalog of notable map features tiles can hold, hidden under fog until a faction's
vision reveals them, then recorded in a per-faction registry. v1 = sim + snapshot producer (the
client markers/readout are a separate slice). Authoritative design:
`docs/plan_exploration_and_sites.md` §3. Catalog `src/data/sites_config.json`, loader
`sites_config.rs` (mirrors `fauna_config.rs`: baked-in builtin + `SITES_CONFIG_PATH` override).

**Catalog** (`SitesConfig`): `catalog` keyed by `site_id` — each `SiteDef` carries `category`
(`landmark`/`settle_site`, free-form so new categories need no schema change), `display_name`,
`glyph`, `placement_rule`, and a `discovery_reward` (v1: a single `morale_bonus` lever, a struct
so future per-category rewards slot in). `placement` holds the per-rule tuning (`max_sites`,
`min_spacing`, and the union of rule inputs). Shipped: `great_peak` (landmark, rule
`prominent_mountain`) + `verdant_basin` (settle_site, rule `fertile_settle`).

**Placement** (`sites::place_wondrous_sites`, Startup after `spawn_initial_world` +
`apply_tag_budget_solver`): for each catalog entry, run its `placement_rule` against the tiles and
stamp a `SiteTag { site_id }` on the chosen tile entities, capped at `max_sites`, spaced by
`min_spacing` (Chebyshev), one site per tile. Deterministic under the map seed (`WorldGenSeed ^
SITE_PLACEMENT_SEED_SALT`; idempotent — a world that already carries `SiteTag`s is skipped).
- `prominent_mountain`: tiles whose `Tile.mountain` relief `>= min_relief`, tallest-first (ties by
  position), greedily placed.
- `fertile_settle`: tiles whose habitability pressure (`tile_morale_pressure` total — the same
  helper the snapshot's `habitability` uses) `<= max_habitability_pressure` **and** that carry a
  `FoodModuleTag` with `seasonal_weight >= min_food_weight`, shuffled (seeded) then greedily placed.
- On an 80×52 earthlike map both rules hit their `max_sites` cap (5 `great_peak` + 5 `verdant_basin`).

**Discovery** (`sites::discover_sites`, `TurnStage::Visibility` **after** `calculate_visibility`):
sites are rare, so it iterates the (few) `Query<(&Tile, &SiteTag)>` × the `VisibilityLedger`'s
factions. If a site's tile is `Discovered`/`Active` (ever seen, `is_discovered`) for faction F and
`(F, pos)` not already in `DiscoveredSites` → record it, apply the reward, push a feed entry.
Newly-found sites are processed in a stable `(faction, y, x, site_id)` order so the feed/reward are
deterministic.
- **Reward (v1):** `discovery_reward.morale_bonus` added once to each of F's `PopulationCohort`
  bands (clamped 0..1). Config-driven — the extension hook for settlement/resource/diplomacy rewards.
- **Command feed:** `CommandEventKind::SiteDiscovered` (`site_discovered`) with label = site display
  name, detail = `category=<c> at (x,y)`.

**Registry + persistence.** `DiscoveredSites` resource: per-faction `Vec<DiscoveredSiteRecord {
pos, site_id }>` + a `seen` set backing an O(1) `contains(faction, pos)`. **Snapshot-persisted** —
`restore_world_from_snapshot` rebuilds it from the snapshot (like the fog reset) so a rollback
neither un-discovers a site nor retains discoveries made after the restore point. (The `SiteTag`s
themselves are worldgen tile tags and, like `FoodModuleTag`, are **not** rebuilt on rollback — the
registry is the durable record.)

**Snapshot (per-faction, no tile leak).** Undiscovered sites are **never** in `TileState`, so the
fog can't leak them. Instead the capture exports a per-faction `discoveredSites`
(`snapshot_discovered_sites`, resolving each record's `category`/`display_name`/`glyph` from the
catalog), mirroring `SedentarizationState`. Wire shape:
`discoveredSites:[DiscoveredSitesState{ faction:uint, sites:[DiscoveredSite{ x, y, site_id,
category, display_name, glyph }] }]` on both `WorldSnapshot` and `WorldDelta` (`snapshot.fbs`,
`sim_schema`). See "Visibility Systems" for the discovery hook in the turn flow.

---

## Scouting & Hunting Expeditions

A **detached traveling party** a faction outfits and drives out — to **explore** (scout) or to
**follow a migratory herd and deliver food** (hunt). One traveling-party system, two verbs. v1 =
sim + snapshot producer (client marker/outfit/recall UI is a separate slice). Authoritative design:
`docs/plan_exploration_and_sites.md` §2 (scout) + §2b (hunt) + the Implementation-model subsection.
Config `src/data/expedition_config.json`, loader `expedition_config.rs` (`EXPEDITION_CONFIG_PATH`
override, not on the hot-reload path).

**An expedition is another `StartingUnit` band.** It reuses `PopulationCohort` + `BandTravel` /
`advance_band_movement` + `LaborAllocation` + `StartingUnit`, tagged with the `Expedition` component
(`components.rs`: `home_band`, `mission: ExpeditionMission::Scout`, `phase: Outbound|AwaitingOrders|
Returning`, `announced`, `pending_reveal: Vec<UVec2>`) and **deliberately lacking `ResidentBand`**.
Carrying `StartingUnit` is required: it makes the party a moving snapshot marker and lets `move_band`
retarget it — but it is **excluded from live faction fog reveal** (`Without<Expedition>` in
`calculate_visibility`), because discovery is comm-range gated.

**Isolation via the positive `ResidentBand` marker.** Every real band gets `ResidentBand` at spawn
(`spawn_population_entity`) and on rollback restore; expeditions never do. Systems that must not see
expeditions filter `With<ResidentBand>`: `simulate_population`, `advance_population_migration`,
`sedentarization_tick`, `apply_starting_inventory_effects`, `balance_supply_networks`, and the
default-band command pickers (`select_starting_band` / `select_founder_band` `None`-bits branch).
Left **bare** (expeditions included): `advance_band_movement`, `advance_expeditions`,
`advance_labor_allocation`, the snapshot capture query, `collect_metrics`, `discover_sites`,
`advance_husbandry`. So expeditions are excluded **by construction** — the safe default survives new
settlement-arc systems. (A future breakaway-to-new-band is an expedition that drops `Expedition` and
gains `ResidentBand`.)

**`advance_expeditions`** (`systems.rs`, `TurnStage::Population`, registered right after
`advance_band_movement`, before the Visibility stage's `discover_sites`) runs per expedition each
turn. **Map documentation — (a)+(b) — is SHARED by every mission (scout AND hunt):** a ranging party
maps the terrain it crosses regardless of verb. **(a) observe** the tiles in `observe_sight_range` LOS
of its current tile into the private `pending_reveal` buffer (reusing
`visibility_systems::visible_tiles_in_range` — the pure geometry behind `reveal_tiles_in_range` —
**without** touching the faction map); **(b) comm check + flush** — when within `effective_comm_range()`
(= `comm_range_tiles × comm_range_tech_factor`, rounded) hex distance of the home band's **live** tile,
promote every buffered tile to `Discovered` on the faction map (`FactionVisibilityMap::discover`,
Unexplored→Discovered, never downgrading `Active`) and clear the buffer — so the map lights up **as a
lump on return** (for a hunt party, at each `Delivering` drop-off / `Returning` fold-back), and
`discover_sites` records any `SiteTag` on the flushed tiles for free. **Scout-only** below: **(c)
provisions** drain by `party × provision_upkeep_per_worker` (hunt lives off its kills; non-fatal at
zero in v1) + opportunistic replenish; **(d) phase transitions** — `Outbound` + arrived (no `BandTravel`) →
`AwaitingOrders` + one-shot `ExpeditionArrived` feed; `Returning` → chase the home band's live tile
(refresh `BandTravel`) and, once within comm range, fold workers + leftover provisions back into the
band + despawn (`ExpeditionReturned`, after the flush so the final findings report); `AwaitingOrders`
waits.

**Hunt verb (PR 2)** — `ExpeditionMission::Hunt { fauna_id, policy: FollowPolicy }` on the same party;
the take **policy is chosen at launch** (`send_hunt_expedition <faction> <band> <party_workers>
<fauna_id> [policy]`, default **Sustain** — not a config lever). `advance_expeditions` branches on
mission:
- **Hunting**: retarget `BandTravel` to the herd's live tile each turn (from `HerdRegistry`); when
  within `hunt.reach_tiles`, take a **productive** hunt's worth of biomass — `workers ×
  per_worker_biomass_capacity`, floored per policy (`hunt_expedition_floor`) — from the herd and
  convert to provisions up to the carry cap (`party × hunt.per_worker_carry`). Deliver only with a
  worthwhile load: policy completion **or** `herd_near_band && carried ≥ hunt.min_deliver_fraction ×
  cap` (the empty-larder flip-flop fix).
- **Per-policy behaviour**: **Sustain** — take the herd down only to `hunt.sustain_floor_fraction ×
  carrying_capacity`, then one trip home + **done** (fold back, herd left robust); **Surplus** — one
  full-cap haul (floor = ecology collapse threshold) + **done**; **Market** — repeated full-cap trips
  via `Delivering`→deposit→**auto-relaunch** (the deposit fires once the party is back within
  communication range of the home band — the shared `near_home` proximity — not necessarily on its
  exact live tile), grinding the herd toward the collapse floor until it
  crashes or you recall; **Eradicate** — no floor, **delivers no food** (denial): keeps taking each
  turn until the herd is extinct, then folds back empty. A lost/extinct herd → shared `Returning`.
- **Lives off its kills** — no launch provisions, no per-turn upkeep (upkeep is scout-only).
- **`hunt_take` primitive** (`systems.rs`, band Hunt take math extracted verbatim: per-policy ecology
  ceiling + `per_worker_biomass_capacity` cap + biomass→provisions) is **unchanged** and still serves
  the band Hunt labor (`advance_labor_allocation`) and the **scout's opportunistic replenish** (Sustain
  nibble when a scout's provisions fall below `party × provision_upkeep_per_worker × replenish.low_turns`
  and a herd is within `replenish.reach_tiles`). The **hunt expedition** uses its own productive
  floor-capped take (above), not the regrowth skim. Domestication accrual is **not** wired to the
  expedition take in v1. Catching a *migratory* herd depends on the deferred fauna-movement redesign
  (herds step 1 tile/turn today, so an equal-speed party can't close a long one-directional route).

**Commands** (full proto/runtime/text/server plumbing, mirroring `move_band`):
- `send_expedition <faction> <band> <party_workers> <x> <y>` — validates land target + `1 ≤
  party_workers ≤ min(available_workers, max_party_size)`, draws `party × distance ×
  provision_draw_per_worker_per_tile` provisions from the band larder (partial OK), removes the
  workers from `band.working`, and spawns the detached `Expedition` cohort. Feed `ExpeditionSent`.
- `send_hunt_expedition <faction> <band> <party_workers> <fauna_id>` — same resident-band gate +
  party validation, validates `fauna_id` resolves to a live herd, draws **no** provisions, removes
  the workers, spawns a `Hunt`-mission party in `Hunting` phase heading for the herd. Feed
  `ExpeditionSent` (hunt flavor).
- `recall_expedition <faction> <expedition_entity_bits>` — resolves the entity via
  `resolve_expedition_entity` (checks the `Expedition` component + faction), sets `phase = Returning`
  (works for both verbs). Feed `ExpeditionRecalled`.
- **Retargeting a scout waypoint is just `move_band` on the expedition entity** — `handle_move_band`
  has a hook that re-arms a moved expedition to `Outbound` + `announced = false`.
- New `CommandEventKind` variants: `ExpeditionSent`, `ExpeditionArrived`, `ExpeditionRecalled`,
  `ExpeditionReturned` (in `as_str` + the server label map); the hunt drop-off / lost-herd feed lines
  reuse `Hunt`.

**Snapshot.** `PopulationCohortState` gains client discriminators `isExpedition` / `expeditionMission`
(`"scout"`|`"hunt"`) / `expeditionPhase` (`outbound`|`awaiting`|`returning`|`hunting`|`delivering`) /
`expeditionTargetHerd` (hunt fauna_id — a **string**, since herd ids are non-numeric) /
`expeditionHuntPolicy` (`sustain|surplus|market|eradicate`) / `expeditionCarryCap` (hunt carry cap =
`party × per_worker_carry`, `0` otherwise) and persistence-only `homeBandEntity` /
`expeditionAnnounced` / `pendingRevealX` / `pendingRevealY`
(`snapshot.fbs`, `sim_schema`). Capture fills them from `Option<&Expedition>`;
`restore_world_from_snapshot` re-attaches `Expedition` for a rolled-back in-flight party (resolving
`home_band` from `homeBandEntity` via the cohort entity-remap; missing home band → log + skip) and
re-attaches `ResidentBand` to every non-expedition cohort so the `With<ResidentBand>` systems keep
running after a rollback. `PopulationCohortState` also echoes `maxExpeditionPartySize` per cohort
(from `expedition_config.max_party_size`, same idiom as `workRange` — a global lever surfaced
per-band, populated for every cohort) so the client outfit stepper pre-clamps to
`min(idle_workers, max_expedition_party_size)`.

See Also: `docs/plan_exploration_and_sites.md` §2 (design), "Wondrous Sites" (discovery rides the
flushed tiles), "Visibility Systems" (the `Without<Expedition>` gate).

---

## Campaign Loop & System Activation

### Start Flow
- **Data**: `StartProfile` records with `starting_units`, `starting_knowledge_tags`, `inventory`, `survey_radius`, `fog_mode`
- **Spawn**: Worldgen seeds the profile's `starting_units`, unlocks `ScoutArea`, `FollowHerd`. Each spawned band's head-count comes from its unit's `band_size` (config lever in `start_profiles.json`; falls back to `DEFAULT_STARTING_BAND_SIZE` = 30 in `start_profile.rs`) — no hardcoded size. `late_forager_tribe` ships a **single ~30-person band** (labor-pool scale per `docs/plan_early_game_labor.md`), not the retired four-band/900-person opening.
- **Camps**: Transient settlement-likes with `PortableBuildings`, `CampStorage`, `DecayOnAbandon` (backlog — not yet built)
- **Sedentarization**: implemented — see the dedicated section below.
- **Founding**: `Command::FoundSettlement { q, r }` requires Founders unit, consumes provisions, spawns Settlement

### Population & Demographics (Settlement & Population Economy — Phase 1)
The bedrock number the rest of the economy builds on. Each `PopulationCohort` (a band — the first
"location"; tile-housed population arrives in Phase 3) carries three fixed-point **age brackets** —
**children / working-age / elders** — plus a local **`stores`** larder (food under the `FOOD` key).
`size` is a derived
`u32` cache of the bracket sum. Design: `docs/plan_settlement_population.md`.

`simulate_population` (`systems.rs`, `TurnStage::Population`) delegates each cohort to the pure
`advance_demographics` (config: `demographics_config.json`):
1. **Consume** — draw `per_capita_draw × weighted_mouths` (dependents eat less) from the band's
   own larder; shortfall is the food **deficit**.
2. **Deaths** — starvation scales with the deficit (dependents more vulnerable via `scarcity`
   weights); cold kills across brackets past `cold.temp_tolerance`.
3. **Births → children** — `birth_rate × working × fed_ratio × (1 + surplus_bonus × surplus_ratio)`.
   Births are **morale-independent** (Civilization Wellbeing — see below): contentment doesn't
   change procreation, and morale **never** causes faction population loss. `advance_demographics`
   no longer takes morale; the retired `births.morale_floor` lever is gone.
4. **Maturation** children→working, **aging** working→elders, **elder mortality**. All flows use
   the turn's *opening* values and apply together (a newborn doesn't mature the same turn); the
   total is clamped to `population_cap`. The **dependency ratio** `(children+elders)/working` is
   the core tension.

**Morale attribution (why morale/population falls).** Morale is now computed as the signed sum of a
**named contributor set** (`MoraleContributions` on the cohort — the Layer-1 spine of Civilization
Wellbeing, below): `settling` (`+population_growth_rate`), `terrain` (`−terrain pressure`),
`climate` (`−cold pressure`), `unrest` (crisis impacts + cultural sentiment, signed). Their sum IS
`last_morale_delta`; adding a future factor is a new `MoraleFactor` variant + one field, not a
rewrite of the morale update. The dominant *negative* contributor becomes `last_morale_cause`
(`MoraleCause` ∈ `None | Terrain | Cold | Unrest`) when the delta is negative, else `None`. Drivers:
`Terrain` = terrain attrition + logistics hardness, `Cold` = temperature-difference penalty,
`Unrest` = crisis impacts + cultural sentiment.
Starvation is deliberately **not** a morale cause — it stays on the days-of-food path. The two
place-based (negative) terms come from the shared **`tile_morale_pressure(terrain, temperature,
&MoralePressureConfig)`** helper (`systems.rs`), which returns the tile-intrinsic per-turn morale
drain (terrain + cold, ≥ 0; KarstCavernMouth ≈ 0.0825 at ambient temperature) so the sim and the
snapshot read from one source. The cold term has a **tolerance dead-band**: `max(0, |temp − ambient|
− temperature_morale_tolerance) × temperature_morale_penalty` (config `temperature_morale_tolerance`
= 9.0 in `simulation_config.json`), so temperate mid-latitudes (|Δ| ≤ 9°) bleed **zero** climate
morale and only genuine extremes (poles/high-alt/equator) drain — e.g. at ambient 18° a −5° pole
(|Δ| = 23°) drains `(23−9)·0.004 = 0.056`, a 30° equator (|Δ| = 12°) drains `0.012`. Habitability
reuses this helper, so most of the map rates Hospitable/Fair and only extremes read Harsh/Hostile. These fields are **derived per-turn, not snapshot-persisted** (a
rehydrated cohort reads `0`/`None` until the next turn). Exported as `PopulationCohortState.moraleDelta`
(fixed-point `long`, `FIXED_POINT_SCALE` = 1e6) + `moraleCause:ubyte` (`0=None, 1=Terrain, 2=Cold,
3=Unrest`). `TileState.habitability:long` carries the band-independent `tile_morale_pressure` total
for the tile (same fixed-point scale) so the client can rate a hex's harshness. All three are wired
through `sim_schema`/`snapshot.rs`; the client consumes them for a morale trend arrow + named cause
and a Tile-card Habitability line (client half).

**Food is band-local from day one** (the same store a settlement/storage-pit will hold later at
scale). Provisions **left `FactionInventory` entirely**: labor income (forage + hunt, in
`advance_labor_allocation`) and husbandry (`advance_husbandry`, split across the
owner's bands) income now credit the acting band's local `stores` (food under the `FOOD` key). At Startup
(`seed_cohort_demographics`) each band is seeded with `startup.food_reserve_days` turns of its own
demand (`food_demand`, shared with the consumption path) plus a well-fed morale bonus — no faction
provisions grant to distribute. Bands **share** via the supply network (below); storage-pit
distribution is a later addition. Starvation is deficit-capped (a 10% shortfall kills at most 10%)
so a dry larder bleeds down over several turns rather than in one.

Each band's goods live in a `LocalStore` (`components.rs`) — a commodity-keyed bag (food under the
`FOOD` = `"provisions"` key) held on `PopulationCohort.stores`, so the same store carries any future
good. Brackets + store persist in the snapshot (`PopulationCohortState.stores`) so rollback restores
the exact larder. A per-faction age-structure + dependency-ratio HUD readout ships as
`PopulationDemographicsState` (new `.fbs` table aggregated at capture, wired through
sim_schema/snapshot/native/`Hud.gd` exactly like `SedentarizationState`).

### Supply Network (logistics from turn 0)
Bands are small logistics nodes: `balance_supply_networks` (`supply.rs`, `TurnStage::Logistics`,
before Population consumes) connects **same-faction** bands within `reach_tiles` (via
`grid_utils::wrapped_distance_sq`) into **supply networks** (union-find connected components) and
each turn moves every commodity toward a **population-weighted per-capita balance** across the
network. Transfers are **throughput-limited** (`throughput_per_turn` per node) and lose `friction`
in transit; sub-`min_transfer` moves are dropped. So a gatherer band automatically feeds a scout
band it's near (you can specialize labor), while a band beyond reach lives off its own larder.
Reach decides *who* shares, throughput *how fast*, friction the leak — "free neighbor sharing" is
just the high-throughput/low-friction limit. The per-commodity math is the pure, unit-tested
`balance_commodity`. Config: `supply_network_config.json`.

Each turn the same pass also records **network membership** in the `SupplyNetworkMembership`
resource (`entity → id`, cleared and rebuilt every turn): each connected component with ≥ 2 bands
gets a stable id (`1, 2, …` in the BTreeMap's sorted-root order), singletons get none. The capture
reads it into each cohort's snapshot field `supplyNetworkId:uint` (`0` = not in a multi-band
network, `>= 1` = shared id) so the client can draw supply links between co-networked bands. It is
derived, not snapshot-persisted — a rehydrated cohort reads `0` until the next turn's balance.

The cohort snapshot also carries two derived per-band food-readout fields the client renders:
`daysOfFood:float` (`larder / one-turn food_demand`; `999.0` = a zero-demand cohort, "not
food-limited") and `activity:string` (`idle | forage | hunt | scout | warrior`, the target-kind
with the most workers in the band's `LaborAllocation`). Both are computed at capture in
`population_state`; alongside them the snapshot exports `laborAssignments`/`idleWorkers`/`workingAge`,
plus `workRange` (from `labor_config.json` `band_work_range`, global config today, surfaced per-band
for the work-range ring) and `scoutRevealRadius` (**repurposed**: now carries the band's effective
**scout vantage distance** — `scout.vantage_distance(scouts)` = `min(vantage_distance_base + scouts ×
vantage_distance_per_scout, vantage_distance_max)`, `0` with no scouts — since scouts now reveal by
posting forward-observer vantages that see around obstacles; field name kept for wire compat).

**Per-source food-income breakdown (retained yield telemetry).** `advance_labor_allocation` rebuilds
`LaborAllocation.last_yields` each turn — one `SourceYield { actual, sustainable }` (f32 provisions)
per assignment, **in the same index order** as `assignments` (so the snapshot zips by index). It is
**derived, not persisted**: it is out of rollback (`#[serde]` never sees it; `labor_allocation_from_state`
restores only the assignments, leaving it empty until the next tick) and is **excluded from
`LaborAllocation`'s equality** (manual `PartialEq` compares assignments only) so it can't perturb the
persisted-intent comparison. Definitions: **`actual`** = the provisions the source produced this turn
(the value added to the larder); **`sustainable`** = what it could yield without drawing down its
stock — **forage `sustainable ≡ actual`** (inexhaustible in today's model, no tile depletion), a
**hunt `sustainable = net_biomass_delta(biomass_before, carrying_capacity, ecology).max(0) ×
hunt.provisions_per_biomass × output_multiplier`** (one turn's net regrowth at the *pre-take* biomass,
in provisions). Scout/Warrior push `{0,0}`. The snapshot surfaces this: each `LaborAssignment` row
carries `actualYield`/`sustainableYield`, and each `PopulationCohortState` carries band-level
`foodIncome` (Σ per-source `actual`) + `foodConsumption` (the same one-turn `food_demand` `daysOfFood`
divides by). All derived at capture (0 on a rehydrated save before the next tick). **The client
consumes these next** (allocation-panel rows + tooltip + ledger footer, a follow-up PR): a per-turn
`actual > sustainable` is the client-derived **overhunting signal** — a *leading* flow indicator,
distinct from the stock-based `ecology_phase`.

This is the general mechanism the arc scales: raise reach/throughput for settlements/cities, and a
future **trade policy** adds a consent gate + a priced return flow on *cross-faction* edges (see the
Trade note below). *v1:* population is the universal balancing weight, so a zero-population storage
node would compute a 0 fair share — revisit (→ capacity weight) when storage-pits land. The
connected-components pass is also what Phase 4 will use to derive settlement clusters.

### Sedentarization
The emergent per-faction "pressure to root in place" — the first slice of the pastoral→
settlement chain, and the consumer of Phase E's domestication seam.

`sedentarization_tick` (`sedentarization.rs`, `TurnStage::Population` after
`advance_labor_allocation`) computes a per-faction 0–100 **`SedentarizationScore`** each turn as
a config-weighted blend of normalized inputs, then **EMA-smooths** it (`smoothing`):
- **domestication** = `HerdRegistry::domesticated_count(faction) / references.domesticated_herds`
  (the Phase E seam),
- **surplus** = Σ band `stores` food larders / `references.surplus` (band-local food, Phase 1),
- **resource density** = `HerdDensityMap::normalized_average()` (map-wide game richness — a v1
  baseline; per-faction-local density is a future refinement),
- **population** = Σ cohort size / `references.population`.

On a **rising** crossing of `soft_threshold` (~40, "establish a seasonal base?") or
`hard_threshold` (~70, "settle?") it pushes a `CommandEventKind::SedentarizationPrompt` to the
command feed (edge-gated on the stored `SedentarizationStage` so it doesn't re-fire; a fall
lowers the stage silently). The score is exported per-faction in the snapshot
(`SedentarizationState`, mirroring `factionInventory`) and shown as a HUD meter. Tunables live
in `data/sedentarization_config.json` (`sedentarization_config.rs`).

> **Reframed by the Settlement & Population Economy arc** (`docs/plan_settlement_population.md`):
> settlements are *derived* from clustered populated tiles + tended improvements (there is no
> discrete founding), and `SedentarizationScore` becomes an emergent readout of accumulated
> *tether* rather than a gate. See that design doc for the population/labor/improvement model
> this score ultimately feeds.

### Civilization Wellbeing (Morale → Discontent → Consequences)
The three-layer spine **factors → morale → discontent → consequences** (Phase 1). Authoritative
design: `docs/plan_civ_wellbeing.md`. Config: `wellbeing_config.rs` / `data/wellbeing_config.json`.
Extension seams are present and empty — future factors/consequences slot in without a rewrite.

- **Layer 1 — factors → morale.** `simulate_population` builds `MoraleContributions` (see morale
  attribution above); morale trends by their signed sum. Adding a factor = a new `MoraleFactor`
  variant + one field. The contributor set doubles as the client's itemized morale breakdown.
- **Layer 2 — discontent state (productivity only).** Each turn the cohort's `discontent_fraction =
  clamp((content_morale − morale) / (content_morale − floor_morale), 0, 1)` (0 at ≥`content_morale`
  0.6, 1 at ≤`floor_morale` 0.1). This drives **productivity only** — migration has its own onset
  (Layer 3b). A `grievance` accumulator (severity × duration) rises by `grievance_gain ×
  discontent_fraction` (× `trapped_multiplier` when *trapped* — below the migration threshold with no
  reachable destination) and decays by `grievance_decay` while content. **Phase 1 only populates
  `grievance`** — no consequence reads it (reserved for a future revolution trigger); it IS
  snapshot-**persisted** (like `age_turns`) so a rollback preserves brewing unrest.
- **Layer 3a — productivity modifier stack.** `output_multiplier(cohort, cfg) = Π(modifiers)`
  (`systems.rs`). Phase 1 has one entry, `discontent_output_modifier = max(floor_mult, 1 −
  discontent_fraction × discontent_weight)` (floor 0.5, weight 1.0). Applied at **payout** at every
  yield site via a single `output_multiplier` call — forage + hunt take (`advance_labor_allocation`),
  husbandry (`advance_husbandry`, `fauna.rs`). Adding
  an education/tech/government modifier is one line in `output_multiplier`, not per-site edits.
- **Layer 3b — tech-gated migration (own morale onset).** `advance_population_migration`
  (`systems.rs`, `TurnStage::Population`, **after** demographics + this turn's payouts).
  **Decoupled from `discontent_fraction`** — migration has its own morale-scaled onset at
  `migration.morale_threshold` (0.25): each band sheds `total × move_fraction`, where
  `move_fraction = max_rate × clamp((morale_threshold − morale) / morale_threshold, 0, 1)` — 0 at
  morale ≥ 0.25, 7.5% at 0.125, up to `max_rate` (0.15) at rock-bottom (gentle at onset, ramping to
  the cap). The total is split across brackets ∝ `bracket_size × weight` (working = 1.0, dependents
  = `dependent_weight` 0.4), so leavers are mostly workers while the headline fraction stays exact.
  They seek the **highest-morale eligible same-faction band within reach** (`base_reach` 4 hexes ×
  a movement-tech factor). *No concrete movement/transport tech signal exists yet, so the factor is
  stubbed at 1.0 with a `TODO(phase2)` hook.* Eligible = `morale ≥ attractive_morale` (0.5) AND
  `morale > source + min_morale_gap` (0.05). Found → **relocate** (source shrinks, destination
  grows; `last_emigrated`/`last_immigrated` recorded); none reachable → **stay** (grievance accrues
  faster via the trapped bonus). **Morale never causes faction population loss** — population is
  conserved within the faction; loss stays with starvation/cold only. Destinations are chosen from
  one pre-migration snapshot and all moves are computed before any is applied, so relocation is
  order-independent.
- **Snapshot.** `PopulationCohortState` gains `outputMultiplier`, `discontentFraction`, `grievance`,
  `lastEmigrated`/`lastImmigrated`, and the four itemized contributions
  `moraleSettling/Terrain/Climate/Unrest` (surfaced so the client can render the breakdown). All
  fixed-point except the two head-counts; all derived per-turn except `grievance` (persisted).

### Capability Flags
`CapabilityFlags` bitflags: `AlwaysOn`, `Construction`, `IndustryT1/T2`, `Power`, `NavalOps`, `AirOps`, `EspionageT2`, `Megaprojects`. Systems are inert until corresponding flag is set.

### Victory Engine
`VictoryState` with per-mode progress meters. Modes: Hegemony, Ascension, Economic, Diplomatic, Stewardship, Survival. `victory_tick` runs after end-of-turn accounting.

---

## Turn Loop

```
per-faction orders -> command server -> turn queue -> run_turn -> snapshot -> broadcaster -> clients
```

### Phases
1. **Collect** - `TurnQueue` awaits faction submissions
2. **Resolve** - Apply directives, execute `run_turn`, capture metrics, broadcast delta
3. **Advance** - Reset queue for next turn

### Turn Pipeline Config (`turn_pipeline_config.json`)
- **Logistics**: `flow_gain_min/max`, `effective_gain_min`, `penalty_min`, `capacity_min`, `attrition_max`
- **Trade**: `tariff_min`, `tariff_max_scalar`
- **Population**: Attrition scaling, temperature penalty, morale weighting, growth clamp, migration thresholds
- **Power**: `efficiency_adjust_scale`, `efficiency_floor`, storage efficiency/bleed clamps

---

## Snapshot History & Rollback

`SnapshotHistory` retains ring buffer of `WorldSnapshot` + `WorldDelta` pairs (default 256). `rollback <tick>` rewinds simulation, resets ECS world, truncates history.

The rollback snapshot round-trips the **authoritative `HerdRegistry`** (via `HerdState` + the shared `EcologyState` record in `WorldSnapshot.herd_registry`), not just the lossy display telemetry — see the herd-persistence note under "Fauna & Wild Game" for details and the bug it fixed.

**Map export**: the `export_map [path]` command (`write_map_export` in `bin/server.rs`) writes the latest `SnapshotHistory.last_snapshot` plus the resolved `SimulationConfig.map_seed`/`map_preset_id` to disk as a `sim_schema::MapExport` JSON (default `exports/map-tick<t>-seed<s>.json`, gitignored). No new protocol — it rides the existing one-way command channel; the seed makes the dumped map reproducible, and the JSON doubles as an offline-inspectable, test-loadable fixture.

---

## ECS Systems Reference

### Power Systems
Fourth in turn chain. `PowerGridState` resource tracks per-node supply, demand, transmission loss, storage charge, stability score.

**Flow**: `collect_generation_orders` → `resolve_generation` → `route_energy` → `apply_storage_buffers` → `satisfy_demand` → `evaluate_instability` → `export_power_metrics`

**Instability**: Stability bands 0-1. Thresholds: 0.4 (warn), 0.2 (critical). Incident types: brownout/blackout, containment breach, cascading failures.

### Crisis Systems
`TurnStage::Crisis` between Population and Finalize. `ActiveCrisisLedger`, `CrisisModifierLedger`, `CrisisIncidentFeed`.

**Archetypes** (from `crisis_archetypes.json`): `plague_bloom`, `replicator_swarm`, `ai_sovereign`. Each has propagation model, mitigation hooks, telemetry contributions.

**Telemetry**: `CrisisTelemetryState` with EMA-smoothed gauges, trend deltas, warn/critical bands.

### Culture Simulation
`CultureLayer` resources at faction/region/settlement scope. Each stores normalized trait vector (15 axes per manual).

**Flow**: `reconcile_culture_layers` copies global baselines down, blends with local deltas. `CultureDivergence` tracks deviation; crossing thresholds emits `CultureTensionEvent` / `CultureSchismEvent`.

**Config**: `culture_corruption_config.json` governs elasticity, `soft_threshold`/`hard_threshold`, trigger tick counts.

### Knowledge & Espionage
`KnowledgeLedger` tracks per-discovery secrecy posture, leak cadence, espionage pressure.

**Leak Timer**: `knowledge_ledger_tick` runs after `trade_knowledge_diffusion`. Recomputes `half_life_ticks` from base + visibility + security − (spy_pressure + cultural_pressure).

**Espionage**: `EspionageRoster` per faction. Mission lifecycle: Planning → Execution → Resolution. `EspionageProbeEvent` / `CounterIntelSweepEvent`.

### Great Discovery System
Constellation-level leaps from overlapping discoveries.

**Flow**: `collect_observation_signals` → `update_constellation_progress` → `screen_great_discovery_candidates` → `resolve_great_discovery` → `propagate_diffusion_impacts`

**Registry**: `GreatDiscoveryRegistry` loads from `great_discovery_definitions.json`. Fields: `id`, `field`, `requirements`, observation gate, cooldown, effect flags.

### Visibility Systems (Fog of War)
Per-faction visibility tracking with three states: `Unexplored` (never seen), `Discovered` (previously seen), `Active` (currently visible).

**Files**: `visibility.rs` (state + ledger), `visibility_systems.rs` (ECS systems), `visibility_config.rs` (config loading)

**Turn Flow** (`TurnStage::Visibility` after Population, before Crisis):
1. `clear_active_visibility` - Reset Active tiles to Discovered
2. `prune_sweep_tracker` - Forget sweep positions of despawned cohorts
3. `calculate_visibility` - Compute visibility from units/settlements
4. `apply_trade_route_visibility` - Mark active trade-route tiles as Active
5. `apply_visibility_decay` - Decay old Discovered tiles to Unexplored (disabled by default; permanent memory)
6. `discover_sites` - Record any `SiteTag` tile a faction has ever seen into `DiscoveredSites`, apply the reward, push a `SiteDiscovered` feed entry (see "Wondrous Sites")

**Visibility Sources**:
- **Units**: `PopulationCohort` with `StartingUnit` marker provides sight from its
  `current_tile`. Because a unit can move several tiles in one turn (see
  `estimate_travel_turns`, travel interpolation), `calculate_visibility` reveals
  the whole **corridor** it swept from its previous position (tracked in
  `VisibilitySweepTracker`) to the current one — not just the endpoint — so
  passed-over tiles are seen (`corridor_tiles`).
- **Settlements**: `Settlement` with `TownCenter` provides sight from settlement position
- **Worked sources** (labor): a band's workers are physically out at the sources they
  work, so those spots provide fog reveal too. For each assignment in the cohort's
  `LaborAllocation`, `calculate_visibility` adds a worked source tile — a **Forage**
  assignment's `tile`, or a **Hunt** assignment's herd's **current tile** (resolved live
  from `HerdRegistry`; an unresolved/extinct herd is skipped, no panic). Each worked source
  reveals at `worked_source_sight_range` via the *same* `reveal_tiles_in_range` LOS path the
  band center and scout vantages use — additive, re-marked Active every turn while the
  assignment is staffed. Scout/Warrior are band-wide roles, not tile sources. Config:
  `labor_config.json` `worked_source_sight_range`.

**Modifiers**:
- **Elevation**: Higher elevation grants sight bonus (configurable per 100m)
- **Terrain**: Water tiles grant bonus range; forest/wetland tiles apply penalty
- **Line of Sight**: Bresenham ray-cast checks for blocking terrain
- **Local scout** (labor): staffed scouts are **forward observers** — with ≥1 scout (from the
  cohort's `LaborAllocation` head-count, `workers_on(&LaborTarget::Scout)`), `calculate_visibility`
  posts vantage tiles out from the band in all 6 hex directions (`scout_vantage_tiles`, reusing
  `grid_utils::hex_neighbor`) at `scout.vantage_distance(scouts)` = `min(vantage_distance_base +
  scouts × vantage_distance_per_scout, vantage_distance_max)`, pulling each back to the last on-map,
  passable (non-`WATER`) tile. Each vantage reveals with `vantage_range` via the *same* per-source
  LOS reveal the band uses (`reveal_tiles_in_range`), so scouts see **around** ridges/forest, not
  merely farther. The band's own base-range LOS from its center is unchanged (scouts are additive);
  the vantages are re-marked Active every turn while scouts are staffed. Config: `labor_config.json`
  `scout`.

**Config** (`visibility_config.json`):
- `decay`: `enabled` (default `false` — permanent memory; Discovered tiles never revert to Unexplored), `threshold_turns` (turns before Discovered → Unexplored when enabled)
- `sight_ranges`: Per-unit-type `base_range` and `elevation_bonus_factor`
- `elevation`: `enabled`, `bonus_per_100m`, `max_bonus`
- `line_of_sight`: `enabled`, `blocking_terrain_tags`
- `terrain_modifiers`: `forest_penalty`, `water_bonus`
- `movement`: `max_sweep_tiles` (cap on the corridor length revealed for a single-turn move; keep above the real max per-turn move distance so genuine moves sweep fully — see `corridor_tiles`)

**Snapshot Export**: `visibility_raster` emits a per-faction `ScalarRasterState` (fixed-point i64 samples) encoding Unexplored=0.0, Discovered=0.5, Active=1.0; the client decodes these to floats and renders black / cloudy / full-color. (`FactionVisibilityMap::to_byte_raster` still exists as a 0/1/2 byte view, but is not the snapshot export.)

---

## Trade-Fueled Knowledge Diffusion

> **Deprecated / to be replaced.** `TradeLink` is dormant on a live game — nothing attaches it at
> runtime (only snapshot rehydration does; its establishment path was never built), so
> `trade_knowledge_diffusion` iterates an empty set and its test is `#[ignore]`d. The Settlement &
> Population arc reframes this: inter-faction trade becomes a **trade *policy* on the supply
> network** (see "Supply Network") — a consent gate + a priced return flow on cross-faction edges —
> and the knowledge-leak-via-open-trade behavior re-homes onto those rails. `TradeLink` /
> `trade_knowledge_diffusion` are slated for removal in that slice (not now, to avoid schema churn +
> a coherent-behavior gap). Latent bug to fix then: the logistics snapshot query requires
> `TradeLink`, so the logistics overlay is empty on a live game.

`TradeLinkState` carries throughput, tariff, `TradeLinkKnowledge` (openness, leak_timer, decay). `trade_knowledge_diffusion` runs after logistics, emits `TradeDiffusionEvent`s, applies progress to `DiscoveryProgressLedger`.

**Migration**: `PendingMigration` payloads carry scaled knowledge fragments; on arrival they merge
into the destination ledger and the whole band emigrates (`cohort.faction = destination`) — the
high-morale "brain-drain" / Cultural Osmosis vector. `simulate_population` gates it on **both** high
morale (`migration_morale_threshold`) **and** a settled duration: a band must have been simulated at
least `migration_min_settled_turns` turns (`PopulationCohort.age_turns`, incremented each turn and
snapshot-persisted) before its population can emigrate. This stops a freshly-spawned, well-fed
starting band from defecting on turn one (the `well_fed_morale_bonus` alone would otherwise clear the
morale threshold immediately).

**Config**: `trade_leak_min/max_ticks`, `trade_leak_exponent`, `trade_openness_decay`, `migration_fragment_scaling`; migration gating (`migration_morale_threshold`, `migration_eta_ticks`, `migration_min_settled_turns`) lives in the `population` block of `turn_pipeline_config.json`.

---

## See Also

- `docs/architecture.md` - System-wide data flow and extensibility
- `sim_schema/README.md` - FlatBuffers schema contracts
- `sim_runtime/README.md` - Shared runtime utilities
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` - Game manual
