---
paths:
  - "core_sim/src/scenario/**"
  - "core_sim/src/command_seam.rs"
  - "core_sim/scenarios/**"
  - "core_sim/tests/scenario_loader.rs"
  - "core_sim/tests/scenario_schema.rs"
  - "core_sim/tests/scenario_rungs.rs"
  - "core_sim/tests/scenario_labor_tiles.rs"
  - "core_sim/tests/scenario_export.rs"
  - "core_sim/src/labor_commands.rs"
  - "core_sim/src/source_holders.rs"
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
| The editor verbs | `handle_list_scenarios` / `handle_read_scenario` / `handle_write_scenario` in `bin/server.rs`; `ListScenariosCommand` = CommandEnvelope **103**, `ReadScenarioCommand` = **104**, `WriteScenarioCommand` = **105**, all answered on `ScenarioOpReply` (`files` = 5, `text` = 6, `schema` = 7, appended after export's `notes` = 4; empty when not that verb's answer). Rust: `CommandPayload::{ListScenarios, ReadScenario, WriteScenario}`; text forms `list_scenarios` and `read_scenario <path>` (`write_scenario` carries a whole file and has none) |
| The editor's path policy, the file list, the schema | `resolve_scenario_path`, `list_scenario_files`, `scenario_schema`, `SCENARIO_DIR` in `core_sim/src/scenario/mod.rs` |
| Checked-in scenarios | `core_sim/scenarios/<name>.json` (reviewed fixtures, not watched): `breeding_ceiling_150.json`, `kept_roads.json` and the `*_playtest.json` files, among them `settlement_town_playtest.json` and `settlement_city_playtest.json` (a village a pit short of a town, a town a granary short of a city; `tests/settlement_scenarios.rs` pins each crossing). Both hold only chief bands (3 and 10, 150 people each, `leader: true`, `auto_craft: true`, three tiles apart) and a knowledge list of every early-game and food tag (the city adds `selective_breeding`); every band opens with a `stock` (kits sized to the work its band does at load so none works bare-handed, one each of `loom`, `tanning_frame`, `bone_awl`, `billet`, a few `axe`/`wedges`, and a bone/fibre/hide/wood/stone reserve for auto craft); every band's elders keep the food order the load gives a led band (no `band_order` and no forage or hunt row is stated; the elders pick the sites and the `herds` entries are the game around the camps), and only the home band states a labor row, the tile row that crews the build with its own hands (the chief food order deals the build its first builder before its hold crews and the build's cap before its refill, so the build holds its full crew). The build's `progress` (town pit 25 of 40, city granary 105 of 120) sets the crossing at about turn 5 and turn 4: a tile build takes at most its rung's `max_builders` heads, so the pace is set by `progress`, not by the head count |
| Sending one | `cargo xtask command load_scenario <path>` · `scripts/run_stack.sh --scenario <path>` |
| Exporting one | `scenario::export_scenario` (each section module's `export`); `handle_export_scenario` in `bin/server.rs`; `ExportScenarioCommand` = CommandEnvelope **102**, answered on `ScenarioOpReply` (**14**); `cargo xtask command export_scenario <path>` |

## Two passes, and nothing is built on a typo

1. **Parse and resolve names** (`parse_scenario`), with no world built. Every struct is
   `deny_unknown_fields`, there are no serde defaults, and a key is optional only where the
   command it replays has an `Option` (a band's `outfit`, `leader`, `auto_craft` and `stock`; a labor entry's target,
   kit, species, floor; a tile's `terrain`, `flora` and `belief`, of which one is required; a road's
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
- **`finish_scenario` resets a `leader: true` band's build-turn leadership** (a band of about a hundred banks
  the practice for a chief on the build turn, and that chief's pass would list sources the file's labor rows
  hold): the leadership record is defaulted and the chief's orders and the rows they listed are dropped. A
  band without the key keeps what the build turn gave it, which the checked-in fixtures rely on.
- **`finish_scenario` then gives each `leader: true` band its leader** (`bands::apply_leaders`, through the
  `BandLeadership::appoint` body emergence runs; `leadership.md` → "Scenario section") **after the outfit and
  auto-craft replays and immediately before labor**: each of those restaffs its band, and a led band's
  restaff runs its chief's managing pass. A `band_order` entry needs the leader.
- **`finish_scenario` then founds each faction's `tribes`** (`tribes::apply`, `leadership.md` → "Scenario
  section") immediately after the leaders (a tribe needs a chief among its members) and before the labor replay.
- **A chief's pass during the load leaves every source the file's labor rows name alone.** Just
  before `apply_leaders`, `finish_scenario` inserts `ScenarioLaborReservations`
  (`source_holders.rs`): each forage, hunt and extract `assign_labor` row's source → its band and
  faction (a hunt by runtime herd id). Only the chief's managing passes read it, through
  `SourceHolders::with_reservations` where `staffing.rs` builds their holders, and a reservation
  counts as a player row of the reserving band. So appointing a chief, or any restaff before a
  later band's row is replayed, cannot list that row's source and get the row refused "X already
  hunts the Y.", whatever order the rows come in and however many bands are led. The resource is
  removed when the labor replay ends, on every exit, so play never sees it. The export still writes
  unled bands' rows first (`work-party.md` → Export).
- **Every band that states an `outfit` has a window to state it in.** The game opens one Grant window
  per faction, on its lowest band. Before the outfit replay `finish_scenario` opens a window of the
  same kind (`LoadoutWindow::opened(LoadoutSupply::Grant { carry_budget: None })`: the band's own carry,
  read live) on each outfitted scenario band that has no open one. A band with no `outfit` gets none.
  The carry gate is the window's own, so `OverCarry` refuses a second band at
  `factions[i].bands[j].outfit` as it does a first.
- `finish_scenario` then replays `outfit` as `SetStartingLoadout` through `command_seam`, which
  **replaces** the default while the opening window is still open.
- **`opening` (optional top-level bool, default `true`) says whether the scenario opens like a new game.**
  `false` opens it mid-game: `build_scenario_world` retires every beat the Telling catalog marks
  `opening` (`BeatLedger::retire_opening_beats`, before the build turn, since the Telling runs on it;
  a `once` ledger entry also holds across a save), and after `finish_scenario` closes every outfitting
  window through `StartingLoadout::close_all` (the body `close_opening_window` runs), leaving the
  default outfit the world-build pass committed. A file with `opening: false` and a band `outfit` is
  refused at that band's `factions[i].bands[j].outfit`, since the outfit is stated in the window it
  has none of. The export omits the key when `true`. Both settlement playtest scenarios set it
  `false`.
- `finish_scenario` then writes each band's `stock` (`bands::apply_stock`) straight into its
  inventory after the outfit replay, so it lands with `opening` true or false, outside the carry cap, on top of whatever the band holds.
- `finish_scenario` then replays each band's `auto_craft: true` through
  `command_seam::apply_set_auto_craft` (after the outfit, before labor). A refusal stops the load at
  `factions[i].bands[j].auto_craft`.
- `finish_scenario` then replays every faction's `labor` entries, in file order, through
  `apply_assign_labor` / `apply_work_priority` / `apply_band_order`. Labor runs only when no outfit was refused, and the
  first refused entry stops the load. A replayed entry writes the same success line to the feed a
  player's command would.
- `finish_scenario` then replays the `build_rung` entries through `command_seam::apply_build_rung`,
  after labor (plan §2.8), so a build finds the band labor put on its source; a tile build's optional
  `progress` (work already done, `intensification.md` → "Scenario section") is banked right after its
  declaration. A refused build stops the load at that entry.
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

- **Path policy (read and write).** The write path is read against the server's working directory
  (`SCENARIO_BASE_DIR`, `.`; `run_stack.sh` runs the server from the repo root), folded lexically
  (`.` and `..` removed without touching the disk), and must be a file **directly inside**
  `SCENARIO_DIR` (`core_sim/scenarios`) ending in `.json`. A `..` that climbs out, a subdirectory, a
  wrong extension or an absolute path outside the directory is refused at `FILE_REFUSAL_PATH` with a
  sentence saying why. `load_scenario`'s own path handling is unchanged: xtask and `run_stack.sh`
  pass absolute paths.
- **Read and list also see the export directory; write does not.** `export_scenario` writes under
  `SIM_SCENARIO_DIR` (`scenario_export_dir()` in `bin/server.rs`, default `./scenarios`), so
  `list_scenarios` and `read_scenario` take a `*.json` directly inside **either** directory, by the
  same lexical no-escape rule (`resolve_scenario_read_path`; the export directory may be absolute).
  `write_scenario` takes `resolve_scenario_path` alone, fixture directory only: an export is opened,
  trimmed in the editor and **saved as a reviewed fixture** in `core_sim/scenarios` (plan §2.10), never
  rewritten in place, so the editor never overwrites what the exporter produced.
- **`list_scenarios`** answers `files` (the fixture directory's `*.json` sorted, spelled
  `core_sim/scenarios/<name>.json`, then the export directory's sorted, spelled `<export dir>/<name>.json`;
  each is a path `read_scenario` accepts, and a missing export directory adds nothing) and `schema`.
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
other herd by `(species key, y, x, id)`. A worldgen herd that has died out keeps its slot as a
`gone` entry, so the ids after it do not shift. Every section that names a band or a herd uses these ids.

**A worldgen herd that died out stays dead.** The loader leaves a worldgen herd the file does not name
as worldgen made it, so the export names the dead ones. `spawn_initial_herds` records each herd it
spawns in `WorldgenHerdRoster` (`fauna.rs`: id, species, spawn tile; not part of `SimState`). The
export writes `{"origin": "worldgen", "at": <spawn tile>, "stock": 1.0, "gone": true}` for every
roster herd no longer in the registry, and for a worldgen herd at zero stock. The loader despawns
the matched herd. A herd the build turn already dispersed is accepted, because the roster still
names it. Pass 1 refuses `gone` on a placed herd, `gone` with a stock other than 1.0, and
`gone: false`. A world restored from a save has no roster: the export notes that a dead worldgen
herd will respawn on reload.

**What a section writes.** The per-arc tables (`campaign.md`, `work-party.md`, ...) carry each
section's export line. Out of every section: a faction with no band, or one the world gained after it
was built (it cannot be named by `world.archetypes`), is noted and left out; a part-learned knowledge
tag, worn gear, hay, a leader's derived name and kind, banked leadership practice, a pen's rings, parties in the field, pending dealings, treaties,
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
note. An outfit lists only the materials the human archetype's pick list names; any other material a
band holds, and any gear no kit is made of, is written as the band's `stock`. Every band's kits are written as its own `outfit`. A held rung is replayed through the build gate, so a faction must know the
lesson it needs, which only `knowledge` tags the start profile names can say.
