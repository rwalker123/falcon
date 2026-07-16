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
| `src/data/labor_config.json` | Early-Game Labor allocation: `band_work_range` (true odd-r **hex-distance** radius of in-range sources — `grid_utils::hex_distance_wrapped`, wrap-aware), `worked_source_sight_range` (fog reveal range around each worked Forage tile / Hunt herd tile in `calculate_visibility`), `hunt_leash_tiles` (extra leashed-follow reach for Hunt), `band_move_tiles_per_turn` (`move_band` speed), `forage` (**depletable-forage** ecology, §0-ii: **`capacity_by_biome`** — the **human food web's** per-biome capacity table, a **total** table (one row per `TerrainType`) mirroring `fauna_config.json`'s `graze.capacity_by_biome` (the *animal* web) row-for-row and meant to **disagree** with it (see "The two food webs"); it replaces the retired flat `carrying_capacity` of 120 — `per_worker_biomass_capacity` gather throughput, `provisions_per_biomass` biomass→food conversion, and an `ecology` block reusing fauna's `EcologyConfig` — `regrowth_rate` tuned higher than fauna's 0.05, plus `collapse_fraction`/`stressed_fraction` phase bands; supersedes the retired flat `per_worker_yield` — **plus the §0-iii policy axis** `surplus_multiplier` / `market.{take_fraction,trade_goods_multiplier,trade_goods_per_biomass}` / `eradicate.take_fraction`, mirroring fauna's follow/market/hunt levers so forage has Sustain/Surplus/Market/Eradicate parity with hunting — **plus the Phase 1a/1b `cultivation` block** `progress_per_turn`/`decay_per_turn`/**`cultivating_yield_fraction`**/`tended_provisions_per_biomass` + the Rung 1b earned-knowledge levers `knowledge_progress_per_turn`/`knowledge_completion_threshold` (Rung 1a: cultivation is the explicit **`Cultivate` policy** — while preparing, the patch yields only `cultivating_yield_fraction × its Sustain/MSY ceiling` (the investment cost) and accrues `progress_per_turn`; at 1.0 the tended patch pays the tending band `biomass × tended_provisions_per_biomass` place-local, higher than wild MSY, and goes feral if abandoned. Rung 1b: Sustain-forage earns faction **Cultivation** knowledge in the `DiscoveryProgressLedger`, the gate on the Cultivate policy — Sustain itself never tames a patch, and the old `claim_threshold` early-claim is **removed**); see "Cultivation"), `hunt.per_worker_biomass_capacity` (per-hunter take cap; biomass→provisions/trade reuses `fauna_config.hunt.*_per_biomass`), `scout.vantage_distance_base`/`vantage_distance_per_scout`/`vantage_distance_max`/`vantage_range` (staffed scouts post forward-observer vantages in all 6 hex directions and reveal LOS from each in `calculate_visibility`, so they see *around* obstacles). **Validated** — `LaborConfig::validate()` runs inside `from_json_str` (every load path, the `fauna_config.rs` convention), rejecting a **partial / all-zero / negative `forage.capacity_by_biome`** (a missing biome would silently read as an invisible zero-forage dead zone — **zero must be stated, never defaulted**); a broken invariant is logged at **error** level (`labor_config.invalid_rejected`) and the builtin is used |
| `src/data/fauna_config.json` | Wild-game species table (display, size class, migratory flag, route length = anchor count, biomass, host biomes, + movement cadence `dwell_turns` / migratory `loiter_turns [min,max]` / `loiter_radius`, + **`fodder_per_biomass`** (Grazing 2b-i — graze the herd eats per unit biomass/turn; cached on `Herd` at spawn) + **`regrowth_rate`** (Grazing 2b-ii — per-species WILD breeding rate, `Option`, cached on `Herd`; rabbit/fowl 0.35, deer/boar 0.10, migratory 0.04 — replaces the single global `ecology.regrowth_rate` for wild herds; see "Phase 2b-ii") + **`husbandry_ceiling`** (Grazing 2d-δ — `wild`|`pastoral`|`pen`, default `pen`; how far up the ladder the species climbs — mammoth/deer `wild`, steppe_runner/marsh_grazer `pastoral`, boar/rabbit/fowl `pen`; cached on `Herd`, gates domestication + corral/extend; see "Phase 2d")) + per-biome spawn abundance + `hunt` / `follow` / `ecology` (regrowth + depensation collapse thresholds) / `immigration` (respawn) / `husbandry` (domestication accrual/decay/claim + **the flow-based yield ladder**: **per-species managed `r`** (Grazing 2d — `pastoral_gain` 1.5 / `pen_gain` 3.0 scale each species' own wild `r`, capped at `husbandry_regrowth_cap` 0.75, retiring the flat `pastoral.ecology.r` 0.25 / `pen.ecology.r` 0.90 which now carry phase bands only) and `pen` (**`upkeep_per_biomass`** — the pen's feed, now footprint-offset — / `starve_shrink_rate`; `capacity_fraction` is **deleted** — a penned herd's `K` is its fenced-footprint graze flow), plus the **`Corral` policy** investment levers `corralling_yield_fraction`/`corral_build_progress_per_turn`; every rung pays MSY against its own ecology, see "The husbandry yield ladder" / "Phase 2d") / `market` (commercial-hunt take + trade multiplier) tuning + **`graze`** (the pasture layer, Grazing Phase 2a — `capacity_by_biome` a **total** per-biome table (one row per `TerrainType`), `ecology` (`regrowth_rate` **0.40**, the fastest vegetal stock in the model), `reseed_floor_fraction` 0.02, **`overgraze_escapement_fraction` 0.25** (Grazing 2b-ii — grazing can't draw a patch below this, the constant-escapement floor that keeps the herd↔graze loop convergent); see "The Graze (Pasture) Layer" / "Phase 2b-ii"). **Validated** — `FaunaConfig::validate()` runs inside `from_json_str` (every load path), rejecting a pen that eats more than it yields, an inverted ladder, a dead ecology, or a **partial / all-zero / negative graze table** (a missing biome would silently read as an invisible zero-graze dead zone); a broken invariant is logged at **error** level (`fauna_config.invalid_rejected`) and the builtin is used |
| `src/data/sedentarization_config.json` | Sedentarization Score tuning: soft/hard prompt thresholds, EMA `smoothing`, input `weights` (domestication/surplus/resource_density/population), and saturation `references` |
| `src/data/demographics_config.json` | Demographic population tuning: `initial_distribution` (children/working/elders split), `consumption` (per-capita food draw + per-bracket factors), `startup` (`food_reserve_days` seeded into each band's larder + `well_fed_morale_bonus`), `births` (rate/surplus_bonus; morale-independent), `maturation_rate`/`aging_rate`/`elder_mortality_rate`, `scarcity` (starvation + per-bracket vulnerability, deficit-capped), `cold` (temperature-death) |
| `src/data/supply_network_config.json` | Supply-network tuning: `reach_tiles` (connection radius), `throughput_per_turn` (max goods moved per node/turn), `friction` (fraction lost in transit), `min_transfer` (dead-band) |
| `src/data/wellbeing_config.json` | Civilization Wellbeing tuning: `discontent` (`content_morale`/`floor_morale` productivity curve, `grievance_gain`/`grievance_decay`/`trapped_multiplier`), `productivity` (`floor_mult`, `discontent_weight`), `migration` (own morale-scaled onset: `morale_threshold`, `max_rate`, `base_reach`, `attractive_morale`, `min_morale_gap`, `dependent_weight`) |
| `src/data/sites_config.json` | Wondrous Sites catalog (`catalog`: per-`site_id` `category`/`display_name`/`glyph`/`placement_rule`/`discovery_reward.morale_bonus`) + `placement` rules (per-rule `max_sites`, `min_spacing`, and the union of rule inputs: `min_relief`, `max_habitability_pressure`, `min_food_weight`). Loader `sites_config.rs`, env override `SITES_CONFIG_PATH`. Not wired into the `reload_config` hot-reload path (mirrors `fauna_config.json`) |
| `src/data/expedition_config.json` | Expedition tuning. Scout: `max_party_size`, `comm_range_tiles` (discovery-report range), `comm_range_tech_factor` (stubbed 1.0 tech hook), `observe_sight_range` (per-turn LOS radius, matches band base sight), `provision_draw_per_worker_per_tile` (launch larder draw = party × distance × this), `provision_upkeep_per_worker` (per-turn drain = party × this, scouts only). Hunt (PR 2) `hunt` block: `per_worker_carry` (carry cap = party × this), `reach_tiles` (how close to the herd to take), `drop_off_within_tiles` (herd-near-band delivery gate), `min_deliver_fraction` (herd-near-band early delivery needs carried ≥ this × cap), `viability_warn_turns` (**20** — the launch forecast flags a trip NOT VIABLE past this many estimated turns-to-fill; = 4× the throughput-implied trip length `per_worker_carry / (per_worker_biomass_capacity × provisions_per_biomass)` = 5 turns), `forecast_horizon_turns` (**60** — how far `hunt_trip_forecast` simulates the trip before reporting "won't fill"; past ~3× `viability_warn_turns` the exact number carries no actionable information, and the bound caps the per-snapshot cost of the exported `huntTripEstimates` table). The retired `sustain_floor_fraction` is **gone**: a Sustain expedition takes the shared MSY *flow* ceiling (`fauna::hunt_policy_ceiling`), not a stock target. The take **policy** is **not** a config lever — it is chosen at launch via the optional trailing arg of `send_hunt_expedition` (default `FollowPolicy::Sustain`). Scout replenish `replenish` block: `low_turns` (top up below party × upkeep × this), `reach_tiles`. Loader `expedition_config.rs`, env override `EXPEDITION_CONFIG_PATH`. Not on the `reload_config` hot-reload path (mirrors `sites_config.json`). **Validated** — `ExpeditionConfig::validate()` runs inside `from_json_str`, so *every* load path (builtin, default file, `EXPEDITION_CONFIG_PATH` override) is covered, following the `crisis_config.rs` convention; a broken invariant is logged at **error** level (`expedition_config.invalid_rejected`) and the config is refused, falling back to the known-good builtin rather than silently disabling a feature. Enforced: `max_party_size ≥ 1`, `comm_range_tech_factor` finite & `> 0`, `observe_sight_range ≥ 1`, `provision_draw_per_worker_per_tile`/`provision_upkeep_per_worker` finite & `≥ 0`, `hunt.per_worker_carry` finite & `> 0`, `hunt.reach_tiles ≥ 1`, `0 < hunt.min_deliver_fraction ≤ 1`, `hunt.viability_warn_turns ≥ 1`, **`hunt.forecast_horizon_turns ≥ max(1, hunt.viability_warn_turns)`** (at `0` the forecast's `1..=horizon` loop runs zero turns and *every* hunting expedition silently reports "won't fill"; below the warn threshold, a trip the player would be told is viable can never be discovered), `replenish.low_turns ≥ 1`, `replenish.reach_tiles ≥ 1`. Deliberately **left free**: `comm_range_tiles` (`0` = "walk back into camp to report"), `hunt.drop_off_within_tiles` (`0` = no early drop-off; a full pack still delivers), and the *upper* end of `max_party_size`/`forecast_horizon_turns` (they only cost snapshot time — the estimate table is `O(policies × max_party_size × horizon)` per herd — an operator's call, not an invariant) |

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
6. **Ocean/coasts** - Distance-transform bands: Shelf → Slope → Deep Ocean; inland seas. See "Continental shelf width" below — the shelf is a continuous ≥1-tile ring off gentle coasts, gated to deep water at steep/cliff coasts. A **final reconciliation post-pass** (`reconcile_coastal_shelf`, Startup chain after hydrology + tag solver + palette clamp) restamps the shelf so no Deep Ocean touches gentle land on the *final* map, covering coasts created later by deltas/marshes/solver tundra.
7. **Climate** - Assign `climate_band` using latitude + elevation + moisture
8. **Hydrology** - Rivers on hex **edges** + navigable rivers as water **hexes**. See "Rivers" below. `RiverDelta` is stamped **only here**, at the last **gentle-coast** land hex of each river that ends in a standing water body — the ocean *or* an inland sea/lake (lacustrine deltas). The mouth hex must border that water; the biome picker and tag solver never create deltas (those would scatter them with no river attached). Delta tiles are protected from the tag solver's reduction passes so genuine river mouths survive.
9. **Biomes** - Stamp `TerrainType` via `terrain_for_position` with micro-variant jitters
10. **Moisture transport** - Humidity blending with wind-driven rain-shadow pass
11. **Resources** - Surface deposits biased by `TerrainDefinition.resource_bias`
12. **Wildlife** - Seed herd spawners, migratory paths, `game_density` raster
13. **Starting areas** - Place candidates respecting World Viability Contract

### Data Shapes
- **Rasters**: `elevation_m: i16`, `climate_band: u8`, `game_density: u8` (the square-8 hex `flow_dir` / `flow_accum` rasters are **deleted** — hydrology routes on the corner graph, see "Rivers")
- **Vectors**: `rivers: [RiverSegment]` — per-edge `RiverEdge { hex, dir, class, discharge: f32 }` chains + a navigable hex tail (see "Rivers")
- **Tiles**: `hydrology_id`, `substrate_material`, `terrain_type`, `TerrainTags`, `river_edges: u16`

### Rivers — a real drainage network on hex EDGES, with a class that grows downstream (`hydrology.rs`)

A river is **not** a polyline through hex centers. Minor/Major rivers run **along hex edges** (so a
future movement system can charge a crossing penalty on exactly the side the river is on), and a
river that outgrows the edge model becomes **water terrain**.

The **routing and extraction** are a real drainage network: steepest descent on a depression-filled,
precipitation-weighted elevation surface, decomposed into main stems and tributaries. Designs:
`docs/plan_rivers.md` (the edge/class/navigable *model*) and
`docs/plan_rivers_drainage_network.md` (the *network* that model expresses).

- **The corner graph.** The dual of "flow along edges" is "route between corners": every
  corner→corner step traverses exactly one hex edge. On a pointy-top odd-r grid each corner is
  shared by exactly 3 hexes, so `V = 6F/3 = 2F` — **two corners per hex**, indexed `(hex_x, hex_y,
  slot)` with `slot ∈ {TOP, BOTTOM}`. Each corner has 3 neighbour corners. A **border corner** (its
  3 hexes are not all on the map) is excluded from routing. Every hex step goes through
  `grid_utils::hex_neighbor`, so horizontal wrap is honored. Corner **elevation is the mean** of its
  3 hexes (not the min — the mean puts a corner low in the *trough* between two low hexes, so rivers
  settle into valleys) **plus a deterministic flat-tie jitter** (below). A corner is a **sink** iff
  any of its 3 hexes is an **OCEAN** hex (`WATER` *without* `FRESHWATER`) — see "Lakes flow through".
- **Canonical edges.** An edge `(H, d)` has two representations — `(H, d)` and `(neighbor,
  opposite(d))`. The canonical one is whichever has `dir ∈ {E, SE, SW}` (`canonical_edge`), so an
  edge has a single key regardless of which hex traced it. An edge exists only if **both** its hexes
  are on the map.
- **The flow field descends the LANDSCAPE, not a cost-to-sea distance transform** (`docs/plan_rivers_drainage_network.md`).
  1. **Jittered elevation.** Corner elevation gets `river_flat_jitter × (hash01(world_seed, corner) − 0.5)`
     — a pure splitmix64 hash, no RNG, no `HashMap`. Pure steepest descent on a plateau picks the same
     direction for every corner and carves artificial parallel channels; the jitter breaks those ties
     into a natural branching pattern, reproducibly. It is `≫ river_fill_epsilon` and `≪` real relief,
     so it decides only ties the terrain does not.
  2. **Priority-flood depression fill** (Barnes + epsilon): seed a min-heap with every sink at its own
     elevation and raise each neighbour to `max(elev[n], filled[popped] + river_fill_epsilon)`. Every
     non-sink corner ends **strictly above** the corner that flooded it, so a **strict descent to a
     sink always exists** — including across the flats of a filled depression, where a naive fill
     stalls. Unreachable corners keep `filled = INFINITY`.
  3. **Downstream = steepest descent on `filled`.** All 3 corner steps are the same length on a regular
     lattice, so "steepest" is simply "lowest filled neighbour"; ties break by corner index ascending.
  4. **Precipitation-weighted accumulation.** Each corner seeds
     `(river_base_runoff + river_moisture_weight × precip) / 2`, where `precip` is the mean of its 3
     hexes' `MoistureRaster` value. Dividing by the 2 corners-per-hex makes **discharge read directly
     as precipitation-weighted upstream drainage area, in HEX-EQUIVALENTS** — a fully-wet hex
     contributes exactly `1.0`. That is the unit the class thresholds live in, which is why they are
     **absolute and map-size independent**. A missing/mis-sized `MoistureRaster` falls back to uniform
     `precip = 1.0` with a warning (never a panic).
- **Extraction: main-stem decomposition, not N independent rivers.** `channel_min =
  river_channel_min_discharge / river_density`; a corner is a **channel** iff it is routable, not a
  sink, and `accumulation ≥ channel_min`. Accumulation is monotone non-decreasing downstream, so the
  channel corners + their descent links form a **forest of trees rooted at outlets, by construction** —
  nothing to reject, space, or count-target. Each outlet (largest first) is then walked **upstream**,
  always taking the largest unclaimed contributor: that path is the classic **main stem** ("the
  Missouri joins the Mississippi"), and every contributor it passes over becomes a tributary stem
  joining at exactly the corner it was passed over at. Every channel corner lands in exactly one river.
  - *Upstream-from-the-outlet, not downstream-from-headwaters*: every headwater's accumulation is
    barely above `channel_min` (nothing upstream of it is a channel), so "the biggest headwater" does
    **not** identify the main stem — but "always take the biggest contributor, walking up from the
    mouth" does, by definition.
  - A stem's final edge (`last corner → terminus`) is what makes a main stem **touch the shore** (the
    terminus is the ocean-touching sink corner) and a tributary **land on its trunk** (the terminus is
    a claimed corner of the parent stem). One uniform rule, no special case.
  - **Strahler order is computed on the real channel tree** (a channel corner with no channel
    contributors is order 1; otherwise `max(contributor orders)`, +1 iff ≥2 share that max) — where it
    is actually defined. The old per-tile computation on the hex flow field is gone.
  - `river_min_length` (in hexes) is the **only** noise gate left: an emitted river shorter than it is
    dropped. There is no spacing, no count target, no source category, and no acceptance loop.
- **Lakes FLOW THROUGH — only the ocean is a sink.** A lake / `InlandSea` corner is an ordinary low
  corner: the fill raises it to its lowest saddle and it **spills**, so the whole upstream catchment
  carries *through* the lake and out a genuine outlet. Real outlet rivers, and a big river below a big
  lake, fall out for free (replacing the old `lake_heads` hack). Two consequences:
  - **A river ENDS at standing water and CONNECTS to it; a new river begins where terrain drains out.**
    The run emits the **first water-touching edge as the mouth** (the connecting edge that reaches the
    water) and terminates there; the *rest* of the consecutive water-touching edges (the shore-hug + the
    submerged stretch) are **skipped, not drawn**, and a new run resumes at the next dry edge. So there
    is exactly **one water-touching edge per river and it is the LAST one** — the river runs *into* the
    lake/sea/trunk and stops rather than hugging the shore, and the drain-out below re-emerges as its own
    segment (connected on its source side, its first corner being water-adjacent). "Standing water" is a
    lake / inland sea / ocean on the terrain map **or** a previously-stamped navigable trunk
    (`StemEmitter::edge_touches_water`, reading `is_water_hex` + `existing_navigable`). The original
    both-banks rule hugged the lakeshore ("V" up a trunk hex); the first fix over-corrected and *dropped*
    the water-touching edge, leaving a visible **gap one step short of the water** — the current rule
    draws the mouth and skips only the shore-hug. The accumulation still flows through underneath
    (discharge/class unchanged), so the outlet stays a big river below a big lake and can independently go
    navigable again below it — **only the rendered segmentation changes.** The split is also required
    because a segment's edge chain and navigable chain are both *paths* — a chain with a water-shaped hole
    in it would be neither contiguous nor drawable. Guarded by
    `hydrology_earthlike::edge_rivers_terminate_at_water_not_along_it` (a river has **at most one**
    terrain-water-touching edge and it is the **last** — the mouth — so no river runs along a shore; the
    navigable-trunk "V" and the shore-hug tile proxy are tracked by the `drainage_census`).
  - **A navigable river must CONNECT to water, or it isn't navigable.** After the split a navigable chain
    must end at the water it connects to (its last hex is standing water, or hex-adjacent to it —
    `StemEmitter::navigable_reaches_water`). A chain that **dead-ends on dry land** (an endorheic run with
    no ocean) is **demoted to the river's edge (Major) form** — re-traced with the navigable model off,
    so the river survives on the edge model rather than stranding a landlocked navigable dead-end. A
    navigable run shorter than **`river_navigable_min_hexes`** (a 1- or 2-hex puddle) is demoted the same
    way. Both demotions run in `StemEmitter::emit_run`; guarded by
    `hydrology_earthlike::navigable_rivers_connect_to_water` (every navigable run reaches standing water
    and is ≥ the lever, swept over `CENSUS_SEEDS`). Aggregate over the 6-seed sweep: **14 navigable
    segments / 68 hexes, min run 3, max run 22, 0 landlocked, all mouth-connected** (the `drainage_census`
    now reports the landlocked count, the run histogram, and the mouth-connection count).
  - **Deltas are PER-TRANSITION, not per-terminus.** A river now both *enters* a standing water body
    and *leaves* it, so the delta scan stamps a delta at **every land→standing-water transition** along
    the river's ordered hex path (plus the mouth, where the path simply ends against the water) — each
    still **gentle-coast gated** and still required to actually border that water. A lacustrine delta
    and the ocean delta are different tiles on the same river. A delta may never take a **mid-chain**
    navigable hex (the channel flows through it; turning it into depositional land would break the
    chain in two).
- **Class is PER-EDGE and grows downstream.** `RiverEdge.discharge` = the corner accumulation at the
  edge's **upstream** corner, which is monotonically non-decreasing downstream — so a river is
  `Minor` at its headwater and `Major` in its lower course, never uniformly wide. `RiverClass`
  (`sim_runtime`) is `None = 0 | Minor = 1 | Major = 2`; **value 3 is reserved** — "navigable" is
  deliberately *not* a class (see below).
- **Navigable rivers are WATER TERRAIN, not edges.** Once discharge crosses
  `river_class_navigable_min_discharge` the river stops emitting edges: the lower **dry** of the two
  hexes flanking the **last emitted edge** becomes the first hex of a `TerrainType::NavigableRiver`
  chain, and the rest of the chain is read straight off the river's **own corner path** — the hex the
  channel is inside at each remaining step (`RiverSegment.navigable_hexes`). Consecutive steps share a
  corner and the three hexes at a corner are pairwise adjacent, so the chain is **contiguous by
  construction**. Two rules keep it a *simple path*: **sticky** (while the current hex still flanks the
  edge being crossed, the river has not left it) and **no self-crossing** (a channel that would double
  back onto a hex it already occupies ends there — a corner path never revisits a corner, but a *hex*
  is touched by many corners, so the hex path can). A giant river is
  a body of water you need a boat to enter, so it reuses every existing water mechanic.
  `NavigableRiver` mirrors `InlandSea` exactly (`WATER | FRESHWATER`, same movement/logistics/
  attrition profile), is in the biome palette's `must_have` set, and is protected from the tag
  solver's water-reduction pass — like `RiverDelta`, otherwise the solver would erase real rivers.
  - **The join invariant: the edge chain and the hex chain share an EDGE, never a bare corner.**
    The hand-off anchors on the last **emitted** edge, *not* on the un-emitted edge whose discharge
    crossed the threshold. Both are incident to the same corner and **three hexes meet at a corner**,
    so anchoring on the un-emitted one could pick the third hex — one the edge chain never touches.
    The chains then met only at a point, the first navigable hex carried **no `river_edges` bits at
    all**, and a tributary visibly dead-ended at the trunk in the client. Anchoring on the last
    emitted edge makes the shared edge true by construction, so the first navigable hex always
    carries that edge's class in its mask. Guarded by
    `hydrology_earthlike::navigable_chain_joins_the_edge_chain_on_a_shared_edge` (asserts the shared
    edge *and* the resulting tile mask across a 6-seed sweep) and the
    `the_navigable_handoff_anchors_on_the_last_emitted_edge` unit test. A river that goes navigable
    on its very first step has emitted nothing to anchor to, so it falls back to the edge it stopped
    at.
  - `hex_contiguous_chain` survives as a belt-and-braces bridge (a waterway whose hexes don't touch is
    not a waterway), but the corner-path construction above already makes it an identity.
  - **Rivers MERGE ON CONTACT — a navigable river is a path, not a blob** (`truncate_at_existing_channel`).
    Stems are emitted **main-stem-first**, so a tributary that reaches its trunk finds it already
    stamped and **joins** it rather than digging a second channel alongside it: the first hex that is
    an already-stamped chain's hex **or adjacent to one** terminates the chain **on that trunk hex**
    (contact is adjacency, not identity — two water hexes that touch are one body of water). The
    confluence is a genuine shared chain hex, so both chains' `river_channel` bits meet there.
    (Historically the un-concentrated flow accumulation made *several branches of one drainage* cross
    the navigable threshold independently and each trace its own chain to the same sink, packing into a
    2–4 hex wide **blob**; with a real drainage tree the branches now merge *upstream* of the threshold
    in the first place, and merge-on-contact is the backstop.)
  - **The path invariant is asserted on the CHANNEL-EXIT MASK, not on terrain adjacency**
    (`hydrology_earthlike::navigable_rivers_are_paths_not_blobs`, swept over `CENSUS_SEEDS` +
    `BLOB_REGRESSION_SEED`): a mid-chain hex links to exactly **2** channel neighbours, an endpoint to
    **1**, a confluence to **3**; 4+ is a 2D water body. *Terrain* adjacency cannot express this — a hex
    chain that turns 60° puts hex `k` adjacent to hex `k+2` (the three hexes at a bend are mutually
    adjacent, unavoidably), so a bending chain with a tributary merging at the bend **touches** 4
    navigable hexes while remaining a perfectly good path. Terrain adjacency is still bounded, at the
    geometric ceiling a chain can reach (2 chain links + one bend skip-adjacency + one merging
    tributary = 4).
  - The chain's **mouth is a `RiverDelta`**, not open water — a river deposits its load where it
    meets the sea — so the delta contract is unchanged.
- **The gameplay primitive: `Tile.river_edges: u16`** — 2 bits per odd-r direction
  (`class = (river_edges >> (2 * dir)) & 0b11`), populated for **both** hexes flanking every river
  edge, so a hex and its neighbour always agree about the river between them. Helpers:
  `river_class_on_side(dir)` / `set_river_class_on_side(dir, class)` / `has_any_river_edge()`. This
  is what a movement system will read: *entering hex H across direction d crosses
  `H.river_class_on_side(d)`*. **Nothing consumes it yet — that is expected**; movement and fertility
  effects are a follow-up. Exported on the wire as `TileState.riverEdges:ushort`.
- **Where the tributary meets the trunk: `Tile.river_inflow: u16`** — the same 2-bits-per-slot
  packing as `river_edges`, but keyed by hex **CORNER** instead of by side. An edge river runs
  *along* a side, corner to corner, so it does not end mid-edge — **it ends at a vertex**, and that
  vertex is where the water enters the navigable hex. The edge mask cannot say where: a trunk hex
  can flank three river edges (the tributary ran along three of its sides before going navigable),
  which leaves two candidate chain-ends, so the client would be guessing and would draw an arm per
  edge. So the sim states it.
  - **Corner index convention (a wire contract).** Corner `i` is the vertex at screen angle
    `60*i + 30`, **+y down** — matching the client's `MapView._hex_points`: `0` lower-right,
    `1` bottom, `2` lower-left, `3` upper-left, `4` top, `5` upper-right. Mapped onto the sim's
    `(hex, TOP|BOTTOM)` corner model by `HEX_CORNER_LAYOUT` /
    `HexGrid::local_corner_index(hex, corner)` (`hydrology.rs`): `0 = TOP(SE(H))`, `1 = BOTTOM(H)`,
    `2 = TOP(SW(H))`, `3 = BOTTOM(NW(H))`, `4 = TOP(H)`, `5 = BOTTOM(NE(H))`. Side `dir` spans
    corners `{dir - 1, dir}` (`grid_utils::hex_edge_corner_indices`).
  - **Both tables are pinned ABSOLUTELY to the client's geometry, not merely to themselves.**
    `local_corner_index_is_a_bijection_on_every_hex` / `hex_edge_corner_indices_match_the_corner_model`
    only prove *internal consistency* (six distinct corners that round-trip) — **a table rotated by one
    position passes both happily** while putting every tributary on the wrong vertex. So
    `hex_corner_layout_matches_the_clients_corner_geometry` and
    `hex_edge_corner_indices_are_the_shared_edges_endpoints` (`hydrology.rs` tests) compute each
    corner's **world position** twice — once through the sim's `(hex, TOP|BOTTOM)` model (centre at
    `x = √3·R·(col + 0.5·(row&1))`, `y = 1.5·R·row`; `TOP = centre + (0,−R)`, `BOTTOM = centre +
    (0,+R)`, +y down) and once through the client's `corner i at angle 60i + 30` circle — and assert
    the two land on the same point. That is what makes the convention a *contract* rather than a
    convention.
  - **The semantics WIDENED with the drainage network** (`docs/plan_rivers_drainage_network.md` §A).
    `river_inflow` no longer means *"this hex is a navigable chain HEAD"* — it means **"a tributary
    hands over to the channel at this vertex."** Same field, same bits, same corner convention, same
    widest-wins rule; only the *meaning* widened. Two hand-overs are recorded:
    1. a river that **outgrows the edge model itself** hands over at the head of its own navigable
       chain (the old case), and
    2. an **edge-only tributary that lands on a navigable trunk** hands over at a vertex of that
       **trunk hex — mid-chain**. That is impossible without a real network (before it, tributaries
       could only meet a trunk at its head), and it is *the* payoff: without recording it, the
       tributary's edge band ends at a bare vertex while the trunk's arms only reach its edge
       *midpoints*, and the tributary visibly dead-ends short of the water it feeds.
    Both carry the class of the **last emitted edge** (the tributary's own width where it arrives). A
    river navigable from its first step emitted no edges, has no tributary, and reports `0` — no
    invented inflow. `RiverInflow` now carries the target `hex` alongside the `corner`/`class`.
  - **The render contract: `river_channel` is load-bearing for the head/mid-chain distinction.**
    The client cannot key its head-taper off `inflow != 0` any more — that was safe only while inflow
    *meant* "chain head". It now **popcounts the `river_channel` exit bits**: **1 exit = a genuine
    chain head** (taper the channel to a point), **≥ 2 = mid-chain** (full width — no hourglass at a
    tributary junction), **3 = a confluence**. The inflow spur is drawn unconditionally. So the
    channel mask is no longer only anti-web link topology: **the sim must keep its exit count exactly
    equal to the chain's real degree at every navigable hex**, or the trunk pinches or bulges in the
    render. Both halves are landed and verified (client: `terrain_blend.gdshader` + the
    `map_rivers_midchain` ui_preview fixture).
  - **Widest-wins on collision.** Three hexes meet at a corner, so two tributaries running down
    either bank can hand over at the *same* vertex of the same hex (a confluence at a corner). One
    slot holds one class, so `widen_tile_river_class` keeps the wider (`Major` > `Minor`), which is
    also emission-order independent.
  - Helpers: `river_class_at_corner(corner)` / `set_river_class_at_corner(corner, class)` /
    `has_any_river_inflow()`. Exported as `TileState.riverInflow:ushort`. Guarded by
    `hydrology_earthlike::every_river_inflow_is_a_real_tributary_handover_vertex` — the tile's inflow
    corners are exactly the hand-overs arriving there, at the widest arriving class, each an endpoint
    of its river's last emitted edge (checked by the **hex triple** that identifies the vertex, so a
    wrong corner cannot pass), and **mid-chain hand-overs must exist** (if none happen, the network is
    still a set of parallel rivers).
- **The trunk channel is a PATH: `Tile.river_channel: u8`** — **1 bit per odd-r direction**
  (`exits(dir) = (river_channel >> dir) & 1`, `RiverChannel::{BITS_PER_DIR, SLOT_MASK}` in
  `sim_schema`): does this hex's navigable channel flow out through side `dir`? Helpers:
  `channel_exits(dir)` / `set_channel_exit(dir)` / `has_any_channel_exit()`.
  - **Why it must exist.** A navigable river is a chain of water *hexes*, and a chain is a **path** —
    a hex links to its upstream and downstream neighbours and to nothing else. **Terrain cannot say
    which those are.** The client used to arm an arm from each navigable hex's centre to *every*
    neighbour that was navigable/water/`RiverDelta`, so wherever two chains ran adjacent (which,
    before merge-on-contact, was everywhere) or a chain doubled back, every hex cross-linked to every
    navigable neighbour and the trunk rendered as a **web with triangular holes** instead of a river.
    Only the tracer knows chain membership, so the sim states it. (Merge-on-contact removes most
    adjacent chains, but the mask is still the right primitive: two *legitimate* parallel rivers, or a
    bending chain, would cross-link without it.)
  - **Populated from each `RiverSegment.navigable_hexes` chain** in `generate_hydrology`, in two
    passes so the result is independent of trace order. **Pass 1 — the chain:** for each consecutive
    pair, the exit bit is set on **both** hexes facing each other (hex `A` → dir toward `B`, hex `B` →
    the opposite dir), symmetric exactly like `river_edges`. **Pass 2 — the mouth:** a chain's final
    hex also exits toward the water it drains into (the ocean, an inland sea, or the `RiverDelta` at
    its own mouth), or the drawn river would stop one hex short of the sea. That mouth bit is the one
    **asymmetric** bit in the mask — open water carries no channel of its own, so it is not mirrored
    back. Only a genuine **dead end** earns it: a tributary that merged into a trunk also *ends* on its
    last hex, but that hex is a confluence the water already flows on through, and a second exit there
    would draw a spurious arm off the side of the trunk ("has no exit but the one back upstream" is
    the test, and it does not depend on segment order).
  - The **head** needs no exit toward its tributary — the inflow SPUR (`river_inflow`) already draws
    that; double-encoding it would put two arms on one vertex. A hex on two chains (a confluence)
    accumulates the **union** of the bits (OR-ed, never overwritten).
  - Exported as `TileState.riverChannel:ubyte`. Guarded by
    `hydrology_earthlike::navigable_channel_exits_are_the_chain_and_only_the_chain`: symmetry,
    end-to-end chain connectivity, every chain reaching its water, and the **anti-web invariant** — *no
    navigable hex exits toward a navigable hex that no chain actually runs between*.
- **Wire format.** The `HydrologyOverlay` / `RiverSegment` / `HydrologyPoint` polyline tables are
  **deleted** from the snapshot and delta. The per-tile `riverEdges` + `riverInflow` + `riverChannel`
  masks plus the `NavigableRiver` terrain fully determine the render, so a parallel polyline overlay
  would be duplicated state. The client draws the trunk channel from **`riverChannel`** (arming *only*
  the sides whose bit is set — never inferring links from terrain), the edge rivers from `riverEdges`,
  and joins a tributary to its trunk hex at the `riverInflow` **corner** — never at a side midpoint,
  and never one arm per flanked edge.
- **Delta placement is gentle-coast gated.** A delta is a depositional fan, so it only forms where
  the river meets the water across low ground — reusing the shelf's own
  `ShelfConfig.coast_height_threshold` rather than inventing a second threshold. A river that meets
  the sea at a cliff has no delta (it is an estuary). This also keeps `reconcile_coastal_shelf`'s
  "no DeepOcean touches gentle land" invariant coherent: every delta is gentle land, so every delta
  gets a shelf seaward of it.
- **Config** (`hydrology` block of `simulation_config.json` → `HydrologyOverrides`, overriding the
  per-preset `river_*` keys in `map_presets.json` — overrides > preset > default):

  | Key | Default | Meaning |
  |---|---|---|
  | `river_density` | 1.0 | How wet the map reads. A **multiplier on the channel threshold**: `effective = river_channel_min_discharge / river_density` (higher density → lower threshold → more channels). Clamped to `[0.1, 5.0]`. |
  | `river_fill_epsilon` | 1e-5 | The depression fill's drainage gradient across flats. Far above `f32` noise at map elevations (~1e-7), far below the jitter. |
  | `river_flat_jitter` | 5e-4 | Elevation tie-break amplitude. **Must stay `≫ river_fill_epsilon`** (so it decides ties the fill cannot) **and `≪` real relief** (so it can never reorder genuine terrain). |
  | `river_base_runoff` | 0.2 | Per-hex runoff floor, so an arid basin still trickles. |
  | `river_moisture_weight` | 0.8 | How hard rainfall drives discharge. With `base_runoff = 0.2` a fully-wet hex contributes exactly **1.0** — which is what makes discharge read as hex-equivalents. |
  | `river_channel_min_discharge` | **3.0** | The network-extraction threshold. |
  | `river_class_major_min_discharge` | **12.0** | Minor → Major. |
  | `river_class_navigable_min_discharge` | **25.0** | Major → `NavigableRiver` hex chain. |
  | `river_navigable_enabled` | true | Kill switch for the navigable tail. |
  | `river_navigable_min_hexes` (`navigable_min_hexes` in the override block) | **3** | Shortest navigable hex chain that still reads as a river; a shorter run is demoted to the edge (`Major`) form (a 1–2 hex navigable is a puddle). |
  | `river_min_length` (`min_length` in the override block) | 2 hexes | The **only** noise gate. Keep it low. |

  **The three discharge thresholds are `f32` and ABSOLUTE.** Discharge means *precipitation-weighted
  upstream drainage area in hex-equivalents*, so a river draining 300 wet hex-equivalents is a big
  river on an 80×52 map and on a 256×192 map alike; a bigger map simply has more of them and longer
  ones. Do **not** re-express them as a fraction of the map maximum — one giant basin would skew it.

  **Determinism** is guarded by `integration_tests/tests/determinism.rs`: no `HashMap`/`HashSet`
  iteration order in the routing or extraction, no unseeded RNG, every sort has an explicit index
  tie-break, and the flat jitter is a pure hash of `(world_seed, corner_index)`.

  **The three discharge thresholds were tuned from a 45-cell sweep**, not guessed:
  `hydrology_earthlike::drainage_threshold_sweep` (`#[ignore]`d) crosses
  `channel × major × navigable` over `CENSUS_SEEDS` and reports rivers/edges/class-split/navigable
  runs per cell. Re-run the sweep before changing any of the three. **They were NOT re-tuned for the
  erosion pass** (below) — they were deliberately held fixed so the erosion A/B is attributable.

  **Measured** shape at those thresholds, on the **eroded** landscape
  (`hydrology_earthlike::drainage_census`, `#[ignore]`d; run with `-- --ignored --nocapture`),
  aggregate over 6 seeds of an 80×52 earthlike map (after the "connect to the mouth + demote landlocked/
  puddle navigable" fix): **14.5 rivers per map**, 81.1% Minor / 18.9% Major, **~2.3 navigable segments
  / ~11 navigable hexes per map** (14 segments / 68 hexes over the 6-seed sweep, min run 3, 0 landlocked
  — the shore-hugging false chains, the landlocked dead-ends, and the 1–2 hex puddles are all gone);
  land-corner accumulation p50 = 0.60 / p95 = 10.2 / p99 = 64.4 / **max 587**; corner confluences
  **11.6%** of land corners (4.1% before the drainage-network rewrite); Strahler on the drainage tree
  o1 = 12366, o2 = 2246, o3 = 837, o4 = 254, o5 = 34 (the accumulation/confluence/Strahler figures read
  off the corner network, which the segmentation fix does not touch). Per-seed spread is large and
  *should* be — see the verdict below.

### Fluvial erosion — the heightfield the drainage runs on
The drainage-network rewrite left the *router* correct and the *landscape* wrong: continents were
**sponges** (48–64% of a continent's tiles touched water, because the coastline is an iso-contour of
fractal noise) and they **shed radially** with no trunk valleys to capture drainage across a divide.
`heightfield::apply_fluvial_erosion` attacks the landscape directly, at the end of
`build_elevation_field` — **before** `mapgen::generate_land_mask`, which is the whole point: the mask
ranks tiles by elevation, so the coastline *is* a level set of this field, and reshaping the field
reshapes the coast.

- **The model** is the classic landscape-evolution equation minus uplift: `∂z/∂t = D∇²z − K·A^m·S^n`,
  iterated on the **square raster** (D8 — the hex/corner graph is hydrology's and stays there). Per
  pass: priority-flood the depressions (+`fill_epsilon`), route D8 steepest descent on the *filled*
  surface, accumulate **uniform** unit drainage (this is landscape evolution, *not* the
  precipitation-weighted discharge model), incise, then diffuse. Deterministic: pure arithmetic, no
  RNG, explicit index tie-breaks on every sort and every descent comparison.
- **Both terms are needed, and they do different jobs** (measured, not assumed): **stream power**
  carves the trunk valleys that give a continent *capture* but leaves the coastline noise untouched
  (it is concentrated where `A` is large, which is nowhere near a headwater coast); **diffusion** is
  what planes that noise off and *de-sponges*. Incision alone moved coastal 59.2% → 57.5%; with
  diffusion it reaches **52.8%**.

> #### Two things the pass had to learn the hard way — do not "simplify" them away
>
> **1. Base level is the land-mask's rank contour, NOT `sea_level`.** On the earthlike preset only
> **24–37%** of cells sit above `sea_level = 0.62`, while `macro_land.target_land_pct` claims **38%**
> of them for land — so the coastline actually falls at elevation **0.55–0.61, *below* sea level**.
> A pass that freezes everything under `sea_level` freezes the entire coastal band it exists to
> reshape, and measures as a **no-op** (it did: coastal 59.2% → 58.8%). `heightfield::land_contour`
> computes the real thing.
>
> **2. A valley incised *to* base level DROWNS.** The mask ranks by elevation, so a trunk cut to the
> contour ranks below it and becomes a sea inlet — taking its basin with it (measured: seed 4's
> biggest basin collapsed **546 → 99**). `incision_floor` exists to bound this; it ships at **0.0**
> because measurement said the drowned stretches read as *estuaries* and leave the coast **smoother**
> — but the lever is there, and the failure mode is real.
>
> **3. `anchor_contour_to_sea_level` is what lets the carving reach hydrology at all.**
> `restamp_elevation`'s lowland branch is only order-preserving *above* sea level; below it,
> `((v − sea_level)/(1 − sea_level)).clamp(0,1)` is an **order-destroying clamp** that plates every
> such cell — **a third of all land** — flat onto exactly `sea_level`. Carving valleys there is
> pointless: they are erased before hydrology sees them. So the pass finishes with a strictly
> monotone, piecewise-linear rescale that puts the coastline exactly on `sea_level`, making the
> pipeline's "land ⟺ above sea level" assumption *true*. Monotone ⇒ it cannot reorder the field, so
> the land mask still picks the same tiles.

**Config** — the `erosion` block of each preset in `map_presets.json` (`ErosionConfig`):

| Key | Default | Meaning |
|---|---|---|
| `enabled` | true | Kill switch. `false` reproduces the pre-erosion maps **exactly**, and is the A/B control the census measures against. |
| `iterations` | 40 | Passes. Past ~40 the sponge stops improving and the big basins start planing away. |
| `erodibility` | 0.1 | Stream-power `K`. Below ~0.05 nothing carves; above ~0.3 incision **saturates** against the downstream clamp (the result stops depending on `K` at all) and the coast gets *worse*. |
| `area_exponent` | 0.5 | `m` — classic. |
| `slope_exponent` | 1.0 | `n` — classic. |
| `timestep` | 0.1 | `Δt`. Only `K·Δt` matters; split for readability. |
| `min_slope` | 1e-4 | Slope floor, so a filled flat still incises and can cut itself an outlet. |
| `fill_epsilon` | 1e-6 | The priority-flood's gradient across a filled flat. |
| `diffusivity` | 1.0 | Hillslope `D`. **The term that de-sponges.** Past ~2 it planes real relief off the continent. |
| `incision_floor` | 0.0 | How far above base level a valley may cut, as a fraction of the land band. See note 2. |
| `anchor_contour_to_sea_level` | true | See note 3. |

**Measured A/B** (`hydrology_earthlike::drainage_census`, `#[ignore]`d, 6 seeds, 80×52, shipped
river thresholds held at 3.0/12.0/25.0 so the comparison is clean):

| metric | erosion OFF | erosion ON |
|---|---|---|
| coastal tiles of the largest landmass (**SPONGE** — must fall) | **59.2%** (spread 14.3) | **52.8%** (spread **9.6**) |
| biggest basin / largest landmass (**CAPTURE** — must rise) | 11.0% (spread 39.5) | 13.3% (spread 34.1) |
| navigable rivers (post "end-at-water" fix) | 21 segments / 67 hexes / **max run 7** | 21 / 75 / **max run 21** |

> **Honest verdict: one of the two failures is fixed, the other is only dented.** The **sponge is
> genuinely better** — every seed improves and the spread halves — and the **~13-hex navigable
> ceiling is gone** (longest river 7 → **21** hexes post the "end-at-water" fix; the ceiling was never
> the threshold, it was the landscape). **Capture is not fixed.** The mean barely moves and the spread stays huge: seed 5 goes
> 4.7% → 21.0% and seeds 1/3 roughly double (2.2 → 4.2, 3.5 → 5.2), but seeds 1/3/TEST are still
> single-digit while seed 4 still runs at 38%. **Incision deepens the valleys a continent already
> has; it does not move its divides.** The divides come from the continent-scale fbm, so the next
> lever is the *noise*, not the erosion — see `TASKS.md`.
>
> `apply_coastal_smoothing` was **measured, not assumed** (the suspicion was that its 3×3 blur would
> soften the incised valleys right where they matter). It does not blunt the result: the sponge metric
> is **bit-identical** with the blur zeroed (the land mask is decided from the base field *before*
> `restamp_elevation` ever runs), and zeroing it actually made rivers **worse** (max navigable run
> 25 → 15). Leave it alone.

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

**Continental shelf width** (`classify_bands` + `effective_shelf_width`, `mapgen.rs`; `ShelfConfig`, `map_preset.rs`): `ContinentalShelf` is the ocean band within a computed distance of the coast (slope collapses to `DeepOcean` downstream, so only the shelf boundary affects ocean composition). The model mirrors real margins — a **continuous ≥1-tile shelf off gentle (passive-margin) coasts, and deep water right at steep/cliff (active-margin) coasts** — via two knobs on top of the width scaling:
- `min_width_tiles` (default **1.0**) — floors the computed width so a qualifying coast gets a *continuous* ≥1-tile ring instead of a sub-tile sparse fringe. Applied after the `width_frac`/`width_exp` (or `width_tiles`) computation, so a preset that bumps `width_frac` still scales the shelf wider than the floor on big maps.
- `coast_height_threshold` (default **0.10**, earthlike **0.10**) — the coast-height gate. A shelf-candidate ocean tile becomes `ContinentalShelf` only if the coast land it abuts rises *gently*: the MIN normalized rise (`elevation.sample − sea_level`) over its immediately-adjacent land tiles is **below** this. Cliff/mountain/highland coasts (rise ≥ threshold) instead show `ContinentalSlope`→deep water at the edge. On earthlike, lowland coasts rise into the compressed band `[sea_level, elevation_base]` (≤ ~0.10) while mountain-mask coasts jump to ≥ ~0.16, so the threshold sits in the bimodal gap and cleanly splits gentle vs. steep. This self-limits the shelf %: steep coasts add zero shelf, so the 1-tile floor doesn't blow the fraction up on small maps the way a blanket ring would.

  **The immediate coastal ring is HEX-aware (odd-r 6-neighbour).** The default 1-tile shelf ring's coast-adjacency uses the authoritative odd-r hex neighbours (`grid_utils::hex_neighbors_wrapped`, wrap-aware — the same adjacency gameplay + the client render), not 4-connected square neighbours. An ocean tile joins the ring iff it is hex-adjacent to ≥1 Land tile **and** the min rise over its Land hex-neighbours is `< coast_height_threshold`. This closes the old hex-diagonal gaps: the 4-cardinal set covers only two (E/W) of the six hex directions, so before the fix a gentle coast could sit directly against DeepOcean on a hex-diagonal (`min_adjacent_coast_rise` + `classify_bands`, `mapgen.rs`). The broader worldgen distance transforms (ocean-distance, mountain masks, rivers) remain **square-grid** — pre-existing modeling, out of scope; only the immediate shelf ring is hex-exact (a full hex distance-transform for `width_frac`-widened shelves, `full > 1`, is the follow-up). Guarded by `mapgen::tests::earthlike_bands_have_no_gentle_coast_shelf_gap` (0 DeepOcean-vs-gentle-Land hex adjacencies over real earthlike coastlines) + `classify_bands_shelf_covers_hex_diagonal_coast`.

  **Final reconciliation pass — the shelf is hex-exact on the *final* map, not just at band time.** `classify_bands` decides the shelf early (stage 6), but later Startup stages repaint terrain near the coast *after* the shelf exists: `generate_hydrology` stamps `RiverDelta`/`Floodplain`/`FreshwaterMarsh` at river mouths, and `apply_tag_budget_solver` paints polar `Tundra` over near-shore ocean — each creating fresh gentle-land-vs-`DeepOcean` adjacencies with no shelf between them (band-level zero-gap ≠ final-map zero-gap). `reconcile_coastal_shelf` (`systems.rs`) is a deterministic post-pass registered in the Startup `.chain()` **right after `apply_biome_palette_clamp`** (so after hydrology + tag solver + palette clamp — the last word on ocean tiles): every `DeepOcean` tile odd-r hex-adjacent (`grid_utils::hex_neighbors_wrapped`, wrap-aware, honoring the active `map_topology.wrap_horizontal`) to a **gentle** land tile — non-`WATER` tags, rise `elevation.sample − sea_level < coast_height_threshold` (the SAME gate + hex convention as `classify_bands`) — is reclassified to `ContinentalShelf` (a `must_have` palette biome, so no palette conflict). So downstream-created coasts (deltas, marshes, solver tundra) all get a shelf seaward, while **steep** coasts (every land hex-neighbour rises `≥` threshold) still keep deep water right at the edge. Guarantees on the final map: **no `DeepOcean` tile touches gentle land.** Guarded by `integration_tests/tests/shelf_ratio.rs::earthlike_no_deep_ocean_touches_gentle_land_on_final_map` (0 gaps across sizes/seeds, + a steep-coast-keeps-deep-water assertion) and `earthlike_delta_and_marsh_coasts_have_shelf_not_deep_water`.
- `width_tiles` (default 2) — legacy absolute band width, used only when `width_frac` is unset (e.g. `polar_contrast`). `width_frac` + `width_exp` (earthlike) scale the pre-floor width with map size as `width_frac * min(w, h)^width_exp`.

  Because the shelf is now a ~1-tile ring off *most* coastline, the fraction is **no longer** the old size-invariant 5-8%: it varies with coastline steepness and **shrinks as the open ocean grows** — measured full-pipeline (slope folded into deep water) with the hex-exact ring **plus** the final reconciliation pass it runs ~29-33% of ocean at 80×52 down to ~14% at 256×192 (a touch higher again than the band-only ring, since the post-pass also stamps shelf on the hydrology/tag-solver coasts; re-measured after the border-ring bathymetry fix below, which removed the orphaned offshore shelf the drowned border land used to strand). Guarded by `integration_tests/tests/shelf_ratio.rs`: a per-map sanity band (6-50%) plus the model assertion that coast land next to shelf tiles is lower than coast land next to deep-water-at-the-edge tiles. This is a pure ocean-tile reclassification — it does **not** touch the land mask, so mountains/rivers/land ratio are unchanged.

  The gate keys off the *immediately-adjacent* (hex-neighbour) coast land, which fully covers the 1-tile default (every shelf tile touches land). Deferred: a preset that widens the shelf past `d==1` leaves outer-ring tiles ungated (they touch no land, so they pass) and those outer rings still ride the square-connected `ocean_distance` — carrying the nearest-coast rise through a hex distance-transform is the follow-up for wide shelves. Also still deferred: a true *depth-based* shelf would need real offshore bathymetry (today ocean elevation is fractal noise with no coast-relative deepening); and if the narrower shelf's reduced `CoastalUpwelling` forage frontage matters for gameplay, lock the `Coastal` tag to stamp compensating `TidalFlat` (the tag solver's coastal pass). Neither shipped preset locks `Coastal` today.

**Elevation ↔ biome coupling** (`restamp_elevation`, `mapgen.rs`): mountain biomes come from the tectonic mountain mask + relief, so the elevation field is tied to that same signal to keep them consistent (mountains genuinely tall — see the `mountain_tiles_out_top_lowland_tiles` regression test). Every mountain-mask tile is floored into `[elevation_base, 1.0]`, ordered by relief and scaled by per-type prominence; non-mountain land is compressed into `[sea_level, elevation_base]`. Tunables live in each preset's `mountains` block: `elevation_base`, `fold_prominence`, `fault_prominence`, `volcanic_prominence`, `dome_prominence`, `belt_texture` (small spine-vs-edge elevation texture added on top of the relief floor; bounded so it never reorders relief bands). The non-mountain `elev ≥ high_dry_elevation → CanyonBadlands` / `elev ≥ high_wet_elevation → RollingHills` cutoffs (`terrain.rs`) live in `terrain_classifier` and default to the top of the compressed lowland band.

**Highland biomes are mask-driven, never noise-driven.** `classify_terrain` (the base climate classifier) does NOT pick AlpineMountain/HighPlateau/CanyonBadlands/etc. — it has no real elevation, so it used to invent them from a tile hash and scatter flat "mountains." Mountain biomes now come only from the tectonic mask (`select_mountain_terrain`) + the real-elevation `terrain.rs` branches. `apply_belt_relief` (`mapgen.rs`) scales belt-tile relief by belt strength (`mountains.relief_belt_gain`, default 1.2) so belt cores clear the AlpineMountain relief threshold (`terrain_classifier.alpine_relief_threshold`, default 1.45) and taper to plateaus/hills — genuine Alpine spines that are also tall. Polar rows are skipped (they keep their low-relief-basin tuning). Regression guards: `mountain_tiles_out_top_lowland_tiles`, `alpine_biome_tiles_are_tall`.

**Number of ranges** is emergent tectonics: land connected-components → plates (area buckets, ≤4/continent) → fold belts form only where two plates' drift *converges* (`dot <= mountains.belt_convergence`, `derive_mountain_mask`). Drift is radial-outward so most boundaries diverge; raising `belt_convergence` toward 0 (earthlike default **0.25**; polar_contrast keeps the tighter **−0.1** to preserve its low-relief-basin contrast) lets more boundaries become ranges. Range count also scales strongly with **map size** — a full 256×192 map has 30+ ranges, an 80×52 "Standard" ~4–13, a 56×36 "Tiny" ~2–6.

**`classify_terrain`'s map-border "edge rings" are LEGACY, preset-less-only.** The classifier opens with three `edge < coastal_deep_ocean_edge / coastal_shelf_edge / coastal_inland_edge` early-returns that stamp DeepOcean / shelf / InlandSea+marsh. `edge` is the distance to the **map frame**, not to a coastline: it was the only coastline proxy the pre-bands (preset-less) world had. Under a preset the map has **real bathymetry** — `classify_bands` already partitioned it into Land / ContinentalShelf / InlandSea / DeepOcean, and `terrain_for_position_with_classifier` is called *only* for band-`Land` tiles — so running the rings there noise-coin-flipped **248–295 band-`Land` tiles per 80×52 map (~16–19% of all land)** into water biomes hugging the map border, deleting the land out from under legitimate shelf rings (118–153 **orphaned** shelf tiles with no land hex-neighbour, sitting 3–7 hexes out) and pinching off isolated deep pockets. The rings are therefore **skipped whenever real bathymetry is present** (`BathymetryContext::Present`, derived from the caller passing `Some(elevation)` — the *context*, never a config flag), and the tile falls through to the normal polar/anomaly/humidity **land** ladder. The preset-less fallback path passes `None` → `BathymetryContext::Absent` and keeps its historical behavior exactly. Invariant: **a band-`Land` tile can never end WATER-tagged.** Guards: `mapgen::tests::earthlike_band_land_never_ends_water_tagged`, `mapgen::tests::earthlike_shelf_is_never_orphaned`.

**Tag Budget Solver**: After biome stamping, iterates locked tag families (water → wetlands → fertile → coastal → highland → polar → arid → volcanic → hazardous) nudging tiles until coverage falls within `tolerance`.

  **`terrain_tag_targets.Water` MUST track `1 − macro_land.target_land_pct`.** The landmask decides land vs. water; a locked `Water` target only tells the solver what that same map should *already* look like. Disagreement makes the solver invent bathymetry the pipeline never modeled: too high and its **add-water** branch scatters `DeepOcean` over `COASTAL`-tagged land (earthlike's old `Water = 0.65` vs `target_land_pct = 0.38` ⇒ 62% water would have drowned ~125 coastal tiles — the border-ring bug was accidentally supplying that missing 3%); too low and its **remove-water** branch mass-converts ocean into `Tundra`/`AlluvialPlain`. earthlike now sets `Water = 0.62` (`= 1 − 0.38`) and the water pass is **inert** (0 conversions in either direction on all sampled seeds). Any preset that changes `target_land_pct` must move `Water` with it (see the `_comment_water_target` note in `map_presets.json`).

**Per-Map Biome Palette** (`biome_palette.rs`, design `docs/plan_biome_palette.md`): a curated,
seed-driven, map-size-scaled subset of the 37 biomes chosen at world-gen time — small maps read
legibly, large maps stay rich, and the full library is preserved for replay variety. **This is how
maps generate now, not an opt-in mode.** Each biome carries an intrinsic `BiomeNiche` (8-way
partition) + `must_have` flag (`terrain.rs` `biome_niche`/`biome_must_have`, folded into
`TerrainDefinition` by `def`). The `BiomePalette` resource is built in `spawn_initial_world` from
`world_seed ^ PALETTE_SEED_SALT`: per niche it keeps the `must_have` members and seed-samples up to
`K` (size-interpolated from the preset's `biome_palette` block — `small_map_tiles`/`large_map_tiles`
+ per-niche `k_small`/`k_large`), then force-includes the solver's locked-tag fallback biomes.
Enforcement is a **climate-aware niche-nearest remap** (`BiomePalette::remap(biome, is_polar)`): at
the `bias_terrain_for_preset` seam and again in the post-solver `apply_biome_palette_clamp` system
(inserted in the Startup chain right after `apply_tag_budget_solver`), any off-palette biome is
replaced by an allowed member of the same niche — polar tiles only remap to POLAR-tagged members, so
the palette never stamps temperate plains/marshes at the poles; `RiverDelta` is `must_have` so real
river mouths pass through. **Must-have set** (`biome_must_have`, 9): DeepOcean, ContinentalShelf,
InlandSea, AlluvialPlain, PrairieSteppe, Tundra, RiverDelta, Glacier, **NavigableRiver** (the last
for the same reason as `RiverDelta` — it is hydrology-placed, and off-palette it would remap to
`DeepOcean` and cut the continent in half with open sea; adding it gave the Ocean niche a **fourth**
must-have, so earthlike's Ocean `k_large` was widened 4 → 6 to keep the two *interchangeable* ocean
flavours, CoralShelf and HydrothermalVentField, reachable at all). `must_have` is reserved for a
single *physically-gated* member inside an otherwise-thinnable niche: `InlandSea` in Ocean (else
off-palette inland water renders as DeepOcean) and `Glacier` in PolarLowland (else a tall polar peak
remaps down to flat Tundra — it's the polar analog of AlpineMountain, placed only where relief clears
`alpine_relief_threshold`). **Physically-gated-vs-interchangeable principle** (`docs/plan_biome_palette.md`
§3.2b): thinning only ever applies to interchangeable flat-land climate/flavor niches. The fully
physically-gated niches — `Highland` (relief/elevation/mask regimes) and `Volcanic` (volcanic-arc
mask) — are **never thinned**: their palette `K` is set to full membership at both endpoints
(`Highland` 5/5, `Volcanic` 3/3, in the `BiomePaletteConfig` default + earthlike JSON), so AlpineMountain
and every highland/volcanic member is always available and never remapped away. Un-thinning Volcanic
never forces volcanoes onto a non-volcanic map (the niche is simply absent with no arc + no fumarole
hit). Do **not** add other highland biomes to `must_have` — the niche's full `K` already keeps them
always-available while staying tunable. Reconciled with the
tag solver by construction (force-included fallbacks) plus the clamp as insurance. Also revives 3
previously-unreachable biomes (`§3.6`): Glacier (high-relief polar mountains), BasalticLavaField
(low-relief volcanic mask via `terrain_classifier.basaltic_relief_threshold`), AquiferCeiling (one of
the six anomaly biomes) — so "all 37" is now literal. **Anomaly rarity:** anomaly/"discovery" biomes
(crater/sinkhole/karst-cavern/fumarole/volcano/aquifer) are gated in `classify_terrain` by a config
lever `terrain_classifier.anomaly_fraction` (default 0.04 — ~4% of eligible flat lowland, split evenly
across the six), replacing the old fixed 6-of-16 (~37%) slice that blanketed the land. **Niche note:** BorealTaiga is homed in `PolarLowland` (not `FertileLowland` as
the design table lists) because it is POLAR-tagged — see the comment on `biome_niche`. Biome ids are
unchanged (no client/schema impact). Independent of terrain-texture work.

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
marsh_grazer (long routes); big game deer/boar/**aurochs** (2–3 tiles); small game
rabbit/fowl/**crag_goat** (~1 tile, stationary). The two **pennable grazer livestock**
(Grazing 2d) are `pen`-ceiling: **Wild Aurochs** (🦬, wild r 0.09 → slow ranch cattle) on
grass + woodland edge (`savanna_grassland`/`temperate_forest`/`mixed_woodland`), **Crag
Goats** (🐐, wild r 0.22 → fast hardy hill stock) on highland/dry-upland
(`montane_highland`/`semi_arid_scrub`).

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
spectrum: Sustain takes the **Maximum Sustainable Yield** (`sustainable_yield(..)` — regrowth at
the most-productive biomass K/2, so a group *at carrying capacity* still yields a positive skim and
a collapsing group yields nothing; Sustain draws the group toward K/2 and holds it there), Surplus
takes that × `follow.surplus_multiplier`
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
worker count (0 unassigns; clamps to free headroom); `cancel_order` clears all assignments + stops
movement (fully idle). The snapshot exports `laborAssignments`/`idleWorkers`/`workingAge`, and still
summarizes `activity` (target-kind with most workers) + `huntMode` (largest Hunt's policy) for the
pre-3b client. Husbandry re-homes here: a Sustain Hunt on a Thriving herd accrues domestication. The
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
- *Emergent accrual*: in `advance_labor_allocation` (Population), a **Sustain** Hunt assignment on a
  **Thriving** herd adds `husbandry.progress_per_turn` for the acting faction (sets
  `owner` on first accrual; only the owner accrues). At `1.0` the herd auto-domesticates.
- *Decay + yield*: `advance_husbandry` (`fauna.rs`, `TurnStage::Logistics` after
  `advance_herds` — runs *before* the same turn's accrual, so a Sustain-followed herd nets
  `progress_per_turn − decay_per_turn` and an untended one only decays by
  `husbandry.decay_per_turn`, clearing `owner` at 0). A **domesticated (pastoral)** herd pays its
  owner the **MSY of the pastoral ecology** each turn — `fauna::managed_yield_biomass` under
  `husbandry.pastoral.ecology` (`r` = 0.25) → `hunt_provisions`, split evenly across the owner's bands.
  It stays **passive** (no worker, no upkeep — a roaming herd grazes the land for free) but the harvest
  now **draws the herd down**, which is what makes it sustainable (see "The husbandry yield ladder"
  below). The retired flat `provisions_per_biomass` (0.01) paid a share of standing **stock** with no
  draw-down: a Red Deer herd at capacity printed 12 food/turn — *sixteen* ~30-person bands' entire
  demand — free, forever.
- *Collapse immunity*: `regrow_biomass` uses plain `logistic_regrowth` (never the collapse
  branch) for a domesticated herd — a managed group recovers and never crashes.
- *Explicit claim*: the `domesticate <faction_id> <herd_id>` command (`handle_domesticate`,
  full proto/runtime/text/server plumbing) lets the owner claim a herd **early** once
  `domestication_progress ≥ husbandry.claim_threshold` (snaps progress to 1.0); rejected for a
  non-owner or an under-threshold herd. The emergent Sustain-follow is how progress is built.
- `HerdRegistry::domesticated_count(faction)` is the seam the future `SedentarizationScore`
  (`TASKS.md`) reads for its "domestication progress" input.

### The husbandry yield ladder — every rung pays MSY

Authoritative design: `docs/plan_corral_managed_population.md`. **Management buys a *growth rate*, not
a licence to eat the standing stock.** Every rung of the ladder pays the Maximum Sustainable Yield; the
rungs differ *only* in the **ecology** that MSY is computed against, and in what that ecology costs you:

| Rung | Ecology | `r` (Grazing 2d — **per-species**) | Costs |
|---|---|---|---|
| Wild, Sustain hunt | `ecology` | `wild_r` (rabbit 0.35 · deer 0.10 · mammoth 0.04) | a worker |
| Mobile domesticated (**pastoral**) | `husbandry.pastoral.ecology` | `min(cap, wild_r × pastoral_gain)` (gain 1.5) | **none — passive** |
| Corral, building | `corralling_yield_fraction × MSY` | — | a worker, 25 turns |
| Corral, finished (**pen**) | `husbandry.pen.ecology` | `min(cap, wild_r × pen_gain)` (gain 3.0, cap 0.75) | a worker + **feed (footprint-offset)** + pinned |

- **Grazing 2d retired the flat pastoral 0.25 / pen 0.90.** The managed rungs now scale each species'
  **own wild `r`** by `husbandry.pastoral_gain` (1.5) / `pen_gain` (3.0), clamped to
  `husbandry_regrowth_cap` (0.75) — a penned rabbit (`r` 0.75, booms) and a penned mammoth (`r` 0.12, a
  long-haul investment) are different economies. This also fixes the fast-breeder pastoral inversion
  (pastoral `r` = `wild_r × 1.5 > wild_r` for every species). `fauna::herd_ecology` folds the per-species
  rate in; `pen_ecology_for` / `pastoral_ecology_for` are the seams, `managed_regrowth_rate` the `wild_r ×
  gain → capped` map.
- **A penned herd's `K` is its FENCED FOOTPRINT's graze flow** (`hex_range_tiles(corralled_at,
  pen_radius)`), recomputed each turn — penned herds are no longer frozen and `pen.capacity_fraction` /
  `pen_capacity` are **deleted** (a penned herd's `K` is just `herd.carrying_capacity`, so
  `herd_capacity` collapses to that field for every herd). A penned herd **grazes its footprint**
  (escapement-floored, like a wild herd) and the grass it eats **offsets its keeper's larder bill**:
  `larder_upkeep = pen.upkeep_per_biomass × biomass × (1 − pasture_fraction)`, `pasture_fraction =
  clamp(footprint_intake / (fodder_per_biomass × biomass), 0, 1)`. A pen on lush steppe feeds itself for
  free (`pasture_fraction → 1`, larder → 0); a **wholly-barren** footprint keeps the herd's frozen `K`
  and pays the full larder bill (the pre-2d worst case, preserved). See "Phase 2d".
- **`fauna::herd_ecology(herd, fauna)` and `fauna::herd_capacity(herd, fauna)` are THE single source of
  that mapping.** `regrow_biomass`, `hunt_policy_ceiling`, `hunt_forecast`, `refresh_ecology_phase`,
  the expedition ceiling/bound/simulation — **every** consumer resolves through them. **No call site may
  re-derive an ecology or a capacity**: a second copy of this mapping is exactly how a forecast starts
  promising a number the take won't pay (see "Pre-commit Yield Forecast").
- **The managed harvest draws the herd down**, and that is what makes it sustainable: it converges the
  herd on `K/2` and holds it there, paying `r·K/4` forever. Both husbandry rungs take it through the one
  shared helper **`fauna::managed_yield_biomass`**.
- **You are not paid twice for the same animals.** `advance_husbandry` **skips the passive pastoral
  rung** for a herd a labor assignment worked last turn (`Herd::worked_this_turn`, a transient flag set
  in the Hunt arm of `advance_labor_allocation` — the same one-turn lag as `corralled_tended_this_turn`).
  A band working the herd is already paid through the labor arm (its `hunt_take`, the `Corral` build dip,
  or the pen's harvest). **Without the skip the corral's investment cost becomes a profit**: a Red Deer
  under construction would pay the dip (0.50 × 1.50 = 0.75) **plus** the passive rung (1.50) = 2.25/turn
  — *more* than the 1.50 of walking away — recreating on the animal side exactly the "free path" the
  intensification ladder exists to delete (a plain Sustain hunt on a tamed herd was double-paid the same
  way). With the skip, building the pen costs a real **0.75/turn for 25 turns (~19 provisions forgone)**,
  recouped ~9 turns after completion (pen net ≈3.66 vs pastoral 1.50 at `B*`).
- **It is constant-*escapement* MSY** — `take = min(peak_regrowth(K), max(0, B − K/2))` — **not** the
  constant-catch `sustainable_yield` a *wild* `Sustain` hunt takes. The sim regrows in Logistics and
  harvests in Population, so a constant-catch take is evaluated at the **post**-regrowth biomass; above
  `K/2` that is harmless (both forms cap at MSY and converge on `K/2`), but **below `K/2` it takes
  `g(B + g(B)) > g(B)`** — strictly more than the herd grew. At the wild `r` = 0.05 that leak is a
  rounding error; at the pen's fast per-species `r` (up to 0.75) it is fatal: a **fully fed** pen knocked below `K/2` spirals
  to zero in ~12 turns and can never recover. Escapement never takes a herd below `K/2`, so a depleted
  managed herd **rebuilds** (yielding less, or nothing, while it does) and then pays `r·K/4` forever —
  stable from *both* sides, same yield at capacity and at the operating point.
- A managed harvest therefore **never overdraws**: its yield telemetry reads `actual == sustainable`
  (no ⚠), and `workers_needed = TENDED_SOURCE_WORKERS_NEEDED` (maintenance labor, not scaling gather).

Ecology/husbandry tunables live in the `ecology` (`regrowth_rate`, `collapse_fraction`,
`collapse_rate`, `stressed_fraction`, `extinction_floor`), `immigration`, and `husbandry`
(`progress_per_turn`, `decay_per_turn`, `claim_threshold`, **`pastoral.ecology`**, **`pen`** — see
"Corral" — plus `corralling_yield_fraction` / `corral_build_progress_per_turn` /
`knowledge_progress_per_turn` / `knowledge_completion_threshold`) blocks of `fauna_config.json`.
**`FaunaConfig` is validated** (`FaunaConfig::validate`, run inside `from_json_str`, so every load path
— builtin, default file, `FAUNA_CONFIG_PATH` override — is covered; the `expedition_config.rs` /
`crisis_config.rs` convention). A broken invariant is logged at **error** level
(`fauna_config.invalid_rejected`) and the known-good builtin is used instead. Enforced: **the pen's
best-case net-positive floor** (Grazing 2d §2.4 — `pen.upkeep_per_biomass < r_pen · p / (2 + r_pen)`
for the **fastest** species' `r_pen = min(cap, max_wild_r × pen_gain)`; a slow breeder or poor-pasture
pen may run at a **loss by design**, so the old every-pen guarantee is retired for a best-case sanity
floor), **the ladder is monotone as gains** (`pen_gain > pastoral_gain > 1`), ordered ecology phase
bands (`extinction_floor < collapse_fraction < stressed_fraction < 1`) in all three ecologies, every
`regrowth_rate > 0`, `husbandry_regrowth_cap > 0`, `0 ≤ pen.starve_shrink_rate ≤ 1`,
`0 < corralling_yield_fraction < 1`, `corral_build_progress_per_turn > 0`,
`knowledge_progress_per_turn > 0`, `0 < knowledge_completion_threshold ≤ 1`,
`progress_per_turn > decay_per_turn`, `hunt.provisions_per_biomass > 0`, and the follow/market bounds.

### Corral (Intensification Rung 1c)

The **animal mirror of the tended patch** (`docs/plan_intensification.md` §4b) — the place-bound form
of the *existing* herd domestication, and the fauna-side twin of "Cultivation" under Depletable
Forage. Taming a herd is *symmetric* with preparing a patch, but the **product differs and that
difference is the settle mechanic**: an *un*corralled domesticated herd stays **mobile** (pastoralism
travels with the band); **corralling pins it**. Like Cultivate, corralling is an **explicit `Corral`
policy with an investment cost** — not a free command. A `Herd` carries `corral_progress: f32` (0–1,
the pen under construction), `corralled_at: Option<UVec2>` (`Some` = penned at that tile) + a transient
`corralled_tended_this_turn` flag. *Sim-only — the client readout is a follow-up (see below).*

- **Rung 1c earned-knowledge gate — Herding.** The animal parallel of Cultivation knowledge, *learned
  by doing* and **never start-granted**: a **Sustain** hunt on a **Thriving** herd accrues faction
  **Herding** knowledge (discovery `HERDING_DISCOVERY_ID` = 2004, `fauna.rs`) in the per-faction
  `DiscoveryProgressLedger` at `husbandry.knowledge_progress_per_turn` (in the Hunt arm of
  `advance_labor_allocation`, alongside the existing domestication accrual). The **`Corral` policy** (and
  the `corral` command that sets it) is refused until
  `get_progress(faction, 2004) >= knowledge_completion_threshold`. The `herding` tag →
  discovery 2004 mapping is declared in `start_profile_knowledge_tags.json` purely so it is mappable;
  **no start profile lists it**. **Asymmetry vs. Cultivation:** Herding gates *only corralling* —
  mobile domestication (pastoralism) stays ungated (a patch, by contrast, cannot even *tame* until the
  faction knows Cultivation), because a mobile herd needs no place-binding knowledge.
- **The `Corral` policy — the investment.** In `advance_labor_allocation`'s **Hunt** arm, a herd worked
  under `FollowPolicy::Corral` (Hunt-only) **costs a yield dip while the pen is built**: the take
  ceiling is `husbandry.corralling_yield_fraction × sustainable_yield(..)` (`hunt_policy_ceiling`,
  reusing the **shared** MSY helper — the crew is building, not hunting; a fraction of MSY is a
  sustainable draw, so the herd stays healthy) and `corral_progress` accrues
  `husbandry.corral_build_progress_per_turn` (0.04 → 25 turns). **Gates:** the faction knows **Herding**
  AND owns the **domesticated** herd; a gate that lapses **mid-build** just stops accrual that turn
  (progress is kept — a half-built pen is materials on the ground; unlike cultivation it does **not**
  decay *gradually*). That "progress is kept" applies to a **mid-build** lapse only — a **completed
  pen whose herd escapes loses its progress outright** (reset to `0.0`; see *Escapes-if-untended*
  below). Accrued **after** the take, so the turn pays exactly what the forecast promised. At `1.0`
  `Herd::corral_at` pens it (sets `corralled_at`, stops roaming, grants the one-turn tended grace) and
  pushes a `CommandEventKind::Corral` feed line.
- **`corral` command (repurposed)** — `corral <faction> <x> <y>` (`handle_corral`; unchanged
  proto/runtime/text plumbing, `CommandEventKind::Corral`, `CorralCommand` proto field 38) now **sets
  the `Corral` policy** on the band(s) already hunting the herd standing on that tile — the command
  form of the client's policy picker. It **pens nothing outright**. Rejections: no herd there / faction
  hasn't learned Herding / not domesticated / not the owner / already corralled / **no band is hunting
  it** (staff it first). Same gates as the `assign_labor … corral` path (`validate_labor_policy`).
- **The pen is a managed POPULATION** (`docs/plan_corral_managed_population.md`): its yield follows the
  animals you actually keep, those animals **eat** every turn, and underfeeding **shrinks** the herd. A
  one-off 25-turn build that then printed food forever is now a **sustained commitment with a running
  cost**. Corralled = fixed + place-local worker-tended + **fed** + escapes-if-untended:
  - *Fixed* — `advance_herds` skips a corralled herd's `advance_herd_roam` (it stays at `corralled_at`,
    no heading arrow); it still grazes its footprint + regrows toward the footprint's `K` (Grazing 2d).
  - *Place-local worker-tended* — a **Hunt assignment on a corralled herd** is herding/tending it, and
    the turn has two halves (the tend branch of `advance_labor_allocation`'s Hunt arm, which `continue`s
    before `hunt_take` — a corralled herd is never both hunt-drawn AND paid):
    1. **FEED (footprint-offset, Grazing 2d §2.3).** The pen grazes its fenced footprint
       (`advance_herd_grazing` → `footprint_intake`), and the larder pays only what the pasture can't
       cover: `demand = pen.upkeep_per_biomass × biomass × (1 − pasture_fraction)`,
       `pasture_fraction = clamp(footprint_intake / (fodder_per_biomass × biomass), 0, 1)`.
       `LocalStore::take` returns what it *actually* took; `pen_fed_fraction = pasture_fraction +
       (1 − pasture_fraction) × (paid / demand)` (the total fed share — pasture plus the paid part of
       the reduced larder bill). A lush footprint feeds the pen for free; a barren one pays the full
       bill — **the tether that gives "the pen pins the band" its teeth**, now cheap on good land.
    2. **HARVEST.** The keeper takes the **pen's MSY** (`fauna::pen_yield_biomass` →
       `managed_yield_biomass` under the herd's per-species pen ecology (`pen_ecology_for`), against its
       footprint `K` = `herd.carrying_capacity`), which **draws the herd
       down** — exactly what makes it sustainable (see "The husbandry yield ladder"). The credited yield
       is **gross**: the feed is a separate debit, so the player sees both halves of the trade rather
       than one netted number.
  - *Starves if underfed* — `advance_husbandry` reads last turn's `pen_fed_fraction` and, if the keeper
    could not pay, shrinks the herd by `pen.starve_shrink_rate × (1 − fed) × biomass`, floored at
    `pen.ecology.extinction_floor × K_pen`. **The pen's growth is what the feed buys**: `regrow_biomass`
    scales a penned herd's growth by `pen_fed_fraction`, so an unfed pen does **not** grow (without this
    the pen's own fast `r` out-runs the 10%/turn wasting several times over — an "unfed" herd would keep
    growing and quietly pay a yield for feed nobody bought). The herd **withers to a remnant and
    recovers when fed again**: it does **not** despawn (a penned herd is exempt from `advance_herds`'
    dispersal retention — dispersal is the *mechanism* of local extinction, and a confined herd cannot
    disperse) and it does **not** lose the pen. Deliberate: a recoverable famine the player can see and
    fix is better play than silently voiding a 25-turn investment. It is **never silent** — an
    edge-gated `CommandEventKind::Corral` feed line fires on the turn the famine *starts*
    (`"The <species> herd is starving — the pen has no feed"`, detail `status=starving fed=<f>
    action=corral herd=<id>`), not every turn it continues. **Starving your animals to feed your people
    becomes a *decision*, not an accident.**
  - *The decision this creates* — the pen stops being a strictly-dominant upgrade and becomes a **wager
    on staying**: it out-pays every other rung, but only while you feed it, every turn, forever — and
    its food cost lands **exactly when food is scarce**, so a bad winter forces a real choice (eat the
    seed corn and lose future yield, or go hungry).
  - *Escapes-if-untended* — in `advance_husbandry` (Logistics, which runs *before* Population — a
    deliberate one-turn-lag flag, exactly like `ForagePatch::tended_this_turn`) a corralled herd
    tended last turn is spared; an **untended** one **escapes**: `corralled_at` is cleared, **and
    `corral_progress` is reset to `0.0`**, reverting it to a mobile domesticated herd (resuming the
    passive even-split husbandry yield). **The pen is lost, not merely opened** — re-penning pays the
    full 25-turn `Corral` investment again, at the herd's new position. *Why zero, when a patch's
    `cultivation_progress` only decays gradually:* **a patch is a place and a herd is not.**
    `cultivation_progress` can survive partially because the improvement sits on a tile that cannot
    move, so leftover progress still refers to the same patch; `corral_progress` lives on the **herd**,
    which roams — so any retained progress would re-materialize the pen at whatever tile the animal has
    since wandered to (a teleporting corral) and make abandoning a pen cost **one** turn instead of the
    rebuild. Losing the pen is what makes the tending obligation real (the "pins the band" mechanic).
    Because the escape now **destroys a 25-turn investment**, it is **never silent**: it pushes a
    `CommandEventKind::Corral` feed line to the owner — the same kind the pen's *completion* pushes
    (one kind for the pen's whole life) — reading `"The <species> herd broke out — untended, the pen
    is lost"` (human text names the **species**, never the internal herd id) with
    `status=escaped reason=untended action=corral herd=<id> x=<x> y=<y>` in the detail field.
    A corralled herd is exempt from the pastoral even-split here (it's paid place-local by its keeper).
    `corral_at` grants a one-turn grace so a freshly-penned herd doesn't escape before its keeper can
    tend it. **This binary escape is the *no-keeper* case only** — nobody is minding the gate. A keeper
    who is present but *broke* starves the herd instead (above); it never breaks out.
- **Persistence** — `corralled_at`, `corral_progress`, **and `pen_radius` / `pen_extend_progress` /
  `pen_extending` (Grazing 2d)** round-trip through the rollback snapshot on `HerdState` (authoritative
  sim state), so a rollback rewinds a half-built pen (or an in-flight fence extension) rather than losing
  the investment;
  `corralled_tended_this_turn`, **`pen_fed_fraction`, `pen_starving`, `footprint_intake` and
  `pen_pasture_fraction`** are transient (not persisted) — a rehydrated pen reads "untended, fully fed",
  so a rollback can only *delay* an escape or a starvation turn by one turn, never invent a famine.
- **Config** (`fauna_config.json` `husbandry`): the **`pen`** block — `ecology` carries **phase bands
  only** now (its `regrowth_rate` is unused; the pen `r` is per-species — Grazing 2d),
  **`upkeep_per_biomass` (0.002 — the feed, now footprint-offset)** and `starve_shrink_rate` (**0.10** —
  a fully-unfed herd loses 10%/turn); `capacity_fraction` is **deleted** (`K_pen` is the fenced
  footprint's graze flow). Plus the **per-species growth gains** `pastoral_gain` (1.5) / `pen_gain`
  (3.0) / `husbandry_regrowth_cap` (0.75), **`pen_radius_max`** (2 — the `ExtendPen` fence cap, 2d-β,
  validated `>= 1`), the **`pastoral`** block (phase bands only),
  **`corralling_yield_fraction` (0.50 — the investment cost, the animal twin of
  `cultivating_yield_fraction`)**, **`corral_build_progress_per_turn` (0.04 → 25 turns to build; a
  dedicated lever so pen speed and *tame* speed tune independently)**, `knowledge_progress_per_turn`
  (0.05 — ~20 Sustain-hunt turns to learn Herding), `knowledge_completion_threshold` (1.0).
  `claim_threshold` (0.6) stays — it is the **`domesticate`** command's early-claim gate on *mobile*
  taming, unrelated to corralling (which has no early claim). The retired flat rates
  `provisions_per_biomass` (0.01) / `corral_provisions_per_biomass` (0.012) and `fauna::corral_provisions`
  are **deleted**.
  - **Retuned once, against measurement** (a scripted 100-turn campaign on three pinned seeds — the
    default `map_seed` is `0`/entropy, so a probe *must* pin one): the first cut (`pastoral` 0.15,
    `pen` 0.60, dip 0.25) left a freshly-taming band at income **1.275** vs consumption **1.294** — a
    permanent one-day-of-food treadmill, no savings, no affordable expedition — and made the pen
    reachable only through a **~50% population crash** (the build dip had to be paid out of a famine).
    The shipped values put the pastoral rung clearly *above* subsistence (a real surplus) and let the
    pen's dip be paid from it. **`upkeep_per_biomass` was deliberately NOT touched** — the running cost
    is the point of the arc, and weakening it to fix balance would delete the mechanic.
  - **Every invariant above is enforced by `FaunaConfig::validate()`** — most importantly
    the pen's **best-case net-positive floor** (Grazing 2d §2.4 — `upkeep_per_biomass < r_pen · p /
    (2 + r_pen)` for the **fastest** species' `r_pen = min(husbandry_regrowth_cap, max_wild_r ×
    pen_gain)` = `0.75 × 0.02 / 2.75 ≈ 0.0055`; shipped 0.002): derivation — at the operating point the
    pen yields `r·K/4 · p` and eats `u · K·(2 + r)/4`, so `net > 0 ⟺ u < r·p/(2 + r)`. **This inverts
    the old every-pen guarantee:** with per-species `r` and pasture-dependent feed, a slow breeder or a
    poor-pasture pen may run at a **loss by design** (a placement decision), so validate only guarantees
    the best pen (fastest breeder, fully larder-fed) still pays. See "The husbandry yield ladder".
- **The band's food ledger — `PopulationCohortState.penFeedUpkeep` (the per-band roll-up).** A pen's
  feed is taken straight off `cohort.stores` (`LocalStore::take`, the corral-tend branch), so it lands
  in **neither** `foodIncome` (Σ per-source `actual`) **nor** `foodConsumption` (the food the *people*
  actually ate — `PopulationCohort::last_food_consumption`, the real opening-brackets `stores` debit,
  the symmetric twin of this pen debit; **not** a post-turn `food_demand`, which the same turn's
  births would inflate). A band keeping a pen would therefore display a surplus **overstated by exactly the
  upkeep** — on a Red Deer pen a phantom **+1.74/turn** against a band that eats ~1.2 — and the player
  would watch the larder drain unexplained. `penFeedUpkeep` is **the food the band actually PAID** this
  turn (the summed `LocalStore::take` *return*, not the demand — a band that can only part-pay reports
  only what it handed over, and its herds starve for the rest), carried on
  `LaborAllocation::last_pen_feed_upkeep` (derived per-turn, not persisted, excluded from equality —
  same treatment as `last_yields`). It closes the identity
  ```text
  larder_delta == foodIncome − foodConsumption − penFeedUpkeep
  ```
  which `integration_tests/tests/pen_food_ledger.rs` pins against a **real turn** through the real
  systems and the real snapshot export, both fully-fed and part-fed. **It is deliberately NOT folded
  into `foodConsumption`**: "my people ate X" and "my animals ate Y" are separate lines, and that
  separation is the readout this arc exists to give. The sim answers the number so the **client does
  zero arithmetic** (it must not sum `penUpkeep` across herds itself) — the same discipline as the
  Pre-commit Yield Forecast.
- **Display snapshot (on the wire).** The corral state is exposed to the client stream on both
  `WorldSnapshot` and `WorldDelta` (`snapshot.fbs`, `sim_schema`, `snapshot.rs`
  `herd_snapshot_entries`): `HerdTelemetryState.corralled:bool` (= `Herd::is_corralled()`) and
  **`corralProgress:float`** (0..1, the pen-building meter — the animal twin of
  `ForagePatchState.cultivationProgress`), plus **`penUpkeep:float`** and **`penFedFraction:float`**.
  Both are **per-herd** (the herd drawer + the starving warning):
  - **`penUpkeep`** = the feed this pen **demands, or would demand once built**, at the herd's
    **current** biomass (`pen.upkeep_per_biomass × biomass`) — a *projection* for an unpenned herd, the
    *live* demand for a penned one. It is **always meaningful, never `0`-because-unpenned**, and is
    computed on the **same biomass basis** as `corralYield`, so the two are a **matched pair the client
    subtracts**. That is the point: the pre-commit `Corral` row is by definition looking at a herd that
    is *not yet penned*, so a `0` there would quote the payoff while hiding the running cost at the one
    moment the running cost should drive the decision — the same defect class as advertising the
    **gross** yield (a preview quoting a number the player will never bank). At or below `K/2` the
    projected `corralYield` is honestly `0` (escapement — the pen pays nothing until the herd
    rebuilds).
  - **`penFedFraction`** = last turn's fed fraction (`1.0` = fully fed, `< 1` = **starving** — the herd
    and its yield are shrinking, and it recovers when fed again).
  - **Demanded ≠ paid** (load-bearing): a starving pen demands more than it is paid, and
    `penFedFraction` is that ratio. The band's **actual** ledger debit is the per-band
    `PopulationCohortState.penFeedUpkeep` (the real `LocalStore::take` amount) — the food ledger draws
    **that**, never `penUpkeep`. So no consumer needs a "0 when unpenned" reading, and one field with
    one meaning beats two that must be kept in lockstep.

  Plus the forecast pair `ceilingCorral` / `corralYield` (see
  "Pre-commit Yield Forecast"). See "Intensification display snapshot" under Cultivation for the
  plant-side + faction-knowledge fields.
- **Follow-up (final Phase-1 slice):** the **client _rendering_ for both ladders** — cultivation +
  Cultivation-knowledge + tended-patch on the plant side, and domestication + Herding-knowledge +
  corral on the animal side — is the last remaining client-dev slice (the data is now all on the wire).
  **Phase 1b of the managed-population arc rides with it:** the pen's `penUpkeep` as a *negative* row in
  the band's food ledger, the `penFedFraction` starving warning, and the corrected policy hints.
  `docs/plan_corral_managed_population.md` §6 — **Phase 1a (the sim) must not ship to a player without
  1b**, only to `main`: without the readout the player watches their larder drain with no explanation.
  **Phase 2 (deferred):** the pen's upkeep is drawn *first* from the tile's `ForagePatch` biomass (the
  animals eat grass — a resource humans can't), and only the **shortfall** is hauled from the larder.

See Also: "Cultivation (Intensification Phase 1a)" under Depletable Forage — the plant twin of this
mechanic (the two are near-mechanical transposes).

> `FaunaPursuit` is **not** snapshot-persisted (unlike `HarvestAssignment`): a
> `rollback` mid-pursuit cleanly cancels the in-flight hunt (the rehydrated cohort
> simply lacks the component). Pursuits are short-lived; revisit if needed. Domestication
> state lives on the `Herd` (in `HerdRegistry`), alongside `biomass`.

> **The authoritative `HerdRegistry` *is* rollback-persisted** (as of the intensification
> arc's first slice, `docs/plan_intensification.md` §0-i). Each live `Herd` — identity,
> movement (`route`/`step_index`/`current_pos`/`dwell_remaining`/`roam`/`next_pos`/`corralled_at`),
> **and** its depletable-ecology subset (`biomass`/`carrying_capacity`/`ecology_phase`/
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
"Sedentarization" under Campaign Loop); **corrals shipped** (Intensification Rung 1c — see "Corral"
below). Still deferred (`docs/plan_wildlife_hunting_overlay.md`): the `Camp` entity, and wiring the
sedentarization hard prompt to an actual `found_settlement`. The tile-based `HuntGame` handler stays
neutralized (its client button no longer surfaces).

---

## Depletable Forage (Intensification §0-ii)

Forage tiles are **depletable**, the herd biomass/regrowth model transposed onto plants (design:
`docs/plan_intensification.md` §0). Every `FoodModuleTag` tile carries a live per-patch
`{ biomass, carrying_capacity, ecology_phase }` (`ForagePatch`, `forage.rs`) held in the
authoritative **`ForageRegistry`** resource, keyed by tile coord. Foraging now **draws the stock
down** and the patch **regrows**, so the yield instrument's overdraw ⚠ (PR #110) lights up for
forage exactly as it does for overhunting. *Sim-only — the client already renders forage
`sustainable_yield` from the snapshot.*

- **Seeding** (`spawn_initial_forage`, Startup after `spawn_initial_herds`): one full patch
  (`biomass = carrying_capacity`) per `FoodModuleTag` tile, at **that tile's biome capacity** —
  `forage.capacity_by_biome[terrain]`, the human food web's per-biome table (see "The two food webs"),
  never a global constant. A food-module tile whose biome carries **nothing human-edible** (a stated
  `0` — glacier, salt pan, deep-sea vent field; the module classifier tags these off their *tags*, not
  off anything growing there) is seeded **no patch at all**, exactly as a zero-graze tile holds no
  `GrazePatch`: "no food here" is an *absent* reading, never a zero one. Idempotent (a restored world
  is skipped).
- **Regrowth** (`advance_forage_regrowth`, `TurnStage::Logistics` alongside `advance_herds`): each
  patch regrows toward its cap and refreshes its `EcologyPhase`. Unlike a wild herd, a patch uses
  **pure `logistic_regrowth`** (no Allee / critical-depensation crash) and **never despawns** —
  plants reseed, so a depleted (feral) patch always recovers. Because `logistic_regrowth` is `0` at
  `biomass = 0`, `regrow_patch` first applies a **reseed floor** — it lifts a depleted patch up to
  `reseed_floor_fraction × carrying_capacity` (a small standing crop, `max()` so a healthy patch is
  untouched) *before* regrowth — so a patch driven to exactly `0` (repeated Eradicate + f32
  underflow, `take_fraction = 1.0`, or a restored snapshot carrying `biomass = 0`) still has a seed
  stock and recovers via normal regrowth instead of sticking at `0` forever. The floor is below
  `collapse_fraction`, so Eradicate still crashes a patch hard into the Collapsing band — it just
  can't hold it permanently at `0`.
- **Draw-down** (`forage_take`, the plant mirror of `hunt_take`): resolves the per-policy ecology
  ceiling, caps it by gather throughput (`workers × per_worker_biomass_capacity × seasonal_weight`),
  clamps to the patch's biomass, **subtracts the take**, and converts to provisions
  (`take × provisions_per_biomass × output_multiplier`). Foraging honors the **full policy axis**
  (Sustain/Surplus/Market/Eradicate — §0-iii, **parity with hunting**), mirroring `hunt_take`'s
  rungs: **Sustain** = the **Maximum Sustainable Yield** (`sustainable_yield(..)` — regrowth at the
  most-productive biomass K/2, so a patch *at carrying capacity* still yields a positive skim and a
  collapsed patch yields nothing; Sustain draws the patch toward K/2); **Surplus** = that ×
  `surplus_multiplier` (slow
  decline); **Market** = `market.take_fraction × biomass` (a commercial share → fast depletion) and
  the `Forage` arm sells the take as trade goods (`take × market.trade_goods_per_biomass ×
  market.trade_goods_multiplier × output_mult` → `FactionInventory` — gathered goods sold, **Market
  only**); **Eradicate** = `eradicate.take_fraction × biomass` (strip the patch, no floor, no trade
  goods — denial). The `Forage` arm of `advance_labor_allocation` (Population) passes the
  assignment's policy into `forage_take` and writes the real `sustainable =
  sustainable_yield(biomass_before, cap, forage.ecology) × provisions_per_biomass ×
  output_multiplier` (MSY-based) into the
  yield telemetry, so a non-Sustain gather reads `actual > sustainable` (the over-forage ⚠) exactly
  as an over-hunt does.
- **Config** (`labor_config.json` `forage`): **`capacity_by_biome`** (the per-biome capacity table —
  see "The two food webs"; **validated total** over every `TerrainType` by `LaborConfig::validate`),
  `per_worker_biomass_capacity`,
  `provisions_per_biomass`, an `ecology` block reusing fauna's `EcologyConfig` (`regrowth_rate` tuned
  higher than fauna's 0.05; `collapse_fraction`/`stressed_fraction` phase bands), a
  `reseed_floor_fraction` (0.02 — the reseed standing crop as a fraction of `carrying_capacity`, so a
  crashed patch recovers from a seed stock rather than sticking at `0`; below `collapse_fraction`),
  plus the **policy axis** levers (§0-iii, mirroring fauna's `follow`/`market`/`hunt`):
  `surplus_multiplier` (1.6),
  `market: { take_fraction 0.20, trade_goods_multiplier 4.0, trade_goods_per_biomass 0.005 }`,
  `eradicate: { take_fraction 0.30 }`. The old flat `forage.per_worker_yield` lever is **retired**,
  and so is the flat `forage.carrying_capacity` (120 on every food-module tile) it was replaced by:
  a **constant** human web could not diverge from the spatial animal one, so *"your best farm is not
  your best pasture"* was untrue **by construction**. Per-biome (not per-`FoodModule`) is deliberate —
  the two tables must be comparable tile-for-tile and must be able to disagree *within* a module.
  Because every yield is linear in `K` (MSY = `r·K/4`), the cultivation incentive and every policy
  ceiling scale with the tile and need no re-derivation.
- **Policy plumbing** (§0-iii, the 5-site mirror of Hunt's policy): `LaborTarget::Forage` carries a
  `policy: FollowPolicy` (a policy change on the same tile is the **same source** in `same_source`,
  a mutable property); the `assign_labor forage <x> <y> [policy] <workers>` command-text parse takes
  an optional policy token; `handle_assign_labor` builds it via `parse_follow_policy`; and the
  policy round-trips through the rollback snapshot (`LaborAssignmentState.policy`, no schema change).
- **Persistence** — `ForageRegistry` round-trips through the rollback snapshot exactly like the
  `HerdRegistry` (the §0-i pattern): a per-tile `ForageState` (= tile key + the shared
  `sim_schema::EcologyState`) captured coord-sorted into `WorldSnapshot.forage_registry` and rebuilt
  on restore via `ForageRegistry::update_from_states`. `progress`/`owner` on `EcologyState` now carry
  **cultivation** (Phase 1a, below) — a mutate-then-restore rewinds it like biomass. Not wired to the
  FlatBuffers client stream.
- **Companion client slice:** the sim side of the forage policy axis (§0-iii) is complete — the
  client `%ForageAssignControls` policy picker (mirroring `%HerdAssignControls`) that emits the
  policy in the `assign_labor forage` command is a **client-dev follow-up**. A client patch-ecology
  readout (thriving/stressed/collapsing on the map/tile, like herds) is a possible later slice.

### Cultivation (Intensification Phase 1a)

The **plant analog of animal husbandry** (`docs/plan_intensification.md` §3), evolved past the
mechanical husbandry transpose into **Rung 1a — the worker-tended, place-local tended patch**, and now
into an **explicit policy with an investment cost**. A patch carries `cultivation_progress` (0–1,
`1.0` = cultivated) + `owner: Option<FactionId>` on `ForagePatch`, mirroring a `Herd`'s
`domestication_progress`/`owner`, and rides the shared `EcologyState` (`progress`/`owner`) through the
rollback snapshot. A completed patch is a **tended patch**: **worker-tended + place-local +
higher-output + feral-if-abandoned**. *Sim-only — the client readout is a follow-up.*

> **The free path is gone (design fix).** Cultivation used to accrue **silently and for free** under
> Sustain: same labor, same tile, no cost ⇒ cultivating was always correct and there was **no
> decision**. It is now the **`Cultivate` policy** (`FollowPolicy::Cultivate`, Forage-only) with a real
> up-front cost, and the **early-claim `claim_threshold` is removed** (it would let the player skip the
> investment — the whole point). Sustain still *teaches* the faction Cultivation knowledge; it just
> never tames a patch. The animal twin is the **`Corral` policy** — see "Corral".
- **Rung 1b — the earned-knowledge gate (`docs/plan_intensification.md` §4b).** Cultivation is a
  faction-level knowledge *learned by doing*, **never start-granted**: a **Sustain** forage on a
  **Thriving** patch accrues faction **Cultivation** knowledge (discovery `CULTIVATION_DISCOVERY_ID`
  = 2003, `forage.rs`) in the per-faction `DiscoveryProgressLedger` at
  `cultivation.knowledge_progress_per_turn` (`add_progress`, clamped to `1.0`). **A patch cannot accrue
  `cultivation_progress` until the faction *knows* Cultivation** — `advance_labor_allocation` only calls
  `accrue_cultivation` once `ledger.get_progress(faction, 2003) >= knowledge_completion_threshold`.
  Knowledge is all Sustain earns — it **never** accrues `cultivation_progress`. The `cultivation` tag →
  discovery 2003 mapping is declared in `start_profile_knowledge_tags.json` purely so it is mappable;
  **no start profile lists it**, so no faction begins knowing Cultivation.
- **The `Cultivate` policy — the investment.** In `advance_labor_allocation`'s **Forage** arm
  (Population), a patch worked under `FollowPolicy::Cultivate`:
  - **Costs a yield dip while preparing.** Its take ceiling is
    `cultivation.cultivating_yield_fraction × sustainable_yield(..)` — a *fraction of the MSY ceiling*
    (`forage_policy_ceiling`, reusing the **shared** `sustainable_yield` helper, never a second
    formula). The crew is clearing and planting, not gathering. Because the take is a fraction of MSY
    it is **sustainable**, so the patch stays Thriving (which the accrual gate requires) — the cost is
    a pure yield dip, not a depletion.
  - **Accrues `progress_per_turn`** toward `1.0` (sets `owner` on first accrual; only the owner
    accrues), **gated** on the faction *knowing Cultivation* AND the patch being **Thriving**. If a
    gate lapses mid-run (another band overdraws the patch to Stressed) progress simply **stops accruing
    that turn** — it is not lost and the policy is not silently switched; the patch is still marked
    worked, so it doesn't decay either, and accrual resumes when it recovers.
  - **Accrues AFTER the turn's take**, so the turn pays exactly what the pre-commit forecast promised
    (forecast == actual). The turn progress reaches `1.0` is the last preparing take; the full tended
    yield starts the next turn.
  - **Marks the patch `tended_this_turn`**, so `advance_cultivation` spares a patch under active
    preparation — the investment accrues at the **full** `progress_per_turn` (25 turns at the default),
    not net-of-decay.
  - **Break-even** (defaults `fraction` 0.25, `progress_per_turn` 0.04): the dip costs ~75% of that
    patch's Sustain yield for ~25 turns ≈ `0.75 × 0.375 × 25` ≈ **7 prov** forgone; the tended patch
    then out-pays wild Sustain by `1.2 − 0.375` = **0.825 prov/turn**, recouping the investment ~8–9
    turns after completion. Cultivating is correct only if you intend to stay — the decision the free
    auto-accrual erased.
  - `ForagePatch` methods: `is_cultivated`/`accrue_cultivation`/`decay_cultivation` (the early-claim
    `claim_cultivation` is **removed**).
- **Tended yield — paid to the tending band, place-local, higher output** — a tended (cultivated)
  patch is **worked, not passive**. In the **Forage** arm, when the assignment's patch
  `is_cultivated()`, the band whose Forage assignment tends it (≥1 worker on the tile → place-local by
  construction) is paid `biomass × cultivation.tended_provisions_per_biomass × output_multiplier`
  directly into `cohort.stores` (FOOD) — a **managed harvest** of the full standing crop **without**
  drawing biomass down (a tended patch regrows freely, so biomass sits near cap). It is *maintenance
  labor*: presence gates it (the `workers == 0` skip above), the amount is biomass-based, not
  head-count-scaled. The patch is marked `tended_this_turn` (a transient, non-persisted per-turn flag)
  so the decay pass can tell tended from abandoned. Yield telemetry reads `actual == sustainable` (a
  managed harvest never overdraws — no ⚠). **This out-yields the same patch's wild MSY skim** — the
  intensification incentive: a tended patch pays `K × 0.01` ≈ 1.2 prov/turn vs a wild patch's best
  sustainable MSY `regrowth_rate × K/4 × forage.provisions_per_biomass` ≈ 0.375 prov/turn (~3.2×; see
  the `CultivationConfig` tuning note). The old even-split-across-all-the-owner's-bands payment in
  `advance_cultivation` is **retired**.
- **Feral if unworked** — `advance_cultivation` (`forage.rs`, `TurnStage::Logistics` alongside
  `advance_forage_regrowth`) is the **decay/feral** pass only. A patch **worked as an improvement this
  turn** (`tended_this_turn` — tending a completed patch *or* preparing one under Cultivate) is
  **spared**; everything else decays by `decay_per_turn`. So an **untended cultivated** patch **goes
  feral** (drops below `1.0` → reverts to a wild gather patch, then decays to 0 over
  ~`1/decay_per_turn` turns; owner clears at 0) and an **abandoned part-prepared** patch loses its
  investment the same way. **Stage-ordering:** Logistics runs *before* Population, so the
  `tended_this_turn` flag this pass reads was written by the labor arm **last** turn (a deliberate
  one-turn-lag carry-across-turns signal; the flag is cleared here and re-set next Population stage).
  Net: a patch worked every turn never decays; a patch whose band leaves reverts one turn later.
- **The loop (the settle pull).** Sustain-forage a thriving patch → *learn* Cultivation → **choose** to
  pay the Cultivate dip for ~25 turns → the patch becomes tended → a band tending it collects the
  higher tended yield **place-locally** → move the band away and it goes feral, reverting to wild.
  Place-locality + feral + a sunk investment = the band is **pinned near its farm**: intensifying
  raises output *and* deepens the anchor.
- **`cultivate` command (repurposed)** — `cultivate <faction> <x> <y>` (`handle_cultivate`; unchanged
  proto/runtime/text plumbing, `CommandEventKind::Cultivate`) now **sets the `Cultivate` policy** on
  the band(s) already foraging that tile (`set_policy_on_working_bands`) — the command form of what the
  client's policy picker does. It **claims nothing**. Gates (shared with `assign_labor` via
  `validate_labor_policy`): faction knows Cultivation, patch is **Thriving**, not already cultivated,
  not another faction's; plus a rejection when **no band is foraging** the tile (staff it first).
- **Policy validation** — `FollowPolicy::valid_for_forage` / `valid_for_hunt`: `Cultivate` is
  Forage-only and `Corral` Hunt-only. `handle_assign_labor` rejects an invalid combo (and a failed
  gate) with a clear failure event before touching the allocation; unassigning (`workers == 0`) is
  always allowed, so a player can always abandon an investment.
- **Sedentarization (folded)** — `sedentarization_tick` reads `herds.domesticated_count(faction) +
  forage.cultivated_count(faction)` for its **domestication** input: plant + animal domestication
  share the one driver (no new weight, no re-balance).
- **Config** (`labor_config.json` `forage.cultivation`, `CultivationConfig`, cloning
  `HusbandryConfig`): `progress_per_turn` (0.04 → 25 turns to prepare), `decay_per_turn` (0.01, the
  feral-reversion rate), **`cultivating_yield_fraction` (0.25 — the investment cost: the preparing take
  ceiling as a fraction of the patch's Sustain/MSY ceiling)**, `tended_provisions_per_biomass` (0.01 —
  the **tended-harvest** rate on the full standing crop, distinct from and *lower per-biomass* than the
  gather `forage.provisions_per_biomass`, but paid on the whole crop so it beats the wild MSY skim;
  keep it `> regrowth_rate/4 × forage.provisions_per_biomass` so intensifying always pays), plus the
  **Rung 1b earned-knowledge** levers `knowledge_progress_per_turn` (0.05 — faction Cultivation earned
  per Sustain-forage-Thriving turn, ~20 turns to know) and `knowledge_completion_threshold` (1.0 = the
  ledger's completion value). The early-claim `claim_threshold` is **removed**. Intended invariants:
  `progress_per_turn > decay_per_turn`, `0 < cultivating_yield_fraction < 1`,
  `tended_provisions_per_biomass > 0`, `knowledge_progress_per_turn > 0`,
  `0 < knowledge_completion_threshold <= 1`. As with fauna, **these are asserted over the *builtin*
  only — `LaborConfig` has NO `validate()`** — so a `LABOR_CONFIG_PATH` override that breaks one is
  accepted silently. Same open follow-up (see `TASKS.md`).
- **Intensification display snapshot (on the wire, consumed by the client-dev rendering slice next).**
  The intensification-ladder state is now exported to the FlatBuffers client stream (append-only per
  the schema discipline; `snapshot.fbs`, `sim_schema`, `snapshot.rs`), on both `WorldSnapshot` and
  `WorldDelta`:
  - **Forage patch cultivation** — a new per-tile `foragePatches:[ForagePatchState]` list
    (`snapshot_forage_patches`, from the `ForageRegistry`, stable `(y, x)` order). Per patch: tile
    `(x, y)`, `cultivationProgress:float` (0..1), `isCultivated:bool` (tended = progress ≥ 1.0),
    `owner`/`hasOwner` (tending faction; `hasOwner = false` = wild), plus `biomass`/`carryingCapacity`/
    `ecologyPhase` for optional patch-health. This is the client's first per-tile forage-patch payload
    (previously forage was visible only via `laborAssignments`).
  - **Faction Cultivation/Herding knowledge** — a new per-faction
    `intensificationKnowledge:[IntensificationKnowledgeState{ faction, cultivation, herding }]` list
    (`snapshot_intensification_knowledge`, from the `DiscoveryProgressLedger`), mirroring
    `sedentarization[]`. `cultivation`/`herding` are the 0..1 progress on discoveries
    `CULTIVATION_DISCOVERY_ID` (2003) / `HERDING_DISCOVERY_ID` (2004); a faction is emitted only once it
    has begun learning either ladder (both zero → skipped). Client renders these as learning/known
    meters like the sedentarization meter.
  - **Herd corral** — `HerdTelemetryState.corralled` (see the corral section above).
- **Follow-ups:** **Rung 1c — corral** (the fauna-side pen behind a `herding` gate) **shipped** — see
  "Corral (Intensification Rung 1c)" under Fauna & Wild Game. The **client _rendering_ for both ladders**
  (tile-card cultivation N% / tended-patch + Cultivation/Herding knowledge meters + herd corral
  indicator) is the **final Phase-1 slice** and remains a client-dev follow-up; the sim/schema data is
  now all on the wire (fields above).

---

## The Graze (Pasture) Layer (Grazing Phase 2a)

**Humans and animals do not eat the same things.** The land carries **two vegetal stocks, on two food
webs** (authoritative design: `docs/plan_grazing_foundation.md`):

| | `ForagePatch.biomass` (Depletable Forage) | **`GrazePatch.biomass`** |
|---|---|---|
| Who eats it | **humans** (Forage assignments) | **animals** (herds, wild and penned) |
| Where it is | `FoodModuleTag` tiles (sparse) | **any vegetated land**, by biome (dense) |
| What it is | seeds, nuts, tubers, fruit, shellfish | grass, browse, forbs — **cellulose humans cannot digest** |
| Its capacity | `forage.capacity_by_biome` (`labor_config.json`) | `graze.capacity_by_biome` (`fauna_config.json`) |

That is not flavor: it is the economic basis of herding (a pastoralist converts a resource
**worthless to humans** into meat and milk), and it is why *your best farm is usually not your best
pasture*. `graze.rs` mirrors `forage.rs` (which mirrors the herd model) exactly — the proven,
rollback-persisted pattern.

### The two food webs — two tables, meant to disagree

**Both webs are per-biome tables over the same `TerrainType` set, in the same shape, with the same `validate()`
discipline** (total table required; a missing row would read as an invisible zero and **zero must be
stated**). They are per-**biome**, not per-`FoodModule`, precisely so they are comparable tile-for-tile
and can disagree *within* a module — **that disagreement is the agropastoral decision.** The
`FoodModuleTag` model is untouched: the module still decides what *kind* of gathering a tile offers
(and its `seasonal_weight`); the table decides *how much* is there.

| biome | graze (animals) | forage (humans) | the story |
|---|---|---|---|
| `PrairieSteppe` | **240** (the reference pasture) | 70 | grass: the animals feast, humans get seed heads |
| `RiverDelta` / `Floodplain` | 130 | **210 / 205** | the richest human ground there is |
| `AlluvialPlain` | **110** | **195** | silt + water = **cropland**. The FARM, not the pasture |
| `MixedWoodland` | **55** | **190** | nuts, mast, berries under a canopy that shades out the ground cover — **the flagship inversion** |
| `Tundra` / `AlpineMountain` | 100 / 65 | 25 / 20 | **rangeland**: pastoralism lives exactly where farming can't |
| `ContinentalShelf` / `CoralShelf` | **0** (water) | 130 / 180 | the coastal larder — a fishery is a food module on *water* |
| `RollingHills` / `PeatHeath` | 150 / 135 | 80 / 55 | |
| glacier / lava / salt flat / deep ocean | **0** | **0** | a *stated* zero |

**The silt lowlands were LOWERED on the graze side** (`AlluvialPlain` 230 → **110**, `Floodplain`/
`RiverDelta` 230/220 → 130): a river plain is prime *cropland*, not prime range, and its value moved to
the human web where it belongs. `AlluvialPlain` is additionally the tag solver's universal fallback
biome (~25% of all land even after the `FertileLowland` palette fix), so leaving it tied with prairie
for best pasture baked a **worldgen artifact into the fauna model**.

**Measured, not asserted** (`integration_tests/tests/graze_distribution.rs::two_food_web_report`,
earthlike 80×52, seeds 11/4242/90210 — run with `--nocapture` for the joint histogram):
- **Correlation between the two webs across living land: −0.11 / +0.03 / −0.01.** Near zero, as
  intended: knowing a tile's pasture tells you almost nothing about its farm. (Across *all* land it is
  +0.13…+0.24 — bare rock is a shared **zero**, an irreducible positive term that says nothing about
  the design claim; a farm-vs-pasture decision needs land that can feed *somebody*.)
- **Land that is top-decile in BOTH webs: 0.0% on every seed** (independence would give 1%). *Your best
  farm is not your best pasture* — measured, not claimed. (The top-**quartile** overlap is printed too
  but **not** guarded: `AlluvialPlain` is ~25% of land, so the 75th-percentile graze cut lands *inside
  that one biome* and the number flips 0% ↔ 24% on a hair. That is a cliff, not a measurement — do not
  tune a capacity table to it.)
- **Balance impact on the human food economy: map-wide capacity −18…−20%, but the early game is flat.**
  The mean capacity of patches within `band_work_range` of the start is **123 / 128 / 99** vs the
  retired flat **120** (mean 117 across seeds, −3%). The map-wide drop is almost all tundra, bare rock
  and scrub — land nobody starts on, which the old flat 120 was pricing as richly as a river delta.
  Individual starts *do* move (a grassland/tundra start is thinner, a river-valley start richer): that
  spatial variance is the feature, and it is the thing to watch in a live campaign.

> **Phase 2a ships this layer INERT.** It seeds, regrows, persists and exports — and **nothing reads
> it for gameplay**. No herd behaviour changes, zero balance impact. Herd carrying capacity,
> competition, overgrazing, migration and spawn placement all become functions of it in Phase 2b/2c;
> the layer ships inert first so its *distribution can be looked at on a real map* before the fauna
> model is bet on it.

- **`GrazeRegistry`** (resource, `graze.rs`) — per-land-tile `GrazePatch { biomass, carrying_capacity,
  ecology_phase }`, keyed by tile coord. **Only tiles with a positive capacity hold a patch**, so
  "this biome has no pasture" is an *absent* reading, never a zero one.
- **Seeding** (`spawn_initial_graze`, Startup right after `spawn_initial_forage`): one full patch
  (`biomass = carrying_capacity`) per non-`WATER` land tile whose biome has a positive
  `graze.capacity_by_biome`. Idempotent (a restored world is skipped) — the `spawn_initial_forage`
  guard.
- **Regrowth** (`advance_graze_regrowth`, `TurnStage::Logistics` right after
  `advance_forage_regrowth`): **pure logistic regrowth over a reseed floor**, then a phase refresh.
  **No Allee / collapse branch — grass has no depensation**, and it **never despawns**: an eaten-out
  tile always recovers (slowly). Shares the one plant curve `fauna::reseeding_logistic_regrowth` with
  `forage::regrow_patch`, so the two stocks can never drift apart. Permanent degradation
  (desertification) is a deliberate later lever, not this arc.
- **Capacity is a property of the LAND, not the animal** — `graze.capacity_by_biome`, a **data table
  over every `TerrainType`, not a formula**, and **read against its twin** `forage.capacity_by_biome` (see
  "The two food webs" above, which owns the joint tuning table and the measurements). Anchor:
  `PrairieSteppe` = **240** is *the* reference pasture; every other row is a claim relative to it.
  `MixedWoodland` (55) / `BorealTaiga` (40) are deliberately **poor** — a closed canopy shades out the
  ground cover, the inversion the two-stock split exists to create. Cold/high **rangeland** (Tundra
  100, AlpineMountain 65, HighPlateau 75, SemiAridScrub 100) is deliberately *better for animals than
  for humans*: pastoralism exists precisely where farming cannot. Water / glacier / lava / salt flat
  are a **stated 0**. The absolute scale is a free parameter; only the ratios matter until Phase 2b's
  `fodder_per_biomass` denominates it into animals.
- **Config** (`fauna_config.json` `graze` — homed here, not in a file of its own, because graze is the
  *substrate of the fauna model*: every consumer of it is a fauna system, and it lets the block reuse
  `FaunaConfig::validate` verbatim): `capacity_by_biome`, `ecology` (`regrowth_rate` **0.40** —
  **grass is the fastest-renewing vegetal stock in the model**: wild fauna 0.05 ≪ forage 0.25 <
  **graze 0.40** ≪ a fed pen 0.90; `collapse_rate` is *inert* for graze, as it is for forage — pure
  logistic never reads it; `collapse_fraction`/`stressed_fraction` are the phase bands the overgrazing
  readout uses), `reseed_floor_fraction` (0.02, mirroring forage's — kept **below**
  `collapse_fraction` so the floor stops *permanent death* without *hiding overgrazing*).
- **Validated** (`FaunaConfig::validate`, so every load path is covered): the table must be **total**
  over every `TerrainType` (a missing row silently reads `0` — an invisible dead zone nothing would ever
  explain: **zero must be stated, never defaulted**), every row finite and `>= 0`, **at least one row
  positive** (an all-zero table disables the whole layer while parsing perfectly), the graze ecology
  live and phase-ordered, and `reseed_floor_fraction < collapse_fraction`.
- **Persistence** — `GrazeRegistry` round-trips through the rollback snapshot exactly like
  `ForageRegistry`/`HerdRegistry`: a per-tile `GrazeState` (tile key + the shared
  `sim_schema::EcologyState`) captured coord-sorted into `WorldSnapshot.graze_registry`, rebuilt on
  restore via `GrazeRegistry::update_from_states`. Graze is **wild ground** — never owned, tended or
  improved — so `EcologyState`'s `progress`/`owner` ride at their defaults.
- **Wire — on `TileState`, not a patch list.** `TileState.grazeBiomass:float` /
  `grazeCapacity:float` / `grazeEcologyPhase:ubyte` (`0` = none, `1` thriving, `2` stressed, `3`
  collapsing — the `moraleCause:ubyte` idiom; `none` is the default so "no pasture" can never be
  misread as "healthy pasture"). **Measured, not assumed** (earthlike 80×52, 1511 patches): the
  TileState fields cost **+12.9 KB** on a 3.63 MB FlatBuffers snapshot (**+0.36%**) and **+0.58 ms**
  on a ~22 ms turn; the rollback record costs +55.9 KB (+1.6%). A `ScalarRaster` channel — the obvious
  alternative for a dense per-tile scalar — would cost **33.3 KB** (2.6× more: it pays for all 4160
  tiles, water included), carry **one** scalar instead of three (no capacity → no % → no overgrazing
  signal on the tile card), and re-ship **whole** on any single tile's change, where `TileState` is
  **per-entity diffed** and so costs *zero* delta bytes on an ungrazed turn. The dense shape is the
  one place graze deliberately diverges from `ForagePatchState`.
- **Forage-potential twin — `TileState.forageCapacity:float`** (append-only, beside the graze fields on
  both `WorldSnapshot` and `WorldDelta`). The exact human-food mirror of `grazeCapacity`, so the client
  can draw a **Forage overlay** the same way it draws the pasture one. Sourced **directly from
  `forage.capacity_by_biome` (`ForageLaborConfig::capacity_for(tile.terrain)`)** for *every* tile —
  **not** from the sparse `ForageRegistry` — precisely so the biome's potential shows on the ~95% of
  tiles (all the best cropland) that carry no `ForagePatch`. Consequence, preserved deliberately: it is
  **non-zero on fishery water** (`ContinentalShelf` 130 / `CoralShelf` 180 / `InlandSea` 110 — a fishery
  is a food module on water), a real divergence from graze where all water is 0; only a *stated-zero*
  biome (deep ocean, glacier, lava, salt flat) reads 0. On a food-module tile that *does* hold a
  `ForagePatch`, that patch was seeded at the same `capacity_for(biome)`, so `forageCapacity` equals the
  patch's `carryingCapacity` — no drift between the potential and the realized patch. Cost: **+1 float
  per tile** (per-entity diffed, so zero delta bytes on an unchanged tile). Populated at capture beside
  the graze fields in `snapshot.rs::tile_state`.
- **Distribution, measured on real maps** (`integration_tests/tests/graze_distribution.rs` — run with
  `--nocapture` for the histogram; the guards keep the model claims true under retuning). Earthlike
  80×52, three seeds: ~1500–1560 land tiles carry ~162–177 k total graze capacity, and only
  **0.8–1.0% of land is zero-graze** (glacier / volcanic / fumarole). Prairie is the richest per-tile
  pasture (240), as intended. Two earlier findings are now **closed**: the `FertileLowland` palette
  niche is no longer thinned (`k_small` 2 → 4, `map_presets.json`), so **forest and floodplain exist on
  the standard map** — the flagship inversion is observable in play — and `AlluvialPlain`, which was
  absorbing both of them as their niche-mate, no longer carries the map's pasture: at graze 110 its
  share of total graze falls to ~16–24% (from 37–48%), and the *dominant* pasture is the steppe again,
  not the fallback biome. See "The two food webs" for the joint (graze + forage) measurement.
- **Follow-ups:** the **client** pasture overlay + tile-card readout — and the twin **Forage overlay**
  off `TileState.forageCapacity` (both are client-dev slices: the data is on the wire; note each overlay
  must be built from `TileState`, since neither graze nor forage is a raster channel). **Phase 2b**
  (herds eat it, `K_herd` = `range graze flow / fodder_per_biomass`) and **Phase 2d** (the pen becomes
  fenced land, retiring `pen.capacity_fraction`) have since landed.

### Phase 2b-i — herds eat their range, movement is graze-aware (INERT on K)

The first 2b slice (`docs/plan_grazing_2b.md` §8). Herds now **draw the graze layer down** on the
tiles they occupy, and **movement avoids barren ground** — but **carrying capacity is still the
species constant**, so the hunting economy (hunt/forecast yields) is byte-identical to 2a. This
de-risks the K change (2b-ii) by proving the eating + movement first, exactly as 2a shipped the graze
layer inert.

- **`grid_utils::hex_range_tiles(center, radius, w, h, wrap)`** — every tile within odd-r hex distance
  `radius` (the hex disk: `1, 7, 19, …`), wrap-aware horizontally + pole-clamped. Bounding-box scan +
  exact `hex_distance_wrapped` filter. Shared by the herd range (and the pen/anything later).
- **`SpeciesDef.fodder_per_biomass`** (`fauna_config.json`, `#[serde(default)]`) — the fodder one unit
  of animal biomass demands per turn. **Cached onto `Herd` at spawn** (mirroring `carrying_capacity`)
  and round-tripped through the rollback snapshot (`HerdState.fodder_per_biomass`, sim-side only — not
  on the client wire). Shipped anchors (smaller animals eat MORE per unit biomass; **inert this slice**,
  retuned from a measured anchor in 2b-ii): rabbit **0.10** / fowl **0.09** / boar **0.06** / deer
  **0.05** / steppe_runner **0.05** / marsh_grazer **0.03** / mammoth **0.011**. Each is
  `range_tiles × per-tile MSY (0.1·capacity) ÷ species K`, so a herd near its constant K eats ~its
  range's sustainable graze flow and holds the range near half capacity.
- **`Herd::graze_range_radius(&SpeciesDef)`** — the footprint a herd grazes, from `size_class`: Small
  → **0** (its one tile), Big → **1**, Migratory → **loiter_radius** (the current loiter cluster, not
  the whole route).
- **`advance_herd_grazing`** (Logistics, registered **after `advance_herds`** and **before
  `advance_graze_regrowth`**) — the `forage_take`-style draw-down: each **mobile, non-corralled** herd
  demands `fodder_per_biomass × biomass` and draws it from its range's `GrazeRegistry` patches,
  **proportional to each tile's available graze** and floored at each patch's `reseed_floor_fraction ×
  capacity` (never permanently kills a tile). Herds draw **sequentially in `HerdRegistry` order** (that
  Vec is rollback-persisted in a fixed order), so a shared tile is order-independent under rollback.
  Corralled herds are fed from the larder (`pen_upkeep`), not the land, so they are skipped.
- **Graze-aware movement** (§4.1): `advance_herd_roam` (`best_land_neighbor_toward` /
  `wander_near_anchor`) **never steps onto a zero-graze tile** (no patch / zero capacity) and **biases
  toward higher graze capacity** among candidates, folding graze into the *existing* per-turn seeded
  RNG (deterministic under rollback). A herd hemmed in by barren stays put. `build_route` (spawn-time)
  biases migratory anchors onto the most fertile nearby ground, reading capacity **directly from
  `graze.capacity_by_biome`** (graze patches don't exist yet — `spawn_initial_herds` runs before
  `spawn_initial_graze`). Movement keys off **capacity** (stable land fertility), *not* live biomass —
  chasing *receding* grass (leaving a cluster because it was eaten out) is the emergent 2c dynamic,
  deliberately deferred. `advance_herds` takes the graze layer as `Option<Res<GrazeRegistry>>`: a
  `None`/empty registry falls back to plain land movement (the isolated fauna test harnesses).
- **Measured** (`core_sim/tests/grazing_2b.rs`, earthlike seed 119304647): herd-occupied pasture sits
  below untouched pasture (grazing visibly draws range down); a vacated cluster recovers to capacity
  once herds leave; ~0 herds end a turn on a zero-graze tile (movement avoids barren). NB the 2b-i
  draw-down floor moved from the reseed floor to `graze.overgraze_escapement_fraction` in 2b-ii.

See Also: `docs/plan_grazing_foundation.md` (design), `docs/plan_grazing_2b.md` (the 2b arc),
"Depletable Forage" (the human-edible twin and the `ForageRegistry` pattern this mirrors), "Fauna &
Wild Game" (the model this becomes the substrate of in Phase 2b).

### Phase 2b-ii — carrying capacity becomes ecological; `regrowth_rate` becomes per-species

The big rebalance (`docs/plan_grazing_2b.md` §2/§3/§5). A mobile herd's `K` is **no longer the species
constant** — it is derived each turn from the graze its range yields, and each wild species breeds at
its **own** rate. Gated by a convergence test (§2.2), because a coupled consumer–resource system
oscillates or crashes if built carelessly.

- **`K` is range-derived, recomputed in `advance_herds`.** After a mobile (non-corralled) herd roams,
  `ecological_carrying_capacity` sets `herd.carrying_capacity =
  Σ_range graze_sustainable_flow(G_tile) / fodder_per_biomass` over `hex_range_tiles(current_pos,
  graze_range_radius)` — the **same** tiles `advance_herd_grazing` eats, at their **current** (drawn-
  down) biomass. So overgrazing a range lowers its flow → lowers `K` → shrinks the herd (the emergent
  overgrazing spiral); a range held at/above its MSY point yields full flow → `K` at max. This is the
  **one** write; `herd_capacity(herd, fauna)` still reads the cached field, so **every downstream
  consumer is unchanged** (no `&GrazeRegistry` threaded through the ~15 capacity call sites). Since
  **Grazing 2d** a **corralled** herd's `K` is likewise recomputed — over its *fenced footprint*
  (`hex_range_tiles(corralled_at, pen_radius)`), via the same `ecological_carrying_capacity` seam (a
  wholly-barren footprint keeps the frozen `K` and is fully larder-fed). A non-grazing herd
  (`fodder ≤ 0`) or an absent graze layer keeps the constant `K`.
- **`graze_sustainable_flow` — NOT `sustainable_yield`.** The K flow is pure logistic at the MSY-clamped
  biomass (`logistic_regrowth(min(G, cap/2), cap, r_graze)`), deliberately **without** the Allee cutoff
  `sustainable_yield` applies — **grass has no depensation**, so a heavily-but-recoverably grazed tile
  must still yield a positive `K` (the design's formula named `sustainable_yield`, but that would read
  `K = 0` below `collapse_fraction` and crash a herd on ground that in fact regrows).
- **Per-species `regrowth_rate` (`SpeciesDef.regrowth_rate: Option<f32>`, `#[serde(default)]`).** Cached
  on `Herd` at spawn (`regrowth_rate_or(fauna.ecology.regrowth_rate)`), round-tripped through
  `HerdState.regrowth_rate` (sim-side only). **`herd_ecology` now returns an owned `EcologyConfig`**
  with the wild curve's `regrowth_rate` swapped for the herd's own (phase bands stay shared); pastoral
  (0.25) / pen (0.90) keep their rung's rate. This is still THE single seam — every consumer reads the
  folded rate there. Anchors: rabbit/fowl **0.35**, deer/boar **0.10**, migratory **0.04** (was one
  global 0.05). **This is the PR #117 fix**: small game bred at a mammoth's rate was the artifact behind
  "a rabbit warren can't provision an expedition."
- **The convergence gate — `graze.overgraze_escapement_fraction` (0.25).** Grazing (`graze_take`) may
  draw a patch down to this fraction of capacity but **no lower** in a turn — constant-*escapement*, the
  same lesson the corral learned (`docs/plan_corral_managed_population.md` §3). Without it the herd's
  constant-*catch* demand strips an over-subscribed range into a permanently-stripped attractor at the
  reseed floor (a stunted remnant on dead ground); with it an **overgrazed range recovers** to a stable
  smaller herd. Validated `>` `reseed_floor_fraction` and `< 0.5` (the graze MSY point — overgrazing
  below the productive intensity stays possible/visible). It bounds `K` below at ≈ 0.84·`K_max`, so
  overgrazing shrinks a herd by ≤ ~16% — a modest but stable force; lower it for deeper overgrazing at
  rising crash risk.
- **Turn order (discretization that converges):** recompute `K` from **pre-eat** graze → herd grows
  toward it (clamped) → herd eats (`advance_herd_grazing`) → graze regrows (`advance_graze_regrowth`).
  The hard clamp `biomass ≤ K` plus the flat-K plateau above `cap/2` plus the escapement floor make it
  converge monotonically (no growing oscillation) from **every** start.
- **Measured — the convergence gate** (`core_sim/tests/grazing_2b_convergence.rs`, ≥300 turns, pinned):
  every regime (rabbit `r`=0.35, deer 0.10, mammoth 0.04, and the hottest `r`=0.40 = graze) reaches a
  **stable fixed point** from under-grazed / over-populated / over-grazed / two-herds-sharing starts;
  under- and over-populated starts converge to the **same** `K`; an overgrazed range (graze 0.12)
  **recovers** to graze ~0.33–0.61 / herd 88–100% `K_max`, never the stripped floor; the coupled system
  is deterministic (two runs bit-identical). Biomass tail bands are 0; the graze fraction holds a fixed
  ≤0.7% micro-2-cycle (a small band, not growing).
- **Measured — the K distribution + hunting economy** (`grazing_2b::the_2b_ii_measurement_report`,
  earthlike seed 119304647, 120 turns): Red Deer `K` mean **1352** (460 forest → 2150 steppe) vs the
  retired **1200**; Rabbit **163** (48–240) vs 200; Wild Boar **1049** vs 1000 — the sedentary species
  land near their old constants with real biome spread. Migratory `K` came in **below** the old
  constants (Steppe Runners 3212 vs 9000, Marsh Grazers 5629 vs 9000) — their loiter-cluster range ×
  cap doesn't reach the old biomass-max, a **retune flag** (lower migratory `fodder` to raise `K` if
  the megafauna hunting economy wants it). Sustain MSY (`r·K/4·p`) roughly **doubled** for deer/boar
  (both `r` and `K` up) and rose **~5.7×** for rabbit (**0.05 → 0.285** food/turn) — the **small-game
  viability reversal**: a rabbit warren is now a fast provisioner (and the small/Market hunting
  expedition, which never filled under the old uniform `r`, now completes).
- **The fast-breeder ladder inversion — FIXED in 2d.** A wild rabbit's `r`=0.35 exceeded the retired
  flat pastoral 0.25, so taming a rabbit *used* to be a growth downgrade. Grazing 2d makes the managed
  rungs a *multiple* of each species' own wild `r` (§ "Phase 2d"), so pastoral `r = wild_r × 1.5 >
  wild_r` for every species and the inversion is gone.
  `fauna_husbandry::the_husbandry_ladder_is_a_per_species_growth_rate_ladder` asserts the per-species
  gross growth-rate ladder.

See Also: `docs/plan_grazing_2b.md` §2.2 (the convergence risk), §9 (the measure list),
`docs/plan_corral_managed_population.md` §3 (the constant-escapement lesson this reuses).

### Phase 2d — the pen economy: a pen becomes fenced land

The pen slice (`docs/plan_grazing_2d.md`). A pen stops being a special case (a single frozen tile fed
entirely from the larder) and becomes **a piece of fenced land the herd grazes**:

- **`Herd.pen_radius`** (default `0` = today's single tile) — the pen's footprint is
  `hex_range_tiles(corralled_at, pen_radius)`. All footprint logic (`herd_footprint`) reads it; the
  `ExtendPen` command grows it (2d-β, below).
- **Footprint `K`** — `advance_herds` recomputes a penned herd's `K` over its footprint via the same
  `ecological_carrying_capacity` seam a mobile herd uses (penned herds stop being frozen). A
  **wholly-barren** footprint keeps the frozen `K` and is fully larder-fed (§2.3's preserved worst case).
- **Penned grazing** — `advance_herd_grazing` no longer skips corralled herds; a pen draws its footprint
  down with the same `graze_take` + `overgraze_escapement_fraction` (0.25) floor as a wild herd,
  capturing `footprint_intake`.
- **The larder offset** (§2.3) — the FEED phase pays only `pen.upkeep_per_biomass × biomass ×
  (1 − pasture_fraction)`, `pasture_fraction = clamp(footprint_intake / (fodder_per_biomass × biomass),
  0, 1)`; `pen_fed_fraction` = the total fed share (pasture + the paid part of the reduced bill). The
  food-ledger identity (`penFeedUpkeep`) is untouched — it draws the *actual* paid amount.
- **Per-species husbandry `r`** (§3) — retires flat pastoral 0.25 / pen 0.90 for `min(cap, wild_r ×
  gain)` (`pastoral_gain` 1.5, `pen_gain` 3.0, `husbandry_regrowth_cap` 0.75). `capacity_fraction` /
  `pen_capacity` are **deleted**; `herd_capacity` collapses to `herd.carrying_capacity`.
- **The net-positive invariant** is reworked to a **best-case floor** (§2.4): validate guarantees only
  the *fastest* species' pen nets positive when fully larder-fed; a slow breeder or poor-pasture pen may
  run at a **loss by design** (it pays off only when self-feeding drives upkeep → 0).
- **Wire** (append-only on `HerdTelemetryState`): `penRadius`, `penFootprintTiles` (server in-bounds
  count), `penPastureFraction`, `penExtendProgress`. Convergence gated by
  `core_sim/tests/grazing_2d_pen.rs` (a pen converges at radius 0/1; lush → free, barren → full bill).

**2d-β — the `ExtendPen` command + build ladder** (§4). Growing a pen's fenced footprint is a labor
investment worked off over turns, reusing the corral build ladder — no materials economy:

- **`Command::ExtendPen { faction, target_x, target_y }`** (full proto/runtime/text/server plumbing —
  `ExtendPenCommand` proto field **39**, verb `extend_pen <faction> <x> <y>`), routed like `Corral`
  through `handle_extend_pen`. It reuses `CommandEventKind::Corral` (one kind for the pen's whole life).
  Validation (each with a clear rejection): a herd **penned exactly at that tile** (`corralled_at`, the
  fixed anchor — *not* the roaming `position()` `corral` keys off), owned by the faction, the faction
  knows **Herding**, `pen_radius < husbandry.pen_radius_max`, **no extension already in flight**, and a
  band is **keeping** it (a Hunt assignment on the herd — else the ring never accrues and an untended
  pen escapes anyway). On success it sets the herd's **`pen_extending`** state via
  `Herd::begin_pen_extension` (which re-checks penned / not-extending / below-max, so the command's
  validation and the mutation can never disagree).
- **The build ladder** rides the corral-tend branch of `advance_labor_allocation`: while `pen_extending`,
  the keeper's HARVEST is **dipped to `corralling_yield_fraction`** (the forgone yield *is* the labor
  cost of the ring, the same dip the corral *build* pays), and `Herd::accrue_pen_extension` adds
  `husbandry.corral_build_progress_per_turn` (0.04 → ~25 turns/ring) to `pen_extend_progress` **after**
  the take. At `1.0` the ring completes: `pen_radius += 1` (saturating at `pen_radius_max`),
  `pen_extend_progress` resets, `pen_extending` clears, and a `Corral` feed line fires; the larger
  footprint's higher K arrives on the next `advance_herds`. The FEED (larder offset) is unchanged while
  extending — self-feeding and the harvest dip are orthogonal.
- **Config:** `husbandry.pen_radius_max` (**2** → up to a 19-tile footprint; validated `>= 1`). The only
  new lever. **`pen_extending`** persists on `HerdState` alongside `pen_radius` / `pen_extend_progress`,
  so a rollback rewinds an in-flight extension. `penExtendProgress` on the wire now carries the live ring
  meter (α left it at 0) for a client "Fencing N%" badge.
- **Tests:** `grazing_2d_pen::extend_pen_accrues_a_ring_flips_the_radius_raises_k_and_caps_at_max` (the
  ring accrues over ~25 turns, flips `pen_radius` 0→1, K rises with the 7-tile footprint, and caps at
  `pen_radius_max`); `server::tests::extend_pen_*` (the five validation rejections + the happy path).
- **Deferred (2d-γ, client):** the footprint highlight, the feed-split readout (`penPastureFraction` +
  `penUpkeep`), and the extend affordance / "Fencing N%" badge (`penExtendProgress`).

**2d-δ — the husbandry ceiling: which species climb the ladder** (§4a). Not every animal can be herded,
and not every herdable one can be penned. The ladder is a **sequence** (wild → pastoral → pen), so a
species' reach is a single **enum** (`fauna_config::HusbandryCeiling` = `Wild | Pastoral | Pen`), not
two flags — which makes the incoherent "pennable but not tameable" state unrepresentable (no
`validate()` combo guard).

- **`SpeciesDef.husbandry_ceiling`** (`#[serde(default)]` = `Pen`, so an untagged/future species keeps
  the full ladder) is **cached onto `Herd` at spawn** (mirroring `regrowth_rate`/`fodder_per_biomass`),
  round-tripped through `HerdState.husbandry_ceiling`, and read by the gates via `Herd::can_domesticate()`
  / `can_pen()`. Roster: **mammoth/deer = `wild`** (hunt-only), **steppe_runner/marsh_grazer =
  `pastoral`** (nomadic herding — follow, don't fence), **boar/rabbit/fowl = `pen`** (pigs/hutches/poultry).
- **Three gates.** (1) **Domestication accrual** — `Herd::accrue_domestication` self-guards on
  `can_domesticate()`, so a `wild` species never tames and never picks up an `owner` (robust regardless
  of call site). (2) **The `domesticate` claim** — `handle_domesticate` rejects a `wild` species
  ("{Species} is wild game — hunt-only…"). (3) **The `corral` / `extend_pen` commands + the `Corral`
  policy accrual** — `validate_labor_policy` (shared by `handle_corral` and `assign_labor … corral`) and
  the `Corral` accrual in `advance_labor_allocation` both require `can_pen()` (only `pen`), so a
  `pastoral` species tames and roams but the pen path is closed ("{Species} cannot be penned").
  `handle_extend_pen` carries the same check belt-and-braces (unreachable via the gated corral path).
- **Wire:** `HerdTelemetryState.husbandryCeiling:string` (`wild`|`pastoral`|`pen`; append-only, mirrors
  `sizeClass`/`ecologyPhase`) so the client can hide the corral/extend affordance on a non-`pen` herd and
  the whole domestication track on a `wild` one.
- **Note — a mid-build gate change:** the `Corral` accrual gate is checked each turn, so a
  (command-unreachable) non-`pen` herd mid-corral-build would simply **stop progressing** — a soft
  stall, not a crash — and there are no shipped saves to carry such a state.

---

## Pre-commit Yield Forecast (per-source, on the wire)

The **retained yield telemetry** (`SourceYield.actual/sustainable/workers_needed`, above) is
**post-hoc** — the player only learns they over-assigned *after* committing and advancing a turn. The
forecast is its pre-commit twin: per in-range source, the snapshot exposes enough for the client to
show a live **"Expected yield: +X.XX /turn"** and **cap its worker stepper at the max-useful count
while the player is composing an assignment**.

**Wire fields** (append-only, on both `WorldSnapshot` and `WorldDelta`) — the same shape on
`ForagePatchState` (per tile) and `HerdTelemetryState` (per herd):
`perWorkerYield:float` + `ceilingSustain` / `ceilingSurplus` / `ceilingMarket` / `ceilingEradicate`
(all `float`, **food/turn**, at the source's CURRENT biomass), **plus the investment rung**:
`ForagePatchState.ceilingCultivate` + `tendedYield` and `HerdTelemetryState.ceilingCorral` +
`corralYield`. The investment policy's `ceiling*` is the **preparing** yield
(`fraction × ceilingSustain` — the dip); `tendedYield`/`corralYield` is what the source will pay
**once the improvement completes**, so the client can show **"preparing X → then Y"** *before* the
player commits to the cost. (Sim-side both live on the shared `SourceYieldForecast` as
`ceiling_prepare` / `managed_yield` — the two investment policies are kind-exclusive, so one field
serves both.)
- `perWorkerYield` = food/turn one worker contributes (throughput → provisions; **forage folds in the
  tile's `seasonal_weight`**, as `forage_take` does — it can be `0` in a dead season, so consumers must
  not divide by it; hunt has no seasonal factor).
- Each `ceiling*` = that policy's food/turn cap, **already clamped to the source's remaining biomass**.
- Captured at `output_multiplier = 1.0` (the productivity multiplier is per-band): the client scales
  every field by the acting band's `PopulationCohortState.outputMultiplier` — a linear factor, so
  `max_useful_workers` is invariant to it.
- Client composition: `expected(workers, policy) = min(workers × perWorkerYield, ceiling[policy])`,
  `max_useful_workers(policy) = ceil(ceiling[policy] / perWorkerYield)`.
- A **tended (cultivated) patch** / **corralled herd** is maintenance labor, not scaling gather: every
  ceiling is its managed yield and `perWorkerYield` equals it, so `max_useful_workers == 1`
  (`TENDED_SOURCE_WORKERS_NEEDED`) and the policy is irrelevant.

**Invariant: forecast == actual — no duplicated yield math.** The forecast and the take path read the
*same* pure helpers, so the UI can never promise a number the sim won't pay:
- forage (`forage.rs`): `forage_policy_ceiling` (the 4 extractive rungs **+ Cultivate**, biomass) · `forage_per_worker_biomass`
  (`per_worker_biomass_capacity × seasonal`) · `forage_provisions` (biomass→provisions ×
  `output_multiplier`) · `tended_provisions` (the tended-patch managed harvest) — all called by both
  `forage_take` / the tended-patch arm of `advance_labor_allocation` **and** `forage_forecast`.
- fauna (`fauna.rs`): `hunt_policy_ceiling` (the 4 extractive rungs **+ Corral**) · `hunt_provisions` ·
  **`managed_yield_biomass`** (the husbandry harvest, via `pen_yield_biomass`) · **`herd_ecology` /
  `herd_capacity`** (which ecology/capacity a herd lives under — *no call site may re-derive either*) —
  called by both `systems::hunt_take` / the corral arm of `advance_labor_allocation` **and**
  `hunt_forecast`. The shared `SourceYieldForecast` struct (with `::tended`) is the common return shape.
  A corralled herd's `managed_yield` is **gross**; its `penUpkeep` is exported separately.
- Guarded by `systems::labor_yield_tests::{forage,hunt}_forecast_equals_actual_take_for_every_policy_and_staffing`
  (every policy × labor-bound/ceiling-bound staffing, comparing against the payout of a real
  `advance_labor_allocation` run) and `tended_patch_and_corral_forecast_full_yield_with_one_worker`.
  **Any change to the take math must go through these helpers** — never re-derive a ceiling or a
  biomass→provisions conversion at a call site.

Capture: `snapshot_forage_patches` / `herd_snapshot_entries` (`snapshot.rs`); the herd's
`carrying_capacity` (absent from the display telemetry) is resolved from the authoritative
`HerdRegistry`, and the per-tile `seasonal_weight` from the `FoodModuleTag` query.
**Client follow-up:** rendering the live "Expected yield" line + the worker-stepper cap in the
forage/herd assign controls.

### Assign-time yield seeding (the `+0.00` fix)

The retained `SourceYield` telemetry used to be written **only** during turn resolution, so between
"player assigns workers" and "player advances the turn" a brand-new source had no row and the display
snapshot serialized `actual_yield = 0.0` — the map annotation and the Band panel read **`+0.00`** for
every fresh assignment, and the client cannot distinguish "0 because not computed yet" from "0 because
the source is barren". Fixed server-side: `handle_assign_labor` (and the `cultivate`/`corral` policy
shorthands, via `set_policy_on_working_bands`) **seeds the touched source's `SourceYield` from its
pre-commit forecast** right after mutating the `LaborAllocation` (`server.rs::seed_source_yield` →
`LaborAllocation::set_source_yield`). Because forecast == actual (above), the seeded number is exactly
what the turn then pays under unchanged conditions — **no jump** — and it is the same number the
client's compose-time "Expected yield" row promises. Shape:
- **The expected take** is the one shared helper `fauna::forecast_expected_take(&SourceYieldForecast,
  workers, policy) = min(workers × per_worker_yield, forecast.ceiling_for(policy))`
  (`SourceYieldForecast::ceiling_for` is the `ceiling[policy]` lookup; the two investment policies
  share `ceiling_prepare`, the reduced `cultivating_yield_fraction`/`corralling_yield_fraction` bite —
  once the improvement *completes* the source is `::tended`, whose every ceiling already **is**
  `managed_yield`). The client preview, the seed, and the forecast==actual tests all call it.
- The kind-specific seeds `forage::forage_source_yield_preview` / `fauna::hunt_source_yield_preview`
  compose the full row through the shared `forecast_source_yield`: `actual` = the expected take,
  `sustainable` = the same MSY value the resolution path records (a *managed* source reads
  `sustainable == actual` — no ⚠), `workers_needed` = the same overstaffing inversion (a managed source
  = `TENDED_SOURCE_WORKERS_NEEDED`). No new formula, no new config lever.
- **Only the source the command touched** is seeded (other sources keep their real actuals), and only
  where the turn would actually pay: out of `band_work_range` / past the hunt leash, an unseeded patch
  or a vanished herd keeps its zero row, and a **genuinely barren source still seeds `0.0`** — `+0.00`
  stays reachable, and correct, there. Consequence (intended): a fresh assignment now *previews* its
  contribution to the Food-line net rate + the Gathered/Hunted breakdown, and can pre-trip the
  overdraw ⚠ if the chosen policy would overdraw — ⚠ is a leading flow signal by design.
- `LaborAllocation` now keeps `last_yields` **index-aligned with `assignments`** across every mutation
  (`set_assignment`/`normalize`/`clear` — the snapshot zips the two by index, so a row left behind by a
  removed assignment used to be attributed to the *next* source). New rows default to
  `SourceYield::ZERO`.
- Guarded by `server::tests::{assigning_forage,assigning_hunt}_workers_seeds_the_expected_yield_before_the_turn`,
  `resolved_{forage,hunt}_yield_equals_the_seeded_yield` (the no-jump property),
  `changing_the_policy_reseeds_the_expected_yield`, `a_barren_source_seeds_zero`,
  `unassigning_a_source_drops_its_yield_row`.

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
- **Hunting**: retarget `BandTravel` to the herd's live tile each turn (from `HerdRegistry`). The
  take **and the trip-completion decision both live inside the `hunt.reach_tiles` guard** — a party
  still walking to its herd never concludes the trip. Once in reach, take a **productive** hunt's
  worth of biomass — `workers × per_worker_biomass_capacity`, capped per policy (below) — from the
  herd and convert to provisions (`fauna::hunt_provisions`) up to the carry cap (`party ×
  hunt.per_worker_carry`). Deliver only with a worthwhile load: a full pack **or** `herd_near_band &&
  carried ≥ hunt.min_deliver_fraction × cap` (the empty-larder flip-flop fix). An empty pack at
  completion reports **why** (no sustainable take / no take possible), never a cheerful zero.
- **Per-policy behaviour**: **Sustain** — takes the **shared MSY *flow* ceiling**
  (`fauna::hunt_policy_ceiling(Sustain, …)`, the *same* take a resident band's Hunt arm makes from
  the same herd state: "Sustain" has **one meaning** across the sim). It is **not** a stock target —
  there is no sustain floor and no stock-line completion; the trip ends on a full pack, a near-band
  delivery, a recall, or a lost herd, and the herd is held steady (skim = regrowth). **Surplus** —
  one full-cap haul, capped by *stock* headroom down to the ecology collapse threshold
  (`hunt_expedition_floor`) + **done**; **Market** — the same stock headroom, in repeated full-cap
  trips via `Delivering`→deposit→**auto-relaunch** (the deposit fires once the party is back within
  communication range of the home band — the shared `near_home` proximity — not necessarily on its
  exact live tile), grinding the herd toward the collapse floor until it crashes or you recall;
  **Eradicate** — no floor, **delivers no food** (denial): keeps taking each turn until the herd is
  extinct, then folds back empty. A lost/extinct herd → shared `Returning`.
- **Launch viability forecast — a bounded forward SIMULATION, not a division** (`hunt_trip_forecast`,
  `systems.rs`). It runs the trip forward turn by turn — `fauna::regrow_biomass` (what `advance_herds`
  does in Logistics) then `expedition_take_biomass` (what the `Hunting` arm does in Population), in
  that order, accumulating the larder on the **fixed-point `Scalar` grid** exactly as the real one
  does — until the pack is full or `hunt.forecast_horizon_turns` (**60**) is hit. There is no second
  copy of the model to drift: each simulated turn is the same pair of calls the sim makes.
  - *Why not a closed form?* **There is no single rate.** Dividing the carry cap by one per-policy
    number is exact only when that number is a genuine per-turn **flow** (Sustain's MSY) or when the
    party is throughput-bound for the whole trip (Surplus/Market on a *big* herd). Under
    **Surplus/Market on a small herd it is a total *stock*** — the party strips the headroom down to
    the collapse floor in a turn or two and then crawls at the herd's regrowth trickle. The division
    read a full Rabbit Warren (K = 200, 4 hunters, Surplus) as a **~5-turn** trip; the simulation says
    that party **never fills inside the 60-turn horizon** — only a *1-worker* party fills (in **23
    turns**: a quarter the pack, so the regrowth trickle can still reach it). Simulating collapses
    both regimes into one honest answer.
  - The estimate is **turns spent hunting once you arrive** — **travel is not counted**, and the herd
    is assumed stationary and in reach. **Eradicate** delivers no food (`delivers_food == false`) → no
    ETA, ever.
  - Past the horizon the answer is "**won't fill**", not a number: `viability_warn_turns` is 20, so a
    60+-turn trip is emphatically not viable and the precise value carries no actionable information
    — and the bound is what keeps the per-snapshot export cheap.
  - **The "cannot fill" answer is O(1), not 60 simulated turns** (`hunt_trip_provisions_bound`). Most
    of the exported estimate table is trips that never fill — small game under every policy, Sustain
    on most herds — and simulating one to its horizon is spending the entire budget proving a "no" the
    slowest possible way (measured: **85% of the table's cost**). So before simulating, the forecast
    computes a **true upper bound** on the provisions the party could land over the whole horizon:
    `min(horizon × party throughput, ecology)`, where *ecology* is `horizon × fauna::peak_regrowth`
    for Sustain (a per-turn *flow* ceiling, capped by the logistic peak at K/2) and standing headroom
    down to `hunt_expedition_floor` **plus** `horizon × peak_regrowth` for the depleting policies (a
    *stock* draw-down — by conservation, everything the party can ever remove). Bound `< carry cap` →
    "won't fill", returned after simulating only the **first** turn (the forecast still reports its
    opening rate). Both terms over-estimate by construction, and the bound carries an explicit
    rounding cushion (`CANNOT_FILL_BOUND_MARGIN` + the `Scalar` quantization slack — load-bearing:
    the sim's `f32` conversion can land *exactly* on a cap an `f64` bound reads a hair below), so it
    can never reject a trip that would have filled. Pinned by `systems::hunt_trip_bound_tests`, which
    asserts the short-circuited forecast is **identical** to the unabridged simulation across every
    policy × party size × herd state (wild + domesticated, sub-Allee → at-capacity), on the shipped
    levers *and* off-nominal hot-reloadable ones. Exported table unchanged; measured **~2.3 ms →
    ~0.8 ms per snapshot** at 122 herds (~19% → ~7% of a ~11.5 ms capture).
    - *Not done, and why:* collapsing the 8 party sizes into one simulation where the trip is
      throughput-bound (rate and cap both scale with workers → identical `turns_to_fill`) is
      **measurably worthless on the shipped levers**: a hunter's 40 biomass/turn exceeds every game
      herd's ceiling (Sustain's MSY is `0.0125 × K` — under 40 for any `K < 3200`, i.e. all non-
      migratory herds), so the *herd* binds, not the party, and `turns_to_fill` genuinely varies with
      party size. Only **4 of 488** (herd × policy) rows on a real map are constant across all 8
      sizes — 0.005 ms of an 0.8 ms table. Revisit only if `hunt.per_worker_biomass_capacity` drops
      far enough to make parties throughput-bound.
  - Shipped-lever reality check (4 hunters, full herd): Red Deer ~5 turns under Surplus/Market and ~54
    under Sustain; a full Rabbit Warren (K = 200) **never fills a 4-hunter pack inside the horizon**
    under *any* policy (simulated past the horizon it would take ~320 turns under Sustain and ~495
    under Surplus/Market — the forecast reports "won't fill" rather than those numbers). The *only*
    small-game trip that fills at all is a **lone hunter** under Surplus/Market (**23 turns** — a
    quarter the pack, so the herd's regrowth trickle can still reach it), and that is well past
    `viability_warn_turns`. Small game simply cannot provision an expedition — the forecast now says
    so.
  - `handle_send_hunt_expedition` folds the verdict into the `ExpeditionSent` feed line — viable
    (`≤ hunt.viability_warn_turns`) → "est. ~N turns to fill"; marginal (`>` it) → the same plus "NOT
    VIABLE at this herd's yield"; **won't fill inside the horizon** → "the party will NOT fill its pack
    within N turns; NOT VIABLE"; impossible (a sub-Allee herd — `first_turn_provisions == 0`) → "the
    party will return empty"; **denial** (Eradicate) → "denial mission: the party delivers NO food".
    It still launches — the player's call. `detail` carries `eta_turns=…`.
  - Pinned end-to-end by `expedition_hunt.rs` (`party_fills_on_the_forecast_turn`), which launches a
    **real party**, runs the sim forward, and asserts the larder first reaches the carry cap on exactly
    the promised turn — across the throughput-bound, flow-bound and **stock-exhausted** regimes. The
    forecast is pinned to the sim, never the reverse.
- **Lives off its kills** — no launch provisions, no per-turn upkeep (upkeep is scout-only).
- **The investment policies are NOT an expedition concept.** `Cultivate`/`Corral` are place-bound work
  a *resident* band does (prepare a patch, build a pen, then tend it) — a detached party cannot pen a
  herd and walk home. `handle_send_hunt_expedition` **rejects** them at launch (alongside an
  unparseable token), so the expedition's whole axis is `FollowPolicy::EXTRACTIVE` (the four extractive
  rungs). `systems::hunt_expedition_ceiling`'s investment arm is therefore **unreachable**, and yields
  **`0.0` + a `debug_assert!`** rather than quietly falling back to the Sustain flow: if that
  validation ever regresses the party takes *nothing* and the hole is loud, instead of a
  plausible-looking Sustain trip hiding it. (An unreachable arm must fail loudly, never quietly do
  something plausible.) Guarded by
  `server::tests::send_hunt_expedition_rejects_the_investment_policies`.
- **Shared take helpers** (`fauna.rs`): **`hunt_policy_ceiling(policy, biomass, cap, fauna)`** is THE
  single source of the per-policy take ceiling, exhaustive over all six policies (Sustain =
  `sustainable_yield` / MSY, Surplus = × `follow.surplus_multiplier`, Market = `market.take_fraction ×
  biomass`, Eradicate = `hunt.take_from`, **Corral** = `husbandry.corralling_yield_fraction ×
  sustainable_yield(..)` — the investment dip while the pen is built, expressed through the *same* MSY
  helper — and **Cultivate** = `0.0`, the forage-only policy's symmetric defensive case, mirroring how
  `forage_policy_ceiling` yields nothing for `Corral`), and **`hunt_provisions(take, fauna,
  output_multiplier)`** the single biomass→provisions conversion (an `f32`; the take path quantizes it
  onto the larder's `Scalar` grid). `hunt_policy_ceiling` is the *building*-phase ceiling: a
  **completed** corral is never hunt-drawn at all — the Hunt arm takes the tend branch (paid
  `corral_provisions`, no biomass drawn) — and `fauna::hunt_forecast` is the one place that phase split
  lives (`herd.is_corralled()` → `SourceYieldForecast::tended`). `hunt_take` (`systems.rs` — band Hunt
  labor + the **scout's
  opportunistic replenish**, a Sustain nibble when a scout's provisions fall below `party ×
  provision_upkeep_per_worker × replenish.low_turns` and a herd is within `replenish.reach_tiles`) and
  the hunt expedition both call them, so no formula has a second copy. The expedition applies **no**
  output multiplier (`EXPEDITION_OUTPUT_MULTIPLIER` — a detached party carries no band morale
  modifier). **The expedition take is FOOD-ONLY — a known, tracked gap.** The band's Hunt arm credits
  food **+ trade goods** (Market) **+ husbandry/domestication accrual** (Sustain on a Thriving herd)
  from the same take; the expedition credits food and nothing else, so a Sustain *expedition* builds no
  domestication and a Market *expedition* yields no trade goods. Whether a detached party *should* earn
  those side-effects — and what Market's goods and Eradicate's denial are ultimately *for* — is the
  **"Hunt policy payoffs"** arc in `TASKS.md` (design: `docs/plan_exploration_and_sites.md` §2b).
  Catching a *migratory* herd depends on the deferred fauna-movement redesign (herds step 1 tile/turn
  today, so an equal-speed party can't close a long one-directional route).

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

**Pre-launch export — the client does ZERO arithmetic.** The launch forecast above only rides the
*post-commit* `ExpeditionSent` feed line; the outfit UI needs the trip's economics **before** the
player commits workers, as they pick party size / herd / policy. The expedition's trip length is **not
a formula** (see the forecast above: a small-herd Surplus party exhausts *stock*, so no per-turn rate
describes the trip), so the sim exports the **answer** it simulated, and the client's job is a **table
lookup**:
- `HerdTelemetryState.huntTripEstimates:[HuntTripEstimate{ policy:string, partyWorkers:uint,
  turnsToFill:uint, deliversFood:bool }]` — per **huntable** herd, one entry per
  `FollowPolicy::EXTRACTIVE` × every legal party size (`1..=expedition_config.max_party_size`, so
  4 × 8 = 32 rows/herd; `policy` is a free-form string like `species`, so a new policy needs no schema
  change). **The four extractive rungs ONLY** — the investment policies are launch-rejected (above), so
  a `Cultivate`/`Corral` row would be a number for a trip that cannot be launched, and would inflate a
  table we just optimized (2.28 ms → 0.79 ms) for nothing. `turnsToFill` is the
  simulated hunting-turn count; **`0` = does not fill** within `hunt.forecast_horizon_turns` → render
  "won't fill", never a number. `deliversFood == false` (Eradicate) → render "no food delivered
  (denial)", never an ETA. **Travel is excluded** — the number means "turns spent hunting once you
  arrive".
- `HerdTelemetryState.huntPolicyCeilings:[HuntPolicyCeiling{ policy:string, provisionsPerTurn:float }]`
  — the **BAND / local-hunt** ceiling only, one row per `FollowPolicy::HUNT_POLICIES`: the four
  extractive rungs **plus `Corral`** (a legitimate *band* Hunt policy — its deliberately dipped yield
  is exactly what the player must see before committing to a 25-turn pen). `Cultivate` is Forage-only,
  so a herd has **no** cultivate row. Each is the worker-independent ceiling for the herd's current
  state, in provisions/turn, **clamped to the herd's remaining biomass** (so it is a true maximum take,
  not a formula value a nearly-extinct herd could never supply — inert under today's levers, but
  `regrowth_rate` / `surplus_multiplier` / `market.take_fraction` are levers and raising one must not
  silently over-state the readout). A collapsing (sub-Allee) herd exports `0` under Sustain/Surplus.
  **Sourced by projecting the herd's `fauna::hunt_forecast`** (`SourceYieldForecast::ceiling_for`) —
  the *same* object the scalar `ceilingSustain`/…/`ceilingCorral` fields export, so the list and the
  scalars are literally the same numbers and cannot drift, and the take path pays exactly them
  (forecast == actual). That also makes `Corral` **phase-correct for free**: the
  `corralling_yield_fraction × MSY` dip while the pen is being built, and the **full corral yield**
  once `is_corralled()` (a penned herd forecasts as `SourceYieldForecast::tended` — every ceiling is
  its managed yield, one keeper suffices). There is **no expedition ceiling field** — the retired
  `expeditionProvisionsPerTurn` was exactly the "one number that means a flow for Sustain and a stock
  for Surplus/Market" design smell the estimate table replaces.
- `PopulationCohortState.huntPerWorkerProvisions:float` (one hunter's
  provisions/turn throughput = `labor_config.hunt.per_worker_biomass_capacity ×
  fauna_config.hunt.provisions_per_biomass`) and `.expeditionViabilityWarnTurns:uint`
  (`expedition_config.hunt.viability_warn_turns` — the NOT-VIABLE threshold the client applies to
  `turnsToFill`) — global levers echoed onto **every** cohort (the `maxExpeditionPartySize` idiom; the
  outfit UI lives on the resident-band panel).

**The two hunt readouts, and what each reads:**
- **Expedition (pre-launch trip)** — a lookup: `huntTripEstimates[(policy, partyWorkers)]` →
  `turnsToFill` (`0` = won't fill), `deliversFood`. Viable iff `0 < turnsToFill ≤
  expeditionViabilityWarnTurns`. No arithmetic, no ecology model, no rate.
- **Resident band (local-hunt yield preview)** — pure arithmetic over the **band** ceiling, **× the
  cohort's already-exported `outputMultiplier`** (a band applies its morale/discontent productivity
  modifier at payout): `rate = min(workers × huntPerWorkerProvisions, bandCeiling_for(policy)) ×
  outputMultiplier`. That is arithmetically `hunt_take(.., carry_room_biomass = INFINITY)` — what the
  band's Hunt labor arm really takes (the conversion and the multiplier are linear, so they factor out
  of the `min`, and the exported ceiling is biomass-clamped exactly as the take is).

`core_sim/tests/expedition_hunt.rs` pins **both — each to the sim's REAL behaviour, never to another
preview** (the lesson of the ~34-vs-~6-turn Surplus bug: the old guard compared the client against
`hunt_trip_forecast`, so two copies of the same wrong ceiling agreed with each other while both
disagreed with the take). `exported_hunt_trip_estimates_match_a_real_party_run` asserts every exported
estimate (small-game / big-game / collapsing herd × all four policies × every legal party size) equals
what a **real party run forward through the real systems** actually does — including the
stock-exhaustion case that motivated the rewrite; `exported_snapshot_fields_reproduce_band_hunt_take`
does the same for the band arithmetic against `hunt_take(..)` (healthy / clamp-binding depleted /
collapsing herd × every worker count × all four policies × a unit and a discontent-reduced output
multiplier). If either readout ever drifts from the sim, that test fails.

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
`LaborAllocation.last_yields` each turn — one `SourceYield { actual, sustainable, workers_needed }`
(f32 provisions + a worker count)
per assignment, **in the same index order** as `assignments` (so the snapshot zips by index — every
`LaborAllocation` mutator keeps the two aligned; see "Assign-time yield seeding"). It is
**derived, not persisted**: it is out of rollback (`#[serde]` never sees it; `labor_allocation_from_state`
restores only the assignments, leaving it empty until the next tick) and is **excluded from
`LaborAllocation`'s equality** (manual `PartialEq` compares assignments only) so it can't perturb the
persisted-intent comparison. A row is also written **at assign time**, seeded from the source's
pre-commit forecast, so a brand-new assignment shows its expected yield instead of `+0.00` before the
turn resolves (see "Assign-time yield seeding (the `+0.00` fix)" under Pre-commit Yield Forecast). Definitions: **`actual`** = the provisions the source produced this turn
(the value added to the larder); **`sustainable`** = what it could yield without drawing down its
stock. As of §0-ii **forage is depletable too**, so a forage `sustainable =
sustainable_yield(biomass_before, carrying_capacity, forage.ecology) × forage.provisions_per_biomass ×
output_multiplier`** (**MSY** — regrowth at the most-productive biomass K/2, so a *full* patch still
reads a positive sustainable harvest, no longer 0) — the plant mirror of the
**hunt `sustainable = sustainable_yield(biomass_before, carrying_capacity, ecology) ×
hunt.provisions_per_biomass × output_multiplier`** (MSY at the *pre-take* biomass). `sustainable_yield`
is shared by hunt + forage (`fauna.rs`); `net_biomass_delta` remains the **actual** per-turn biomass
evolution used by `regrow_biomass`/`advance_herds` (0 at K — correct there, unchanged).
A Sustain gather/hunt reads `actual ≈ sustainable`; an over-draw reads `actual > sustainable` (the
overdraw ⚠). Scout/Warrior push `{0,0,0}`. **`workers_needed`** is the parallel **overstaffing**
signal: the *minimum* assigned workers that would have produced the same take — `ceil(take_biomass /
per_worker_capacity)` clamped into `[1, assigned]` when anything was taken, else `0`, computed in both
the Forage arm (capacity = `forage.per_worker_biomass_capacity × seasonal_weight`, matching
`forage_take`'s worker cap so a low-season labor-bound patch isn't falsely flagged) and the Hunt arm
(capacity = `hunt.per_worker_biomass_capacity`, no seasonal) via the shared `workers_needed_for_take`
helper. A *tended* patch / *corralled* herd (maintenance labor, not scaling gather) is fixed at `1`
(`TENDED_SOURCE_WORKERS_NEEDED`). When the binding constraint on a source's take is **not** labor
(policy ceiling / biomass / regrowth), `workers_needed < assigned` → the source is overstaffed and the
extra workers were idle. The snapshot surfaces all of this: each `LaborAssignment` row
carries `actualYield`/`sustainableYield`/**`workersNeeded`** (client accessor `workersNeeded()`), and
each `PopulationCohortState` carries band-level
`foodIncome` (Σ per-source `actual`) + `foodConsumption` (the food the people **actually ate** this
turn — `PopulationCohort::last_food_consumption`, the real `stores` debit at the turn's *opening*
brackets, **not** a `food_demand` re-derived at capture on the post-turn brackets; the same turn's
births would inflate that and break the larder ledger identity by exactly the growth. `daysOfFood`
still divides by the post-turn `food_demand` — a forward "turns I can last", a different question).
All derived at capture (0 on a rehydrated save before the next tick). **The client
consumes these next** (allocation-panel rows + tooltip + ledger footer, a follow-up PR): a per-turn
`actual > sustainable` is the client-derived **overhunting signal** — a *leading* flow indicator,
distinct from the stock-based `ecology_phase` — and `workers > workersNeeded` is the **overstaffing**
indicator (flag the wasted labor on the source row + the forage biomass/cap tile-card row).

All of the above is **post-hoc** (it reports what a committed turn produced). Its **pre-commit** twin —
the per-source `perWorkerYield` + policy ceilings the client uses to show an expected yield and cap the
worker stepper *before* the player commits — is the "Pre-commit Yield Forecast" section below, which
shares the take path's yield helpers so forecast == actual.

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
- **domestication** = `(HerdRegistry::domesticated_count(faction) +
  ForageRegistry::cultivated_count(faction)) / references.domesticated_herds` (the Phase E seam +
  the Phase 1a cultivation fold-in — plant + animal domestication share one driver; see "Cultivation"),
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

The rollback snapshot round-trips the **authoritative `HerdRegistry`** (via `HerdState` + the shared `EcologyState` record in `WorldSnapshot.herd_registry`), not just the lossy display telemetry — see the herd-persistence note under "Fauna & Wild Game" for details and the bug it fixed. The **`ForageRegistry`** rides the same pattern (per-tile `ForageState` = tile key + the shared `EcologyState`, in `WorldSnapshot.forage_registry`) so a rollback rewinds forage depletion — see "Depletable Forage".

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
