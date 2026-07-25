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

## Where the rest of this document lives

This file is the **hub**: build commands, global config layout, the shared
food-module vocabulary, and the turn loop — the things true of *all* `core_sim`
work. The per-arc engineering rationale lives in `.claude/rules/core_sim/`,
scoped with `paths:` frontmatter so a file loads only when you touch the code it
describes.

**The heavy config rows went with them.** A per-config-file description belongs
with the arc that owns that config, so the big ones — `fauna_config.json`,
`flora_config.json`, `labor_config.json`, `intensification_ladder.json`,
`expedition_config.json`, `combat_config.json`, the beat and campaign configs —
now ride in their rule file's `## Config files` table. What stays below is the
global/boot set (`simulation_config.json`, `turn_pipeline_config.json`,
`start_profiles.json`, the ECS-subsystem configs) plus hot reload and the
environment overrides. A new config's row goes in its arc's rule, not here.

| Rule file | Covers | Loads when you touch |
|---|---|---|
| `worldgen.md` | Pipeline stages, elevation authority, rivers & drainage, fluvial erosion, rain shadow, temperature/climate authority, map presets | `mapgen.rs`, `hydrology.rs`, `climate.rs`, `heightfield.rs`, `map_presets.json` |
| `fauna.md` | Wild game, ecology & depensation, herd movement, hunting policy | `fauna*.rs`, `tests/fauna_*.rs` |
| `husbandry.md` | The husbandry yield ladder, the `Tame` verb, Corral | `fauna.rs`, `tests/fauna_husbandry.rs` |
| `intensification.md` | The 3-rung ladder engine, the knowledge pattern, behavior primitives | `intensification.rs`, `intensification_ladder.json` |
| `flora.md` | Depletable forage, the flora roster, per-tile realization, fodder, cash crops | `flora_config.rs`, `forage.rs`, `tests/flora_*.rs` |
| `cultivation.md` | Cultivation, the `Sow` verb, the Field | `forage.rs`, `tests/forage_*.rs` |
| `graze.md` | The pasture layer, the two food webs, ecological carrying capacity, the pen economy | `graze.rs`, `tests/grazing_*.rs` |
| `combat.md` | Combat & casualties, predation | `combat/`, `tests/predators.rs` |
| `yield-forecast.md` | Pre-commit per-source yield forecast, assign-time seeding | `labor_config.rs`, `snapshot/`, `orders.rs` |
| `telling.md` | The narrative beat engine, `when` grammar, stance, fork tier, memory threads | `telling/`, `tests/telling*.rs` |
| `expeditions.md` | Wondrous sites, scouting & hunting expeditions | `sites.rs`, `expedition_config.rs` |
| `campaign.md` | Start flow, population & demographics, supply network, sedentarization, wellbeing, victory | `supply.rs`, `demographics_config.rs`, `sedentarization*.rs` |
| `ecs-systems.md` | Power, crisis, culture, knowledge & espionage, great discovery, fog of war, trade diffusion | `power.rs`, `crisis.rs`, `culture.rs`, `visibility*.rs` |

**Cross-reference convention.** A quoted phrase like `see "The knowledge pattern"`
names a *section heading*, not a file. Resolve it with
`grep -rn '^#* The knowledge pattern' .claude/rules/core_sim/`. Directional words
("below"/"above") are only reliable *within* one file.

**Adding to these docs.** Put per-arc rationale in the rule file that owns the
arc — that is what keeps two concurrent worktrees off the same file. Only add
here if it is true of all `core_sim` work.

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
Hot reload: `reload_config [path]` or `reload_config turn|overlay|crisis_archetypes|crisis_modifiers|visibility [path]`

### Environment Overrides

| Var | Effect |
|-----|--------|
| `SIM_CONFIG_PATH` | Load an alternate `simulation_config.json` instead of the baked-in default. |
| `SIM_PORT_BASE` | Shift all four TCP listen ports to a fresh block so multiple checkouts/worktrees don't collide. The base maps to `snapshot=base+0`, `command=base+1`, `snapshot_flat=base+2`, `log=base+3`; `base=41000` reproduces the historical fixed ports (41000–41003). Applied in `load_simulation_config_from_env` (`resources.rs`) over whatever the config JSON specifies, preserving each bind's host. A non-numeric or out-of-range value (needs `1 ≤ base` and `base+3 ≤ 65535`) is warned and ignored rather than fatal. `scripts/run_stack.sh` derives a per-checkout base automatically and forwards the matching `STREAM_PORT`/`COMMAND_PORT`/`LOG_PORT` to the Godot client; `cargo xtask command …` still defaults to `127.0.0.1:41001`, so pass `--port <base+1>` when targeting a shifted server. **Setting this var also makes the base *explicit*, which disables the auto-bump** (see "Port block allocation" below). |
| `SIM_PORTS_FILE` | Full path (not a directory) of the ports handshake file, overriding the per-user default below. Used by tests and by any launcher that wants the handshake somewhere specific. |

