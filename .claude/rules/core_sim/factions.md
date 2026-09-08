---
paths:
  - "core_sim/src/orders.rs"
  - "core_sim/src/start_profile.rs"
  - "core_sim/src/data/start_profiles.json"
  - "core_sim/src/data/simulation_config.json"
  - "core_sim/src/metrics.rs"
  - "core_sim/src/victory.rs"
  - "sim_runtime/proto/command.proto"
  - "core_sim/src/bin/server.rs"
  - "core_sim/src/systems/mod.rs"
  - "core_sim/src/systems/worldgen.rs"
  - "core_sim/src/systems/population.rs"
  - "core_sim/src/starting_loadout.rs"
  - "core_sim/tests/faction_support/mod.rs"
  - "core_sim/tests/multi_faction_start.rs"
  - "core_sim/tests/defection_contact_gate.rs"
  - "core_sim/src/snapshot/capture.rs"
  - "core_sim/src/snapshot/population.rs"
  - "core_sim/tests/foreign_band_redaction.rs"
  - "core_sim/tests/frame_is_viewer_scoped.rs"
  - "core_sim/src/snapshot/economy.rs"
  - "core_sim/src/snapshot/knowledge.rs"
  - "core_sim/src/snapshot/campaign.rs"
  - "core_sim/src/snapshot/subsistence.rs"
  - "core_sim/src/snapshot/crafting.rs"
  - "core_sim/src/great_discovery.rs"
  - "core_sim/src/knowledge_ledger.rs"
---

# Factions — who plays the world, and who is allowed to command

`FactionRegistry` (`orders.rs`) is the answer to *"which factions does this world have, and how is
each one driven"*. Everything downstream — the turn queue's await set, the espionage rosters, the
counter-intel budgets, the security policies — is built from its `factions` list, so the registry is
the single place a world's roster is decided.

## The roster is ONE HUMAN PLUS N AI, and the player picks N

A world's roster is not authored anywhere. It is a **count of AI factions**, chosen by the player at
New Game, and the registry is built from it: `FactionId(0)` is always the human the player commands,
and ids `1..=N` are the sim's. Pick 2 and the world holds three peoples; pick 0 and it is the
single-faction world that has always shipped.

**The count is rivals, not roster size.** Naming it for what it is (`ai_faction_count`) is what keeps
the off-by-one out of every caller's head — nobody has to remember whether "2 factions" includes
themselves.

Two things fall out of that shape, and both used to be rules enforced against an authored list:

- **Ids are positional**, so a duplicate id or a gap is unrepresentable rather than validated
  against. The registry's `control` map is keyed by id; a duplicate would silently collapse two
  factions into one entry, and a gap would leave an id in `factions` with no control.
- **"Non-empty" and "at least one human" are structural.** There is always a faction 0 for worldgen
  to place, always somebody to play it, and always somebody for the turn queue to await. Nothing can
  express a world without them, so nothing has to check for one. What replaced those two checks is
  the **ceiling** below: `N >= 0` is the `u32`, and the map decides the rest.

> **The start profile no longer declares a roster.** `StartProfileOverrides.factions`, `FactionSpec`,
> `default_factions`, `validate_factions` and `faction_roster_error` are all gone. A profile that
> named who plays the world made the roster authored content a player could not reach — which is the
> whole reason it moved. A profile still stocks the opening (units, knowledge, inventory, the loadout
> window), and swapping one mid-session leaves the peoples alone. `FactionControl` stays: the
> registry still *records* how each faction is driven, it is simply derived from an id's position.

## The registry's invariant, and its one constructor

```rust
pub struct FactionRegistry {
    factions: Vec<FactionId>,
    control: BTreeMap<FactionId, FactionControl>,
}
```

**`control` is keyed by exactly the ids in `factions`.** `FactionRegistry::with_ai_factions(n)` is
the only constructor there is, and it derives *both* fields from one count, so the two cannot be
written apart; a `debug_assert!` there states the invariant. `Default` is exactly
`with_ai_factions(0)`, so "the default world" has one statement rather than two, and
`ai_faction_count()` reads the count back off the roster.
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
or something the sim drives. It still lives in `start_profile.rs` — where it was declared, and moving
it would churn every import for nothing — but nothing declares it any more: id 0 is `Human` and every
id after it is `Ai`, by construction.

