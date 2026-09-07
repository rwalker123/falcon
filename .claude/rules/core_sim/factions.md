---
paths:
  - "core_sim/src/orders.rs"
  - "core_sim/src/start_profile.rs"
  - "core_sim/src/data/start_profiles.json"
  - "core_sim/src/bin/server.rs"
  - "core_sim/src/systems/mod.rs"
  - "core_sim/src/systems/worldgen.rs"
  - "core_sim/src/systems/population.rs"
  - "core_sim/src/starting_loadout.rs"
  - "core_sim/tests/faction_support/mod.rs"
  - "core_sim/tests/multi_faction_start.rs"
  - "core_sim/tests/defection_contact_gate.rs"
---

# Factions — who plays the world, and who is allowed to command

`FactionRegistry` (`orders.rs`) is the answer to *"which factions does this world have, and how is
each one driven"*. Everything downstream — the turn queue's await set, the espionage rosters, the
counter-intel budgets, the security policies — is built from its `factions` list, so the registry is
the single place a world's roster is decided.

## Ids are POSITIONAL, never authored

A start profile declares a list of `FactionSpec`s; entry `i` **is** `FactionId(i)`. Nothing in the
JSON names an id.

That is deliberate. An authored id can be written twice or leave a gap, and the registry's `control`
map is keyed by id — a duplicate would silently collapse two factions into one map entry, and a gap
would produce an id that is in `factions` but has no control. With positional ids both are
unrepresentable rather than merely validated against.

```jsonc
// core_sim/src/data/start_profiles.json — top level of a profile object
"factions": [ { "control": "human" }, { "control": "ai" } ]   // human = 0, ai = 1
```

## The registry's invariant, and its one constructor

```rust
pub struct FactionRegistry {
    factions: Vec<FactionId>,
    control: BTreeMap<FactionId, FactionControl>,
}
```

**`control` is keyed by exactly the ids in `factions`.** `FactionRegistry::new(&[FactionSpec])` is
the only constructor that can produce a non-default registry, and it derives *both* fields from one
declaration list, so the two cannot be written apart; a `debug_assert!` there states the invariant.
Readers ask through `factions()`, `control_of`, `is_ai` and `contains` rather than touching either
field, and an unregistered faction answers `None` / `false` rather than panicking — a faction nobody
declared is not the sim's to drive.

**Both fields are private, and privacy is what makes the invariant an invariant.** While they were
`pub` the `debug_assert!` guarded only the constructor, and three integration fixtures wrote
`factions.factions = vec![FactionId(0), FactionId(1)]` onto a default registry — leaving the
one-entry control map behind, so `FactionId(1)` was listed while `contains(FactionId(1))` was false
and `control_of` was `None`. That is exactly the "registered but uncontrolled" state this section
calls meaningless, and it is the state in which `apply_command`'s membership gate silently drops a
faction's commands. Serde reads and writes private fields, so nothing about the save format depends
on their visibility.

`FactionControl` is `Human | Ai`. There is no third arm: every faction is either somebody's to play
or something the sim drives. It lives in `start_profile.rs` with `FactionSpec`, because the profile
is where control is *declared*; `orders.rs` imports it.

## Where the roster comes from

- **`build_headless_app`** validates the active profile's roster and then seeds the registry from
  it, before `TurnQueue::new(registry.factions().to_vec())`. Raising the count therefore extends the
  await set with no further edit.
- **`apply_start_profile`** (`bin/server.rs`) re-seeds the registry **and everything else the boot
  path seeds from it** when a runtime command names another profile. It has to:
  `rebuild_world_from_config` calls `build_headless_app` *first*, so a `new_game` arrives holding the
  **boot** profile's roster, and applying the chosen one to `SimulationConfig` alone left a
  two-faction profile producing a one-faction world. See the table below for the set.
- **`save::apply_save`** rebuilds the `TurnQueue` from the registry it has just restored, beside that
  restore rather than in the load handler. A load's replacement app is also a `build_headless_app`,
  so its queue awaits the *file's* profile; a two-faction save opened on the shipped one-faction
  profile would otherwise resolve turns without ever awaiting faction 1. The rollback path
  (`bin/server.rs`) rebuilds it from the registry for its own reason — the discarded future's
  submissions must not survive — and the two now say the same thing. **The load needs nothing
  further**: every other roster-derived resource is checkpoint state and comes back with the save.
