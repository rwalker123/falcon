---
paths:
  - "core_sim/src/scenario/**"
  - "core_sim/src/command_seam.rs"
  - "core_sim/scenarios/**"
  - "core_sim/tests/scenario_loader.rs"
  - "core_sim/tests/scenario_schema.rs"
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
section's is in `routes.md`, the bench section's is in `crafting.md`.

## Where things are

| piece | where |
|---|---|
| File shape, pass 1, `ScenarioRefusal`, `build_scenario_world`, `finish_scenario` | `core_sim/src/scenario/mod.rs` |
| The profile and roster steps every world build runs (`apply_start_profile`, `seed_faction_roster`) | `core_sim/src/world_roster.rs` |
| One module per section | `core_sim/src/scenario/bands.rs`, `labor.rs`, `tiles.rs`, `rungs.rs`, `herds.rs`, `pens.rs`, `deposits.rs`, `roads.rs`, `bench.rs` |
| Replayed verbs that **return** their refusal | `core_sim/src/command_seam.rs` (the labor verbs' bodies are in `labor_commands.rs`) |
| The verb and its reply | `handle_load_scenario` in `bin/server.rs`; `LoadScenarioCommand` = CommandEnvelope **101**, `ScenarioOpReply` = QueryReplyEnvelope **14** |
| The editor verbs | `handle_list_scenarios` / `handle_read_scenario` / `handle_write_scenario` in `bin/server.rs`; `ListScenariosCommand` = CommandEnvelope **102**, `ReadScenarioCommand` = **103**, `WriteScenarioCommand` = **104**, all answered on `ScenarioOpReply` (`files` = 4, `text` = 5, `schema` = 6, appended; empty when not that verb's answer). Rust: `CommandPayload::{ListScenarios, ReadScenario, WriteScenario}`; text forms `list_scenarios` and `read_scenario <path>` (`write_scenario` carries a whole file and has none) |
| The editor's path policy, the file list, the schema | `resolve_scenario_path`, `list_scenario_files`, `scenario_schema`, `SCENARIO_DIR` in `core_sim/src/scenario/mod.rs` |
| Checked-in scenarios | `core_sim/scenarios/<name>.json` (reviewed fixtures, not watched): `breeding_ceiling_150.json`, `kept_roads.json` and the `*_playtest.json` files |
| Sending one | `cargo xtask command load_scenario <path>` · `scripts/run_stack.sh --scenario <path>` |

## Two passes, and nothing is built on a typo

1. **Parse and resolve names** (`parse_scenario`), with no world built. Every struct is
   `deny_unknown_fields`, there are no serde defaults, and a key is optional only where the
   command it replays has an `Option` (a band's `outfit` and `auto_craft`; a labor entry's target,
   kit, species, floor; a tile's `terrain` and `flora`, of which one is required; a road's
   `keeper`, which a trail has none of). Every other section key is required (`[]` for none). Names are
   checked against the live
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
   never touched. `build_scenario_candidate` in `bin/server.rs` is the one body that takes scenario
   TEXT to a built candidate (pass 1, then `prepare_world_app`, then the build below), and
   `handle_load_scenario` and `handle_write_scenario` both run it: load swaps the candidate in,
   write drops it. **`scenario::build_scenario_world` is the one body that does the rest** and is what
   `build_scenario_candidate` and `tests/scenario_loader.rs` both call: the `world` section's grid,
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
- `finish_scenario` then replays every faction's `bench` entries, in file order, through
  `command_seam::apply_bench_enqueue` / `apply_bench_enqueue_kit`, after the
  pens (plan §2.8: outfit, labor, builds, pens, bench, splits). A refused entry stops the load at
  `factions[i].bench[k]`.
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

**The two bench queue verbs** (`apply_bench_enqueue`, `apply_bench_enqueue_kit`) return an
outcome struct carrying what the server's success feed line names (display name, queue position,
crew, kit order id), so the server writes the same line it always did. Each has a
`BandId` form (the scenario path, resolved through `resolve_band`, refused `NoSuchBand` rather than
panicking) and an `_on` form taking the `Entity` that holds the body (the server path, after its own
`select_starting_band`). A bench refusal is `CommandRefusal::Bench(String)`, the handler's sentence
verbatim. The server now resolves the band **before** it judges the recipe, kit or count, so a
command with both a bad band and a bad recipe reports the band.

## Not replayable, and dev-only

`LoadScenario` is in `is_replayable`'s exclusion list beside `new_game` and `load_game`, because
setters are not commands and a log that began before them could not reproduce them. The three editor
verbs sit beside it: they read or write a file and build a world that is dropped. Nothing in
`start_profiles.json`, the New Game screen or the main menu names it.

## The editor verbs: list, read, write

Three dev verbs for the Workbench's scenario editor, answered on the asking connection exactly as
`load_scenario` is (`answer_scenario_op`, the same `request_id`), and in `is_replayable`'s exclusion
list beside `LoadScenario`. None of them touches a world.

- **Path policy (read and write).** The path is read against the server's working directory
  (`SCENARIO_BASE_DIR`, `.`; `run_stack.sh` runs the server from the repo root), folded lexically
  (`.` and `..` removed without touching the disk), and must be a file **directly inside**
  `SCENARIO_DIR` (`core_sim/scenarios`) ending in `.json`. A `..` that climbs out, a subdirectory, a
  wrong extension or an absolute path outside the directory is refused at `FILE_REFUSAL_PATH` with a
  sentence saying why. `load_scenario`'s own path handling is unchanged: xtask and `run_stack.sh`
  pass absolute paths.
- **`list_scenarios`** answers `files` (the directory's `*.json`, sorted, spelled
  `core_sim/scenarios/<name>.json`, so each is a path the other two accept) and `schema`.
- **`read_scenario`** answers `text`, the file verbatim; a read failure is a refusal at `<file>`.
- **`write_scenario` writes only a file that would load, and the check is a trial build.** The text
  goes through `build_scenario_candidate` (pass 1 against the live app's configs, then
  `prepare_world_app` and `build_scenario_world`, the body `load_scenario` runs), and the candidate is
  then **dropped**. The handler borrows the live app shared and takes neither `world_active`,
  `world_epoch`, the snapshot sink nor the command log, so none can be touched. **Why a full build:**
  a scenario builds its OWN seed, so checking it against the live world would miss every fit refusal
  (a band on this seed's water, a tile override that does not fit); only the trial build means "this
  file would load". It works with no world active, as load does. Zero refusals writes `text` byte for
  byte (a `.partial` temp file in the same directory, then a rename); any refusal writes nothing and
  the reply carries the full `<path in the file>: <reason>` list unchanged. Begin, refusals and
  completion log on `shadow_scale::scenario`.

## The scenario file's schema

`scenario_schema()` is generated with `schemars` from the exact structs `parse_scenario`
deserializes (`ScenarioFile` and every section module), so a key added to any section appears in the
Workbench form with no client edit. It is never hand-written.

- Subschemas are inlined (no `$ref`); every object is `additionalProperties: false`
  (`deny_unknown_fields`); the `unique_keys` maps carry `#[schemars(with = "BTreeMap<String, u32>")]`.
- **`required` is what the loader requires.** serde makes an `Option` field optional, so an `Option`
  key (`type: [T, "null"]`) is **not** in `required`: the loader accepts it omitted or `null`, and
  the shipped scenarios omit them.
- An externally tagged enum (a labor, rung, pen or bench entry) is a `oneOf` of single-key objects;
  `HerdOrigin` is a `oneOf` of `const` strings.
- **Pick annotations**, as schema extensions on the field (`#[schemars(extend(...))]`):
  `"x-pick": "tile"` on every `[x, y]` array (bands, herds, tiles, deposits, roads, a held rung's
  `at`); `"x-pick-tile": ["target_x", "target_y"]` on the objects whose tile is a key pair
  (`assign_labor`, `work_priority`, `band_order`, `build_rung`); `"x-pick": "band"` on `band` (labor,
  bench, `build_rung`) and a road's `keeper`; `"x-pick": "herd"` on `herd_id`, `fauna_id`, a held
  rung's `herd` and a pen entry's `herd`. A new field of that kind must carry the same annotation.
- **Guard: `core_sim/tests/scenario_schema.rs`** walks every checked-in `core_sim/scenarios/*.json`
  against the schema: an object's keys must be within its properties and cover its `required`, each
  `oneOf` value matches exactly one variant, every `x-pick: tile` holds a two-integer array, every
  `x-pick: band | herd` names an id the file declares, and a two-element array or a `band` /
  `keeper` / `herd` / `herd_id` / `fauna_id` key with no annotation is itself a failure.

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