## Where the roster comes from

- **`build_headless_app`** seeds the registry from `simulation_config.json`'s
  `default_ai_faction_count`, clamped against that file's own grid, before
  `TurnQueue::new(registry.factions().to_vec())`. Raising the count therefore extends the await set
  with no further edit. This is the roster a test harness and a `ResetMap` rebuild start from — the
  shipped server boots idle and builds no world at all.
- **`seed_faction_roster`** (`bin/server.rs`) seeds the registry **and everything else the boot path
  seeds from it** from the count a `new_game` carried. It has to: `rebuild_world_from_config` calls
  `build_headless_app` *first*, so the replacement app arrives holding the **config file's** roster,
  clamped against the **config file's** grid — and a `new_game` is neither of those things. See the
  table below for the set. The count it takes is expected to be already clamped, because only the
  caller knows which grid the world is being built on.
- **`apply_start_profile` does NOT touch the roster**, and no longer can: a profile declares none.
  A `set_start_profile` mid-session restocks the opening and leaves the peoples alone.
- **`ResetMap` carries the roster it had**, re-clamped to the grid it is moving to
  (`registry.ai_faction_count()` in the dispatch arm). A resize is a map reroll, not a re-pick of who
  is playing — and without the re-clamp a shrink could register more factions than the new map
  seats, while a grow would silently drop this world's rivals back to the config default.
- **`save::apply_save`** rebuilds the `TurnQueue` from the registry it has just restored, beside that
  restore rather than in the load handler. A load's replacement app is also a `build_headless_app`,
  so its queue awaits the *file's* profile; a two-faction save opened on the shipped one-faction
  profile would otherwise resolve turns without ever awaiting faction 1. The rollback path
  (`bin/server.rs`) rebuilds it from the registry for its own reason — the discarded future's
  submissions must not survive — and the two now say the same thing. **The load needs nothing
  further**: every other roster-derived resource is checkpoint state and comes back with the save.
- **`FactionRegistry::default()`** is one human faction — `FactionId(0)`, `control { 0: Human }` —
  and is what a test harness or any other construction path that never asked for rivals gets. It is
  `with_ai_factions(0)`, not a second statement of it.
- **`StartProfileOverrides` derives `Default` again.** It was hand-written for one reason — a derived
  `Default` gave `factions` an *empty* `Vec` regardless of the serde attribute — and with the roster
  gone every field's default is the empty one.

### ⛔ THE ROSTER-DERIVED SET IS FIVE RESOURCES, AND A RUNTIME PATH OWES ALL OF THEM

`build_headless_app` builds five things from `faction_registry.factions()`. Re-seeding some of them
on a roster change is the *same defect* as re-seeding none, one resource further along — and worse to
read, because the next person infers the short list is the whole list.

| Resource | `new_game` / `ResetMap` | A load |
|---|---|---|
| `FactionRegistry` | re-seeded in `seed_faction_roster` | restored — `WorldStatics` |
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
## THE MAP DECIDES THE CEILING, AND THE SIM IS THE AUTHORITY

**How many peoples a map can seat is stated once**, in `systems/worldgen.rs` beside the placement it
constrains:

| Function | Answers |
|---|---|
| `max_faction_starts(grid, min_separation)` | how many starts fit — the count of points on a lattice of that spacing, per axis |
| `faction_start_capacity(grid, min_separation, configured_default)` | the **default** to open a control on and the **ceiling** to stop it at: `max_ai_factions = starts - 1`, and the default clamped by it |
| `granted_ai_faction_count(requested, grid, min_separation)` | what a request is actually granted, warning `worldgen.ai_faction_count_clamped` with both numbers when the clamp binds |

**A lattice count, not a packing number.** Points at `0, s, 2s, …` are exactly `s` apart along an
axis and further apart diagonally, so every lattice point clears the separation — which makes this an
*achievable* count rather than an upper bound nothing could reach. It is deliberately **blind to
land**: the sea does not exist until worldgen has run, and the client asks about a grid it has not
generated yet. Where the land cannot honour it, `faction_start_tiles` relaxes and warns — that
relaxation is the safety net *beneath* this ceiling, not a competing rule.

