# Plan: Scenarios — start a game with the state already built

**Status:** design. Design issue #787, arc #920. The slices are #921–#929; §4 maps each issue to
its slice.

## 0. The gap, in one line

**Every feature past the opening costs a playtest of 100+ turns to reach.** The breeding ceiling
(#688) needs a people of 150, and a pen's top rung needs a band that has spent the whole early game
climbing. A scenario starts the game with that state already in place: the bands, worked tiles, pens
and their rungs, deposits, herds, roads, kits, materials and knowledge.

---

## 1. Decisions this plan builds on

These were settled in #787 before this plan was written. They are stated once here, and the rest of
the document builds on them.

| # | Decision |
|---|---|
| D1 | **The scenario FILE is the durable artifact, not a save.** A save stops loading at every `SAVE_FORMAT_VERSION` bump (`core_sim/src/save.rs`; there is no back-compat). A scenario is a **world seed plus a list of setup entries**, replayed onto a freshly generated world by the current code. Saves are still how a playtest is parked mid-session |
| D2 | **JSON first.** Point-and-click comes later, as a Workbench mode that **edits the same JSON** and holds no state the file cannot express |
| D3 | **Replay a command where one exists; set state directly only where none does** |
| D4 | **Each arc owns its own section** of the format, the same way rule files own arcs |
| D5 | **Strict loading** (`.claude/rules/core_sim/config-loading.md`): an unknown key or a missing required field is an error, and there are no serde defaults |
| D6 | **An entry that does not fit its tile is REFUSED, and the refusal says why.** A scenario may also override a tile's terrain or flora |
| D7 | **Dev-only.** Never reachable from `start_profiles.json` or the player's New Game screen |
| D8 | **A live game can be exported** to a scenario file |

---

## 2. The model

### 2.1 A scenario is a recipe, not a world

A save holds the world's *values*. A scenario holds the *instructions* that produce them:
*generate this seed, put a band of 150 here, have it know messengers, put 12 of its hands on that
cereal stand.* Because the current code carries out every instruction, the file never goes stale the
way a save does. A scenario breaks in only two ways, and both produce a refusal that names the cause:

- **A key is renamed or retired.** Strict loading (D5) refuses the file and names the key, and the
  fix is a one-line hand edit.
- **Worldgen changes and the seed no longer produces the same map.** An entry whose tile stopped
  fitting is refused (D6). §2.6 describes how to keep scenarios out of this trap.

### 2.2 Two kinds of entry

| kind | what it does | examples |
|---|---|---|
| **command entry** | Replays one of the game's own `CommandPayload` verbs (`sim_runtime/src/commands.rs`), through the same handler a player's command uses | assign labor, declare a build (`BuildRung`), extend a pen, the opening outfit (`SetStartingLoadout`), a bench order, split a band |
| **setter** | Writes state that no command produces | head-count and age shape, founding lines, a larder, knowledge already earned, a rung already held (a tile, herd or working meter), a depleted herd, a deposit's remaining stock |

**A setter passes the same gates the command would have.** "A scenario can't create a state the game
couldn't reach" (D3) would mean little if a setter could skip the checks. For example, a held
`cultivate` rung is refused when the faction does not hold the knowledge `build cultivate` requires,
and a held pen is refused on a herd the species roster says cannot be tamed. Each setter calls the
predicate its command already uses (`build_gates.rs`, the ladder's prerequisites and the habitat
checks) and does not write its own copy, so the two cannot drift apart.

**A command entry uses the command's own field names.** It is the `CommandPayload` variant in
snake_case, with two changes: `faction_id` is implied by the section, and `band_id` is replaced by
`band`, the band's **scenario id** (§2.3). A field later added to a command is therefore a key the
scenario can already use. No second vocabulary has to be maintained alongside it.

### 2.3 The file's shape

```json
{
  "name": "breeding_ceiling_150",
  "note": "One isolated people at 150, to watch births stop at L × K.",
  "world": {
    "preset": "earthlike",
    "width": 80,
    "height": 52,
    "seed": 41733,
    "profile": "late_forager_tribe",
    "archetypes": { "major": 0, "minor": 0 }
  },
  "factions": [
    {
      "faction": 0,
      "knowledge": ["messengers"],
      "bands": [
        {
          "id": "home",
          "at": [31, 18],
          "size": 150,
          "ages": { "children": 0.30, "working": 0.60, "elders": 0.10 },
          "founding_lines": 8,
          "larder": [{ "class": "dry", "amount": 900.0 }],
          "outfit": {
            "kits": { "big_game": 10, "gathering": 20 },
            "materials": { "fibre": 20, "hide": 12 }
          }
        }
      ],
      "labor": [
        {
          "assign_labor": {
            "band": "home", "role": "forage", "workers": 12,
            "target_x": 32, "target_y": 18,
            "take_species": [], "move_with_herd": false
          }
        }
      ]
    }
  ],
  "tiles": [
    { "at": [32, 18], "flora": ["wild_emmer"] }
  ]
}
```

- **`world`** holds the `new_game` arguments (`.claude/rules/core_sim/campaign.md` → Start Flow), and
  every one of them is required. A seed of `0` is **refused**, because `new_game` reads `0` as "pick a
  random seed" and a scenario has to name a real one.
- **`factions[]`** is keyed by faction index. A faction the file does not list spawns exactly as the
  profile says.
- **A band's `id`** is local to the file. Every later entry names its band by this id, and the loader
  maps it to the `BandId` that spawning allocated. No `BandId` or `Entity` appears in the file,
  because neither survives a regeneration (`.claude/rules/core_sim/checkpoints.md` → "No `Entity`
  crosses a persistence boundary").
- **The keys after `bands`** (`labor`, then the sections slice 2 adds: `rungs`, `pens`, `herds`,
  `deposits`, `roads`, `bench`) belong to their arcs (§2.4). `tiles` is a top-level key beside
  `factions`, because a tile belongs to no faction. The example shows slice 1 plus one labor row and
  one tile override.

### 2.4 Each arc owns its section

- **Code:** `core_sim/src/scenario/mod.rs` holds the shape of the file, the loader, the apply order
  (§2.8) and the refusal type. Each section is its own module (`scenario/bands.rs`,
  `scenario/labor.rs`, `scenario/pens.rs`, …) and implements one trait: **parse, check fit, apply and
  export** (export is slice 3). Two concurrent worktrees that add two sections touch two files, plus
  one line each in a section list that changes only when a section is added.
- **Docs:** a new rule, `.claude/rules/core_sim/scenarios.md`, owns the loader, the apply order and
  the refusal contract. **A section's key table lives in the rule file of the arc that owns it**
  (`husbandry.md` for pens, `extraction.md` for deposits, and so on), under a `## Scenario section`
  heading. That rule's `paths:` frontmatter gains the section's module, so whoever changes the arc's
  commands also loads the scenario keys those commands back.

### 2.5 Strict, and checked before anything is built

Loading has two passes. The first touches no world:

1. **Parse.** Every struct uses `deny_unknown_fields`, with no `#[serde(default)]` and no
   `Option` standing in for "absent means default". A field that is genuinely optional (a command
   field that is `Option` on `CommandPayload`) stays optional, because that is the command's own
   contract.
2. **Resolve names.** Every material, kit, species, rung verb, knowledge tag, keeping class and band
   id has to exist. These are checked against the loaded configs before worldgen runs, so a typo costs
   nothing to report.

The second pass runs on the generated world: **fit** (§2.6), then **apply** (§2.8).

### 2.6 Tiles: an entry fits its tile or it is refused

Every entry that names a tile is checked against the tile it lands on **before anything is applied**.
For example, forage on ocean, a pen on a herd that is not there, a held `fell` on a tile with no wood
deposit, or a band placed on water are each refused with the reason. **No check is ever relaxed to
make an entry fit.**

**Tile overrides (`tiles` section) exist for scenarios whose point is the tile.** If the scenario is
"I need a cereal stand here", it says so: `{ "at": [32,18], "terrain": "…", "flora": ["…"] }`.
Overrides are **a worldgen input, not a post-hoc edit.** They apply after the biome stage and before
anything derives from a tile's biome (flora realization, habitat and herd placement, spawn
placement), so every derived field is built from the overridden tile and nothing stale survives.
Slice 2a settled the list, limited to fields with a re-derivation point:

- **`terrain`, land to land only.** Elevation is the terrain authority, and the land mask, the
  coast, the rivers and a mountain's metadata are derived from it, so a change between land and
  water or onto or off a mountain has no re-derivation point and is refused. A land biome swap
  re-derives the tile's tags, then its food module, sites, herds, forage and graze from the new
  biome. Temperature is not overridden.
- **`flora`**, the species the tile grows, each one of the tile's biome roster. A tile's basket is
  otherwise a seeded draw recomputed on every read, so the override is stored on the tile and read
  by the one composition seam, and a save carries it.

**How to keep a scenario alive across worldgen changes:** override every tile the scenario depends
on. A scenario that overrides its few key tiles survives a worldgen retune that would have moved
the cereal stand it relied on.

### 2.7 Where files live, and how the server is told

- **Checked-in scenarios:** `core_sim/scenarios/<name>.json`. These are the durable playtest fixtures
  (D1), reviewed like any other file. The loader does not watch them.
- **Exports** (slice 3) are written to `SIM_SCENARIO_DIR` (default `./scenarios` relative to the
  server's working directory, gitignored like `saves/`). An export worth keeping is trimmed by hand
  and moved into `core_sim/scenarios/`.
- **The verb is `load_scenario <path>`**, a dev command on `CommandEnvelope` (next free field). It
  builds a new world exactly as `new_game` does, plus the setup, and answers on `QueryReplyEnvelope`
  with a `request_id`. Like the save verbs, it is answered because a dev who cannot tell whether the
  load landed has not loaded. It can be sent from:
  - `cargo xtask command load_scenario <path>`;
  - `scripts/run_stack.sh --scenario <path>`, which sends it once the server's ports are up;
  - in slice 4, the Workbench's scenario page.
- **Not replayable, and it re-bases the origin** — the same as `new_game` and `load_game`
  (`.claude/rules/core_sim/save-game.md` → "None of the save verbs are replayable"). Setters are not
  commands, so a log that began before them could not reproduce them. The origin is captured
  **after** setup, which means a rollback can never undo the scenario.
- **Dev-only (D7).** Nothing in `start_profiles.json`, the New Game screen or the main menu names
  it. Its presence on the wire is the same kind of dev surface as `set_config_override`.
- **The client must take a world it did not ask for.** Today a client at the main menu sends
  `new_game` or `load_game` and waits for the answer, so a scenario sent from a terminal reaches a
  client that did not request it. Slice 1 has to confirm what the client does when a world becomes
  active underneath it and make it leave the menu for that world. This is the same obligation a load
  already meets: `save-game.md` → "What a load owes beyond restoring the world".

### 2.8 Turn 1: the order setup applies in

`.claude/rules/core_sim/starting-loadout.md` is the constraint here. Every band gets an outfitting
window when it spawns, the sim commits that window's **default** immediately, and the window stays
open until the first turn is finalized. **The scenario uses this mechanism and adds no special case
for itself.**

| step | what | why here |
|---|---|---|
| 1 | Parse and resolve names (§2.5) | Costs nothing, and builds nothing on a typo |
| 2 | Worldgen with `world` and the **tile overrides** | Every derived field reads the overridden tile |
| 3 | **Spawn with the scenario's bands** in place of the profile's `starting_units` for every listed faction. `at`, `size` and `founding_lines` are spawn parameters. `ages`, `larder` and the faction's `knowledge` (added to the profile's `starting_knowledge_tags`) are inputs to the Startup seeding (`apply_starting_inventory_effects`) | Lines are minted only at a starting band's spawn (`FoundingLines::founded`), so that is where they are set. The size and larder are in place **before** `stamp_starting_loadout` opens each window, which means `outfit_opening_bands` fits the default against the band the scenario built (a 150-person carry), not a 30-person one. Knowledge is in place before kits that need it are validated |
| 4 | **Fit** every remaining entry against the built world (§2.6) | All refusals are found before any apply |
| 5 | **Setters**: held rungs and meters, herds, deposit stock, held roads | A command such as "labor on the pen" can require the pen to exist |
| 6 | **Command entries**, section by section in the fixed section order and in file order within a section. `outfit` comes first (as `SetStartingLoadout`, so it **replaces** the committed default under the carry cap), then a band's `auto_craft` (as `set_auto_craft`), then labor, builds, pens, the bench queue and splits | The opening window is still open, so the outfit is an ordinary revision |
| 7 | Capture the origin, then publish | The player lands on the turn-1 decision with every window still open and can revise the outfit as usual |

**`outfit` is optional.** Leaving it out keeps the default that step 3 already fitted to the band's
size. Including it replaces that default, and an order over the band's carry is refused
(`OverCarry`) on the same terms the outfit card uses.

### 2.9 How a refusal is reported

**Fail closed and whole**, on the same principle as `apply_starting_loadout`. A scenario with one bad
entry builds no world. Half a scenario is a world the dev never described, and a playtest on it would
answer the wrong question.

- Steps 1–5 **collect every refusal** before giving up, so one load lists all of a file's problems
  and the dev does not have to fix them one at a time.
- Each refusal names its **path in the file** and the **reason**, in the words the game's own refusal
  uses: `factions[0].labor[2]: forage at (40,7): tile is Ocean`.
- A command entry that the game's handler refuses in step 6 stops the load at that entry, and the
  report gives its path and the handler's reason. **This requires the replayed verbs to return their
  refusal.** Today `apply_command` lives in `bin/server.rs` and reports a refusal only on the event
  feed (`emit_command_failure`). Slice 1 moves the dispatch of the verbs a scenario replays into a lib
  seam that returns `Result<(), CommandRefusal>`. The server's own path calls that seam and still
  writes the feed event, so the two paths cannot differ.
- When a load is refused, the **previous world stays** if one was active, and the server stays idle
  if none was. The reply carries the whole list, and the server log carries it too.

### 2.10 Export (slice 3)

`export_scenario <path>` walks the live world and asks each section to **write what it can
express**. Bands become spawn parameters, their gear becomes `outfit`, held rungs, herds and
deposits become setters, and standing labor rows become command entries. The `world` block is the
seed and arguments the world was built from, which the server already keeps.

- **The export says what it could not express.** Whatever no section owns, such as an expedition in
  the field or a pending dealing, is listed in the export's `note` field and the log. It is never
  dropped silently.
- **An export loads back to an equivalent turn-1 state, not an identical world.** Ages, larders and
  meters come back as the setters wrote them. Anything a turn would have derived (forecasts, the
  ledger, events) is derived again. The export's round-trip test is **export → load → export gives
  the same file**. A world-equality check would compare state the format deliberately does not carry.
- **Trimming is by hand.** Exporting from an autoplayed run and deleting what the playtest does not
  need is the expected workflow.

### 2.11 The Workbench editor (slice 4)

A **Scenario** page on the Workbench registry (`.claude/rules/client/workbench.md`):

- It **edits the JSON** (D2). The page's state is the parsed file and nothing else, so every gesture
  is an edit to an entry. Clicking a tile fills in `at` or `target`, picking a band selects its
  `id`, and the section forms are generated from each section's keys.
- **The server validates and writes the file.** The page sends the edited JSON through a
  `write_scenario` dev command, which runs §2.5 and §2.6 against the live world and writes only a
  file that would load. A refusal comes back with the same paths §2.9 uses, and the page shows each
  one on the entry it names.
- **Load** sends `load_scenario` for the file being edited.
- The page follows the Workbench's existing contracts (named services, the shell budget, and a hidden
  page that ingests nothing). It needs no exception to any of them.

