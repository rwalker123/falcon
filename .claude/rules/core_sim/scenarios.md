---
paths:
  - "core_sim/src/scenario/**"
  - "core_sim/src/command_seam.rs"
  - "core_sim/scenarios/**"
  - "core_sim/tests/scenario_loader.rs"
  - "core_sim/tests/scenario_rungs.rs"
  - "core_sim/tests/scenario_labor_tiles.rs"
  - "core_sim/src/labor_commands.rs"
  - "xtask/src/main.rs"
  - "scripts/run_stack.sh"
---

# Scenarios: the loader, the apply order, the refusal contract

A scenario is a **recipe for a world, not a world**: a seed plus setup entries, replayed onto a
freshly generated world by the current code, so the file never goes stale the way a save does. The
design is `docs/plan_scenarios.md`. This file owns the loader. **A section's key table lives in the
rule of the arc that owns it**, under `## Scenario section`. The bands section's table is in
`campaign.md`, the labor section's is in `work-party.md`, the tiles section's is in `worldgen.md`,
the rungs section's is in `intensification.md`, the herds section's is in `fauna.md`, the pens
section's is in `husbandry.md`, the deposits section's is in `extraction.md`, the roads
section's is in `routes.md`.

## Where things are

| piece | where |
|---|---|
| File shape, pass 1, `ScenarioRefusal`, `build_scenario_world`, `finish_scenario` | `core_sim/src/scenario/mod.rs` |
| The profile and roster steps every world build runs (`apply_start_profile`, `seed_faction_roster`) | `core_sim/src/world_roster.rs` |
| One module per section | `core_sim/src/scenario/bands.rs`, `labor.rs`, `tiles.rs`, `rungs.rs`, `herds.rs`, `pens.rs`, `deposits.rs`, `roads.rs` |
| Replayed verbs that **return** their refusal | `core_sim/src/command_seam.rs` (the labor verbs' bodies are in `labor_commands.rs`) |
| The verb and its reply | `handle_load_scenario` in `bin/server.rs`; `LoadScenarioCommand` = CommandEnvelope **101**, `ScenarioOpReply` = QueryReplyEnvelope **14** |
| Checked-in scenarios | `core_sim/scenarios/<name>.json` (reviewed fixtures, not watched): `breeding_ceiling_150.json`, `kept_roads.json` and the `*_playtest.json` files |
| Sending one | `cargo xtask command load_scenario <path>` · `scripts/run_stack.sh --scenario <path>` |

## Two passes, and nothing is built on a typo

1. **Parse and resolve names** (`parse_scenario`), with no world built. Every struct is
   `deny_unknown_fields`, there are no serde defaults, and a key is optional only where the
   command it replays has an `Option` (a band's `outfit` and `auto_craft`; a labor entry's target,
   kit, species, floor; a tile's `terrain` and `flora`, of which one is required; a road's
   `keeper`, which a trail has none of). Names are checked against the live
   app's config handles (presets, profiles, archetypes, knowledge tags, keeping classes, kits,
   materials, labor roles, terrains, the flora roster). **Every refusal is collected**, so one load lists all of a
   file's faults. A serde error has no field path, so it reports `line L, column C`.
   **A repeat is refused, never last-wins**: a knowledge tag twice in one faction's `knowledge`
   (`factions[i].knowledge[k]`, "listed twice"), a larder class or band id twice, and a repeated
   key in the JSON objects `outfit.kits`, `outfit.materials` and `world.archetypes` (a serde error,
   so `line L, column C`; `bands::unique_keys` is the deserializer, serde_json's own map keeps the
   last key silently).
2. **Build a candidate, fit, apply.** The server's `prepare_world_app` builds the candidate app with
   **no snapshot sink attached and the shared `world_epoch` not advanced**, and the live world is
   never touched. **`scenario::build_scenario_world` is the one body that does the rest** and is what
   `handle_load_scenario` and `tests/scenario_loader.rs` both call: the `world` section's grid,
   preset and seed into the config, `apply_start_profile` for the profile the file names,
   `seed_faction_roster` from `world.archetypes` (requested counts clamped by
   `granted_archetype_counts` to what the grid seats), the `ScenarioSetup` inserted before Startup,
   the build turn, then `finish_scenario`. Only the swap-in (epoch, sink, `publish_loaded_world`,
   command-log re-base) is server-only. Fit
   refusals found during Startup (band on water or off the map, absent faction, an archetype with no
   `starting_units`, a tile override that does not fit its tile) are recorded on `ScenarioSetup`. `finish_scenario` then replays the command
   entries.

**Fail closed and whole.** Any refusal drops the candidate. The previous world stays, and an idle
server stays idle. The reply and the server log both carry the full list. On success the server
retires the old publisher, advances and stamps the epoch, attaches the sink, publishes a fresh full
frame (`publish_loaded_world`), and **re-bases the command log after setup**, so a rollback can
never undo the scenario.