**Clamped, never refused.** A player who asked for more rivals than the map holds still gets a game;
refusing would leave somebody who moved a slider with no world at all. Every path that takes a count
— boot, `new_game`, `ResetMap` — goes through `granted_ai_faction_count`, so there is one
implementation of *"the map decides"*.

**There is no boot panic left to write.** The count is a `u32`, so `N >= 0` is the type; the only
other rule is the clamp, and it binds at boot the same way it binds at runtime — because the grid the
config file was parsed against is not necessarily the grid a world is built on.

### How the client learns the default and the max — a QUERY, not a header field

The New Game screen needs both numbers, and it needs them **for the grid the player is choosing**,
on a screen where **there is no world**. Neither half fits a snapshot header:

- The shipped server **boots idle** and publishes no frame at all until `new_game`, so a header field
  would be unavailable on exactly the screen that needs it.
- The ceiling is a property of the map **about to be built**. A header describes the world the server
  is running, which on the New Game screen is either nothing or the previous game.

So it rides the query channel — `QueryPayload::FactionCapacity { width, height }` →
`QueryReply::FactionCapacity { default_ai_faction_count, max_ai_faction_count }` — answered by
`answer_query` **ahead of the `world_active` gate**, from the live `SimulationConfig` and the grid in
the ask. That is character for character the arrangement `list_saves` already has, and for the same
reason: a player opens the load menu before there is a world too. **The `.fbs` is unchanged**; this
is the command protocol, not the snapshot.

The point of answering it at all is that the rule must not be reimplemented in GDScript: a client
computing its own spinner bounds would be a second copy of the ceiling, free to disagree with the
clamp the server actually applies.

> **The answer is read off the config the SERVER currently holds**, while a `new_game` rebuild
> re-reads every config from disk (`config-loading.md` → staged overrides). A staged override to
> `faction_start_min_separation` between the ask and the pick would therefore move the ceiling under
> the answer. The clamp is what makes that harmless — the pick is granted down, never refused.

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

> #### Relaxation degrades GRACEFULLY, and never fails
>
> **A cramped map is a worse world, not a dead one** — a faction left unplaced has no land, no band
> and nobody to play it, which is a strictly worse outcome than two peoples starting near each
> other. So `faction_start_tiles` never refuses a faction. It picks in **two passes**:
>
> 1. among the candidates that clear `faction_start_min_separation`, **the highest score**;
> 2. when none clear it, **the candidate that maximises the minimum distance to every start already
>    placed** — score is only the tie-break, and the row-major `(y, then x)` order breaks the rest,
>    so determinism is unchanged. It warns
>    `worldgen.start_separation_relaxed=unachievable` with the faction, the target and the distance
>    actually achieved.
>
> > ⛔ **Pass 2 is NOT "take the best remaining tile", and that rule — the one this arc originally
> > shipped — was the bug.** Good ground clusters, so the highest-scoring *remaining* tile is
> > normally a **neighbour of the start that just took the highest-scoring tile outright**. The
> > moment the separation stopped being satisfiable, peoples stacked on adjacent hexes: the worst
> > possible answer for the exact case the fallback exists to handle.
> >
> > Reported from a playtest and reproduced on `map_seed 10954655273796111774` at the shipped 80×52
> > earthlike grid. The separation is *met* there at 1/2/3 rivals (33.1 / 24.4 / 23.4 tiles) — the
> > trigger is the **rival count**, because the ceiling below is land-blind and the New Game screen
> > offers up to 11 rivals on that grid. Achieved minimum pairwise distance, before → after:
> > 8 rivals **9.5 → 17.1**, 9 rivals **1.0 → 16.8**, 10 rivals **1.0 → 16.0**, 11 rivals
> > **1.0 → 15.5**. Counts at or below 7 rivals are byte-identical — pass 1 is untouched.
> >
> > Guarded by `start_tile_selection_tests::a_separation_nothing_can_satisfy_spreads_the_starts_out`
> > (a flat grid whose four relaxed picks must be its four corners) and
> > `multi_faction_start::the_playtest_map_spreads_a_full_rival_roster_instead_of_stacking_it` (the
> > reported map at a full roster). Both fail at an achieved distance of **1.00** against the old
> > rule.

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
of them needed an edit. That shape change **bumped `SAVE_FORMAT_VERSION` to 7** — see the changelog
table on that constant, where per-faction victory is the row after it.

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

## What a foreign band publishes — THREE TIERS, by what the viewer can see

