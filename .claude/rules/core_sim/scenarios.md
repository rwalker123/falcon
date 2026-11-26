---
paths:
  - "core_sim/src/scenario/**"
  - "core_sim/src/command_seam.rs"
  - "core_sim/scenarios/**"
  - "core_sim/tests/scenario_loader.rs"
  - "core_sim/tests/scenario_rungs.rs"
  - "core_sim/tests/scenario_labor_tiles.rs"
  - "core_sim/tests/scenario_export.rs"
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
| Checked-in scenarios | `core_sim/scenarios/<name>.json` (reviewed fixtures, not watched): `breeding_ceiling_150.json`, `kept_roads.json` and the `*_playtest.json` files |
| Sending one | `cargo xtask command load_scenario <path>` · `scripts/run_stack.sh --scenario <path>` |
| Exporting one | `scenario::export_scenario` (each section module's `export`); `handle_export_scenario` in `bin/server.rs`; `ExportScenarioCommand` = CommandEnvelope **102**, answered on `ScenarioOpReply` (**14**); `cargo xtask command export_scenario <path>` |

## Two passes, and nothing is built on a typo

1. **Parse and resolve names** (`parse_scenario`), with no world built. Every struct is
   `deny_unknown_fields`, there are no serde defaults, and a key is optional only where the
   command it replays has an `Option` (a band's `outfit` and `auto_craft`; a labor entry's target,
   kit, species, floor; a tile's `terrain` and `flora`, of which one is required; a road's
   `keeper`, which a trail has none of). Every section key is required (`[]` for none). Names are
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
- **`finish_scenario` stamps each band's `size`, `ages` and `larder` again**, first thing, through
  the same `systems::seed_scenario_band` the Startup seeding calls: the build turn fed, aged and
  buried the bands, and a band opens at **exactly what the file says** (before the outfit replay,
  because the larder counts against carry).
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

## Export: the live world written out as a scenario

`export_scenario <path>` (`CommandEnvelope` field **102**, dev-only, answered on `ScenarioOpReply`
with its `request_id`) walks the live world and writes the file that rebuilds it. It changes no world,
so it is not replayable (excluded beside the save and scenario verbs). `<path>` is required: a relative
one resolves under **`SIM_SCENARIO_DIR`** (default `./scenarios`, under the server's working directory,
which `run_stack.sh` sets to the repo root; `/scenarios/` is gitignored, `core_sim/scenarios/` is the
tracked fixture dir), an absolute one is used as it is; the directory is created. `world.name` is the
path's file stem.

**The reply.** `ok` is true once the file is written. `path` is where it landed, `notes` is one line
per thing the live world holds that the file cannot say (each line also goes to the server log, and the
same lines joined are the file's `note`), and `refusals` is what the file's own **pass 1**
(`parse_scenario` on the text just serialized) refused: the file is written regardless, because an
export the dev trims by hand is still the thing they want. A failed write is `ok: false` with one
refusal.

**Shape.** Each section module has a free `export` beside its `check` and `apply`;
`scenario::export_scenario(world, name)` calls them in turn. Every file and entry type is `Serialize`
(`deny_unknown_fields` kept, no defaults; an `Option` that is `None` is absent), so the output
reparses strictly. A float is rounded through `round_export` to `EXPORT_FLOAT_DECIMALS` places (one
constant), so an exported value is already at the precision the file holds it at.

**Ids are positional.** A band is `f<faction>_band<n>`, `n` its index among that faction's bands in
ascending `BandId` order (a reload spawns in file order, so the lowest id comes back first and holds the
opening outfit window). A herd is `herd<n>` in the export order: worldgen herds by runtime id, then every
other herd by `(species key, y, x, id)`. Every section that names a band or a herd uses these ids.

**What a section writes.** The per-arc tables (`campaign.md`, `work-party.md`, ...) carry each
section's export line. Out of every section: a faction with no band, or one the world gained after it
was built (it cannot be named by `world.archetypes`), is noted and left out; a part-learned knowledge
tag, worn gear, hay, a leader, a pen's rings, parties in the field, pending dealings, treaties,
goodwill and tile or band improvements are noted. `tiles` is always `[]` with the line "tile overrides
are not exported". A band whose larder plus outfit exceeds its carry is exported anyway and noted
("will be refused OverCarry on load; trim by hand").

**The round-trip contract** is export -> load -> export giving the same file
(`tests/scenario_export.rs`, every checked-in scenario run three turns first). Every key converges
**except `note`, which is not compared**: it describes the live world the file was exported from (a
herd that wandered, a party out), so a reload, which has not lived, has less to note.

**What export adapts so the file loads.** A worldgen herd is written at its spawn tile (`route[0]`), and
the herd fit matches the spawn tile first, else the tile the herd stands on after the build turn; a
`game_` immigrant past the build turn is written as `placed`. A placed herd whose current tile fails
the placement predicate is written at the nearest tile that passes (hex distance, then `(y, x)`) with a
note. An outfit lists only the materials the human archetype's pick list names; any other material the
first band holds is noted. A held rung is replayed through the build gate, so a faction must know the
lesson it needs, which only `knowledge` tags the start profile names can say.
