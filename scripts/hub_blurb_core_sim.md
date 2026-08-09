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
| `crafting.md` | Materials — the characteristic vector, the band merge rule, the batch store, pooling per rating, the yield edges | `materials_config.rs`, `data/materials.json`, `tests/materials.rs` |
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