A snapshot is **one viewer's view**. Herds are fog-filtered, connections are filtered on the
observer's faction and roads are gated on `Discovered` — the band list was the one collection
published whole, with no `With`, no faction predicate and no visibility test on
`PopulationSnapshotQuery`. Every connected client therefore received every band's complete internal
state: morale, larder, runway, knowledge fragments, labor assignments, equipment, bench, build
queue, reachable stockpile, outfitting window, exact position — and its **pending defection**. The
Godot client declined to *draw* most of it, which is presentation, not a boundary.

| Tier | Which band | What the frame carries |
|---|---|---|
| 1 | **your own** | the full row, unchanged in every field |
| 2 | **a foreign band standing where you can see** | a **redacted** row: `entity`, `band_id`, `faction`, `name`, `current_x`/`current_y`, `size`. Everything else at its default |
| 3 | **a foreign band anywhere else** | **no row at all** |

**Tier 2 exists rather than folding into tier 3** because the client already colours foreign markers
by faction and draws them: with no row there is nothing to draw, and rendering that works today
breaks. What it publishes is what standing on a ridge watching a stranger's camp tells you — where
they are, who they are, roughly how many. `size` comes from `cohort.size`, which is exactly the
`size` the owner's own row publishes, so an observer and the owner never disagree about how many
people are standing there.

**`entity` is on the list because it is the DELTA'S ROW KEY**, not a fact about the band:
`diff_new` keys populations by it, so omitting it would collide every redacted row onto `0` and
leave the append-only delta unable to tell two of them apart.

### The redaction is an ALLOW-LIST, not a set of deletions

`redacted_population_state` names six fields and takes **everything else from `Default`**. A row
built the other way round — full, then blanked — fails open on exactly the field nobody thought
about, and the wire is append-only, so there is always a next field. Built this way, a field added
to `PopulationCohortState` later is redacted by construction.

**A non-optional table is redacted by being DEFAULT-VALUED, not by being absent.**
`PopulationCohortState.bench` is a plain `BenchState`, so the codec writes a bench on every row and
always will; the default bench is an *idle* one — empty recipe, no crew, no progress — which says
nothing about the band. `core_sim/tests/foreign_band_redaction.rs` accordingly asserts the bench's
**content**, not its presence.

### The gate is the herd path's visibility seam, and fog is NOT a disclosure switch

Tier 2 vs tier 3 asks `VisibilityLedger::is_visible(viewer, x, y)` and short-circuits on
`config.fog_enabled` — character for character the question `HerdSnapshotInputs::herd_is_visible`
asks, so there is one notion of *"the viewer can see this"* rather than a second one free to drift.
`Active`, not `Discovered`, for the herd list's reason: ground you saw two hundred turns ago says
nothing about where a band is camped today. It **fails closed** — a band whose tile does not resolve,
and an absent faction map, both read as not-visible, matching the all-unexplored raster
`visibility_raster_from_ledger` emits in the same state.

> #### ⛔ `fog_enabled` MOVES THE LINE BETWEEN TIERS 2 AND 3, NEVER BETWEEN 1 AND 2
>
> Fog decides what you can **see**; it is not an entitlement switch. With fog off every foreign band
> gets a row — you can see where their camps are — and every one of those rows is still **redacted**.
> Wiring `fog_enabled` into the ownership branch would turn a rendering/debug convenience into a
> data-disclosure toggle, which is the failure this callout exists to name.

### What follows from redacting, downstream of the row

- **`snapshot_demographics` reports nothing for a foreign faction.** It aggregates the published
  whole-people triple, which a redacted row leaves at zero. That is the correct answer — a rival's
  age structure is not yours to read — rather than a gap.
- The sentiment / corruption / military overlays are built from the same published list, so they no
  longer carry a rival's morale or unrest either. One seam, one answer.
- `population_state` guarantees `size == children + working + elders` for a band you own. A redacted
  row deliberately publishes the **scale without the structure**, which is what looking at a camp
  from a distance gives you; that invariant is an own-band one.

**The `.fbs` did not change**, deliberately: every redacted field expresses "absent" as its existing
default on the existing table, so there is no append and no merge hazard.

## Which frame sections are viewer-scoped

