# Shadow-Scale Prototype Architecture

## Overview
- **Headless Core (`core_sim`)**: Bevy-based ECS that resolves a single turn via `run_turn`. Systems run in the order materials → logistics → population → power → tick increment → snapshot capture.
- **Networking**: Thin TCP layer (`core_sim::network`) streams snapshot deltas, emits structured tracing/log frames, and receives control commands. Commands flow over a single length-prefixed Protobuf `CommandEnvelope` socket (`SimulationConfig::command_bind`), while snapshots broadcast on `SimulationConfig::snapshot_bind` / `snapshot_flat_bind` and logs on `SimulationConfig::log_bind`.
- **Simulation Defaults**: `core_sim/src/data/simulation_config.json` seeds `SimulationConfig` with map dimensions, environmental tuning, trade/power/corruption multipliers, migration knobs, and the default TCP bind addresses/snapshot history depth. Designers can edit these baselines (grid size, mass bounds, leak curve, corruption penalties, networking ports) without touching Rust; the loader converts floats to fixed-point `Scalar` values on startup.
- **Serialization**: Snapshots/deltas represented via Rust structs and `sim_schema::schemas/snapshot.fbs` for cross-language clients.
- **Shared Runtime (`sim_runtime`)**: Lightweight helpers (command parsing, bias handling, validation) shared by tooling and the headless core.
- **Inspector Client (`clients/godot_thin_client`)**: Godot thin client that renders the map, streams snapshots, and exposes the tabbed inspector; the Logs tab subscribes to the tracing feed, offers level/target/text filters, and renders a per-turn duration sparkline alongside scrollback. A Bevy-native inspector is under evaluation (see `shadow_scale_strategy_game_concept_technical_plan_v_0.md` Option F) but would live in a separate binary to keep the headless core deterministic.
- **Benchmark & Tests**: Criterion harness (`cargo bench -p core_sim --bench turn_bench`) and determinism tests ensure turn consistency.

### Brand & Campaign Labels
- Working marketing label: “Trail Sovereigns” for the late-forager nomadic campaign described in the manual (§2a). Engineering keeps `ShadowScale` identifiers in code/assets until a rename decision lands.
- UI copy: surface “Trail Sovereigns” in client shells, campaign selection, and marketing strings loaded from localization tables; treat as data so alternating labels are possible without rebuilds.
- Data hooks:
  - Extend `StartProfile` schema (`core_sim/src/data/start_profiles.json`) with optional `display_title` and `display_subtitle` fields. Loader should fall back to legacy behaviour when the fields are absent so existing scenarios stay valid.
  - Thread the resolved label through `StartProfileHandle`, include it in snapshot metadata (`WorldSnapshot::campaign_label`), and persist on save creation so client shells can display it without re-querying static data.
  - Provide localization keys rather than raw strings once the localization pass lands; the loader should accept either literal text or `{ "loc_key": "campaign.trail_sovereigns.title" }` objects to future-proof against multi-language builds.
- Implementation status: `core_sim` now loads `core_sim/src/data/start_profiles.json`, stores campaign label text/keys in a `CampaignLabel` resource, and serializes them via the new `SnapshotHeader.campaignLabel` FlatBuffer field consumed by `clients/godot_thin_client`.
- Localization scope: when the localization system comes online, mirror the label fields into the string table (`clients/shared/localization/*.json`) and expose campaign-facing taglines/blurbs under the same namespace. Track formatting tokens (e.g., `{world_name}`) so marketing copy can remain dynamic.
- Client wiring: the Godot HUD consumes `campaign_label` dictionaries, resolves their localization keys via `res://src/data/localization/en.json`, shows the title/subtitle above the turn readout, and logs `[analytics] campaign_label …` whenever the active label changes so telemetry streams can ingest the event.
- Scenario picker: the inspector's Map tab surfaces `campaign_profiles` from snapshots, lets users choose a start profile, dispatches the `start_profile` command, and optionally regenerates the map to realize the new selection.
- Telemetry: emit the active campaign label in session analytics and marketing beacons so external telemetry can map KPI trends back to campaign branding choices.

## World Generation (Map Builder)
Implements the procedural map pipeline that produces terrain, coasts, rivers/lakes, climate bands, resources, and wildlife spawners. Player-facing framing lives in the manual (§3a World Bootstrapping, §3b Terrain Palette); this section defines the data and steps that feed snapshots and downstream systems.

### Pipeline (MVP → Extensible)
- Macro landmask: Place `macro_land.continents` seeds (biased toward high elevation), grow each continent using weighted BFS until it meets its quota (`target_land_pct`, `min_area`, `jitter`). Remaining tiles fill by descending elevation so the global land percentage matches the preset.
- Tectonics pass: For each continent, derive drift vectors, collision belts, interior fault seams, volcanic arcs, and dome plateaus. Emit a mountain mask (`Fold`, `Fault`, `Volcanic`, `Dome`) with strength metadata before shelves/elevation restamping run. Fold belts widen according to `mountains.belt_width_tiles` and feed elevation restamping to form continuous ridges.
- Polar microplates: When a land component intersects the configurable polar band (`mountains.polar_latitude_fraction`), subdivide the polar tiles into micro-plates whose density comes from `mountains.polar_microplate_density`. Converging vectors raise fold strength (multiplied by `mountains.polar_uplift_scale`), divergent seams cap relief (`mountains.polar_low_relief_scale`), and shear seams sprinkle extra faults. The `mapgen.tectonics.polar_microplates` trace target logs per component so designers can tune presets; see the player framing in §3a “World Bootstrapping” of the manual. The built-in `polar_contrast` preset pins a repeatable polar layout via `seed_policy = "preset_fixed"` + `map_seed`, and bumps density/uplift (`polar_microplate_density ≈ 0.0065`, `polar_uplift_scale ≈ 1.85`) so each cap fractures into multiple belts without losing flat corridors—no edits to `simulation_config.json` required.
- Heightfield: Generate a seeded multi-octave height raster; apply erosion-ish smoothing; persist `elevation_m` per tile.
- Coastal smoothing: After tectonic restamping, blend shoreline land tiles toward a local 3×3 blur based on `land_distance`. This widens lowlands and mutes sheer coastal cliffs without flattening interior plateaus.
- Ocean/coasts (distance-transform bands): Compute landmask via the macro pass, flood-fill from map edges to separate true ocean from interior basins, then run a distance transform from land to classify coherent bands:
  - Shelf rule: `ContinentalShelf = ocean tiles with distance_to_land ≤ shelf_width` (no mid‑ocean shelves).
  - Slope rule: tiles with `distance ∈ (shelf_width .. shelf_width + slope_width]` are slope; beyond is deep ocean.
  - Inland water: interior basins become `InlandSea` (never `ContinentalShelf`). If a basin lies within `merge_strait_width` tiles of the ocean, we open a narrow strait and merge; otherwise it remains inland.
  - Elevation consistency: re-stamp elevations to monotonically fall Land → Shelf → Slope → Abyss; inland seas are shallow lacustrine. Optional mid‑ocean ridges/plateaus can be added by preset knobs.
- Climate: Assign `climate_band` using latitude proxy + elevation + moisture; store per-tile band for biome decisions.
- Hydrology: Compute flow direction (D8), fill sinks (limited), compute flow accumulation; choose sources above threshold; trace polylines to nearest sea/lake; classify river order/width; mark `Floodplain`/`FreshwaterMarsh` adjacencies and `RiverDelta` at coast.
- Elevation Field: Shared `ElevationField` (multi-octave value noise shaped by preset `continent_scale`/`mountain_scale`) seeds both terrain stamping and hydrology so rivers follow coherent basins instead of hash-noise pits.
- Biomes: Use climate + elevation + moisture to stamp `TerrainType` (via `terrain_for_position`) with micro-variant jitters along rivers/coasts to respect adjacency rules. Structural mountain tiles short-circuit this pass: fold/fault/volcanic/dome cells pick their dedicated highland terrains (Alpine vs Plateau vs Badlands vs Volcano) before the climate classifier runs so later passes see consistent tags.
- Moisture transport: per-tile humidity blends latitude, coastal distance decay, interior aridity, and a wind-driven rain-shadow pass. Prevailing winds derive from latitude bands with preset-controlled jitter (`biomes.prevailing_wind_flip_chance`), ridge tiles add windward lift via `biomes.windward_moisture_bonus`, and downwind corridors dry out according to `biomes.rain_shadow_strength` / `biomes.rain_shadow_decay`. These knobs pair with the player-facing explanation in the world manual (§3b Terrain Palette).
  - **Highland Propagation**: the tectonic mask now feeds directly into the moisture solver and biome tagging. Each `MountainType` injects windward lift proportional to relief, seeds a decaying rain shadow along the prevailing wind direction, and selects a highland terrain override (`Fold`→`AlpineMountain`, `Fault`→`KarstHighland`, `Volcanic`→`ActiveVolcanoSlope`, `Dome`→`HighPlateau`/`RollingHills`). The resulting tags are preserved through `bias_terrain_for_preset` and counted by the tag-budget solver even when presets lock `Highland`. This keeps moisture, climate bias, and snapshot overlays in sync; designers can depend on highland tags surviving to presentation.
  - Smoothing: the moisture raster is blurred using `biomes.transition_width` before classification, yielding ecotones (savanna/semi-arid scrub) instead of hard biome seams.
  - Terrain picks now respect humidity/temperature bands directly; preset biases still nudge outcomes post-classification.
- Resources: Surface ore/organics/energy deposits biased by `TerrainDefinition.resource_bias` and world chemistry tables.
- Wildlife (Game): Seed herd spawners and migratory paths, density weighted by biome and water proximity; expose “game density” scalar raster for foraging/hunting yields.
- Starting areas: Place candidate starts respecting the “World Viability Contract” (manual §3a). For the nomadic default, spawn bands within reach of freshwater, forage clusters, and at least one soft metal + fuel path within N tiles.

### Data Shapes
- Rasters: `elevation_m: i16`, `climate_band: u8`, `flow_dir: u8`, `flow_accum: u16`, `game_density: u8` (all fixed-point compatible), dimensions `width x height`.
- Vectors: `rivers: [RiverSegment]` where `RiverSegment { id, order, width, polyline: Vec<Hex>, edges: Vec<(Hex, EdgeDir)> }` and per-tile `hydrology_id: Option<id>` for fast joins (edges track which hex boundaries the water occupies for movement penalties and bridge logic).
- Tiles: Extend per-tile metadata with `hydrology_id`, `substrate_material`, and keep `terrain_type` + `TerrainTags`.
- Snapshots: Add optional overlays `overlays.hydrology` (river polylines with order/width) and `overlays.game_density` (compressed scalar raster). Keep terrain overlay unchanged.

### Integration Points
- Movement/logistics: River tiles and banks adjust movement/attrition; navigable segments enable early boats; floodplain tags affect yields and disasters.
- RouteNetwork: Early traversals will naturally hug river/coast corridors; the derived overlay (see below) will reflect this without bespoke logic.
- Crisis/Events: Hydrology feeds flood events; volcano/impact masks continue to seed their terrain as today.

### Map Presets & Tuning (Data-Driven)
Expose worldgen knobs via JSON presets in `core_sim/src/data/map_presets.json`. A preset controls macro shape (oceans/continents), climate/hydrology intensity, and desired proportions of tag categories (e.g., `Water`, `Fertile`, `Wetland`, `Hazardous`) that biomes belong to.

- Loader: `SimulationConfig` gains `map_preset_id` and loads the referenced preset at startup. Include `preset_id` in `WorldSnapshot` for clients.
- Preset shape (schema sketch):
  - `id`, `name`, `description`
  - `seed_policy` (values: `use_simulation_seed`, `preset_fixed`, or `preset_seed`)
  - `map_seed` (optional world-seed override used by `preset_fixed`/`preset_seed`)
  - `dimensions` (`width`, `height`)
  - `sea_level`, `continent_scale`, `mountain_scale`, `moisture_scale`
  - `river_density`, `lake_chance`
  - `river_accum_threshold_factor`, `river_min_accum`, `river_min_length`, `river_fallback_min_length`
  - `river_accum_percentile`, `river_land_ratio`, `river_min_count`, `river_max_count`
  - `river_source_percentile`, `river_source_sea_buffer`, `river_min_spacing`, `river_uphill_step_limit`, `river_uphill_gain_pct`
  - `climate_band_weights` (e.g., `{ polar: 0.12, temperate: 0.46, tropical: 0.42 }`)
  - `terrain_tag_targets` (normalized shares for `Water`, `Coastal`, `Wetland`, `Fertile`, `Arid`, `Polar`, `Highland`, `Volcanic`, `Hazardous`)
  - `locked_terrain_tags` (handful of tag families the solver must keep within tolerance; others become advisory)
  - `biome_weights` (optional per-`TerrainType` multipliers to bias specific biomes)
  - `postprocess` (adjacency weight, river/wetland jitter strength, floodplain spread)
  - `tolerance` (acceptable error on tag targets, e.g., ±2%)