- **`FactionRegistry::default()`** is one human faction — `FactionId(0)`, `control { 0: Human }` —
  and is what a test harness or any other non-profile construction path gets. It shares
  `start_profile::default_factions()` with the profile layer's default so the two statements of
  "the default world" cannot drift.
- **`StartProfileOverrides` implements `Default` by hand** for exactly this reason. A derived
  `Default` gives `factions` an *empty* `Vec` regardless of the serde `default =` attribute, and an
  empty roster has no faction 0 for worldgen to place, nobody to play and nobody to await. The empty
  roster has to be unrepresentable from every construction path, not only the deserialising one.

### ⛔ THE ROSTER-DERIVED SET IS FIVE RESOURCES, AND A RUNTIME PATH OWES ALL OF THEM

`build_headless_app` builds five things from `faction_registry.factions()`. Re-seeding some of them
on a roster change is the *same defect* as re-seeding none, one resource further along — and worse to
read, because the next person infers the short list is the whole list.

| Resource | `new_game` / `set_start_profile` | A load |
|---|---|---|
| `FactionRegistry` | re-seeded in `apply_start_profile` | restored — `WorldStatics` |
| `TurnQueue` | rebuilt from that registry | **rebuilt in `apply_save`** — server-side order intake, so no payload carries it |
| `CounterIntelBudgets` | re-seeded, through `CounterIntelBudgets::new` | restored — `SimState` |
| `FactionSecurityPolicies` | re-seeded, through `FactionSecurityPolicies::new(.., Standard)` | restored — `SimState` |
| `EspionageRoster` | **deliberately not** — `initialise_espionage_roster` is a `Startup` system that seeds from whatever registry it finds, and the caller runs Startup afterwards | restored — `SimState` |

Both re-seeds go through the **boot path's own constructors**, so a fresh faction's starting state
has one definition rather than two.

**Neither of the two espionage resources can report its own omission.**
`CounterIntelBudgets::available` answers `scalar_zero` for a faction with no row and
`FactionSecurityPolicies::policy` answers its `default_policy` — which *is* the seeded value — so a
forgotten faction is silently indistinguishable from a seeded one at every reader. That is why
`FactionSecurityPolicies::contains` exists: the row's presence is the only thing a test can assert
on.
### Validation is a boot panic

`StartProfileOverrides::validate_factions(profile_id)` enforces two rules and panics naming the
profile when either breaks:

| Rule | Why |
|---|---|
| the list is non-empty | a world with no factions has nobody to place, play or await |
| at least one entry is `human` | a world nobody plays is not a world |

This is the `config-loading.md` boot rule applied to a profile key: **absent means the builtin
default, present-but-broken stops the boot.** The shipped profile omits `factions` entirely, so the
builtin can never trip it.

**A profile chosen at RUNTIME is refused instead, never panicked on.** `new_game` and
`set_start_profile` ask `StartProfileOverrides::faction_roster_error()` — the same two rules,
returning the broken one instead of panicking — and answer the way each already answers a profile id
it cannot resolve: `new_game` warns `new_game.rejected=unusable_roster` and returns **before the
outgoing world is torn down**, and `apply_start_profile` writes nothing and warns
`start_profile.rejected=unusable_roster`. Boot panics because there is no earlier world to decline
back to; taking a live server down because a player picked a bad profile is a worse answer than
declining the pick.

## Command authorization

Two gates, at two different distances from the player.

- **Membership, once, at `apply_command`** (`bin/server.rs`). `commanding_faction(&Command)` names
  the faction *issuing* each command — deliberately exhaustive with no `_` arm, so a new verb must
  state whether it is somebody's order or the server's own business. A command whose faction is not
  in the registry is dropped with `warn!(target: "shadow_scale::command", …
  "command.rejected=unknown_faction")` and never reaches its handler. It emits **no**
  `CommandEventFailure`: the feed is per-faction, and filing a failure under a faction that does not
  exist is the exact defect the gate removes (before it, such a command ran to its handler and was
  refused downstream by `no_such_band` / `wrong_faction`, which reads as a real faction that happens
  to own nothing).