⛔ **A PUBLISHED FRAME IS ONE VIEWER'S VIEW.** The band list was the first unfiltered section found,
and one unfiltered section meant the *section list* needed sweeping rather than one spelling
correcting. It did: at the time of the sweep, **thirteen** faction-keyed sections were published
whole to every client.

**The rule, per section:** the viewer's own rows in full; a rival's absent. A section whose subject
has **no thing on the map to look at** — a stockpile total, a knowledge track, a stance — has no
"visible" middle tier at all, so there is nothing to redact it down to; the band list's three tiers
apply only where the row describes something that can be *seen*.

| Frame section | Built in | Scope |
|---|---|---|
| `populations` | `snapshot/capture.rs` + `snapshot/population.rs` | **Three tiers** — see the section above |
| `demographics` | `snapshot/population.rs` | **Viewer** — derived from the redacted band list, so a foreign faction aggregates to nothing |
| `foragePatches` | `snapshot/subsistence.rs` | **The row is terrain and always rides; the IMPROVEMENT on it is viewer-scoped** — see below |
| `herds` | `snapshot/subsistence.rs` | **Fog-filtered** (`Active`), plus your own animals wherever they stand |
| `routes` | `snapshot/routes.rs` | **Fog-filtered** (`Discovered`) — a road does not wander off |
| `connections` | `snapshot/connections.rs` | **Viewer** — edges whose *observer* band is the viewer's |
| `factionInventory` | `snapshot/economy.rs` | **Viewer** |
| `sedentarization` | `snapshot/subsistence.rs` | **Viewer** |
| `intensificationKnowledge` | `snapshot/subsistence.rs` | **Viewer** |
| `craftKnowledge` | `snapshot/crafting.rs` | **Viewer** |
| `discoveryProgress` | `snapshot/knowledge.rs` | **Viewer** |
| `discoveredSites` | `snapshot/knowledge.rs` | **Viewer** — the row is whose scouts have been there, not what is on the ground |
| `greatDiscoveryProgress` | `great_discovery.rs` | **Viewer** — it carries `covert` and an ETA |
| `greatDiscoveries` | `great_discovery.rs` | **Viewer + any record flagged `publicly_deployed`** — see the exemption below |
| `greatDiscoveryTelemetry` | `great_discovery.rs` | **Viewer** — and each counter is defined as *how many rows of the list beside it*; see "A derived aggregate is faction-keyed data" |
| `knowledgeLedger` / `knowledgeTimeline` / `knowledgeMetrics` | `knowledge_ledger.rs` | **Viewer** — entries by `owner_faction`, timeline by `source_faction` (world-level lines, which carry none, are kept), metrics recomputed over the viewer's own entries |
| `commandEvents` | `snapshot/campaign.rs` | **Viewer** |
| `pendingForks` / `stanceAxes` / `voiceMedium` | `snapshot/campaign.rs` | **Viewer** |
| `openingLoadout` | `snapshot/campaign.rs` | **Viewer** — already took `viewer_faction` for its known-crafts list |
| `tiles`, the rasters, `foodModules`, `climateBands`, the catalogues (`kits`, `materials`, `recipes`, `ladderKnowledge`, `routeRungs`, `campaignProfiles`) | various | **World** — terrain and per-world constants, carrying no faction. The client fogs the map from `visibilityRaster` |
| `victory.modes[].progress` | `snapshot/campaign.rs` | **Viewer** — progress is one people's, so the frame carries the viewer's mode rows and nobody else's (`VictoryState::modes_for`) |
| `victory.winner` | `snapshot/campaign.rs` | **World** — a winner is public by definition, and it names the faction that actually achieved it |

### ⛔ A DERIVED AGGREGATE IS FACTION-KEYED DATA, EVEN WITH NO FACTION FIELD

**A count, sum, max or any/all over per-faction state launders that state into a figure that looks
world-level.** It carries no `faction`, so a sweep that enumerates sections whose *rows* are keyed by
faction walks straight past it — which is exactly what happened to `greatDiscoveryTelemetry`, whose
`totalResolved` was `ledger.records.len()`: every faction's resolved discoveries, including the ones
the viewer-scoped `greatDiscoveries` list beside it deliberately withholds. The symptom on the client
was a panel printing *"Resolved discoveries: 7"* above a list of 2.