### Terrain Classifier Knobs
- Presets surface a `terrain_classifier` block that feeds `TerrainClassifierConfig`. The key live knob today is `polar_latitude_cutoff` (default `0.35`), which defines the absolute distance from the equator where we flip climate bands to “polar” and where `bias_terrain_for_preset` forces polar-friendly biome substitutions. Aligning this value with the manual’s polar expectations keeps climate overlays, tag budgets, and player-facing copy consistent; see `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §3a for the design framing. The systems layer also exposes `POLAR_LATITUDE_THRESHOLD` so tests and helpers share the same cutoff.

### Tag Budget Solver (Hitting Target Shares)
After initial biome stamping, presets may optionally lock a subset of tag families (`locked_terrain_tags`). The post-process iterates only those families—water → wetlands → fertile → coastal → highland → polar → arid → volcanic → hazardous—and nudges nearby tiles until the locked tag falls back inside `tolerance`, bailing out if an iteration cap triggers. Unlocked tags are best effort; they inherit whatever coverage the upstream generator produced once the locks have converged.
- Compute current tag coverage from stamped `TerrainTags`, honouring hazardous ratios as land-only.
- Promote/demote tiles with adjacency checks (rivers, coastlines, mountain metadata) depending on which family is locked, then re-read coverage before the next pass.
- Use fixed-point friendly scoring; keep adjustments local to avoid destabilizing hydrology classifications.

### Default Preset: Earthlike (Oceans/Continents)
- Macro: 60–70% water with continental shelves and inland seas; 2–5 large continents + scattered archipelagos.
- Climate: weighted temperate/tropical bands; ice caps; rain-shadow support via `mountain_scale` and moisture noise.
- Hydrology: moderate-to-high river density; deltas and floodplains common on large coasts.
- Tag targets (example): `Water: 0.65`, `Coastal: 0.06`, `Wetland: 0.06`, `Fertile: 0.22`, `Arid: 0.12`, `Polar: 0.08`, `Highland: 0.10`, `Volcanic: 0.01`, `Hazardous: 0.03` (overlap permitted; solver aims for balanced coverage across overlapping tags).

### Ecosystem Food Modules

Pre-agricultural survival depends on diversified food portfolios. To keep early play resilient—and to support 6–12 simultaneous factions on the default 80×52 map—we model food availability as reusable “modules.” Each module maps to concrete worldgen tags, snapshot payloads, and client affordances.

| Module / Biome        | Primary Inputs                                                | Secondary Inputs / Tactics                         | Storage / Seasonality Hooks                        |
|-----------------------|---------------------------------------------------------------|----------------------------------------------------|----------------------------------------------------|
| **Coastal Littoral**  | Shellfish beds, tidal fish runs, kelp/seaweed                 | Seabird eggs, drift scavenging, small seals        | Fish racks, kelp drying lines, shell middens       |
| **Riverine / Delta**  | Freshwater fish, mussels, cattail/arrowroot gardens          | Salmon runs, floodplain tubers, reeds              | Smokehouses, earthen tuber pits                    |
| **Savanna Grassland** | Herd shadowing, wild yams, agave hearts                      | Locust swarms, baobab nuts, scavenged kills        | Jerky racks, nut caches                            |
| **Temperate Forest**  | Oak/chestnut groves, berries, small deer, mushrooms          | Maple sap, wild barley, honey, bird eggs           | Clay-lined nut pits, smoked fish/meat              |
| **Boreal / Arctic**   | River/ice fishing, seals, stored fat                         | Caribou herds, ground-nesting birds                | Permafrost pits, blubber/pemmican stores           |
| **Montane / Highland**| Alpine tubers, marmots, seed grasses                         | Meltwater fishing, highland berries                | Sun-dried meat, stone caches                       |
| **Wetland / Swamp**   | Cattail rhizomes, lilies, amphibians/reptiles                | Migratory birds, insect larvae, crayfish           | Mud/cold storage pits, smoke curing                |
| **Semi-Arid Scrub**   | Drought tubers, rodents, cactus fruits                       | Agave roasting, wild millet, scavenged carcasses   | Roasting pits, dried seed cakes                    |
| **Coastal Upwelling** | Sardine/salmon surges, seaweed, seabirds                     | Driftwood fires, tidal pools                       | Large smokehouses, lattice drying frames           |
| **Mixed Woodland**    | Mixed nuts/seeds, berries, edible greens, small game         | Bee larvae, wild grains, opportunistic scavenging  | Fermented greens, buried clam baskets              |

Implementation beats:

1. **Worldgen tags.** Stamp per-tile annotations (e.g., `ShellfishBed`, `RootGarden`, `NutGrove`, `TermiteMound`, `SalmonRun`) as `FoodModuleTag` components. Each tag carries: tile entity, module id, seasonal weight, and respawn timer. Persist them via snapshot overlays so the client can render markers.
2. **Ledgers + commands.** Add a `ForageSiteLedger` resource tracking capacity per tag and per faction. Commands such as `gather_roots`, `harvest_shellfish`, `dry_fish`, and `follow_herd` consume capacity, adjust `FactionInventory`, and push `CommandEventLog` entries for HUD feedback.
3. **Client affordances.** Reuse the HUD selection panel: when a unit stands on a tagged tile we show contextual buttons (Harvest, Gather, Dry). Herd markers remain globally visible—multiple factions are expected to exploit the same migration loops to encourage encounters rather than partitioning resources per faction.

### Start Profiles ↔ Food Modules

Even though only the Trail Sovereigns profile ships today, we target six archetypes long-term. Each profile declares a **primary** and **secondary** module so worldgen can bias start positions and early tasks.

| Profile (planned)         | Primary Module       | Secondary Module      | Notes |
|---------------------------|----------------------|-----------------------|-------|
| Trail Sovereigns (current)| Savanna Herd Shadow  | Riverine / Tubers     | Keep migratory herds central but guarantee cattail beds within ≤2 tiles for redundancy. |
| Tidal Foragers            | Coastal Littoral     | Coastal Upwelling     | Spawn along kelp/shellfish belts; tutorialize drying racks. |
| River Lanterns            | Riverine / Delta     | Wetland / Swamp       | Focus on fish traps + amphibian harvest before herds unlock. |
| High Mesa Branchers       | Montane / Highland   | Semi-Arid Scrub       | Mix marmot hunts with agave roasting pits. |
| Deep Canopy Guild         | Temperate Forest     | Mixed Woodland        | Nut caches and snares; strong preservation loop. |
| Dawn Ice Kin              | Boreal / Arctic      | Riverine / Delta      | Anchor near seal leads plus summer salmon runs. |

Placement rules:

- Primary module tiles must appear within two steps of each faction’s spawn; secondary modules within six steps.
- When multiple factions share a module (e.g., herds), bias spawns toward the same migratory loops. Conflict is expected—no need to reserve one herd per faction.
- Module metadata lives inside `start_profiles.json` so AI, tutorials, and worldgen can branch without hard-coding.

Next steps:

- Populate `start_profiles.json` with the additional archetypes and module metadata.
- Extend `WorldSnapshot` with a `food_modules` overlay so clients can render markers and surface contextual commands.
- Update tutorial/quest scaffolding to walk players through exploiting both primary and secondary modules before the farming tech tree unlocks.

### Snapshot & Client
- Hydrology state is generated at startup and stored server-side. Exporting a dedicated hydrology overlay (raster/polylines) to clients requires a schema update; tracked in TASKS.md. For now, debug logs report river counts and generation details.

## Campaign Loop & System Activation
This section connects the headless simulation to a Civ-like playable loop: campaign start, capability gating, and win detection. It mirrors player-facing details in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §2a.

### Start Flow (Nomadic Default → Organic Settlement)
- Data: `StartProfile` records (JSON) loaded by `SimulationConfig` at worldgen. Fields: `id`, `manual_ref`, `starting_units`, `starting_knowledge_tags`, `inventory`, `survey_radius`, `fog_mode`, `ai_profile_overrides`, `victory_modes_enabled`.
- Schema details: each profile entry uses `starting_units[{ kind, count, position?, tags[] }]`, `inventory[{ item, quantity }]`, and optional flags such as `fog_mode` (`standard`, `revealed`, `shroud`). The loader now materializes these fields into `SimulationConfig.start_profile_overrides`, so downstream systems can read deterministic overrides without reparsing JSON.
- During worldgen `spawn_initial_world` consumes the overrides to seed the initial population bands—`starting_units` spawn targeted `PopulationCohort`s tagged with their role, `starting_knowledge_tags` now pre-fill `PopulationCohort.knowledge` and the `DiscoveryProgressLedger`, and `inventory` entries populate the new `FactionInventory` resource for each player faction. The fallback path still produces the legacy cluster when a profile omits data.
- Knowledge tags are defined in `core_sim/src/data/start_profile_knowledge_tags.json` (env-overridable). Designers map each tag to a concrete `discovery_id`, default progress, and fidelity, and the loader caches the catalog so spawning cohorts instantly inherit the scripted tech fragments.
- Inventory items now translate to immediate systemic nudges. `provisions` raise morale on every starting cohort (modeling surplus food/supplies) and `trade_goods` bump logistics/trade openness so early caravans hit the ground running. Both are consumed during startup, leaving `FactionInventory` free for future runtime stockpiles.
- The inspector's scenario picker now surfaces the structured overrides it receives via snapshots—unit loadouts, inventory payloads, tagged knowledge, and fog settings—so scenario authors can sanity-check JSON edits without digging through logs.
- Wildlife telemetry: `HerdRegistry` + `advance_herds` seed 2–6 migratory herds per world, march them along deterministic loops (biasing toward the starting province), and export their id/species/position/biomass via the new `herds` snapshot block. The inspector’s Fauna tab and the `FollowHerd` command now operate on these live herds instead of hashed coordinates, so scenario authors can track and command actual wildlife flows.
- Fog wiring: `survey_radius`/`fog_mode` now drive the `StartLocation` resource and the snapshot fog raster. `fog_mode = revealed` clears the entire fog overlay, while `fog_mode = shroud` forces opaque fog everywhere except the guaranteed reveal radius around the start tile; the standard mode keeps blending global/local knowledge coverage. Runtime scouting pulses feed a new `FogRevealLedger`, so ad-hoc reveals (e.g., the `ScoutArea` command) can temporarily zero fog anywhere on the grid without mutating global knowledge.
- Default profile: `late_forager_tribe` (manual §2a, §3a) with 2–3 bands and no permanent buildings.
- Spawn: Worldgen seeds 2–3 bands (`UnitKind::BandScout`, `BandHunter`, `BandCrafter` or `BandGuardian`) and immediately unlocks the early interaction verbs: `ScoutArea`, `FollowHerd`, and `FoundCamp`. `ScoutArea` now injects a timed reveal pulse (3+ hex radius for eight turns, scaled by the profile’s `survey_radius`) via `FogRevealLedger`, letting UI clicks clear fog before we have full unit pathing. `FoundCamp` relocates the `StartLocation` marker and reapplies the profile’s survey radius so the knowledge halo follows the player’s chosen base. `FollowHerd` now targets the live fauna routes exposed by `HerdRegistry`, so nomadic commands chase actual migrations instead of hashed coordinates.
- Camps: `Camp` entities are transient settlement-likes with `PortableBuildings`, `CampStorage`, `DecayOnAbandon`, and `TrailKnowledge` markers. They unlock a light construction queue; decay unless maintained.
- Sedentarization: Add `SedentarizationScore` resource per faction computed each turn from local resource density, surplus stability, storage tech/spoilage modifiers, domestication progress, trade hub potential, travel fatigue, and security. When thresholds are crossed, the server emits `CampaignEvent::SedentarizationPrompt { level }` without forcing action.
- Founding: `Command::FoundSettlement { q, r }` remains available (manual scenario: Founders), but for the nomadic path it is enabled after sedentarization ≥ threshold and a camp is upgraded in place.
- Persistence/Network: Emissions include `CampaignEvent::{Founded, SeasonalCampFounded, SeasonalCampAbandoned, SedentarizationPrompt}`.

### Capability Flags (System Gating)
- Resource: `CapabilityFlags` (bitflags) lives in `core_sim` as a `Resource` and is persisted in `WorldSnapshot`. Example flags: `AlwaysOn`, `Construction`, `IndustryT1`, `IndustryT2`, `Power`, `NavalOps`, `AirOps`, `EspionageT2`, `Megaprojects`.
- Schedule wiring: Each major schedule (e.g., `power_tick`, `air_ops_tick`) is registered but inert until the corresponding capability bit is set. Use Bevy run criteria to skip entire system groups when flags are unset.
- Triggers: Flags flip when Great Discoveries resolve with effect hooks (see §Great Discovery System Plan). Scenario profiles may also preflip flags (e.g., Frontier Colony).
- Telemetry: Include `capability_flags` in snapshots; clients gray/lock UI tabs until visible.

### Victory Engine
- Resource: `VictoryState` with `modes: [VictoryModeState]`, per-mode progress meters, and a terminal `winner: Option<FactionId>`.
- Config: `victory_config.json` now exposes `continue_after_win` plus per-mode thresholds/kinds, so scenarios can decide whether the sim keeps evaluating progress after a winner is crowned.
- Checkpoints: `victory_tick` runs after end-of-turn accounting and mixes real metrics instead of stubs:
  - **Hegemony** blends total population, morale, logistics flow, and grid stability.
  - **Ascension** rewards Great Discovery completions and cultural morale.
  - **Economic** monitors trade openness, surplus margins, and throughput.
  - **Diplomatic** looks at openness/morale and how long the campaign has run without collapse.
  - **Stewardship** mirrors crisis grid-stress gauges plus cohesion.
  - **Survival** tracks epidemiological gauges (`R0`) and sustained survival time.
- Emission: When a mode crosses its terminal threshold, set `winner`, emit `CampaignEvent::Victory { mode, faction, tick }`, and (if `continue_after_win` is `false`) pause further evaluation so UI flows can transition to end-of-game without jitter. If scenarios opt in to free-play, progress meters keep updating even after a victory.
- Snapshot surface: `core_sim/src/snapshot.rs` exports `VictoryState` into both full and delta snapshots (plus mirrors it into the FlatBuffer header) so remote tooling can render live progress meters or log the first winner tick without ingesting every other payload. Deltas only include the `victory` block when progress or the winner changes, keeping idle frames slim.

### Early Diplomacy & Route Network
- Derived overlay (no parallel graph): `RouteNetwork` is computed from existing movement/logistics traversals (bands, convoys, boats). Each time a unit traverses between adjacent hexes, record a segment hit and maintain an exponentially decaying occupancy counter per segment (fixed-point). Visibility thresholds determine which segments appear in the overlay. Pathfinding remains owned by existing logistics; RouteNetwork is read-only telemetry + diplomacy keys.
- Route rights: Add a lightweight diplomacy primitive `RouteRightsTreaty` granting passage/toll norms on specific segment keys (the same `(from_hex, to_hex)` identifiers used by traversal logs) or seasonal circuits. Path cost/conflict checkers consult treaty state to adjust friction and hostility.
- Cultural diffusion: While nomadic, cultural influence spreads along traversed routes; surface time-weighted occupancy by faction to the `Victory Engine` for Cultural Diffusion mode.

### Scenario Profiles & Loader
- Files: `core_sim/src/data/start_profiles.json`, `core_sim/src/data/victory_config.json`.
- Loader: `SimulationConfig` reads the selected `StartProfile` and victory modes, sets initial units/knowledge/inventory, and preflips capability bits if specified.
- AI: Start profiles can override AI doctrines for more interesting asymmetries at game start.

### Client/UI Expectations
- Lock/Unlock: Telemetry includes `capability_flags` and `VictoryState`. The client disables tabs/buttons until flags appear and renders victory progress meters.
- Campaign Log: Clients subscribe to `CampaignEvent` frames for lightweight narration (founding, milestones, victories, crises).

## Nomadic Entities & Commands (Specs)
- Entities:
  - `Camp`: components `CampStorage`, `PortableBuildings`, `DecayOnAbandon { half_life }`, `TrailKnowledge { movement_bonus, attrition_bonus }`.
  - `Band`: domain-agnostic movables with role tags (`Scout`, `Hunter`, `Crafter`, `Guardian`), inventory capacity, and fatigue.
- Commands:
  - `FoundSeasonalCamp { q, r }`: validates terrain scarcity and proximity; spawns `Camp` with initial buildings; registers decay timer; emits `SeasonalCampFounded`.
  - `AbandonCamp { camp_id }`: flags decay immediately; transfers salvageable inventory to bands; emits `SeasonalCampAbandoned`.
  - `SplitClan { band_id, split_spec }` / `MergeClan { a, b }`: reassigns population/cohorts and inventory; updates upkeep and discovery throughput.
  - `NegotiateRouteRights { segment_ids|circuit_id, other_faction, terms }`: proposes a route treaty over derived segment keys or named seasonal circuits; upon accept, grants passage/toll modifiers referenced by traversal-derived identifiers.

### Terrain Type Taxonomy
See `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §3b for the player-facing palette. Implementation uses a `TerrainType` enum (u16) plus per-type metadata and tag bitsets that downstream systems interpret.

