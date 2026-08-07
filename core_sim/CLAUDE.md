# core_sim - Simulation Engine

<!-- HUB BANNER — source of truth: scripts/hub_banner_core_sim.md, emitted into
     core_sim/CLAUDE.md right after the H1 by scripts/split_claude_md.sh.
     Edit the source file; an edit made only in the hub is reverted by the next
     re-run. Verify the two agree with: scripts/split_claude_md.sh --check -->

> ## ⛔ THIS IS A HUB FILE — rationale does NOT go here
>
> Before adding a paragraph, section, callout, or config row **anywhere in this file**, ask:
> **is this true of *all* `core_sim` work?**
>
> - **No** — it explains one arc's system, one config's keys, one bug's mechanism, one as-built
>   note → it belongs in the **rule file that owns the arc** (`.claude/rules/core_sim/*.md`, routing
>   table below). That is also what keeps two concurrent worktrees off the same file.
> - **Yes** — a build command, an environment override, a boot-config row, a genuinely
>   subsystem-wide invariant, or a **new row in the routing table** → here.
>
> This file loads into **every session in this repo**; a rule file loads only when you touch the
> code it describes, so a hub paragraph is paid for by every session forever. **If the owning
> rule's `paths:` already cover the code you changed, a hub copy is pure duplication** — the reader
> who could break the invariant loads the rule anyway. Root `CLAUDE.md` → "The hub files are not
> where rationale goes" has the long form.

Bevy-based ECS headless simulation that resolves turns via `run_turn`. Systems execute in `TurnStage` order (`src/lib.rs`): Influence → Logistics → Knowledge → GreatDiscovery → Population → Visibility → Crisis → Telling → Finalize → Victory → Snapshot.