**The rule that replaces "is it filtered": an aggregate is defined as *how many rows of the list it
summarises*.** "Filtered" passes on any number that happens to be small; agreement with the list is
the claim a reader of the panel actually depends on, and it is what a test can pin. Where a counter
summarises no published list, it is scoped by the same predicate the list would use, shared as a
named function so the two cannot drift — `discovery_reaches_viewer` and
`constellation_is_a_candidate` exist for that reason and have two callers each.

| Published aggregate | Verdict |
|---|---|
| `greatDiscoveryTelemetry.totalResolved` / `.activeConstellations` / `.pendingCandidates` | **Was world, now viewer.** All three counted across every faction |
| `knowledgeMetrics` (leak warnings/criticals, countermeasures, common knowledge) | **Viewer** — recomputed over the viewer's own ledger entries |
| `header.populationCount` | **Viewer for free** — it counts the already-filtered band list |
| `demographics` | **Viewer for free** — same reason: it aggregates the redacted band rows |
| the sentiment / corruption / military rasters | **Viewer for free** — all three are built from the filtered `population_states` |
| `openingLoadout.craftableRecipeIds` | **Viewer for free** — an `all()` over a known-crafts map already resolved for `viewer_faction` |
| `crisisTelemetry` (`modifiersActive`, `warningsActive`, `criticalsActive`, the gauges) | **World, correctly.** A crisis is an event on the map; `ActiveCrisisLedger` is not keyed by faction, and `CrisisGaugeState.band` is a *severity* band, not a band of people |
| `powerMetrics`, `header.tileCount` / `.powerCount` / `.influencerCount` | **World, correctly** — none of the underlying rows carries a faction |
| `sentiment`, `axisBias` | **World, correctly** — culture-wide axes with no per-faction storage |
| `greatDiscoveryDefinitions` | **World, correctly** — *how many constellations exist to chase* is the legitimately world-level number in this arc, and it ships as a catalogue with no faction at all |
| `victory.modes[].progress` | **Was world, now viewer.** It was the aggregate-leak category's worst case — not a filter that was missing but a *model* that was absent: progress was evaluated from the world's `SimulationMetrics` and the winner hard-coded `FactionId(0)`. Victory is per faction now (`campaign.md` → "Victory is evaluated PER FACTION"), and the published rows are the viewer's |
| `cultureLayers` / `cultureTensions` | **World, correctly.** `CultureOwner` *can* name a band — but only global, regional and tile-local layers are ever published (`capture.rs`), so no band-scoped layer reaches the wire |

### Three deliberate exemptions, and why each stays

1. **`connections` publishes an edge whose *subject* is foreign.** That is the point of a connection:
   the row exists because the viewer's band met theirs. It is already filtered on the **observer**.