- **Data Shape**
  - `TerrainType` IDs: `DeepOcean`, `ContinentalShelf`, `InlandSea`, `CoralShelf`, `HydrothermalVentField`, `TidalFlat`, `RiverDelta`, `MangroveSwamp`, `FreshwaterMarsh`, `Floodplain`, `AlluvialPlain`, `PrairieSteppe`, `MixedWoodland`, `BorealTaiga`, `PeatHeath`, `HotDesertErg`, `RockyReg`, `SemiAridScrub`, `SaltFlat`, `OasisBasin`, `Tundra`, `PeriglacialSteppe`, `Glacier`, `SeasonalSnowfield`, `RollingHills`, `HighPlateau`, `AlpineMountain`, `KarstHighland`, `CanyonBadlands`, `ActiveVolcanoSlope`, `BasalticLavaField`, `AshPlain`, `FumaroleBasin`, `ImpactCraterField`, `KarstCavernMouth`, `SinkholeField`, `AquiferCeiling`.
  - `TerrainTags`: smallbit set for shared traits (`Water`, `Freshwater`, `Coastal`, `Wetland`, `Fertile`, `Arid`, `Polar`, `Highland`, `Volcanic`, `Hazardous`, `Subsurface`, `Hydrothermal`). Tags unlock grouping logic (e.g., wetlands for disease sims, volcanic for eruption checks).
  - Metadata per type: `movement_profile` (per domain penalties), `logistics_penalty`, `attrition_rate`, `resource_bias` (weights into ore/material tables), `detection_modifier`, `infrastructure_cost`, `disaster_hooks` (event dispatcher keys), `albedo`/`heat_capacity` seeds for climate modelling.
  - Climate coupling: worldgen stores `climate_band`, `elevation_m`, `hydrology_id`, and `substrate_material` alongside `TerrainType` so chemistry-driven systems stay synchronized.

- **Generation Pipeline**
  - Heightfield + hydrology drive coarse assignments. We stamp `TerrainType` first, then layer micro-variants (e.g., jitter between `Floodplain` and `FreshwaterMarsh` along meanders) while respecting adjacency constraints (`DeepOcean` → `ContinentalShelf` → `TidalFlat`/`RiverDelta` before land, `KarstCavernMouth` must border `KarstHighland` or `SinkholeField`, etc.).
  - Store adjacency metadata for amphibious logic (`MangroveSwamp` flagged as both `Coastal` and `Wetland`).
  - Hook geothermal/impact masks so `HydrothermalVentField`, `ActiveVolcanoSlope`, and `ImpactCraterField` remain rare but purposeful.

- **Simulation Hooks**
  - Movement & logistics: `movement_profile` feeds route selection, attrition ticks, and throughput caps. Naval pathing treats `InlandSea`, `RiverDelta`, and `HydrothermalVentField` as navigable; land convoys incur heavy penalties on `SaltFlat`, `AshPlain`, `BasalticLavaField` absent upgrades.
  - Resource surfacing: `resource_bias` seeds procedural deposits (e.g., `RockyReg` boosts rare metals, `PeatHeath` favors organic fuel, `FumaroleBasin` injects geothermal isotopes). Keep aligned with material generation tables.
  - Event systems: `disaster_hooks` register with crisis generators—`Floodplain` handles river surges, `ActiveVolcanoSlope` registers eruption flows, `ImpactCraterField` feeds meteor resonance, `SinkholeField` checks collapse cadence.
  - Detection & stealth: `detection_modifier` ties into reconnaissance; dense cover (`MixedWoodland`, `MangroveSwamp`) reduces spotting while open `PrairieSteppe` amplifies.