- **Ownership, per resolver.** `resolve_starting_unit_entity`, `resolve_expedition_entity` and
  `band_entity_and_tile` all require `cohort.faction == faction`: a band id is a durable, guessable
  handle, so a resolver matching on the id alone would hand a caller another faction's band.
  `road_verb_refusal` *also* checks band ownership and is what produces the message a player reads
  ("Band {id} is not one of your people…"); the resolver's gate is defence in depth behind it, not
  its replacement.

**A faction named inside a payload is not a commanding faction and is not gated.** Espionage verbs
legitimately name another faction as owner or target, and `resolve_shipment` deliberately never asks
faction at all — a cross-faction trade destination works by construction.

## Every faction gets land, people and an opening

Worldgen places **every registered faction**, not faction 0. `spawn_initial_world` takes the
`FactionRegistry` and loops the roster over the four things a people needs to exist: a start tile, a
spawned roster of bands around it, the profile's seeded stockpile, and the profile's seeded
knowledge. There is no `PLAYER_FACTION` constant any more — it was the single place the whole
opening was hard-wired to `FactionId(0)`, and every one of its readers now asks the roster.

**The registry is an `Option<Res<..>>` in that system**, like the start-kit handles beside it:
`build_headless_app` inserts it before Startup, but 44 hand-rolled test `World`s do not, and worldgen
must not panic on them. Absent reads as `FactionRegistry::default()` — the one human faction those
worlds have always had.

### The starts are scored once and picked greedily, a minimum distance apart

`faction_start_tiles` scores every land tile with `score_start_tiles` — **the same per-tile scoring as
before, unchanged**, because this arc changed *selection*, not what makes ground good — and then
picks one start per faction:

- The **first** pick is the argmax over every land tile, so a one-faction world opens on exactly the
  tile it always has. Pinned by `multi_faction_start::a_one_faction_world_opens_on_the_tile_it_has_always_opened_on`.
- Each **later** faction takes the highest-scoring remaining tile that is at least
  `faction_start_min_separation` from every start already picked.
- **Ties keep the strict `>` over the row-major scan**, so the lowest `(y, then x)` maximum wins.
  Determinism is load-bearing: a replay that picked a different maximum would build a different world
  from the same seed. A real map's winner is a wide *unique* maximum, so the tie-break is only
  observable on a synthetic grid — which is where `start_tile_selection_tests` guards it.
- **Distance is Euclidean, compared squared**, matching the curated food-site pass's `min_spacing`
  idiom, so the file has one notion of "far enough apart".

> #### Relaxation, never failure
>
> If no remaining tile clears the separation, worldgen takes the best remaining tile anyway and
> warns `worldgen.start_separation_relaxed` with the faction and the distance it achieved. **A
> cramped map is a worse world, not a dead one** — a faction left unplaced has no land, no band and
> nobody to play it, which is a strictly worse outcome than two peoples starting near each other.

### `spawn_default_population_clusters` stays faction 0's alone

The no-`starting_units` fallback — a lattice of 1,000-person clusters around one point — is the
degenerate/debug path, and no campaign profile takes it. Giving every faction a copy would multiply a
scaffold, not place a people, so the call site passes `FactionId(0)` explicitly.

## `StartLocation` is per-faction, and two readers ask it faction-blind

`StartLocation` is a `BTreeMap<FactionId, UVec2>`. `position_for(faction)` is the ordinary read;
`anchor_position()` is the **map's** anchor — the lowest-id faction's start — for readers asking
about the world rather than about a people:

| Reader | Asks | Why |
|---|---|---|
| `snapshot/capture.rs` | `position_for(viewer_faction.0)` | a frame is captured for one viewer, so the wire's single `StartMarkerState` carries **that viewer's** opening ground. The `.fbs` is unchanged — there is no per-faction marker table |
| `bin/server.rs` `found_settlement` | `relocate(faction, target)` | only the **founding** faction's marker moves. It used to move the one global marker with no faction check, which on a two-faction map is a rival re-homing your start |
| `fauna.rs` `spawn_initial_herds` | `anchor_position()` | the migratory-herd anchor is a property of the **map**, not of a people: long-range herds range across the whole world |
| the worldgen suites (`food_site_water_bias`, `graze_distribution`) | `anchor_position()` | they assert about the terrain, and have no faction to ask with |

`Default` is the **empty** map, which is what ~20 integration fixtures insert as scaffolding, so none
of them needed an edit. **`SAVE_FORMAT_VERSION` is 7** for the shape change — see the changelog table
on that constant.