2. **A trade shipment names a destination band in another faction** (`expeditionDestinationBand` on
   the party's own row). A cross-faction shipment is a thing you deliberately sent; withholding its
   destination would break the verb.
3. **`greatDiscoveries` carries a rival's `publicly_deployed` record.** `mark_public` is a live
   mutator and the flag's whole meaning is *"this faction has shown the world"* — withholding such a
   record would leave the flag observable only to its owner, the one reader it is not for. A
   discovery kept quiet stays quiet, and `greatDiscoveryProgress` takes **no** such exemption.

### The improvement on a tile follows the ground

A `foragePatch` row is a fact about a **tile**, and tiles are published whole — so the row always
rides, and its ecology half (biomass, capacity, phase, composition) is world-visible like the terrain
it describes. What is *not* a fact about the tile is the improvement standing on it: `owner`,
`isCultivated` / `cultivationProgress`, `isField` / `fieldProgress`. Those are a fact about a people,
and before the sweep every client got them for every patch on the map — a live readout of exactly
which ground a rival was farming.

They are legible where the viewer's own hand built them, and where a rival's are on ground the viewer
has **explored** — `is_discovered`, not `is_visible`, on `route_states`' precedent: **a field is
built into the ground and does not wander off**, so having seen it once remains true. (A herd is the
opposite case and uses `Active`.) The staleness that buys — a field that has since gone feral still
reading as a field — is the staleness a remembered road already carries.

> **`hasOwner` is why "unowned" and "owned by faction 0" are different readings.** The wire carries a
> presence bit beside `owner:uint`, because faction 0 is a real faction and `owner == 0` cannot mean
> *no owner*. A reader testing `owner != 0` silently drops every patch the first faction tends.

### What the sweep could not observe, and why that is not a gap

`pendingForks` / `stanceAxes` / `voiceMedium` are filtered, and the filter is **currently
unobservable**: the Telling samples for *the registry's lowest id* (the tripwire below), so no other
faction has ever had a row for the filter to drop. The filter is in place ahead of that tripwire
being cleared rather than after it.

### The delta is safe under filtering, for two different reasons

- **`diff_appended`** (the event feed) ships rows with `seq > cursor` and advances the cursor to the
  highest seq it *shipped*, so the gaps a filter leaves in the sequence are rows that were never the
  client's to hold.
- **The indexed diffs** key on `(faction, id)` and several carry no `removed_*` list. That is safe
  here because a viewer never changes mid-session, so a filtered row never transitions from present
  to absent — the state that would strand a stale row on the client.

## The single-faction assumptions that are still live

Two places still read a one-faction world into a roster that may hold more. **Victory used to be a
third and is not any more** — it is evaluated per faction and the winner is whoever achieved it; see
`campaign.md` → "Victory is evaluated PER FACTION".

| Site | What it assumes |
|---|---|
| `telling/mod.rs` (signal sampling) | Takes the registry's **lowest id** as "the player" and filters every band view to it; its own comment says there is no `player_faction` accessor. |
| `visibility.rs` `ViewerFaction` | A single **global** resource read by `snapshot/capture.rs`, so one snapshot is captured and broadcast to every connected client. The band filter above made it *load-bearing* rather than merely limiting: the one captured frame is now redacted for everyone who is not `ViewerFaction`, so a second connected human sees their own people as a foreign band. One frame per viewer is what that needs, not a wider filter. |

## The two-faction fixture

`core_sim/tests/faction_support/mod.rs` builds both arms of every comparison — `one_faction_world()`
and `two_faction_world()`, plus `world_with(ai_factions, tune)` for a world that also needs a config
edit (`NO_RIVALS` / `ONE_RIVAL` name the two counts). **The roster is installed before the first
`update()`**, which is when `Startup` and therefore `spawn_initial_world` run; that is what makes
worldgen place two factions without editing a shipped config file or setting a process-global env var
a parallel test would race on. It rebuilds the
`TurnQueue` from the roster too, for the reason the roster-derived-set table above gives.

The control arm is not optional: it is what distinguishes *"the second faction got its own"* from
*"the first faction got it twice"* and from *"nothing changed at all"*.

## Config files

| File | Key | Default | Purpose |
|---|---|---|---|
| `src/data/simulation_config.json` | `default_ai_faction_count` | **0** | **How many AI factions a world gets when nobody picked a number** — the boot roster, the roster a `ResetMap` rebuild carries when it has none, and the value the New Game screen is offered as its default. It counts **rivals, not the roster**: 0 is the single-faction world, 2 is three peoples. Shipped at 0 because the AI that would drive a rival does not exist yet — a higher default would put peoples on the map that sit and pass. Clamped by the ceiling above, never refused |
| `src/data/simulation_config.json` | `faction_start_min_separation` | **20** tiles | How far apart worldgen tries to put two factions' start tiles — a quarter of the shipped map's width, far enough that two peoples do not open sharing one food shed. Euclidean, compared squared. A **target**: on a map with no land pair that far apart, worldgen relaxes and warns rather than failing to place a faction. Validated `> 0` at parse (`ZeroFactionStartMinSeparation`), because zero would let two peoples open on the same hex |

## Saves win over profile edits

`FactionRegistry` is **ground truth in the save** — a field of `WorldStatics`, captured whole and
restored whole. The AI count, by contrast, is a *request*: `simulation_config.json`'s
`default_ai_faction_count` is re-read on every world build, and the pick that built this world is not
recorded anywhere except in the registry itself.

So the two can drift, and the rule is that **the save's registry wins**: a world loaded from a save
has the roster it was created with, whatever the config default says now. That is the same reason the
rest of `WorldStatics` is saved rather than recomputed — re-deriving a world's ground truth from
tuning that has moved produces a *different world*.

## A free slot

`StartProfileOverrides::ai_profile_overrides` is parsed and read by nothing. It is where per-faction
AI behaviour tuning would attach when there is an AI to tune.