- **Telemetry & Clients**
  - Snapshots expose `terrain_type` per tile and a dedicated `terrainOverlay` raster (width/height + packed samples of terrain ID & tags) so clients can stream biome layers without recomputing from component state.
  - Tag overlay rendering: the snapshot payload already carried `terrain_tags` (bitmask per tile) plus `terrain_tag_labels`; `clients/godot_thin_client/src/scripts/MapView.gd` now registers a `terrain_tags` overlay channel that blends colours for every active tag on a hex, using the curated `TERRAIN_TAG_COLORS` dictionary and averaging when multiple flags (Water + Coastal + Fertile) coexist. The overlay honours selector persistence between snapshots, reuses the legend emission path so HUD + inspector both show coverage percentages, and falls back to the terrain palette when no samples arrive. `Inspector.gd` appends the channel to the overlay selector using the same labels, keeping the UI contract in lockstep with the manual (see `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §3b).
- Godot inspector consumes the same channel (`overlays.terrain`, `terrain_palette`, `terrain_tag_labels`) to colorize tiles and now drives an interactive drill-down: biome list selection refreshes tag histograms, cached tile samples, and a tile list whose hover/selection reveals per-tile telemetry (coords, terrain, tags, temperature, mass, element id). Culture and Military overlay tabs now render live divergence and readiness heatmaps from the new raster channels, with the panel readouts echoing the map legend (min/avg/max plus normalized severity) so designers can cross-check hotspots without leaving the inspector. `MapView.hex_selected` routes map clicks into `InspectorLayer.focus_tile_from_map` so selecting a hex synchronises biome/tile selection in the Terrain panel. `MapView` now owns navigation: mouse wheel zooms about the cursor, right/middle drag pans, and `W/A/S/D` plus `Q/E` cover keyboard panning/zoom so designers can stay anchored on playback shortcuts. Keep the narrative-and-implementation link tight by updating the manual (§3b) before tweaking palette data here, and log future overlay work in `docs/godot_inspector_plan.md`.
  The **Map** tab consolidates live-map controls: the overlay selector now lives beside a logistics overlay toggle and a note that `Enter` flips terrain shading. A map-size dropdown exposes the curated presets (Tiny 56×36, Small 66×42, Standard 80×52, Large 104×64, Huge 128×80; Standard is the default in `SimulationConfig::default`). Changing the dropdown dispatches a `map_size <width> <height>` command through `Inspector.gd`, which the headless server handles by rebuilding the Bevy `App` with the new grid and broadcasting a fresh snapshot (`core_sim/src/bin/server.rs`). A dedicated **Generate Map** button replays the current dimensions so designers can iterate on config tweaks without touching the dropdown; the server now treats same-dimension requests as an explicit regeneration, reinstalls the config file watchers, resets a `map_seed == 0` request back to “random each time,” and seeds a new snapshot before handing control back. Manual context lives in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` (§3b). Hotkeys: press `I` to hide/show the inspector layer and `L` to collapse or restore the HUD legend while reviewing maps.
- Colour mapping: `MapView.gd::_terrain_color_for_id` mirrors the hex values listed in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §3b. Adjust the manual first, then this lookup, then the HUD legend/inspector summaries (see `TASKS.md`).
- Logs tab now consumes the streamed tracing feed over the log socket, rendering a searchable, filterable scrollback (level thresholds, target channels, free-text) alongside the per-turn duration sparkline so designers can audit activity without touching the terminal.
- Commands tab exposes the full command bridge: turn/rollback/autoplay controls plus axis bias adjustment, support/suppress and channel boosts for selected influencers, spawn utilities, corruption injection, and heat debug. Use it to sanity check backend hooks before retiring the CLI.
  - Runtime controls: the thin client binds `ui_accept` to toggle between logistics/sentiment composites and terrain palette mode, aiding QA comparisons of colour accuracy against the documented swatches.
- Inspector migration: see `docs/godot_inspector_plan.md` for the roadmap and progress checkpoints; cross-link new UX notes into the manual when player-facing explanations change. If the Bevy inspector option graduates from evaluation (manual §13 Option F), capture the delta plan here and spin tasks into `TASKS.md`.
  - Logistics/sentiment raster exports now share the terrain grid so overlays blend consistently across clients.

### Frontend Client Strategy
- **Goal**: Select a graphical client stack capable of rendering the live strategy map (zoom/pan, unit animation, layered overlays) while consuming headless snapshots and dogfooding the scripting API.
- **Spikes**: Prioritize a Godot 4 proof-of-concept client that replays mock `WorldDelta` frames and command queues to benchmark frame pacing, overlay rendering cost, and command latency. If Godot exposes blocking gaps, run a focused evaluation of alternative hosts (Avalonia, Qt/QML, Rust+Slint) for visualization only; avoid maintaining a parallel Unity scripting surface.
- **Metrics**: Target ≤16 ms frame time at desktop spec, responsive input-to-command round-trip, and acceptable draw-call budget for layered heatmaps. Capture qualitative notes on tooling ergonomics, asset workflows, and licensing/operational implications. (See `shadow_scale_strategy_game_concept_technical_plan_v_0.md` “Map-Centric Evaluation Plan”.)
- **Scripting Surface**: Design a capability-scoped scripting layer once (JS/Lua sandbox managed by the host). Integrate the facade with the Godot spike and publish an engine-agnostic contract so any future host adopts the same manifest; no Unity contingency needs to stay warm.
- **Decision Artifact**: Summarize results in an engineering decision memo that recommends the preferred client, outlines alternative host risks, and queues follow-on work (e.g., WebGPU dashboard, licensing review) before committing to full UX build-out.
- **Resources**: Godot spike scaffolding lives under `clients/godot_thin_client`; see `docs/godot_thin_client_spike.md` for usage and evaluation notes.
- **Networking**: `clients/godot_thin_client/src/scripts/SnapshotStream.gd` consumes length-prefixed FlatBuffers snapshots from `SimulationConfig::snapshot_flat_bind` (`res/src/native` provides the Godot extension that decodes the schema generated from `sim_schema/schemas/snapshot.fbs`). Snapshot servers now retain the latest frame and replay it as soon as a client connects, and the headless bootstrap runs a single turn plus `broadcast_latest` so inspectors start on a populated map even before any turns advance.
- **Next Steps (UI plumbing)**:
  - Logistics, sentiment, corruption, fog, culture, and military rasters now stream directly from `core_sim`; `SnapshotHistory` caches the layers and the FlatBuffers schema exposes them for clients alongside terrain overlays.
  - The Godot decoder lifts these rasters into `overlays.channels` with stable keys (`logistics`, `sentiment`, `corruption`, `fog`, `culture`, `military`). `MapView.gd` promotes those channels into a selectable overlay palette (defaulting to logistics blue, sentiment red, corruption amber, fog slate, culture violet, military green) and the inspector injects an option selector so designers can flip layers without touching code. Channel descriptions ship alongside the data and the selector/tooltips surface a concise legend so raw vs. normalized values stay interpretable during reviews.
  - `core_sim::corruption_raster_from_simulation` blends ledger intensities with normalized risk weights per tile: logistics throughput, trade throughput, power demand, and morale-adjusted population size feed the baseline, while active incidents and telemetry spikes inject additional pressure. The resulting `Scalar` values stay in the 0–1 fixed-point range (`Scalar::raw`) and diff cleanly because `SnapshotHistory` still treats the raster as optional.
  - `core_sim::culture_raster_from_layers` projects each local culture layer’s divergence magnitude against its hard schism threshold, applies a small boost for time-above-threshold, and emits the ratio as a 0–1 fixed-point sample. Tiles without a local layer fall back to zero so hotspots stand out for the inspector.
  - `core_sim::military_raster_from_state` fuses morale-weighted cohort size, nearby logistics throughput, and local power margin into a readiness scalar (clamped to ~5). The weighting exposes undersupplied garrisons while keeping values diff-friendly for snapshot deltas.
  - The HUD legend mirrors whichever layer is visible: terrain colouring renders the biome palette, and scalar overlays swap to a low/average/high gradient with live min/avg/max raw values plus the channel description so designers can interpret the heatmap without leaving the map view.
  - `core_sim::fog_raster_from_discoveries` inverts the controlling faction’s knowledge coverage. It averages global discovery progress with local cohort fragments, clamps the composite to `[0, 1]`, and writes “1.0 = fully unknown / 0.0 = fully scouted” samples into the raster. Tiles without a dominant cohort default to opaque fog, making gaps obvious to designers.
  - Inspector overlays now ship normalized, raw, and contrast rasters plus live min/avg/max readouts in the legend; the Culture and Military tabs surface the same stats alongside explanatory copy so designers can validate colour ramps against ledger telemetry without the retired CLI inspector (see `shadow_scale_strategy_game_concept_technical_plan_v_0.md` “Map-Centric Evaluation Plan”).

### Shared Scripting Capability Model
- **Runtime Host**: Embed QuickJS via a GDNative module inside the Godot thin client to execute sandboxed JavaScript. Script packs ship with `manifest.json` (id, version, entrypoint, declared capabilities, optional config schema). Development builds hot-reload files under `addons/shared_scripts/` while packaged builds look in `user://scripts`.
- Manifest contract lives at `docs/scripting_manifest.schema.json` (regenerate with `cargo xtask manifest-schema`). Lint manifests locally via `cargo xtask validate-manifests`, which applies the schema and validates capability/subscription coverage against `sim_runtime::scripting::CapabilitySpec`.
- **Capability Families** (aligned with the manual’s player-facing description):
  - `telemetry.subscribe`: host-managed subscriptions to snapshot feeds (`world.delta`, `overlays.*`, `ledger.discovery`, `log.events`). Tokens encode topic id, optional filters, and sampling rate; the host enforces read-only semantics and per-topic back-pressure (`max_messages_in_flight`).
  - `ui.compose`: declarative widget graph expressed through JS builders that map to Godot controls (`Panel`, `VBox`, `Table`, `Chart2D`, `OverlayLayer`, `MapAnnotation`). Script diffs resolve against stable component ids and render on the main thread.
  - `commands.issue`: vetted command endpoints (turn stepping, axis bias, influencer actions, debug hooks) routed through `sim_runtime::command_bus`. Tokens specify throttle windows (commands per turn) and whether debug-only verbs are available.
  - `storage.session`: scoped key/value cache that persists for a simulation session and travels with save games via a `SimScriptState` blob. No raw disk writes; host exposes `storage.snapshot()` for explicit exports within quota.
  - Optional `alerts.emit`: raise toast/banner notifications; host enforces rate caps and prefixes alerts with script ids for audit.
- **Sandbox Enforcement**:
  - QuickJS contexts are created per script on a worker thread with memory limits (default 16 MB) and an instruction watchdog that yields every 4 ms. Disallowed globals (File, Socket, Thread) are removed; only whitelisted helpers (`host`, `console`, math/time shims) remain.
  - Capability tokens are signed blobs issued during manifest load; runtime APIs validate tokens each call to prevent escalation if scripts exchange references.
  - Violations trigger suspension (`ScriptHost` moves the script to quarantine, unsubscribes telemetry, surfaces error toast) until the player re-enables it.
- **Lifecycle & Tooling**:
  - Manifest parsing occurs at client boot and when players toggle scripts via the forthcoming Script Manager. Errors include actionable hints (missing capability, schema mismatch, syntax failure).
  - Hot reload path uses an esbuild-lite pass (in `tools/script_pipeline`) to bundle modules, then recreates the QuickJS context while preserving session storage when allowed.
  - Logging funnels `console.*` through the Godot Logs tab with per-script channels and stack traces. Structured metrics publish to `log.events` so scripts can introspect their own health.
- **Integration Touchpoints**:
  - `clients/godot_thin_client/src/scripts/scripting/ScriptHost.gd` owns runtime initialisation, capability validation, and bridging to `SnapshotStream`/`CommandBridge`.
  - `sim_runtime::scripting::capability_registry` enumerates capability specs (`telemetry.subscribe`, `commands.issue`, `storage.session`, `alerts.emit`, `ui.compose`) so manifests, hosts, and tooling share a single source of truth.
  - `sim_runtime` exports `CapabilitySpec` definitions so manifests can be linted offline and Rust-side tests ensure topic/command ids stay in sync.
- Save/load flows serialise active scripts (`ScriptManifestRef`) and session payloads via new `SimScriptState` struct so resumes restore contexts before the next snapshot.
  - Godot’s `ScriptHostManager` exposes `capture_state()` / `restore_state()` which wrap `ScriptHostBridge.snapshot_active_scripts` and `apply_script_state`, persisting `SimScriptState` payloads alongside the save game.
- **Verification Plan**:
  - Headless harness (`tools/script_harness`) spins up QuickJS with mock feeds to exercise API contracts and fuzz capability enforcement.
  - Integration tests replay recorded turns and assert scripts cannot exceed the 8 ms per-frame budget; watchdog faults are logged and scripts suspended.
  - Security checklist tracks manifest review, capability coverage, and suspension/resume flows, keeping parity with guarantees in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` “Shared Scripting Capability Model (Player-Facing)”.

### Trade-Fueled Knowledge Diffusion
- **Data model**: `TradeLinkState` serializes throughput, tariff, and a `TradeLinkKnowledge` payload (`openness`, `leak_timer`, `last_discovery`, `decay`). `PopulationCohortState` now exposes optional `knowledge_fragments:[KnownTechFragment]`, letting migrations ship tacit knowledge. These additions participate in snapshot/delta hashing.
- **Runtime helpers**: `sim_runtime` ships `TradeLeakCurve`, `apply_openness_decay`, `scale_migration_fragments`, and `merge_fragment_payload`, mirroring the fixed-point arithmetic used inside `core_sim` so tooling can project leak cadence without embedding Bevy.
- **Simulation stage**: `trade_knowledge_diffusion` runs after logistics, refreshes throughput/tariff (already reduced by corruption), decrements leak timers, emits `TradeDiffusionEvent`s when timers expire, applies linear research progress to `DiscoveryProgressLedger`, and resets timers via the configured leak curve. Telemetry increments `trade.tech_diffusion_applied` and archives the record for inspector use.
- **Migration flow**: `simulate_population` manages optional `PendingMigration` payloads. When morale/openess align, cohorts snapshot scaled fragments (`migration_fragment_scaling`, `migration_fidelity_floor`) headed to a destination faction. On arrival the fragments merge into the destination ledger, the cohort flips ownership, and telemetry increments `trade.migration_knowledge_transfers` with `via_migration=true`.
- **Configuration surface**: `SimulationConfig` exposes diffusion knobs (`trade_leak_min_ticks`, `trade_leak_max_ticks`, `trade_leak_exponent`, `trade_leak_progress`, `trade_openness_decay`) and migration knobs (`migration_fragment_scaling`, `migration_fidelity_floor`). Designers should cross-reference `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §8 while tuning these values.
  - JSON keys in `core_sim/src/data/simulation_config.json` map to systems as follows:
    - `grid_size`, `population_cluster_stride`, `population_cap`, `mass_bounds`: world bootstrap size/density and tile mass limits.
    - `ambient_temperature`, `temperature_lerp`, `power_adjust_rate`, `mass_flux_epsilon`: environmental relaxation rates and power-temperature coupling.
    - `logistics_flow_gain`, `base_link_capacity`, `base_trade_tariff`, `base_trade_openness`, `trade_openness_decay`: logistics/trade throughput defaults.
    - `trade_leak_*`, `migration_fragment_scaling`, `migration_fidelity_floor`: knowledge diffusion curves for trade and migration.
    - `power_*` fields: power generation/efficiency caps, storage behaviour, incident thresholds.
    - `corruption_*` fields: subsystem penalties applied when ledgers accumulate corruption.
    - `snapshot_bind`, `snapshot_flat_bind`, `command_bind`, `log_bind`: default TCP endpoints for server sockets.
    - `snapshot_history_limit`: length of the SnapshotHistory ring-buffer used for rollbacks/broadcasts.
  - The headless server reads from `SIM_CONFIG_PATH` when set (fallback: the repo default file) and watches the active path for changes; saving the JSON triggers an automatic reload of `SimulationConfig` (socket changes still require a restart). Remote tooling can issue `reload_config [path]` (or the `ReloadSimulationConfig` payload) to swap configurations programmatically.
- **Telemetry & logging**: `TradeTelemetry` resets each tick, tracks diffusion/migration counts, stores per-event records, and emits `trade.telemetry` log lines after population resolution. Inspector overlays will subscribe directly to these counters.

### Knowledge Ledger & Leak Mechanics
- **Scope**: Centralise the secrecy modelling promised in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §5a (Knowledge Diffusion & Leakage). The ledger tracks every discovery’s secrecy posture, leak cadence, espionage pressure, and public deployment state so other systems (Great Discoveries, diplomacy, crisis triggers) consume a single source of truth.
- **Data model**:
  - `KnowledgeLedger` resource stores `KnowledgeLedgerEntry` rows keyed by (`DiscoveryId`, `FactionId`). Each entry caches tier, last public deployment tick, owning `KnowledgeField`, active `LeakTimerState`, and a breakdown of modifier contributors (visibility, cultural openness, security posture, espionage pressure, forced-publication flags).
  - `LeakTimerState` carries `half_life_ticks`, `progress_percent`, `decay_velocity`, and a `cascade_ready` bool. Base half-life values map directly to the manual’s table (Proto → Exotic) and live in `KnowledgeLeakTemplate` data shipped via `sim_runtime`.
  - `KnowledgeSecurityProfile` enumerates the manual’s posture bands (Minimal/Standard/Hardened/BlackVault) with maintenance costs, Knowledge Debt penalties, and max leak extension; profiles live on the owning faction and are referenced by the ledger for modifier calculations.
  - `InfiltrationRecord` tracks active spy cells, suspected origin (`FactionId`), and accumulated blueprint fidelity; it doubles as a queue for counter-intel sweeps and leak alerts.
- **Timer resolution**:
  - Schedule a dedicated `knowledge_ledger_tick` stage immediately after `trade_knowledge_diffusion` and before Great Discovery progress updates so diffusion signals and secrecy posture adjust in the same turn. The stage iterates ledger entries, recomputes `half_life_ticks` = `base_half_life` + `visibility_bonus` + `security_bonus` − (`spy_pressure` + `cultural_pressure` + `exposure_penalties`), clamps to ≥2, then increments `progress_percent` using fixed-point math.
  - Espionage events (`EspionageProbeResolved`, `CounterIntelSweep`) append transient modifiers (e.g., `spy_pressure = spy_cells * tier_multiplier`, `counter_intel_relief = sweep_strength`) that decay each tick. Battlefield exposures and treaty leaks feed via `KnowledgeExposureEvent` with explicit deltas matching the manual’s leak acceleration values.
  - When `progress_percent` crosses 100 the stage emits `KnowledgeLeakEvent`, seeds rival `KnowledgeFragment`s (re-using trade diffusion helpers for merge logic), and optionally marks discoveries as `common_knowledge` when multiple factions cross the 60% cascade threshold referenced in the manual.
- **Spycraft & counter-intel hooks**:
  - Espionage missions inject `EspionageProbe` components with target discovery/tier, desired fidelity, and stealth score; successful probes raise `InfiltrationRecord.blueprint_fidelity` and shorten the leak timer. Failed probes increase suspicion, lowering future stealth chances and triggering UI alerts.
  - Counter-intelligence commands manipulate `KnowledgeSecurityProfile` (raising maintenance costs) or launch sweeps that consume `CounterIntelBudget`, roll against active probes, and, on success, erase infiltration records while applying short-term leak relief.
  - Knowledge Debt integrates with existing power/culture systems: high security posture writes penalties into `KnowledgeDebtLedger` consumed by power instability (`Power Systems Plan`) and workforce efficiency models.
- **Configuration Surface**: `core_sim/src/data/knowledge_ledger_config.json` captures timeline capacity, default half-life/time-to-cascade, suspicion decay and retention thresholds, countermeasure bonus scaling, infiltrator weighting, and per-tick progress clamps. `KnowledgeLedgerConfigHandle` shares an `Arc<KnowledgeLedgerConfig>` between the ledger and callers, allowing tooling to reload numbers in step with the player experience outlined in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §5a.
  - **Config fields**:
    - `timeline_capacity`: number of timeline entries retained before trimming the oldest events.
    - `default_half_life_ticks` / `default_time_to_cascade`: baseline secrecy timers assigned when a discovery registers.
    - `max_suspicion`: clamp applied to infiltration suspicion meters.
    - `suspicion_decay`: per-tick reduction applied to suspicion when no new probe lands.
    - `suspicion_retention_threshold`: infiltrations below this suspicion/fidelity threshold are purged during decay.
    - `countermeasure_bonus_scale`: multiplier translating countermeasure potency into additional half-life ticks.
    - `countermeasure_progress_penalty_ratio`: portion of countermeasure bonus converted into reduced leak progress that same tick.
    - `infiltration_cells_weight` / `infiltration_fidelity_weight`: weights that transform spy cells and blueprint fidelity into half-life penalties.
    - `max_progress_per_tick`: upper bound on leak progress applied within a single tick after modifiers.
- **Telemetry & UI feeds**:
  - Extend `WorldSnapshot` with `knowledge_ledger:[KnowledgeLedgerState]` entries carrying `discovery_id`, `owner_faction`, tier, current progress %, time-to-cascade estimate, active countermeasures, and suspected infiltrations. Include a child array `KnowledgeModifierBreakdownState` so the Godot inspector can show the modifier tooltips described in the manual’s Knowledge Ledger UI sketch.
  - Publish `KnowledgeEspionageTimeline` frames (ring buffer of the last N leak-affecting events) to snapshots and the `knowledge.telemetry` log channel, aligning with the UI timeline graph.
  - `SimulationMetrics` gains `knowledge_leak_warnings`, `knowledge_leak_criticals`, `knowledge_countermeasures_active`, and `knowledge_common_knowledge_total` so monitoring and dashboards can raise alerts without replaying snapshots.
  - Godot thin client receives a new `KnowledgeLedgerPanel`: subscribe to `knowledge_ledger` stream, render the overview grid (filters, tooltips) and detail drawer (timeline graph, countermeasure toggles, rival comprehension bars). The panel now exposes a mission queue tester (auto-agent selection, tier overrides, schedule offsets) and inline counter-intel controls (faction selector, policy dropdown, reserve/delta buttons) so operators can adjust posture without leaving the UI; command results flow back into the panel’s status readout and the main log stream.
  - Mission control tooling now surfaces telemetry for each probe/sweep template (base success odds, suspicion deltas, misinformation penalties) and renders a live mission queue fed by `espionage.mission.*` logs, including ETA, auto-agent usage, and MISINFO flags alongside the counter-intel posture/budget summary.
- **Integration & dependencies**:
  - Great Discovery resolution (`Great Discovery System Plan`) calls into `KnowledgeLedger::register_discovery` to initialise entries at the correct tier and leak sensitivity. Forced-publication hooks mark entries as visible immediately.
  - Trade diffusion (`trade_knowledge_diffusion`) and migration updates call `KnowledgeLedger::record_partial_progress` so implicit sharing feeds the same ledger math; ledger cascades in turn emit `TradeDiffusionEvent`s when appropriate.
  - Diplomacy and crisis systems consume `KnowledgeLeakEvent`s to trigger treaty renegotiations or crisis seeds when secrecy collapses. Manual references (e.g., Disclosure Pressure) stay aligned via explicit cross-links in both documents.
  - Espionage flows deliver `EspionageProbeEvent` / `CounterIntelSweepEvent` into the ledger module, which materialises infiltrations, countermeasures, and timeline notes before the per-turn tick recomputes leak progression.

### Espionage Mission Outline
- **Agents & Capabilities**: Each faction maintains an espionage roster with stealth, recon, sabotage, and counter-intel proficiencies. Traits, tech, and policies modulate mission odds and detection.
- Author agent archetypes and mission templates in data (e.g., `core_sim/src/data/espionage_agents.json`, `.../espionage_missions.json`) so designers can iterate without code changes; load via the same pattern as `great_discovery_definitions.json`.
- **Mission Lifecycle**:
  - *Planning*: Strategic phase assigns agents to mission templates (lab infiltration, trade interception, battlefield salvage) targeting `KnowledgeLedgerEntry`s. Prep consumes budget, time, and optionally grants modifiers.
  - *Execution*: During turn resolution a mission rolls stealth vs. target defences (security posture, active countermeasures, suspicion). Outcomes include success, partial success, failure, or catastrophic failure.
  - *Resolution Hooks*: Success/partial success emit `EspionageProbeEvent`s with fidelity/suspicion deltas; detected failures raise suspicion, trigger `CounterIntelSweepEvent`s, and can retire agents.
- **Counter-Intelligence**:
  - Defensive missions mirror offensive flow, focused on high-risk discoveries (progress >= 70% or open infiltrations).
  - Security posture budget keeps baseline countermeasures active; successful sweeps emit `CounterIntelSweepEvent`s draining infiltrations and refreshing ledger countermeasure timers.
  - Incident fallout adjusts diplomacy and security budgets.
- **Counter-Intel Automation Hooks**:
  - Introduce a per-faction `CounterIntelBudget` resource consumed when the auto-scheduler queues defensive sweeps. Budget values live in `core_sim/src/data/espionage_config.json` (new `counter_intel_budget` block) with runtime reload support so operators can tune reserves without recompilation.
  - Policy knobs (future `FactionSecurityPolicy` component) influence scheduling heuristics mirrored from the manual’s *Counter-Intel Budgets & Policy Hooks*. Hardened doctrines bias toward protecting tier ≥2 discoveries and lower suspicion thresholds, while lenient policies gate automation unless infiltrations exceed configured risk bands.
  - `schedule_counter_intel_missions` reads both resources before enqueueing: insufficient budget logs a `knowledge.telemetry` warning and skips the mission; policy overrides can either force scheduling despite low funds (crisis stance) or require manual confirmation via command surface (`queue_espionage_mission`).
  - Telemetry extension: emit `knowledge_counterintel_budget_spent` metrics and annotate timeline events with the active policy to keep player tooling aligned.
- **Progression & Feedback**:
  - Agents gain experience or accumulate suspicion (increasing failure odds, eventual exposure).
  - Mission logs feed the timeline/telemetry channel consumed by the Godot Knowledge panel.
  - UI surfaces mission queue, success odds, agent availability, and ledger linkage (e.g., infiltrations per discovery).
- **Configuration & Balancing**: Expose tuning knobs for mission difficulty, suspicion decay, countermeasure potency, mission prep costs, and agent progression.
  - `core_sim/src/data/espionage_config.json` reference:
    - `security_posture_penalties.minimal|standard|hardened|black_vault`: additive modifiers that accelerate leak progress as posture relaxes.
    - `probe_resolution.*`: mission outcome tuning—`recon_fidelity_bonus` rewards recon-ready targets, `suspicion_floor`/`failure_extra_suspicion` set minimum suspicion deltas, `partial_*` scale partial successes, and `failure_misinformation_fidelity` models bad intel backlash.
    - `counter_intel_resolution.*`: defaults for sweeps including posture penalty factor, base potency/upkeep/duration, and suspicion relief applied on success.
    - `agent_generator_defaults.*`: stat ranges consumed when generator templates seed procedural agents.
    - `mission_generator_defaults.*`: bounds governing auto-generated missions (resolution ticks, success odds, fidelity & suspicion deltas, cell gains, relief & suppression outputs).
    - `queue_defaults.scheduled_tick_offset` / `queue_defaults.target_tier`: default scheduling offset and tier bias applied when remote tooling omits explicit values.
- **Implementation Notes**:
  - Stage 1: define agent resources/components and mission queue from data-driven definitions; integrate scheduling commands.
  - Stage 2: mission resolution system producing ledger events; defensive sweeps; hooks into `knowledge_ledger_tick`.
  - Stage 3: UI/telemetry, balancing passes, designer controls.
- **Implementation Status (v0.1)**:
  - Agent and mission catalogs now live in `core_sim/src/data/espionage_agents.json` and `core_sim/src/data/espionage_missions.json`, parsed at startup by `core_sim/src/espionage.rs`. The roster seeds per faction automatically, mirroring the “Prototype Hooks” callouts in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §5a.
  - Generator entries (see `adaptive_sleeper_template`) define procedural agent bands: `generator.enabled` toggles output, `per_faction` controls counts, and each stat band (`stealth` / `recon` / `counter_intel`) clamps RNG in deterministic seeds so designers can rebalance without touching code.
  - Mission templates may now carry `generator` blocks (`auto_probe_template`) that emit calibrated probe variants. Bands for success odds, fidelity gain, suspicion, and cell counts are sampled via deterministic seeds, letting balance designers fan out mission difficulties without new code.
  - Tunable magic numbers (security posture penalties, probe suspicion floors, counter-intel sweep defaults, generator fallback bands) are centralized in `core_sim/src/data/espionage_config.json`; the config is parsed on startup and drives both runtime resolution and unit expectations.
  - A new command envelope (`update_espionage_generators`) lets operators toggle generator `enabled` status or adjust `per_faction` counts at runtime; the `EspionageRoster` reseeds generated agents immediately so telemetry/UI reflect the changes without a restart.
  - Probe resolution now differentiates full/partial successes and misinformation failures; config scalars drive fidelity/suspicion deltas, while successful counter-intel sweeps purge infiltration records and apply suspicion relief.
  - Command surface now exposes `queue_espionage_mission`, wiring directly into `EspionageMissionState::queue_mission` so remote clients can schedule missions without waiting for scripted turn logic, and `update_espionage_queue_defaults` to tweak default scheduling offsets/tiers on the fly.
  - `EspionageMissionState::queue_mission` provides the initial scheduling surface (stubbed command integration) while `resolve_espionage_missions` resolves turns deterministically before `process_espionage_events`. Successful probes emit `EspionageProbeEvent`s; defensive sweeps apply `CounterIntelSweepEvent`s with data-driven countermeasure payloads.
  - Unit coverage (`core_sim/src/espionage.rs` tests) asserts probe/sweep emission paths and countermeasure application, keeping leak metrics deterministic for telemetry exporters.
  - Counter-intel automation now consumes `CounterIntelBudgets` and honors `FactionSecurityPolicies` before queuing sweeps. Budgets regenerate each Knowledge stage, spend is tracked via `SimulationMetrics::knowledge_counterintel_budget_spent`, and the scheduler logs warnings when reserves fall below the configured buffer. Command surface exposes `counterintel_policy <faction> <policy>` and `counterintel_budget <faction> [reserve|delta] <value>` verbs so operators can steer automation mid-run.
  - Follow-up work: surface mission queue commands via the Godot client, extend outcomes (partial successes, misinformation hooks), and expose runtime controls for updating faction security policies.
- **Schema & runtime surface**:
  - `sim_schema/schemas/snapshot.fbs` gains `table KnowledgeLedgerState { discoveryId: uint; ownerFaction: uint; tier: ubyte; progressPercent: ushort; halfLifeTicks: ushort; timeToCascade: ushort; securityPosture: ubyte; activeCountermeasures: [KnowledgeCountermeasureState]; suspectedInfiltrations: [KnowledgeInfiltrationState]; modifiers: [KnowledgeModifierBreakdownState]; flags: uint; }` plus supporting enums (`KnowledgeLeakFlag`, `KnowledgeCountermeasureKind`, `KnowledgeModifierSource`) and child tables for countermeasures, infiltrations, and modifier contributions. A complementary `KnowledgeEspionageTimelineState` table captures timeline events (`tick`, `eventKind`, `deltaPercent`, `sourceFaction`, `noteHandle`).
  - `sim_runtime` exposes strongly typed views (`KnowledgeLedgerSnapshot`, `KnowledgeModifierBreakdownView`) that map the FlatBuffer payloads to ergonomic Rust structs, alongside helper conversions for fixed-point leak math and modifier aggregation. Add serialization helpers in `core_sim::snapshot` that translate ECS resources into the FlatBuffer builders.
  - Extend `SimulationMetrics` with integer counters (`knowledge_leak_warnings`, `knowledge_leak_criticals`, `knowledge_countermeasures_active`, `knowledge_common_knowledge_total`) and register a `knowledge.telemetry` log channel in `sim_runtime` mirroring the ring buffer timeline emitted in snapshots.
  - Update Godot’s generated bindings (`clients/godot_thin_client/autogen/snapshot_bindings.gd`) after regenerating FlatBuffers so the new tables surface in GDScript. Ensure command bindings include verbs for adjusting security posture and launching counter-intel sweeps with validation of capability tokens.

### Corruption Simulation Backbone
- **Subsystem multipliers**: `CorruptionLedgers::total_intensity` aggregates raw intensity by subsystem. `corruption_multiplier` converts that intensity into a clamped scalar applied by logistics (flow gain/capacity), trade (tariff yield), and military power (net generation), making corruption drag explicit.
- **Config knobs**: `SimulationConfig` adds `corruption_logistics_penalty`, `corruption_trade_penalty`, and `corruption_military_penalty` so balance passes can tune how hard incidents bite. Complementary trust fallout and subsystem clamps now live in `core_sim/src/data/culture_corruption_config.json`; `CultureCorruptionConfig::corruption` feeds `sentiment_delta_min/max`, `max_penalty_ratio`, and `min_output_multiplier` into `process_corruption` and `corruption_multiplier`, keeping sentiment losses and throughput floors designer-controlled.
- **Telemetry coupling**: Exposures still feed sentiment/diplomacy via `CorruptionTelemetry` and `DiplomacyLeverage`; the updated config bounds ensure the recorded `trust_delta` values match the JSON, and inspector tooling should surface those clamped numbers for validation. Manual coverage lives in §7c “Designer Tuning & Telemetry”.

#### Inspector Overlay Prototype Plan
- Gate rendering behind the `trade.tech_diffusion_applied` metric; reuse the Godot inspector snapshot stream to surface openness values per trade link (legacy CLI subscription stays available for verification).
- Start with a map-overlay panel that colorizes trade edges by openness and displays countdowns for active leak timers; use the sentiment heatmap widget as a code reference for gradient rendering.
- Add a secondary list widget showing migration-driven knowledge transfers (source faction, destination faction, tech fragment %, remaining turns) to give designers quick validation feedback.
- Instrument a dedicated Godot input action (e.g., `inspector_toggle_trade_overlay`) to show/hide the overlay without disrupting existing layouts, and keep the legacy CLI key binding for verification runs.

#### Inspector Typography Refactor Plan
- **Shared theme bootstrap**: stand up a `Theme` resource at startup (likely in `Main.gd`) that reads `INSPECTOR_FONT_SIZE`, clamps to the existing min/max, and writes the resolved size into a central typography map (`body`, `heading`, `caption`, `legend`, `control`). Apply the theme to the root `CanvasLayer` so child controls inherit defaults instead of each script hand-wiring overrides.
- **Derived scale registry**: encode offset deltas (e.g., heading = base + 4, caption = base − 2) alongside the theme and expose a light-weight helper (`Typography.gd`) that scripts can import. Replace magic numbers in `Inspector.gd` and `Hud.gd` with lookups so dynamic nodes (terrain legend rows, command dropdown labels, BBCode content) all resolve to the same base + delta.
- **Runtime node adoption**: remove `_apply_font_override` loops in `Inspector.gd` in favor of attaching the shared theme or calling a single helper that tags each control with the correct style name. Ensure runtime-created controls (terrain legend rows, HUD labels, dropdown menu items) grab the themed font via `theme_type_variation` or `add_theme_font_size_override` using the shared constants.
- **Layout healing**: swap the inspector’s absolute top offset (`offset_top = 96`) for a computed margin: read the HUD layer’s combined minimum size (post-theme) or add an API on `HudLayer` that returns the stacked label height + margin, then update `InspectorLayer._update_panel_layout()` and `Main.tscn` defaults to respect that value. While touching layout, replace hard-coded legend row heights (`LEGEND_ROW_HEIGHT`) with `Font.get_height()` derived sizing and audit panels that still assume fixed pixel counts, nudging them toward Containers + size flags.
- **Rich text verification**: confirm `RichTextLabel` widgets honor the base font when fed via theme keys (`default_font_size` vs. `normal_font_size`) on the Godot 4 build we ship. If `RichTextLabel` ignores the theme default, extend the helper to set both keys so BBCode sections stay legible at larger scales. Capture any user-facing typography guidance in the game manual when we start messaging accessibility options.

#### Inspector Script Distribution & Trust Model
- **Package format**: inspector scripts ship as `.sscmod` bundles (zip archives) that contain a manifest (`manifest.json`), script payloads, optional assets, and a detached signature (`bundle.sig`). Manifests already follow `docs/scripting_manifest.schema.json`; we now add metadata for certificate fingerprint, target client build, and declared capabilities. Bundles must be signed with Ed25519 keys issued by the studio or trusted creators before the client will load them.
- **Local install flow**: players drop a bundle into `mods/inspector/` or import via the UI. The host verifies the signature (certificate chain + revocation list), lints the manifest, and copies the unpacked payload into a versioned cache. Failed validation leaves the bundle quarantined with surfaced diagnostics so designers can fix manifests without guesswork.
- **Workshop feeds**: curated “Workshop” endpoints expose a JSON index of bundle hashes + download URLs. The client polls the feed, fetches new bundles, validates signatures, and keeps only the most recent version of each mod id. Because bundles are still signed, the feed can be mirrored or operated offline without weakening trust.
- **Load/unload**: enabling a script registers its capabilities, mounts the cached payload, and wires the sandbox to the snapshot/command buses. Disabling a script tears down UI nodes, rescinds capability tokens, and unloads the bundle from memory without deleting the cached files, so re‑enable is instant. Emergency unload (triggered by watchdogs) follows the same path but also flags the bundle until the next manual review.
- See the player-facing summary in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §13 “Technology Stack Exploration” for the narrative framing and expectations.

### Culture Simulation Spine
- **See Also**: `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §7c for player-facing framing of culture layers and trait axes.
- **Data Model**: Introduce `CultureLayer` resources scoped at faction (`Global`), region (`RegionalCultureId` keyed to provinces/territories), and settlement (`LocalCultureId`). Each layer stores a normalized trait vector (`CultureTraitVector { axis: [f32; N] }`, N=15 per manual) plus metadata: inheritance weights, divergence tolerance, last recalculated tick.
- **Trait Propagation**: On turn start, `reconcile_culture_layers` copies global baselines downward, then blends with regional/local deltas using configurable elasticity coefficients. Local events (policies, influencer actions, incidents) write deltas into the relevant layer; the reconcile system decays temporary modifiers and accumulates persistent shifts.
- **Divergence Tracking**: Maintain `CultureDivergence` components (per region/local) storing current deviation magnitude, warning thresholds, and time-above-threshold. When deviation crosses soft limits emit `CultureTensionEvent`; hard limits queue `CultureSchismEvent` for faction split/suppression logic. These events feed Sentiment (Agency/Trust axes) and Diplomacy reaction hooks.
- **Trait Effects Bridge**: Convert trait vectors into system-ready coefficients each turn: e.g., `Aggressive` drives `MilitaryStanceBias`, `Open` modifies knowledge leak timers, `Devout` seeds ritual demand for logistics. Implement via `CultureEffectsCache` resource consumed by population, logistics, diplomacy, and espionage systems.
- **Influencer Coupling**: Extend `InfluencerImpacts` (or companion resource) with culture resonance channels. Each influencer publishes weighted deltas onto specific trait axes; the reconcile pass blends those impulses with policy modifiers before divergence calculations.
  - Implemented: `InfluencerImpacts` now carries `InfluencerCultureResonance` (global/regional/local buckets). `InfluentialRoster::recompute_outputs` aggregates per-axis weights based on scope and coherence, clamping to ±1.0 to stay within Scalar bounds. `CultureManager::reconcile` applies those deltas before divergence checks, averaging within scope to avoid order-dependent bias. Inspector pulls the serialized resonance vectors via FlatBuffers (see below).
- **Sentiment Coupling Tunables**: `core_sim/src/data/culture_corruption_config.json` stores the trust-axis routing plus per-event clamp/scale curves consumed by `process_culture_events`. Editing the `drift_warning`, `assimilation_push`, or `schism_risk` blocks lets designers reshape how much sentiment shifts per alert; the system clamps to those ranges before updating `SentimentAxisBias` and logging entries in `DiplomacyLeverage.culture_signals`. See the manual (§7c “Designer Tuning & Telemetry”) for the player-facing framing.
- **Propagation Tunables**: The same JSON’s `culture.propagation` section now governs per-scope elasticity and divergence grace periods. Adjust `elasticity` to control how quickly layers chase their parents, tweak `soft_threshold`/`hard_threshold` for warning vs. schism magnitudes, and set `soft_trigger_ticks` / `hard_trigger_ticks` to require sustained drift before the engine emits `CultureTensionEvent` or `CultureSchismEvent`. These values hot-load with the rest of the culture configuration so balance passes can lengthen assimilation windows or slow down schism escalation without touching Rust.
- **Religion Integration**: Represent sect dynamics as tagged modifiers on the `Devout`, `Mystical`, and `Syncretic` axes rather than a discrete subsystem. High Devout + Mystical regions spawn `RitualSchedule` entries that schedule pilgrimage logistics and sentiment modifiers; secular regions skip creation.
- **Telemetry & UI**: Extend snapshots with `CultureLayerState` payloads (per layer trait vectors + divergence meters) so the Godot inspector’s Culture tab can surface the “Cultural Inspector” referenced in the manual. Provide layer filters and clash forecasts derived from pending `CultureTensionEvent`s.
  - Inspector update: `Influencers` tab now displays each figure’s strongest culture pushes (weight + current output), while the Culture tab aggregates the top scoped pushes (global/regional/local) for quick validation. Snapshot payload exposes `InfluencerCultureResonanceEntry` entries to keep tooling type-safe.
- **Balance Surface**: `core_sim/src/data/influencer_config.json` defines `InfluencerBalanceConfig` (roster cap, spawn interval min/max, decay factors, notoriety clamps, scope threshold table). The config loads via `InfluencerConfigHandle`, which also powers future runtime reload commands; designers should pair edits with the narrative framing in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §7b.
  - **Config fields**:
    - `roster_cap`: maximum concurrent influencers before spontaneous spawns pause.
    - `support_decay` / `suppression_decay`: per-tick damping multipliers for stored support/suppress charge.
    - `boost_decay`: decay applied to channel boosts earned via commands/events.
    - `spawn_interval_min` / `spawn_interval_max`: cooldown window (ticks) before the roster can spawn another figure.
    - `potential_min_ticks`: minimum time a Potential influencer must remain in state before promotion can occur.
    - `potential_fizzle_ticks`: patience window before a low-coherence Potential lapses to Dormant.
    - `potential_fizzle_coherence`: coherence threshold that keeps Potentials active (also used when reviving Dormant figures).
    - `dormant_remove_threshold`: number of dormant ticks before an inactive influencer is culled.
    - `support_notoriety_gain`: notoriety delta gained per unit of support.
    - `support_channel_gain` / `support_channel_max`: scaling and clamp for targeted support channel boosts.
    - `notoriety_min` / `notoriety_max`: bounds used when clamping notoriety each tick.
    - `scope_thresholds.local|regional|global`: per-scope configuration of promotion/demotion coherence & notoriety thresholds plus dwell times.
- **Task Hooks**: Flag follow-up work in `TASKS.md` (forthcoming entries) for implementing the reconcile system, divergence events, and telemetry serialization so engineering backlog stays aligned with the manual.
- **Schema Plan (`sim_schema`)**: Amend `snapshot.fbs` with the culture payload contracts before engineering begins:
  - `enum CultureLayerScope { Global, Regional, Local }` to disambiguate layer granularity in snapshots/deltas.
  - `enum CultureTraitAxis` enumerating the 15 axes in the manual (`PassiveAggressive`, `OpenClosed`, … , `PluralisticMonocultural`) so trait vectors are stable across runtimes.
  - `table CultureTraitEntry { axis: CultureTraitAxis; baseline: long; modifier: long; value: long; }` capturing inherited baseline, applied adjustments, and resolved value per axis.
  - `table CultureLayerState { id: uint; owner: ulong; parent: uint; scope: CultureLayerScope; traits: [CultureTraitEntry]; divergence: long; softThreshold: long; hardThreshold: long; ticksAboveSoft: ushort; ticksAboveHard: ushort; lastUpdatedTick: ulong; }` describing each layer plus divergence telemetry.
  - `enum CultureTensionKind { DriftWarning, AssimilationPush, SchismRisk }` with `table CultureTensionState { layerId: uint; scope: CultureLayerScope; severity: long; timer: ushort; kind: CultureTensionKind; }` to expose pending clash forecasts to clients.
  - Extend `WorldSnapshot`/`WorldDelta` with `cultureLayers:[CultureLayerState]`, `removedCultureLayers:[uint]`, and `cultureTensions:[CultureTensionState]` payloads so downstream tooling can visualize divergence without additional queries.
- **Culture Telemetry Rollout Checklist** (use whenever the payload or inspector moves):
  1. Update `sim_schema/schemas/snapshot.fbs` with any new fields/enums, then regenerate bindings via `cargo build -p shadow_scale_flatbuffers` (Rust) and `cargo xtask godot-build` (GDNative autogen).
  2. Adjust Rust serialization in `core_sim/src/snapshot.rs` and `core_sim/src/culture.rs`, ensuring `CultureManager::restore_from_snapshot` and snapshot diff logic stay deterministic.
  3. Mirror schema changes inside Godot bindings (`clients/godot_thin_client/native/src/lib.rs`) and the inspector scripts (`Inspector.gd`, overlay selector/legend helpers). Keep overlay copy in sync with `snapshot_overlays_config.json`.
  4. QA pass: capture a before/after snapshot (`cargo xtask command snapshot-save`), diff the FlatBuffers JSON dump, and run the Godot client smoke test (connect, toggle Culture overlay, verify divergence list, logs, and legend values).

## Turn Loop
```text
per-faction orders -> command server -> turn queue -> run_turn -> snapshot -> broadcaster -> clients
```
- Server collects one order bundle per faction before resolving a turn.
- Orders enqueue into [`TurnQueue`] and, once all factions submit, the simulation resolves via `run_turn`.
- Legacy `turn N` command auto-fills missing factions with `EndTurn` orders for rapid testing.
- Each turn emits structured logs (`turn.completed`) with aggregate metrics.
- Frontends may queue multiple `turn` commands (e.g., advance 10 turns) or submit explicit `order <faction> ready`.

### Turn Phases
1. **Collect** – `TurnQueue` awaits submissions from all registered factions (`order <faction> ready`).
2. **Resolve** – When all orders are present the server applies directives, executes `run_turn`, captures metrics, and broadcasts the snapshot delta.
3. **Advance** – Queue resets for the next turn, reopening the Collect phase. Auto-generated orders keep single-faction testing ergonomic while preserving multi-faction semantics.

### Turn Pipeline Configuration
- `TurnPipelineConfig` (`core_sim/src/turn_pipeline_config.rs`) centralizes the previously hard-coded clamps per phase. The JSON backing file (`core_sim/src/data/turn_pipeline_config.json`) ships with defaults, is loaded via `load_turn_pipeline_config_from_env`, and is exposed at runtime through `TurnPipelineConfigHandle` + metadata for live reloads.
- **Logistics**: `logistics.flow_gain_min/max` gates the blended flow gain multiplier, `effective_gain_min` enforces a floor after penalty scaling, `penalty_min` / `penalty_scalar_min` keep terrain penalties sane, `capacity_min` protects against zeroed links, and `attrition_max` caps average attrition used during transfer. These replace the 0.02/0.5/0.005/0.05/0.1/0.95 magic numbers inline in `simulate_logistics`.
- **Trade**: `trade.tariff_min` and `tariff_max_scalar` bound the corruption-adjusted tariff that `trade_knowledge_diffusion` writes onto each link, ensuring designers can allow bonuses above baseline or floor the value without editing Rust.
- **Population**: Attrition/hardness scaling, temperature penalty, morale/culture weighting, growth clamp, migration morale threshold, and migration ETA all now live in the `population` block. `simulate_population` consumes these knobs to compute morale drift, growth, and migration timers.
- **Power**: `power.efficiency_adjust_scale`, `efficiency_floor`, `influence_demand_reduction`, and the storage efficiency/bleed clamps govern the smoothing behaviour in `simulate_power`. Designers regain the ability to make grids sloppier or stricter without recompile.
- Hot reload support mirrors the simulation config path: the headless server watches the resolved file path (if any) and accepts `reload_config turn [path]`. Successful reloads update the Bevy resource, restart the watcher, and log the applied knobs. Manual framing lives in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §7c “Designer Tuning & Telemetry`.
- `SnapshotOverlaysConfig` (`core_sim/src/data/snapshot_overlays_config.json`) moves overlay normalization out of code: corruption channel weights & spike multipliers, culture divergence boosts, military presence/support weights, and fog-of-war blending. Reload with `reload_config overlay [path]`; the inspector exposes buttons for both turn-pipeline and overlay reloads so designers can iterate without leaving the UI.

## Snapshot History & Rollback
- `SnapshotHistory` retains a ring buffer of recent `WorldSnapshot` + `WorldDelta` pairs (default 256 entries; configurable via `SimulationConfig.snapshot_history_limit`).
- `rollback <tick>` rewinds the simulation to a stored snapshot, resets the ECS world, resets `SimulationTick`, and truncates history beyond that point.
- After a rollback the server broadcasts the archived snapshot payload and logs a warning instructing clients to reconnect (current delta stream cannot apply reverse diffs).
- Ring entries expose encoded binary blobs for offline replay tooling and deterministic validation.

## Data Flow
- **Snapshots**: Binary `bincode` frames prefixed with length for streaming.
- **FlatBuffers**: Schema mirrors Rust structs for alternate clients.
- **Logs**: Length-prefixed JSON frames carrying `tracing` events published via the log stream socket (default `tcp://127.0.0.1:41003`).
- **Commands**: Length-prefixed Protobuf `CommandEnvelope` messages covering verbs such as turn stepping, axis bias, influencer directives, spawning, and corruption injection. `sim_runtime::command_bus` exposes builder/decoder helpers, and the Godot tooling issues structured payloads via the native `CommandBridge` instead of raw strings.
- **Metrics**: `SimulationMetrics` resource updated every turn; logged via `tracing` (`turn.completed` now emits `duration_ms` alongside grid metrics for client consumption).

## Power Systems Plan
Power resolution sits fourth in the turn chain (materials → logistics → population → **power** → tick increment → snapshot capture). This section translates the player-facing intent outlined in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §4 into engineering work.

### ECS Structure
- **Identifiers & Resources**
  - `PowerNodeId` (u32) indexes generation sites, substations, storage pools, and demand clusters.
  - `PowerGridState` resource captures the latest per-node supply, demand, transmission loss, storage charge, and stability score.
  - `PowerTopology` holds adjacency lists with per-edge impedance and capacity, seeded during worldgen and mutable via infrastructure projects.
- **Components**
  - `PowerGeneration` (per entity) maps fuel/material inputs to potential output curves with efficiency bands and waste signatures.
  - `PowerDemandProfile` annotates consumer entities (logistics hubs, factories, population blocks, military formations) with baseline draw, surge modifiers, and criticality weights.
  - `PowerStorage` models buffer capacity, charge/discharge efficiency, and bleed rate.
  - `PowerSafety` tracks reactor health, maintenance backlog, and mitigation investments driving instability calculations.
- **Auxiliary Data**
  - `FuelReserve` interfaces with materials/logistics data to ensure generation output honors available feedstock.
  - `GridEventLedger` collects incidents (brownouts, overloads, catastrophic failures) for downstream crisis and knowledge systems.

### Simulation Flow (Power Phase)
1. `collect_generation_orders` resolves facility directives emitted during command processing (fuel assignments, output throttles, maintenance toggles).
2. `resolve_generation` converts materials into produced energy, applying efficiency curves, temperature/terrain modifiers, and downtime for maintenance or damage.
3. `route_energy` propagates supply across `PowerTopology`, accounting for capacity caps, impedance losses, and priority routing; outputs populate per-node surplus/deficit deltas.
4. `apply_storage_buffers` charges or discharges `PowerStorage` entities against node deltas, honoring efficiency/bleed modifiers and spillover to contingency microgrids.
5. `satisfy_demand` decrements consumer queues by delivered energy; unmet demand feeds attrition hooks in logistics, industry, population, and military systems on the following turn.
6. `evaluate_instability` computes stability scores per node, triggers incidents, and records Knowledge Debt adjustments where secrecy constrains workforce familiarity.
7. `export_power_metrics` gathers telemetry (grid stress, surplus margin, instability score, incident feed) into `SimulationMetrics`, snapshot payloads, and overlay rasters.

### Instability Model
- **Stability Bands**: Scores normalised 0–1 combine load ratio, maintenance backlog, reactor health, and redundancy. Thresholds at 0.4 (warn) and 0.2 (critical) drive event probabilities.
- **Incident Types**
  - Brownout/blackout events propagate attrition modifiers to dependent systems and raise unrest.
  - Containment breach incidents inject contamination and heat events into materials/logistics subsystems and flag crisis hooks.
  - Cascading failures traverse `PowerTopology` edges, escalating if redundancy is insufficient.
- **Mitigation Hooks**: `PowerSafety` exposes investments (redundant lines, hardened reactors, microgrids) that increase effective stability; command verbs will toggle these investments and consume resources produced by earlier phases.
- **Knowledge Exposure**: Public deployment of advanced reactors tick leak meters in the Knowledge Diffusion subsystem; incident logs tag technology tiers for reverse-engineering chances.

### Telemetry & Clients
- Extend snapshots with `PowerGridNode` payloads (node id, supply, demand, storage %, stability, active incidents) and `PowerTelemetryState` aggregates (totals, stress/margin, incident feed).
- `SimulationMetrics` gains aggregate values (`grid_stress_avg`, `grid_surplus_margin`, `instability_alerts`) surfaced in the Crisis Dashboard gauges referenced in the manual.
- Godot thin client receives a Power tab extension: grid metrics summary, sortable node list, and incident-aware detail synchronized with the streamed telemetry.
- Headless diagnostics rely on existing script harnesses and tracing exports; add targeted test helpers instead of resurrecting CLI inspectors.

### Integration Points & Dependencies
- **Materials**: Reactor recipes depend on discovered elements/alloys; waste products feed back into materials for recycling/cleanup quests.
- **Logistics**: Fuel routing leverages logistics queues; brownouts inject throughput penalties and additional maintenance cargo demands.
- **Population**: Residential draw scales with prosperity and culture policies; outages influence morale and migration.
- **Military**: High-demand formations reserve power; deficits throttle readiness and advanced weapon availability.
- **Crisis Systems**: Power incidents create triggers for Crisis Framework archetypes (plague/replicator) by disabling containment infrastructure or sparking AI runoff events.

### Follow-on Engineering Tasks
- Finalise schema additions (`PowerGridNode`, power rasters, metrics) in `sim_schema` and `sim_runtime`.
- Implement the ECS systems outlined above inside `core_sim`, ensuring deterministic ordering within the existing schedule.
- Extend Godot inspector to visualise the new telemetry and expose relevant command toggles.
- Author regression tests/benchmarks covering stability band transitions, cascade propagation, and serialization of power telemetry.

## Crisis Telemetry Scope
This section scopes the engineering work needed to deliver the player-facing crisis telemetry experience described in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §10 (*Visualization & Player Experience*).

### Simulation Metrics (`SimulationMetrics` + resource structs)
- Emit explicit gauge primitives each turn: `crisis_r0`, `crisis_grid_stress_pct`, `crisis_unauthorized_queue_pct`, `crisis_swarms_active`, `crisis_phage_density`. Store raw instantaneous values and EMA-smoothed display values (α = 0.35) so both UI and automation can choose presentation; flag warn/critical bands using the manual thresholds (warn 0.9/70%/10%/2/0.35, critical 1.2/85%/25%/5/0.6).
- Add trend deltas (`*_trend_5t`) computed over the last five ticks to support the dashboard’s trend animations and tooltip mini-history.
- Record cadence metadata per metric (`last_updated_tick`, `stale_ticks`) so alerts can detect if feeds stall more than two turns.
- Surface aggregated counts of active crisis modifiers (`crisis_modifiers_active`) and outstanding foreshock/containment incidents to keep Modifier Tray and Event Log summaries aligned with backend state.

### Log & Telemetry Channels
- Introduce a dedicated `crisis.telemetry` tracing target that emits per-turn frames (`{tick, metric, value_raw, value_ema, band, trend}`) for external monitoring and Godot log subscribers, mirroring the manual’s request for loggable KPI series.
- Add `crisis.alerts` log frames whenever a metric crosses warn/critical thresholds or accelerates (trend delta > +10% over five ticks). Include an `ack_required` flag for critical re-entries so client UI can blink/escalate per accessibility rules.
- Extend incident logging (`GridEventLedger`, crisis archetype emitters) to annotate entries with `crisis_overlay_ref` IDs, enabling drill-down from logs into map overlays.

### Snapshot & Overlay Payloads
- Extend `WorldSnapshot` with a `CrisisTelemetryState` table bundling the metric set, trend history (last five ticks per metric), active thresholds, and EMA parameters. This keeps the Crisis Dashboard synchronized without scraping logs.
- Add a `CrisisOverlayState` raster describing infection/replicator/AI control zones, foreshocks, containment lines, and segmentation corridors. Provide both tiled heatmap data (normalized 0–1) and discrete vector annotations for containment lines so the Godot inspector can layer them per manual §10. This is now implemented by the `ActiveCrisisLedger` + `CrisisOverlayCache` resources, updated during the new `TurnStage::Crisis`; the overlay samples blend archetype telemetry weights with active modifier effects before normalization. See the player-facing description in [shadow_scale_strategy_game_concept_technical_plan_v_0.md §10](../shadow_scale_strategy_game_concept_technical_plan_v_0.md#10-visualization--player-experience) for the corresponding UX contract.
- Attach an optional `CrisisNetworkGraphState` (nodes, weighted edges, chokepoint tags) to snapshots to back the Network View overlay, referencing the same transport/comms/power graph already captured for logistics but filtered for crisis context.
- Include per-modifier payloads (`CrisisModifierState` entries with timers, stack counts, decay rules) so the Modifier Tray renders live tooltips.

### Client Responsibilities (Godot thin client)
- Implement the Crisis Dashboard panel: subscribe to `CrisisTelemetryState` and `crisis.telemetry` frames, render gauges with color bands and blink cadence per manual semantics, surface trend sparkles, hover tooltips (definition, source system, last five ticks, linked countermeasures).
- Extend the Event Log/Choice UI to consume `crisis.alerts` frames, pair them with pending countermeasure commands, and respect the `ack_required` flag (pause blink when acknowledged). Provide filters for archetype, severity, and subsystem.
- Augment the Modifier Tray to ingest `CrisisModifierState`, display stack indicators, timers, and provide tooltip breakdowns including decay models and linked policies.
- Add a Crisis Map overlay toggle layering `CrisisOverlayState` heatmaps and vector lines; sync color palette with the manual and pipe chokepoint annotations into the Network View panel (transport/comms/power filters).
- Wire an accessibility toggle (existing settings pane) to disable blinking animations while retaining color state, covering the manual’s accessibility guidance.

### Delivery & Integration Notes
- Crisis telemetry exporters run after crisis-resolution systems each turn so emitted metrics and overlays represent post-resolution state; ensure determinism by placing exporters immediately before snapshot capture.
- Schema updates (`sim_schema/schemas/snapshot.fbs`, Godot GDScript bindings) must version-gate `CrisisTelemetryState`/`CrisisOverlayState` additions; coordinate with tooling to avoid breaking existing viewers.
- Cross-link manual updates: whenever metric definitions, thresholds, or overlay semantics change in the manual (§10), mirror the change here and flag dependent tasks in `TASKS.md`.
- Implementation status: `core_sim` now surfaces `CrisisTelemetryState` with EMA/trend/staleness metadata, emits placeholder `CrisisOverlayState`, and publishes `crisis.telemetry`/`crisis.alerts` tracing targets for per-turn gauges and threshold transitions.

## Crisis System Architecture & Configuration Plan
This section translates the manual’s crisis beats (see `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §§9–10) into implementation scaffolding and configuration artifacts that keep archetypes, telemetry, and overlays data-driven.

### Simulation Structure
- **Archetype registry (`CrisisArchetypeCatalog`)**: data-backed definitions for Plague, Replicator, AI Sovereign, etc. Each archetype lists seeds, propagation rules, mitigation hooks, modifier bundles, and telemetry feeds exposed to the gauges above.
- **State resources**: `ActiveCrisisLedger` (per faction/world state with intensity, spread vectors, mitigation progress), `CrisisModifierLedger` (stacked modifiers with decay), and `CrisisIncidentFeed` (foreshocks, outbreaks, containment wins) driving overlays and alerts.
- **Phase systems**: a dedicated `TurnStage::Crisis` inserted between Population and Finalize to (1) resolve propagation, (2) apply countermeasures, (3) emit incident events, and (4) push fresh samples into `CrisisTelemetry`. Existing Finalize stage then handles corruption sleeves and power fallout.
- `TurnStage::Crisis` now drives the runtime overlay pipeline: the `advance_crisis_system` system consumes `PendingCrisisSeeds`, resolves archetype growth into `ActiveCrisisLedger`, refreshes telemetry gauges, and rebuilds `CrisisOverlayCache` so snapshot capture can copy the real raster instead of the historical power-grid stub.
- Designers can enable automatic crisis seeding for empty worlds by setting `crisis_auto_seed` in `simulation_config.json`; this keeps test boards lively without forcing production sims to start with an outbreak.
- Inspector tooling exposes the same knobs: the crisis tab offers an auto-seed toggle wired through the command surface and a manual spawn action that enqueues catalog archetypes on demand for playtest workflows.
- Operator tooling: `cargo xtask command` wraps the protobuf command surface so designers can issue any verb (`spawn_crisis`, `spawn_influencer`, `inject_corruption`, etc.) without bespoke subcommands. Run `cargo xtask command --list` for verb hints and argument notes; the helper streams envelopes directly to the server's command socket.
- **Event flow**: archetype resolution emits `CrisisIncidentEvent` (map overlays + log frames), `CrisisModifierEvent` (Tray updates), and `CrisisAlertEvent` (warn/critical threshold crossings). Handlers push to telemetry/log channels and schedule UI commands.

### Configuration Artifacts
- `core_sim/src/data/crisis_archetypes.json`: canonical list of crisis archetypes. Fields include `id`, `name`, `manual_ref`, seed prerequisites (discoveries, world chemistry), propagation model parameters (growth curves, spread vectors), mitigation unlocks, telemetry contributions (`r0_source`, `grid_stress_weight`, etc.), overlay palette references, and scripted incident tables.
-   The shipped catalog currently seeds three entries aligned with the manual (§§9–10): `plague_bloom`, `replicator_swarm`, and `ai_sovereign`. Each includes generator ranges for variant strata, telemetry weights tuned to Crisis Dash gauges, and incident tables mirroring the foreshock beats.
- `core_sim/src/data/crisis_modifiers.json`: shared modifier definitions (name, effect handles, stacking rules, decay model, telemetry tags) consumed by both crisis archetypes and other systems.
-   Baseline modifiers mirror the manual’s outcome levers: AI mitigation (`grid_segmented`, `counter_ai_standing`, `deindustrialized_zone`), replicator responses (`shutdown_protocols`, `denial_zone`, `hk_doctrine`), and plague states (`hygiene_regime`, `endemic_burden`, `refugee_pressure`). These feed directly into `ModifierEffects` (`r0_delta`, `grid_stress_pct`, etc.) and surface in telemetry/overlay calculations.
- `core_sim/src/data/crisis_telemetry_config.json`: tunable thresholds (warn/critical bands per gauge), EMA alpha, trend window size, stale tolerance, and escalation cadence (blink rates, alert cooldowns). Defaults align with manual §10 values and are referenced by `CrisisTelemetry`.
- `clients/data/crisis_overlay_config.json`: designer-owned palette + annotation presets for client rendering. Godot (or other consumers) load this directly; the server has no dependency on overlay styling data.
- Loader paths honour `CRISIS_ARCHETYPES_PATH`, `CRISIS_MODIFIERS_PATH`, and `CRISIS_TELEMETRY_CONFIG_PATH`. When unset, the runtime falls back to the bundled JSON in `core_sim/src/data`. Designers can call `reload_config crisis_archetypes|crisis_modifiers|crisis_telemetry [path]` (or use the Inspector commands tab) to hot swap definitions; the headless server watches any provided file path and emits `shadow_scale::config` logs on changes.

### Loading & Hot-Reload
- Mirror the pattern used by knowledge ledger and espionage configs: introduce handle wrappers (`CrisisConfigHandle`, `CrisisTelemetryConfigHandle`) and load from env overrides with builtin fallbacks. Inject into `build_headless_app` so phase systems can access configuration without global state.
- Implement file watchers in the tooling path (`cargo xtask crisis-hotload`) to let designers tweak JSON and refresh the running server in-place; emit `shadow_scale::config` log frames when reloads succeed or fail.

### Client & Command Surface
- Inspector integration: Crisis panels query archetype metadata (name, severity bands, mitigation tips) from streamed catalog payloads derived from `crisis_archetypes.json`.
- Command verbs: extend `CommandEnvelope` with crisis controls (`queue_mitigation_action`, `set_crisis_posture`, `acknowledge_crisis_alert`) referencing IDs from the config. Ensure commands validate against the catalog and log rejections.
- Scenario tooling: trigger catalog seeds via the helper (`cargo xtask command spawn_crisis --archetype=replicator --faction=0`) to align playtest setups with JSON definitions while reusing the shared command machinery.

### Testing & Telemetry Alignment
- Unit suites cover JSON parsing, archetype lookup, and propagation math with deterministic seeds per archetype.
- Integration tests spin a minimal world, load an archetype from JSON, advance the crisis stage, and assert telemetry/log payloads (EMA, trends, incidents) align with expectations.
- Benchmarks: stress-test propagation loops with multiple concurrent archetypes to ensure the dedicated stage stays within the turn budget.

## Great Discovery System Plan
This section translates the player-facing intent in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §5 into engineering scaffolding. The goal is to capture how overlapping discoveries crystallise into a Great Discovery, how those events interact with Knowledge Diffusion (§5a), and how clients observe the leap through snapshots.

### ECS Structure
- **Identifiers & Registries**
  - Reuse `DiscoveryId` (u32) for baseline discoveries and introduce `GreatDiscoveryId` (u16) for constellation-level leaps.
  - `KnowledgeField` enum (`Physics`, `Chemistry`, `Biology`, `Data`, `Communication`, `Exotic`) mirrors the manual’s Knowledge Fields for aggregation and UI labelling.
  - `GreatDiscoveryDefinition` records the canonical name, owning `KnowledgeField`, a list of `ConstellationRequirement` entries, observation prerequisites, and effect hooks (power unlock tags, crisis seeds, culture modifiers) so downstream systems can subscribe without hard-coding IDs.
  - `GreatDiscoveryRegistry` resource owns all definitions plus index structures (by prerequisite discovery, by field) to keep runtime evaluation cheap.
- **Per-Faction State Resources**
  - `GreatDiscoveryReadiness` maps each faction to `ConstellationProgress` (per GreatDiscoveryId) including accumulated progress, gating timers, and leak sensitivity multipliers sourced from Knowledge Diffusion posture.
  - `GreatDiscoveryLedger` tracks triggered events with timestamp, owning faction, downstream effect handles, and whether the discovery has been publicly deployed (feeds diffusion/leak heuristics).
  - `ObservationLedger` records verified observation counts per field; Great Discovery eligibility is blocked until counts cross the threshold specified in the definition (captures manual note on minimum verified observations).
- **Events & Components**
  - `GreatDiscoveryCandidateEvent` emits when constellation progress passes the discovery threshold; `GreatDiscoveryResolvedEvent` confirms once validation succeeds and effect hooks run.
  - `GreatDiscoveryFlag` component can be attached to faction-level proxy entities (if/when those exist) to expose active Great Discoveries to other ECS systems without consulting the ledger resource every frame.

### Trigger Flow
1. `collect_observation_signals` aggregates per-field observation data from exploration, experimentation, and espionage systems into `ObservationLedger` (backed by instrumentation metrics once Knowledge Diffusion hooks land).
2. `update_constellation_progress` consumes `DiscoveryProgressLedger`, applies constellation weights, and writes per-faction `ConstellationProgress`. Partial progress persists between turns; definitions can specify minimum fidelity or freshness windows so stale research decays.
3. `screen_great_discovery_candidates` checks observation gates, faction stability requirements, and cooldowns. Eligible constellations raise `GreatDiscoveryCandidateEvent` with the projected effects bundle.
4. `resolve_great_discovery` validates side conditions (e.g., resource availability, political consent), stamps the ledger, applies cross-system hooks (unlock new power recipes, queue crisis seeds, flip diplomacy modifiers), and schedules leak timer adjustments in Knowledge Diffusion.
5. `propagate_diffusion_impacts` hands the finalized event to Knowledge Diffusion: create high-fidelity `KnowledgeFragment`s for the owning faction, seed rival leak timers based on deployment posture, and inform Trade/Migration diffusion logic of the new top-tier target.
6. `export_metrics` pushes aggregated counts (total leaps, active cascades, pending candidates) into `SimulationMetrics` so monitoring and telemetry can surface GDS activity.

The turn schedule should place `update_constellation_progress` immediately after existing knowledge diffusion (`trade_knowledge_diffusion`) and before population/power so downstream systems react within the same turn. Event resolution can occur in the same schedule block to keep determinism tight.

### Great Discovery Catalog
- **Source of truth**: The shared catalog lives in `core_sim/src/data/great_discovery_definitions.json`, mirroring the player-facing roster in `shadow_scale_strategy_game_concept_technical_plan_v_0.md` §5 *First-Wave Constellations*. Each entry supplies the simulation-critical fields (`id`, `field`, `requirements` with weights/minimum progress, observation gate, cooldown, freshness window, effect flags) plus inspector metadata (summary, tags, leak profile).
- **Loader**: `GreatDiscoveryRegistry::load_catalog_from_str` ingests the JSON during `build_headless_app` startup via the `BUILTIN_GREAT_DISCOVERY_CATALOG` constant. The loader normalises effect-flag strings (`power`, `diplomacy`, `crisis`, `forced_publication`), clamps requirement values, and rejects duplicate IDs before registering. Numeric knobs (`observation_threshold`, `cooldown_ticks`, optional `freshness_window`, and per-requirement `weight` / `minimum_progress`) accept either scalars or `{ "min": ..., "max": ... }` ranges. At load time the registry seeds a `SmallRng` with `hash_identifier(id) ^ seed_offset.unwrap_or(0)` so every field resolves to a single deterministic sample that is mirrored into both the gameplay definition and exported metadata.
- **Inspector alignment**: `WorldSnapshot` now exports `great_discovery_definitions`; the Godot inspector consumes this payload and renders the richer metadata (summary, cadence, requirements) alongside resolved ledger and readiness panels. Tooling stays in sync with the server without duplicating data sources.
- **Distribution**: The JSON file remains the authoring/build-time artifact referenced by the server via `include_str!`, while runtime clients rely solely on the streamed catalog.

### Snapshot Payload Contracts
- Extend `WorldSnapshot` with `great_discoveries: [GreatDiscoveryState]`, one entry per resolved discovery (fields: `id`, `faction`, `field`, `tick`, `publicly_deployed`, `effect_flags`). Clients render the Great Discovery ledger and drive narrative beats from this table.
- Add `great_discovery_progress: [GreatDiscoveryProgressState]` capturing in-progress constellations (per faction id, `GreatDiscoveryId`, current progress 0–1, observation gate remaining, estimated turns). This powers UI cues like “breakthrough imminent” while respecting secrecy—entries flagged as covert only appear for the owning faction.
- Surface a lightweight `GreatDiscoveryTelemetryState` in `SimulationMetrics` mirroring aggregate counts (total resolved, pending candidates, active constellations) to support dashboards and automated testing; snapshot/delta payloads expose the same struct for client consumption.
- Update FlatBuffers (`sim_schema/schemas/snapshot.fbs`) with matching tables/enums; ensure JSON snapshot serialization mirrors the same shape for tooling.

### Integration Points & Dependencies
- **Knowledge Diffusion (§5a)**: Great Discovery resolution modifies leak timers, seeds espionage targets, and may force transparency (e.g., if publicly deployed). The diffusion task in `TASKS.md` depends on these contracts to know which payloads to inspect for UI and AI decisions.
- **Power & Crisis Systems**: Effect hooks map Great Discoveries to unlockable reactors, infrastructure accelerants, or crisis seeds (manual §5 examples). Hook through the registry so new discoveries can register effect lambdas without editing the resolver.
- **AI & Diplomacy**: AI evaluation of secrecy vs diffusion leverages `GreatDiscoveryProgressState` (own faction view) to decide investment and negotiation postures. Diplomacy systems subscribe to `GreatDiscoveryResolvedEvent` to trigger treaty renegotiations or sanction logic.
- **Testing Harness**: Add deterministic unit tests that feed synthetic constellations into the resolver, assert ledger updates, and verify snapshot payloads. Benchmark the trigger loop with high discovery counts to guarantee turn budget stability.

## Extensibility
- Add new systems by extending the `Update` chain in `build_headless_app`.
- Insert additional exporters after `collect_metrics` to integrate Prometheus/OTLP.
- For asynchronous clients, wrap commands in request queues before dispatching to the server.

## Next Steps
- ~~Implement per-faction order submission and turn resolution phases.~~ (Handled via `TurnQueue` + per-faction `order` commands.)
- ~~Persist snapshot history for replays and rollbacks.~~ (Ring-buffered `SnapshotHistory` with `rollback` command.)
- Protobuf `CommandEnvelope` command channel (with host helpers) now handles all control traffic; Godot tooling issues structured requests via the native bridge and the legacy text parser/wire format has been removed. Future protocol work can extend the envelope without reintroducing text compatibility.

### Configuration (Map Presets)
`core_sim/src/data/map_presets.json` adds knobs for physically coherent coasts and biomes:
- `macro_land`: `{ continents, min_area, target_land_pct, jitter }`
- `shelf`: `{ width_tiles, slope_width_tiles }`
- `islands`: `{ continental_density, oceanic_density, fringing_shelf_width, min_distance_from_continent }`
- `inland_sea`: `{ min_area, merge_strait_width }`
- `ocean`: `{ ridge_density, ridge_amplitude }`
- `biomes`: `{ orographic_strength, transition_width, band_profile, coastal_rainfall_decay, interior_aridity_strength }`
- `mountains`: `{ belt_width_tiles, fold_strength, fault_line_count, fault_strength, volcanic_arc_chance, volcanic_chain_length, volcanic_strength, plateau_density }`

### Validation & Debug
- Invariants logged at startup (target `shadow_scale::mapgen`):
  - Every `ContinentalShelf` tile lies within `shelf.width_tiles` of land.
  - No `InlandSea` touches `DeepOcean` (unless merged via a strait).
  - Detached shelf tile count (should be 0 for contiguous coasts).
- Metrics: counts of land, shelf, slope, abyss, inland tiles are emitted for quick inspection.