Each `*_CONFIG_PATH` var in the tables above overrides its specific config file; those are noted per-row.

### Port block allocation & the ports handshake file

The server binds **all four ports as one block, up front, all-or-nothing** (`port_alloc.rs`,
`port_alloc::allocate`), and hands the already-bound `TcpListener`s to `start_snapshot_server` /
`start_log_stream_server` / `spawn_command_listener`. Previously each subsystem bound its own socket
and failed differently — the command listener **panicked** while snapshot/log streaming merely warned
and disabled themselves, so a conflict on 41000 or 41002 left a *running* server that silently never
streamed. **There is no longer any path where the server runs with a socket disabled because it was in
use.**

- **Allocation policy.** If `SIM_PORT_BASE` was set, the base is honoured **exactly** — a conflict is
  fatal (exit code `2`, with an actionable message), never bumped, because `scripts/run_stack.sh` and
  the per-worktree port assignment depend on an explicit base being deterministic. Otherwise the
  server starts at the configured base (the config's `snapshot_bind` port, default 41000) and, on
  `AddrInUse`, advances by `PORT_BLOCK_STRIDE` (**10**) for up to `PORT_SLOT_COUNT` (**100**) slots —
  the same two constants `scripts/run_stack.sh` uses. Only `AddrInUse` bumps; any other IO error (e.g.
  permission) surfaces immediately. Exhausting all 100 slots is fatal. A bump is logged at **WARN**
  (`port_block.bumped`) and the `server ready` INFO line reports the *actual* bound ports plus
  `port_base_bumped`.
- **The ports handshake file** lets the client discover a bumped block. Path resolution, env-derived
  so it needs no extra crate: `SIM_PORTS_FILE` verbatim if set; else Windows
  `%LOCALAPPDATA%\ShadowScale\ports.json`, macOS `$HOME/Library/Application
  Support/ShadowScale/ports.json`, Linux `$XDG_STATE_HOME/ShadowScale/ports.json` (falling back to
  `$HOME/.local/state/…`). Deliberately **not** the temp dir, where AV heuristics are most aggressive;
  parent dirs are created as needed. Contents (exact key names — a contract with the Godot client's
  reader):

  ```json
  {"host":"127.0.0.1","snapshot":41000,"command":41001,"snapshot_flat":41002,"log":41003,"pid":1234}
  ```

  Written after the block is bound and before the main loop, overwriting unconditionally. **Failure to
  write is never fatal** — it logs a warning and continues (only auto-discovery is lost). A
  `PortsFileGuard` removes it when `main` returns; a file left behind by a crash or a **signal**
  (SIGINT/SIGTERM skip `Drop`) is expected and tolerated — the client validates the file and falls back
  to the default block, which is what the recorded `pid` is for. No liveness machinery lives here.
- **Config hot-reload** re-applies the **resolved** base (the `ResolvedPortBase` resource in
  `server.rs`), not the configured one, so a reload of an unchanged file after a bump keeps the live
  binds and doesn't spuriously trip `socket_changed=restart_required`. Rebinding live sockets is out of
  scope; the reloaded config describes the ports the server actually holds.

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

> **A lake is freshwater fishing water, not a coastal upwelling.** `classify_food_module_from_traits`
> routes **`InlandSea` → `RiverineDelta`**, alongside `NavigableRiver` and for the same reason: an
> inland sea is landlocked `WATER | FRESHWATER`. **`CoastalUpwelling` is `ContinentalShelf` alone** —
> upwelling is a deep-ocean nutrient column, and the client accordingly draws that module as a
> *shrimp* labelled "Coastal Upwelling". No client change was needed for the re-route:
> `FoodIcons.for_site` splits `riverine_delta` into reeds vs fish by `terrain_id`, and only
> `alluvial_plain`/`floodplain` take reeds, so a lake resolves to the fish glyph automatically.

> **Wild game is an overlay, not a tile flag.** Game used to overwrite a food
> tile's gather kind with `FoodSiteKind::GameTrail` (×0.75 weight), but food-site
> curation sorts by weight **descending** so game trails never survived (0 on live
> maps). That upgrade + the `wild_game_*` config + `GameTrail` are **retired**;
> wild game now lives in the fauna herd layer (below), so a tile offers **both**
> gathering and hunting. See "Fauna & Wild Game" and
> `docs/plan_wildlife_hunting_overlay.md`.

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

## See Also

- `docs/architecture.md` - System-wide data flow and extensibility
- `sim_schema/README.md` - FlatBuffers schema contracts
- `sim_runtime/README.md` - Shared runtime utilities
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` - Game manual