## The apply order uses the game's own turn-1 machinery

There is no scenario special case after Startup. `ScenarioSetup` is inserted **before** Startup:

- `apply_scenario_tile_overrides` swaps the listed tiles' terrain and flora after the last pass that
  paints terrain and before `reconcile_food_modules`, so everything derived from a tile reads the
  override (`worldgen.md` → "Scenario section").
- `spawn_initial_world` spawns a listed faction's bands, in file order, **instead of** its
  archetype's `starting_units`, and **ahead of** the profile branches. The first listed band
  therefore holds the faction's lowest `BandId`, which is the band `stamp_starting_loadout` opens the
  outfitting window on.
- `apply_starting_inventory_effects` seeds a scenario band from its `ages` and its exact larder. The
  three shares are **divided by their sum** first (`Ages::normalised`), so a file whose shares sum to
  `0.99` within `AGES_SUM_TOLERANCE` opens at the same head-count as one that sums to `1.0`.
- A faction's listed `knowledge` tags are seeded at **full progress** (`SCENARIO_KNOWN_PROGRESS`,
  `1.0`) into the same discovery ledger the game reads, replacing the archetype's own grant of the
  same tag, so `knows_messengers` and every other `progress >= completion_threshold` gate answer
  yes on turn one.
- The default outfit is fitted on the world-build pass against the band the scenario built.
- `finish_scenario` applies the **setters** first: the herd and deposit fits are collected
  together and any refusal stops the load before either is applied, then the herds, the deposit
  stocks, the held rungs (each rung of the climb through the build gate), and the held roads
  (`routes.md` → "Scenario section"). A deposit's stock and
  a held working's rung both open the working through `DepositRegistry::open`, so they compose in
  either order. Setter refusals are all collected, and any one stops the load before a command is
  replayed.
- `finish_scenario` then replays `outfit` as `SetStartingLoadout` through `command_seam`, which
  **replaces** the default while the opening window is still open.
- `finish_scenario` then replays each band's `auto_craft: true` through
  `command_seam::apply_set_auto_craft` (after the outfit, before labor). A refusal stops the load at
  `factions[i].bands[j].auto_craft`.
- `finish_scenario` then replays every faction's `labor` entries, in file order, through
  `apply_assign_labor` / `apply_work_priority` / `apply_band_order`. Labor runs only when no outfit was refused, and the
  first refused entry stops the load. A replayed entry writes the same success line to the feed a
  player's command would.
- `finish_scenario` then replays the `build_rung` entries through `command_seam::apply_build_rung`,
  after labor (plan §2.8), so a build finds the band labor put on its source. A refused build stops
  the load at that entry.
- `finish_scenario` then replays every faction's `pens` entries through
  `command_seam::apply_extend_pen` / `apply_set_herd_output`, after the builds (plan §2.8). A herd is
  named by its scenario id in every section, mapped through `ScenarioApplied.herds`. A refused entry
  stops the load at that entry.
- `ScenarioSetup` is removed in `finish_scenario`. It is never part of `SimState`, a save or a
  snapshot.

## The command seam

`command_seam.rs` holds the replayed verbs as functions that return
`Result<(), CommandRefusal>`. `CommandRefusal`'s `Display` is the reason text the event feed shows.
The server's handler converts the wire, calls the same function, and on `Err` still writes the warn
and the feed event, **so the scenario path and the player's path are one body.** A section that
replays a new verb adds a function and a `CommandRefusal` variant there, never a copy of a
handler. `BuildRung` lives there whole (`apply_build_rung`, every source kind, with
`queue_build_on_working_bands` and the pen helpers), and its refusal carries the feed channel the
server writes the failure on. The yield re-quote those verbs need lives in `core_sim/src/yield_seed.rs` for the same
reason.

## Not replayable, and dev-only

`LoadScenario` is in `is_replayable`'s exclusion list beside `new_game` and `load_game`, because
setters are not commands and a log that began before them could not reproduce them. Nothing in
`start_profiles.json`, the New Game screen or the main menu names it.

## The client takes a world it did not ask for

The landing screen polls the faction-free `WorldStatus` query (answered before the `world_active`
gate) and leaves for a world that becomes active under it. `run_stack.sh --scenario` loads the
scenario **before** launching the client and sets `FALCON_ATTACH_WORLD=1`, so the client attaches to
the running world instead of starting one. The client side is
`.claude/rules/client/` → the boot and menu scripts.

## Carry decides what a shipped scenario can hold

A band's fixed larder counts against its carry (`starting-loadout.md`), so a large larder leaves
nothing for the outfit. At 150 people (90 working × 7.0 = 630 carry), a 900 dry larder leaves an
allowance of 0 and any outfit is refused `OverCarry`. That is why `breeding_ceiling_150.json` ships
450.