<!-- HUB ROUTING BLURB — source of truth: scripts/hub_blurb_core_sim.md, appended into
     core_sim/CLAUDE.md by scripts/split_claude_md.sh. Edit the source file; an
     edit made only in the hub is reverted by the next re-run.
     Verify the two agree with: scripts/split_claude_md.sh --check -->

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
| `equipment.md` | The minimal TOE — the spear / sled / basket kits, the two-tier rule, the durability cliff, the named-kit roster a party is sent out with | `equipment_config.rs`, `creatures_config.rs`, `data/equipment.json`, `tests/kit_selection.rs` |
| `yield-forecast.md` | Pre-commit per-source yield forecast, assign-time seeding | `labor_config.rs`, `snapshot/`, `orders.rs` |
| `telling.md` | The narrative beat engine, `when` grammar, stance, fork tier, memory threads | `telling/`, `tests/telling*.rs` |
| `expeditions.md` | Wondrous sites, scouting & hunting expeditions | `sites.rs`, `expedition_config.rs` |
| `campaign.md` | Start flow, population & demographics, supply network, sedentarization, wellbeing, victory | `supply.rs`, `demographics_config.rs`, `sedentarization*.rs` |
| `ecs-systems.md` | Power, crisis, culture, knowledge & espionage, great discovery, fog of war, trade diffusion | `power.rs`, `crisis.rs`, `culture.rs`, `visibility*.rs` |
| `turn-profiling.md` | Where a turn's time goes (the sim is ~5%, publishing is ~94%), the `turn.profile` event, which snapshot encodes are load-bearing | `turn_profile.rs`, `snapshot/capture.rs`, `network.rs`, `sim_schema/world.rs` |
| `schedule-parallelism.md` | How systems are ordered (declare edges, don't chain), the ambiguity gate, which stages are serial by data, what the multi-threaded executor costs | `lib.rs`, `Cargo.toml`, `tests/schedule_parallelism.rs` |
| `checkpoints.md` | Why rollback restores `SimState` and not `WorldSnapshot`, the three construction rules (no `Entity`, no config, pure capture), when "derived" is unsafe, how omission is made to fail a test | `sim_state.rs`, `snapshot/capture.rs`, `tests/sim_state_coverage.rs`, `integration_tests/tests/replay_determinism.rs` |
| `ports.md` | Port-block allocation, the handshake file, client discovery precedence (**spans both halves**) | `port_alloc.rs`, `server.rs`, `ServerPortsFile.gd`, `run_stack.sh` |
| `world-handoff.md` | Which world a snapshot frame belongs to: no frame replay on connect, the reveal gate, retry-until-answered, per-world client caches (**spans both halves**) | `network.rs`, `Main.gd`, `GameLaunch.gd` |
| `snapshot-socket.md` | Staying alive when a client stops reading: accept split from broadcast, the write timeout and why a timed-out client must be dropped, the bounded frame queue | `network.rs`, `tests/snapshot_socket.rs` |
| `event-feed.md` | The `CommandEventLog` turn window, the event sequence, the append-only `diff_appended` delta, the demographic flow accumulator | `resources.rs`, `systems/population.rs`, `snapshot/mod.rs`, `decode_fixture.rs` |
| `config-loading.md` | The strict boot-loader rule (absent default = builtin, present-but-broken = panic), the `config_load.rs` seam, why hot reload is the opposite | `config_load.rs`, `*_config.rs`, `resources.rs`, `server.rs` |

**Cross-reference convention.** A quoted phrase like `see "The knowledge pattern"`
names a *section heading*, not a file. Resolve it with
`grep -rn '^#* The knowledge pattern' .claude/rules/core_sim/`. Directional words
("below"/"above") are only reliable *within* one file.

**Adding to these docs.** Per-arc rationale goes in the rule file that owns the
arc; a new arc gets a **row above**, not a section here. See the hub banner at
the top of this file for the test.

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
| `SIM_PORT_BASE` | Shift the server's TCP listen ports to a fresh block so multiple checkouts/worktrees don't collide (`command=base+1`, `snapshot_flat=base+2`, `log=base+3`; **`base+0` is reserved and never bound** — it carried the bincode snapshot socket retired in #388, and the block was left in place so the other three keep their numbers; `base=41000` is the historical block). `scripts/run_stack.sh` derives a per-checkout base automatically and forwards the matching `STREAM_PORT`/`COMMAND_PORT`/`LOG_PORT` to the Godot client; `cargo xtask command …` still defaults to `127.0.0.1:41001`, so pass `--port <base+1>` when targeting a shifted server. **Setting this var also makes the base *explicit*, which disables the auto-bump.** |
| `SIM_PORTS_FILE` | Full path (not a directory) of the ports handshake file, overriding the per-user default. Used by tests and by any launcher that wants the handshake somewhere specific. |

Each `*_CONFIG_PATH` var in the tables above overrides its specific config file; those are noted
per-row. **A var naming a missing or broken file is a boot panic, never a silent fallback to the
builtin** — if you are overriding a config, a typo in the path stops the server rather than quietly
running different numbers.

**Port allocation, the handshake file, and how the client discovers a bumped block** are one
two-sided contract — see `.claude/rules/core_sim/ports.md`, which loads on `port_alloc.rs`,
`server.rs`, `ServerPortsFile.gd` and `run_stack.sh`.

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

---

## Snapshot History & Rollback

`rollback <tick>` rebuilds the world from a `SimState` checkpoint and replays the command log forward to that tick. `SnapshotHistory` holds **one** entry — the client view is derived, not archived. See `.claude/rules/core_sim/checkpoints.md` for what a checkpoint carries and why it is not the snapshot.

**Map export**: the `export_map [path]` command (`write_map_export` in `bin/server.rs`) writes the latest `SnapshotHistory.last_snapshot` plus the resolved `SimulationConfig.map_seed`/`map_preset_id` to disk as a `sim_schema::MapExport` JSON (default `exports/map-tick<t>-seed<s>.json`, gitignored). No new protocol — it rides the existing one-way command channel; the seed makes the dumped map reproducible, and the JSON doubles as an offline-inspectable, test-loadable fixture. **It exports the player's view** — `snapshot.herds` is the fog-filtered display list — so anything offline that wants ground truth needs a checkpoint, not an export.

---

## See Also

- `docs/architecture.md` - System-wide data flow and extensibility
- `sim_schema/README.md` - FlatBuffers schema contracts
- `sim_runtime/README.md` - Shared runtime utilities
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` - Game manual