## One opening-loadout window PER FACTION

`stamp_starting_loadout` opens a window for the lowest `BandId` **within each faction**. It used to
take the globally lowest `BandId` carrying `StartingUnit` with no faction filter, justified by
worldgen hard-coding every cohort to `FactionId(0)`; that premise is gone. A globally-lowest pick
would hand the whole opening allocation to whichever faction happened to be placed first and leave
every other people unoutfitted — no kits, no material, and nothing on screen to say why.

## A band defects only to a people it has MET

The knowledge migration's trigger is unchanged — a settled band (`age_turns >=
migration_min_settled_turns`), with knowledge, and **HIGH** morale (`> migration_morale_threshold`).
What changed is the destination: it is now the first registered faction that is not the cohort's own
**and that the cohort's own faction has actually made contact with**. No contact, no destination, no
defection.

Contact is `ConnectionLedger::factions_in_contact(band_factions, a, b)`: does any live tie
(`strength > NO_TIE`), in either direction, join a band of `a` to a band of `b`. **Faction stays a
property of the ENDPOINT** — the caller supplies the `BandId -> FactionId` map, resolved in
`simulate_population` from the same query it then mutates and taken *before* the loop, so a band that
changes sides part-way through a turn cannot make the answer depend on iteration order. The edge
itself carries no faction, which is `connections.md`'s rule.

`ConnectionLedger::tie_is_live` moved onto the ledger for this: `supply.rs` had the only copy, and
two riders asking *"what counts as a live tie"* must not each own an answer free to drift. Supply's
private `tie_is_live` survives as a one-line delegation, because its doc comment is where the
logistics link rule is stated.

**Distance and prosperity are not asked.** The designed end state is scouts defecting to a
*better-off* faction; contact is the half this arc landed, and it is the half that stops a band
walking to strangers on the far side of the world.

## The single-faction assumptions that are still live

Two places still read a one-faction world into a roster that may hold more.

| Site | What it assumes |
|---|---|
| `telling/mod.rs` (signal sampling) | Takes the registry's **lowest id** as "the player" and filters every band view to it; its own comment says there is no `player_faction` accessor. |
| `visibility.rs` `ViewerFaction` | A single **global** resource read by `snapshot/capture.rs`, so one snapshot is captured and broadcast to every connected client. |

## The two-faction fixture

`core_sim/tests/faction_support/mod.rs` builds both arms of every comparison — `one_faction_world()`
and `two_faction_world()`, plus `world_with(roster, tune)` for a world that also needs a config edit.
**The roster is installed before the first `update()`**, which is when `Startup` and therefore
`spawn_initial_world` run; that is what makes worldgen place two factions without editing a shipped
config file or setting a process-global env var a parallel test would race on. It rebuilds the
`TurnQueue` from the roster too, for the reason the roster-derived-set table above gives.

The control arm is not optional: it is what distinguishes *"the second faction got its own"* from
*"the first faction got it twice"* and from *"nothing changed at all"*.

## Config files

| File | Key | Default | Purpose |
|---|---|---|---|
| `src/data/simulation_config.json` | `faction_start_min_separation` | **20** tiles | How far apart worldgen tries to put two factions' start tiles — a quarter of the shipped map's width, far enough that two peoples do not open sharing one food shed. Euclidean, compared squared. A **target**: on a map with no land pair that far apart, worldgen relaxes and warns rather than failing to place a faction. Validated `> 0` at parse (`ZeroFactionStartMinSeparation`), because zero would let two peoples open on the same hex |

## Saves win over profile edits

`FactionRegistry` is **ground truth in the save** — a field of `WorldStatics`, captured whole and
restored whole. The start profile, by contrast, is re-resolved from **live config by id** on load,
which then overwrites `SimulationConfig::start_profile_overrides`.

So the two can drift, and the intended rule is that **the save's registry wins**: a world loaded
from a save has the roster it was created with, even if `start_profiles.json` has been edited to
declare a different one since. That is the same reason the rest of `WorldStatics` is saved rather
than recomputed — re-deriving a world's ground truth from tuning that has moved produces a
*different world*.

## A free slot

`StartProfileOverrides::ai_profile_overrides` is parsed and read by nothing. It is where per-faction
AI behaviour tuning would attach when there is an AI to tune.