---

## 3. Deliberately out of scope

- **Anything a player can reach.** No profile, menu entry or New Game option (D7).
- **Scripting later turns.** A scenario describes the state at turn 1. A command that should land on
  turn 40 is a playtest the dev drives; the file does not contain it.
- **Injecting into a running world.** `load_scenario` always builds a new world. Adding a herd to the
  game being played is not a scenario.
- **Back-compat for scenario files.** Strict loading names a renamed key, and the fix is a hand edit.
  There is no migration code. Nothing in the repo carries back-compat yet, and scenarios are no
  exception.
- **Converting saves.** Loading a save and then exporting it (slice 3) already does this.
- **Rival AI behaviour.** A scenario can place and outfit a rival's bands like any other faction's,
  but the brain that plays them is the launcher's choice (`.claude/rules/core_sim/launcher.md`).

---

## 4. Slices

Each slice is one GitHub issue under the Scenarios arc and links back here.

| # | Issue | Slice | Delivers |
|---|---|---|---|
| 1 | #921 | **Loader + bands** | `scenario/mod.rs` (two-pass load, apply order, refusal list), the `load_scenario` verb and reply, `run_stack.sh --scenario`, the lib seam for replayed verbs (§2.9), the client accepting an unrequested world (§2.7), and the **bands** section: `world`, `at`, `size`, `ages`, `founding_lines`, `larder`, `knowledge`, `outfit`. A checked-in `core_sim/scenarios/breeding_ceiling_150.json` covers the immediate case. Also the `scenarios.md` rule and its routing row |
| 2a | #922 | **Worked tiles, labor and tile overrides** | the `labor` section (`AssignLabor`/`WorkPriority` entries; `ForageTile` is a retired verb the server ignores, and `assign_labor` with role `forage` replaces it) and the `tiles` override section as a worldgen input |
| 2b | #923 | **Held rungs and the build queue** | the `rungs` section: a held rung on a patch, a working or a herd as a gated setter (the ONE held-rung setter: 2c and 2d set none of their own), and a declared build as a `BuildRung` entry, with `BuildRung`'s dispatch moved into the command seam |
| 2c | #924 | **Pens and herds** | the `herds` section (placing or depleting a herd, with a habitat check) and the `pens` section (`ExtendPen`, `SetHerdOutput`; an entry names the herd by its scenario id, not a pen tile, because the pen tile is chosen at runtime, and `held.herd` and `build_rung.herd_id` name a herd the same way). A held tame or pen rung is a `rungs` entry (2b) |
| 2d | #925 | **Deposits** | the `deposits` section: remaining stock. A working's held rung is a `rungs` entry (2b) |
| 2e | #926 | **Roads** | the `roads` section: a held road rung per tile, and road-keeping labor |
| 2f | #927 | **Bench queue** | the `bench` section: `BenchEnqueue*` entries. The auto setting is not here: it is the band key `auto_craft`, replayed after the outfit (slice 2c) |
| 3 | #928 | **Export** | `export_scenario`, the export half of each section's trait, and the export → load → export test |
| 4 | #929 | **Workbench editor** | the Scenario page, `write_scenario`, and tile and band picking |

Slices 2a–2f each depend only on slice 1 and can run in parallel, which is what §2.4's split exists
for. Slice 3 exports only the sections that have landed. A section added later brings its own export
half.
