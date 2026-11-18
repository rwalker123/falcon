---
paths:
  - "core_sim/src/scenario/**"
  - "core_sim/src/command_seam.rs"
  - "core_sim/scenarios/**"
  - "core_sim/tests/scenario_loader.rs"
  - "xtask/src/main.rs"
  - "scripts/run_stack.sh"
---

# Scenarios: the loader, the apply order, the refusal contract

A scenario is a **recipe for a world, not a world**: a seed plus setup entries, replayed onto a
freshly generated world by the current code, so the file never goes stale the way a save does. The
design is `docs/plan_scenarios.md`. This file owns the loader. **A section's key table lives in the
rule of the arc that owns it**, under `## Scenario section`. The bands section's table is in
`campaign.md`.

## Where things are

| piece | where |
|---|---|
| File shape, pass 1, `ScenarioRefusal`, `finish_scenario` | `core_sim/src/scenario/mod.rs` |
| One module per section | `core_sim/src/scenario/bands.rs` (slice 1) |
| Replayed verbs that **return** their refusal | `core_sim/src/command_seam.rs` |
| The verb and its reply | `handle_load_scenario` in `bin/server.rs`; `LoadScenarioCommand` = CommandEnvelope **101**, `ScenarioOpReply` = QueryReplyEnvelope **14** |
| Checked-in scenarios | `core_sim/scenarios/<name>.json` (reviewed fixtures, not watched) |
| Sending one | `cargo xtask command load_scenario <path>` · `scripts/run_stack.sh --scenario <path>` |

## Two passes, and nothing is built on a typo

1. **Parse and resolve names** (`parse_scenario`), with no world built. Every struct is
   `deny_unknown_fields`, there are no serde defaults, and `outfit` is the only optional key. Names
   are checked against the live app's config handles (presets, profiles, archetypes, knowledge tags,
   keeping classes, kits, materials). **Every refusal is collected**, so one load lists all of a
   file's faults. A serde error has no field path, so it reports `line L, column C`.
2. **Build a candidate, fit, apply.** `build_world_app` builds the candidate with **no snapshot sink
   attached and the shared `world_epoch` not advanced**, and the live world is never touched. Fit
   refusals found during Startup (band on water or off the map, absent faction, an archetype with no
   `starting_units`) are recorded on `ScenarioSetup`. `finish_scenario` then replays the command
   entries.

**Fail closed and whole.** Any refusal drops the candidate. The previous world stays, and an idle
server stays idle. The reply and the server log both carry the full list. On success the server
retires the old publisher, advances and stamps the epoch, attaches the sink, publishes a fresh full
frame (`publish_loaded_world`), and **re-bases the command log after setup**, so a rollback can
never undo the scenario.

## The apply order uses the game's own turn-1 machinery

There is no scenario special case after Startup. `ScenarioSetup` is inserted **before** Startup:

- `spawn_initial_world` spawns a listed faction's bands, in file order, **instead of** its
  archetype's `starting_units`, and **ahead of** the profile branches. The first listed band
  therefore holds the faction's lowest `BandId`, which is the band `stamp_starting_loadout` opens the
  outfitting window on.
- `apply_starting_inventory_effects` seeds a scenario band from its `ages` and its exact larder.
- The default outfit is fitted on the world-build pass against the band the scenario built.
- `finish_scenario` replays `outfit` as `SetStartingLoadout` through `command_seam`, which
  **replaces** the default while the opening window is still open.
- `ScenarioSetup` is removed in `finish_scenario`. It is never part of `SimState`, a save or a
  snapshot.

## The command seam

`command_seam.rs` holds the replayed verbs as functions that return
`Result<(), CommandRefusal>`. `CommandRefusal`'s `Display` is the reason text the event feed shows.
The server's handler converts the wire, calls the same function, and on `Err` still writes the warn
and the feed event, **so the scenario path and the player's path are one body.** A section that
replays a new verb adds a function and a `CommandRefusal` variant there, never a copy of a
handler. The yield re-quote those verbs need lives in `core_sim/src/yield_seed.rs` for the same
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
