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
| `src/data/simulation_config.json` | Grid size, environmental tuning, trade/power/corruption multipliers, TCP bind addresses |
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

Hot reload: `reload_config [path]` or `reload_config turn|overlay|crisis_archetypes|crisis_modifiers [path]`

---

## World Generation Pipeline

Implements the procedural map pipeline producing terrain, coasts, rivers/lakes, climate bands, resources, and wildlife spawners. Player-facing framing: manual §3a World Bootstrapping, §3b Terrain Palette.

### Pipeline Stages
1. **Macro landmask** - Continent seeds via weighted BFS to reach `target_land_pct`
2. **Tectonics** - Drift vectors, collision belts, fault seams, volcanic arcs, dome plateaus → mountain mask
3. **Polar microplates** - Subdivide polar tiles, converging vectors raise fold strength
4. **Heightfield** - Multi-octave height raster with erosion smoothing → `elevation_m`
5. **Coastal smoothing** - Blend shoreline tiles via 3×3 blur
6. **Ocean/coasts** - Distance-transform bands: Shelf → Slope → Deep Ocean; inland seas
7. **Climate** - Assign `climate_band` using latitude + elevation + moisture
8. **Hydrology** - D8 flow direction, river polylines, `Floodplain`/`FreshwaterMarsh` marking
9. **Biomes** - Stamp `TerrainType` via `terrain_for_position` with micro-variant jitters
10. **Moisture transport** - Humidity blending with wind-driven rain-shadow pass
11. **Resources** - Surface deposits biased by `TerrainDefinition.resource_bias`
12. **Wildlife** - Seed herd spawners, migratory paths, `game_density` raster
13. **Starting areas** - Place candidates respecting World Viability Contract

### Data Shapes
- **Rasters**: `elevation_m: i16`, `climate_band: u8`, `flow_dir: u8`, `flow_accum: u16`, `game_density: u8`
- **Vectors**: `rivers: [RiverSegment]` with polylines and edge tracking
- **Tiles**: `hydrology_id`, `substrate_material`, `terrain_type`, `TerrainTags`

### Map Presets (`map_presets.json`)
Presets control: `seed_policy`, `dimensions`, `sea_level`, `continent_scale`, `mountain_scale`, `moisture_scale`, `river_density`, `terrain_tag_targets`, `locked_terrain_tags`, `biome_weights`.

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

---

## Campaign Loop & System Activation

### Start Flow
- **Data**: `StartProfile` records with `starting_units`, `starting_knowledge_tags`, `inventory`, `survey_radius`, `fog_mode`
- **Spawn**: Worldgen seeds 2-3 bands, unlocks `ScoutArea`, `FollowHerd`, `FoundCamp`
- **Camps**: Transient settlement-likes with `PortableBuildings`, `CampStorage`, `DecayOnAbandon`
- **Sedentarization**: `SedentarizationScore` computed from resource density, surplus stability, trade potential
- **Founding**: `Command::FoundSettlement { q, r }` requires Founders unit, consumes provisions, spawns Settlement

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

---

## Trade-Fueled Knowledge Diffusion

`TradeLinkState` carries throughput, tariff, `TradeLinkKnowledge` (openness, leak_timer, decay). `trade_knowledge_diffusion` runs after logistics, emits `TradeDiffusionEvent`s, applies progress to `DiscoveryProgressLedger`.

**Migration**: `PendingMigration` payloads carry scaled knowledge fragments. On arrival, fragments merge into destination ledger.

**Config**: `trade_leak_min/max_ticks`, `trade_leak_exponent`, `trade_openness_decay`, `migration_fragment_scaling`.

---

## See Also

- `docs/architecture.md` - System-wide data flow and extensibility
- `sim_schema/README.md` - FlatBuffers schema contracts
- `sim_runtime/README.md` - Shared runtime utilities
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` - Game manual
