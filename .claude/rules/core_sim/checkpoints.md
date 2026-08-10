---
paths:
  - "core_sim/src/sim_state.rs"
  - "core_sim/src/snapshot/capture.rs"
  - "core_sim/tests/sim_state_coverage.rs"
  - "integration_tests/tests/replay_determinism.rs"
  - "integration_tests/tests/determinism.rs"
---

# Checkpoints: what a rollback restores, and why it is not the snapshot

## `WorldSnapshot` is the client view. `SimState` is the save state.

`WorldSnapshot` is shaped by what a player needs to see: herds filtered through fog, derived
rasters, display-only forecast fields. `SimState` (`core_sim/src/sim_state.rs`) is shaped by what a
turn reads.

For most of this codebase's life one type did both jobs, and rollback restored from the client
view. That is the defect the checkpoint arc existed to remove. Thirteen mutable resources had no
representation in `WorldSnapshot` at all — `ActiveCrisisLedger`, `PowerTopology`, the espionage set
— because no client ever needed them, so a rollback silently dropped them and the world diverged
from the one it claimed to restore. **Nothing failed when they were omitted**, which is why it
lasted: the omission was invisible to every test that existed.

**There is one history of worlds, and it holds `SimState`.**

| holds | depth | used for |
|---|---|---|
| `CommandLog` | one origin `SimState` + every event since | rebuilding the world |
| `SnapshotHistory` | `PUBLICATION_RING_DEPTH` = **1** | export, resync, the live delta baseline |

It briefly was two rings of whole worlds — a checkpoint ring beside a 256-deep snapshot ring — and
that is worth knowing because it is what got the snapshot ring measured for the first time: **1.68 GB
resident at 80×52**, a cost it had always carried. The client view does not need archiving, because
it is *derivable*: `handle_rollback` rebuilds the world and **recaptures** the frame from it, so
there is no second history to disagree with the first.

### The client view carries no save state at all any more

`WorldSnapshot` used to carry four collections that were simulation records rather than anything a
player sees — `herd_registry`, `forage_registry`, `graze_registry` and `beat_ledger` — captured
every turn so the old restore-from-the-client-view path had authoritative state the fog-filtered
display telemetry had lost. Restore reads `SimState` now, which carries all four whole
(`SimState::{herds, forage, graze, beat_ledger}`), so the copies on the snapshot had **no reader
left**. They are deleted. The rule the `WorldSnapshot` doc comment now states — *every field is
there because something downstream renders or exports it* — has no exceptions.

**So no field of a `Herd`, `ForagePatch` or `GrazePatch` is exempt from a rollback** — the registries
are cloned whole, field by field, with no per-field opt-out anywhere. A field comment therefore has
nothing to say about persistence, and the dozen that claimed "not snapshot-persisted" were describing
the retired restore path, which rebuilt registries from `*State` records that omitted exactly those
fields. The axis that *does* vary per field is **on the client wire or not**, which is a different
question with a different answer; that is what those comments say now. `pen_starving`,
`tamed_this_turn`, `pen_fed_fraction`, `fodder_delivery_rate` and `footprint_intake` are the ones a
reader is most likely to have been misled by: each is transient *within* a turn — recomputed by a named
system — and each survives a restore verbatim, so an edge-gated notice does not re-fire and a starving
pen does not read as fed.

**The band half is the same story, and it is the larger family.** `BandRecord` stores
`cohort: cohort.clone()` and `labor: entity.get::<LaborAllocation>().cloned()`, so **every**
`PopulationCohort` and `LaborAllocation` field rewinds too — the per-turn readouts
(`last_food_consumption`, `last_morale_delta` / `_cause` / `_contributions`,
`last_fertility_factors`, `discontent_fraction`, `last_emigrated` / `last_immigrated`) and the labor
telemetry (`last_yields`, `last_pen_feed_upkeep`, `last_raid_forfeit`) alike, plus `BandTravel`,
which is why a restored band resumes its move rather than cancelling it. Two things this retires:

- **The old restore rebuilt a `LaborAllocation` from its `*State` record**, via a
  `labor_allocation_from_state` that no longer exists, keeping only the assignments — which is where
  "a rehydrated cohort reads `0` until the next tick" came from. A restored allocation carries its
  telemetry now. The **no-data sentinels are still live and still needed** (`SourceYield::ZERO` for
  an unresolved row, `FertilityFactors`' all-zero NOT-PROJECTED reading, `band_food_flow`'s `None`),
  but what they answer for is a band that has *not yet been through a turn*, not a restored one.
- **`LaborAllocation`'s manual `PartialEq` still compares assignments only.** That is about intent
  versus telemetry in a comparison, not about persistence, and it is unaffected.

**So the assignment's own state rewinds without a per-field record, and that is why the harvest
floor needed no checkpoint work.** `LaborTarget::{Forage,Hunt}` carries a `floor: f32` — the whole of
what the player decides about harvest pressure (`docs/plan_harvest_floor.md`) — and it rides the
cloned component like the tile, the herd id, the crop selection and the crew. `LaborAssignmentState`
is **the client wire projection, not this record**: it is what `WorldSnapshot` ships, so its fields
are append-only, and nothing in the restore path reads it.
`integration_tests/tests/harvest_floor_rollback.rs` pins the rewind at floors no stance names, so a
rewind that quietly defaulted would be caught rather than landing on a plausible value.

The one genuinely non-persisted member of this neighbourhood is `SupplyNetworkMembership`, a
resource keyed by `Entity` that `balance_supply_networks` rebuilds every turn — no `Entity` crosses a
checkpoint, so it could not be carried even if it wanted to be.

Three things this is worth knowing for:

- **The cost was per turn and map-sized.** `forage_registry` and `graze_registry` were collected
  from their `HashMap`s and **sorted by `(y, x)`** so a rollback would see a deterministic order —
  2113 and 1650 entries on the standard 80×52 `earthlike` world, each an allocating `*State` with
  a `String` ecology phase. Interleaved A/B, 5 pairs, standard recipe (release, `map_seed` pinned,
  mean of 30 frames after 5 warm-ups, publisher shut down): **`snapshot.build` 3.170 → 3.054 ms**
  and **`run_turn` 4.660 → 4.501 ms**, distributions non-overlapping. Interleaving is not optional
  here — sequential before/after batches on this machine drift by more than the effect.
- **None of the four was ever on the wire**, so this was not a schema change: no `snapshot.fbs`
  slot, no client work, and `cargo xtask decode-guard` passes against the **unchanged** golden.
  `decode_fixture.rs`'s `OFF_WIRE_SUBTREES` gate is correspondingly back to meaning one thing —
  *encoded but not decoded* — instead of two.
- **The one reader was a test**, `integration_tests/tests/fauna_fog.rs`'s
  `the_rollback_record_keeps_every_herd_the_display_list_hides`, which used
  `snapshot.herd_registry.len()` as the unfiltered count that proves the published `herds` list is a
  strict subset. It reads `capture_sim_state(&world).herds` instead, which makes the claim in its
  own name literal rather than incidental.

**`export_map` consequently exports the player's view, not ground truth.** The `MapExport` JSON
wraps a `WorldSnapshot`, so its `snapshot.herds` is the fog-filtered display list and there is no
longer an unfiltered roster beside it. Anything offline that wants every herd needs a checkpoint,
not an export.

## The three construction rules

**No `Entity` crosses a checkpoint.** Restoring despawns and respawns everything, so bevy hands back
fresh generations and `Entity::to_bits()` names a different thing before and after. Every reference
is a stable sim id: tiles and settlements by `(x, y)`, power nodes by `y * width + x`, bands by
`BandId`. Cloned components have their `Entity` fields set to
`Entity::PLACEHOLDER` at capture, so a stale one cannot be read by accident rather than merely
should not be.

**No config.** Three sim-state types hold configuration, and `checkpoint()` on each leaves it
behind: `InfluentialRoster.config` and `KnowledgeLedger.config` behind `Arc`s, and
`CultureManager.settings` **by value** — which a search for `Arc<*Config>` does not find. Cloning
them whole would capture the tuning that was live when the checkpoint was taken, so a rollback would
silently reinstall it: hot-reload a config, roll back, and the reload is undone with nothing logged.
Restore re-attaches whatever config is live by leaving the field alone.

**Capture is a pure function of the world.** `capture_sim_state(&World)` reads nothing else — no
change detection, no retained deltas, no assumption it ran last turn. That is what keeps
"materialize a checkpoint every Nth turn" a scheduling change rather than a rewrite.

## "Derived" is only safe if nothing publishes the value before the system that rebuilds it next runs

The tempting shortcut is to leave a resource out of the checkpoint because some system recomputes it
each turn. That holds only when nothing reads it in between.

`SimulationMetrics`, `PowerGridState` and `HerdTelemetry` all fail the test: `capture_snapshot`
reads `SimulationMetrics.crisis` for the published crisis telemetry, `PowerGridState` for
`power_metrics`, and `HerdTelemetry` for the display herd list — all in the same turn, all written
by systems that will not have run again by the time a restored world is next captured. They are
carried.

**`HerdTelemetry` is the worked example**, because it produced a *plausible wrong number* rather
than an obviously stale one. It is a mid-system snapshot of herd biomass, not a pure function of
`HerdRegistry`, so rebuilding it from the registry — which is what a reviewer would wave through —
yields a number that is close, well-formed, and different. Only a bit-exact oracle catches that.

## Capture records component presence, not only values

A component's *existence* is state. The worked example was `TradeLink`: `capture_snapshot`'s query
asked for `(&LogisticsLink, &TradeLink)`, so a bare link was invisible to the published `logistics`
section entirely, and a restore that helpfully inserted a default `TradeLink` made **728 links
appear on the wire that the original world never published**. Both components were demolished with
the dead trade slice (`docs/plan_contact_and_logistics.md` §As-built) and the `LinkRecord` that
carried them is gone, but the rule outlives them: `BandRecord::labor` is an `Option` for exactly
this reason, and so is every future record whose component a system attaches conditionally.

## Ordering a simulation by entity allocation order is a bug

The since-deleted `simulate_logistics` sorted its links by `Entity::to_bits()` and then moved mass
along them in that order — a chain, where a later link moves what an earlier one already moved.
Entity ids are stable *within one process run*, which is why the forward-determinism tests never saw
it, and are renumbered by every restore. The fix was to sort on the endpoint positions, the same
natural key the checkpoint stored the links under.

The system itself is gone (§As-built above), and the change had moved **no** world when it landed:
worldgen spawned links in `(y, x)` order against sequential entity allocation, so the two orderings
coincided until a renumber. **The rule is what to keep** — any order-dependent walk over entities
sorts on a natural key the checkpoint also stores, never on allocation order.

## A ring of 256 full worlds was the shape of the thing all along

`SnapshotHistory` kept 256 full `WorldSnapshot`s from long before checkpoints existed. Nobody had
put a number on it. When a checkpoint ring of the same depth was added beside it, the measurement
that followed found **both**: 1.68 GB for the snapshot ring, 1.50 GB for the checkpoint ring, on an
80×52 map at the standard recipe. The arc did not create the memory problem — it added a second one
the same size and caused the first to be measured.

Measured RSS, release, 300 turns so the rings are full:

| grid | snapshot ring 256-deep | + checkpoint ring | after collapsing the snapshot ring |
|---|---|---|---|
| 40×26 | 0.48 GB | 0.92 GB | 0.46 GB |
| 80×52 | 1.68 GB | 3.18 GB | **1.68 GB** |

Both rings were **linear in tile count** (3.5× for 4× the tiles), which is the part that governed:
the figure at any one map size mattered less than the fact that it multiplied with the map. The
pre-fix figure at 160×104 is absent because it could not be run at all — two 256-deep rings of whole
worlds at that size exceeded available memory, which says more about the old shape than a ratio
would.

**Every number above describes designs that no longer exist**, and they are kept only for the lesson
below. Sparse checkpointing brought the ring to 0.18 GB before the ring itself was deleted; neither
figure describes what ships.

The snapshot ring collapsed to a single entry because rollback was its only historical reader, and
that read was redundant — recapturing the client frame from the restored world yields the same bytes.
Both rings of whole worlds are now gone entirely: the log replaced the checkpoint ring, so what
remains resident is one origin `SimState` plus a few hundred bytes per command.

**The transferable part is not either figure.** It is that a per-turn ring of whole worlds is an
easy thing to write and an invisible thing to pay for, and it went unmeasured for as long as it did
because nothing in the system reported a total. Estimating it structurally is not a substitute —
`size_of` does not see the `Vec` and `HashMap` heap that is most of an entry, and a structural
estimate of this ring was wrong by 5× in the direction that mattered.

## Rollback replays a command log; `SimState` is the save format

**A checkpoint is a cache. The thing it caches is the log.** The world at tick N is a pure function
of `(origin) + (the ordered commands and turn boundaries since)`, so the log is the authority and a
materialized world is only ever an optimisation over it. `CommandLog` (`bin/server.rs`) holds an
origin `SimState`, the tick it was captured at, and a `Vec<LogEntry>` of `Turn` and `Command`.

Building the cache without its authority produced three defects in a row, and they are worth keeping
because they all have the same root: a replay that skipped commands entirely; a per-command
checkpoint bolted on to paper over it; and a ring of `history_turns / interval` = 16 slots with two
producers, where a player issuing commands across 16 ticks silently cut a 256-turn window down to
"the last few things you touched". No test caught the last one because the tests issued no commands.

`SimState` keeps its job unchanged — it is the **save-game format**, materialized when the origin is
captured or re-based, never on a cadence.

**"The world at tick N" means immediately after the Nth turn resolved**, before any command issued
while sitting at that tick. A command lands *between* turns, so the phrase is otherwise ambiguous and
the two readings give different worlds.

### Three things re-base the origin instead of being logged

`new_game`, `reset_map`, and **any config reload**. Each captures a fresh `SimState` as the new
origin and clears the log, saying so in the log line, because nothing before that point is reachable
any more.

Config reload is the deliberate answer to a hole this arc flagged early: a `SimState` carries no
config **by design**, so replaying across a reload would run turns under whatever tuning is live
rather than the tuning of that tick. Re-basing is consistent with that decision and needs no config
serialization at all.

## No `Entity` crosses a persistence boundary — and the log is the second one

`SimState` has obeyed this since it was built. The **wire** did not, and the log inherited it: a
command carrying `band_entity_bits` names an entity that restoring the origin has renumbered, so a
replayed command resolved to nothing and did nothing — silently.

An `Entity` is an ECS allocation detail, stable only within one process run of one world. It survived
on the wire this long because the client re-reads `entity` from every frame, so its handles refresh
before anything observes the staleness. A log has no such healing: the handles inside it are frozen
at capture time and the world underneath them is not.

**So the commands themselves carry stable ids, and the log stores them unchanged.** Every
entity-bearing field, audited rather than fixed one at a time:

| command | was | now |
|---|---|---|
| `AssignLabor`, `MoveBand`, `SendExpedition`, `SendHuntExpedition`, `CancelOrder` | `band_entity_bits` | `band_id` (`BandId`) |
| `RecallExpedition` | `expedition_entity_bits` | `expedition_band_id` |
| `Heat` | `entity_bits` | `target_x` / `target_y`, via `TileRegistry` |

Nothing had to be invented — every handle already had a stable id, which is what made this cheap.
`PopulationCohortState.bandId` carries the id to the client so it has something to send back.

The first attempt normalised **at the log boundary** instead: bits → `BandId` when logging, back
when replaying. That works and is self-contained, but it is machinery whose only purpose is to
compensate for the wire leaking a handle — fixing the leak deletes all of it, and with no deployed
clients the protocol change is free now and dearer later. **Prefer fixing the boundary that leaks
over translating at the boundary that suffers.**

Where a `BandId` cannot be resolved it **fails loudly** at dispatch, which covers live commands as
well as replayed ones. A silently-dropped command is the defect that started this rework and must not
return as a silently-dropped replay.

## Replay reproduces a turn only if everything the turn reads is either restored or replayed

Two worked examples, both found by the oracle rather than by reading:

- **A turn is not a pure function of `SimState`.** `resolve_ready_turn` reads `TurnQueue`, which is
  server-side order intake and deliberately not checkpoint state — and it **skips entirely** when the
  queue is not ready. A naive `LogEntry::Turn` therefore replayed fewer turns than happened and
  landed on the wrong tick. One `resolve_turn_with_auto_orders` serves the live path and replay, so a
  `Turn` entry reproduces what a turn actually did.
- **The order queue is reset with the origin.** It is refilled by the log's own `Orders` entries, so
  it must start empty; otherwise a replayed turn sees orders that had not been submitted yet.

`Replaying` is what stops replay from publishing frames the client already applied, or appending to
the log it is replaying. It is checked inside `recapture_snapshot_in_place` rather than at call
sites, because `run_system_once` bypasses the schedule's run conditions.

## Latency, and when checkpoints come back

Replay cost grows with distance from the origin: a replayed turn is ~0.86 ms against a normal turn's
~4.61 ms, so a thousand turns is under a second, and a rollback is a human-paced action. A command is
a few hundred bytes against a full world clone, so log size is negligible beside what the ring cost.

If that latency ever matters, checkpoints return **as a cache over an authoritative log** — which is
safe in a way the ring-only design was not, because the log remains the thing that says what
happened and a checkpoint is only ever an answer it agrees with.

## Reading this arc's numbers

`integration_tests/tests/replay_determinism.rs` reports **differing leaves out of compared leaves**,
per field-path group, plus a separate count of leaves it could *not* compare (present on one side
only, a length mismatch, `null` against a value). A bare difference count is unreadable: it moves
when correctness changes and when the set of comparable leaves changes, and one integer cannot tell
those apart.

**Restore-loss is the headline metric.** It asks a structural question — did restoring reproduce the
checkpoint — whose answer does not depend on which world is running, so it is comparable across any
two commits.

**A dead field cannot diverge, so restore-loss also improves when a feature breaks.** The metric
compares the checkpoint against the restore; a field that is uniformly empty on *both* sides scores
as a perfect restore. The worked example is `bcf993f`, which re-keyed local culture layers from
entity bits to position and recorded exactly 768 leaves recovered — `tiles[].culture_layer` (384)
and `culture_raster` (384). Re-keying the layers was right and did fix their orphaning, but the two
snapshot **readers** were left asking for `CultureOwner(tile.entity)` against a map now keyed by
position, so both fields shipped uniformly zero from that commit until #407 fixed the readers. Part
of that 768 was the fields ceasing to exist rather than the fields being restored. **A restore-loss
improvement is evidence about structure, not about a feature working** — pair it with an assertion
that the field carries a value at all, which is what nothing had for these two.

**Replay divergence is only comparable across commits that do not change simulation behaviour.** A
change that alters the world's trajectory produces a different world, and two replay figures then
measure different simulations. This is not a small caveat: an RNG change during the arc moved replay
from 9,651 to 12,202 differing leaves against an unchanged denominator, which looked like a 26%
regression and was a different world.

## Serialization is absent, and these are the facts about adding it

Nothing in `SimState` derives `Serialize`. The checkpoint is an in-memory `Clone`, which is what an
in-process rollback needs.

- **17 sim-state maps are keyed by `FactionId`, `UVec2` or tuples.** `serde_json` admits only string
  keys, and JSON is the repo's only serde codec today (`sim_schema`).
- **The sim-state closure is 119 types and contains no trait objects, function pointers, closures,
  raw pointers, interior mutability, lock types or manual `Drop` impls.** The only constructs serde
  could not derive through were a `SmallRng` — deleted; the influencer roster draws from derived
  seeds now, like every other RNG consumer — and `Entity`, which the first construction rule
  removes.

## Omission fails a test, not a rollback

`core_sim/tests/sim_state_coverage.rs` enumerates the resources and components a **built app holds
at runtime** — `world.storages().resources` and `world.components()`, never a hand-written list —
and asserts each is classified: checkpoint state, derived (naming the rebuilding system),
world-static, infrastructure, or config. Adding a `Resource` or a `Component` fails that test until
someone decides which it is. It asserts the reverse too, so a table entry naming a type that no
longer exists fails rather than silently excusing nothing.

**The tables are also asserted pairwise disjoint**, which is not tidiness. They are unioned into a
`BTreeSet` before being checked against the runtime, and a set cannot report that it absorbed a
name twice — so a resource listed in two buckets is covered by *neither*: delete it from either one
and every other assertion in the file still passes. That is a hole shaped exactly like the omission
the guard exists to catch, and it opened where this document says the danger is: `HerdTelemetry`,
`PowerGridState` and `SimulationMetrics` — the three worked examples of the derived/state
distinction above — sat in `SIM_STATE_RESOURCES` and `DERIVED_RESOURCES` at once.

**Its scope is the library's resources, which is narrower than "omission" makes it sound.** The app
it walks is `build_headless_app`, so anything the `server` **binary** inserts is invisible to it —
today `ResolvedPortBase`, `ConfigWatcherRegistry`, `CommandSenderResource` and `CommandLog`. Naming
a server-side resource in one of the tables makes the test fail as *stale*, not pass as classified.
That boundary is real and worth knowing before trusting the guard as a general omission check.

It matters less than it looks for the one that carries rollback. The property `CommandLog` needs is
*every command is logged*, and that is guaranteed **structurally, by the single uniform dispatch
seam**: a new command variant is logged whether or not anyone remembers it exists. A coverage table
would be the weaker guarantee — it catches a forgotten *type*, where the seam catches a forgotten
*case*.

The component half is not optional. `PowerNode.base_generation` / `.base_demand` is component-level
omission that had already happened — restore set `base = current`, so the next turn re-applied
modifiers to an already-modified base — and a resource-only guard would have missed the very bug
that motivated the guard. Registered components are walked rather than live ones, because
`Expedition` has no instances in a fresh world and an archetype walk would miss exactly the state a
rollback is most likely to drop.

The world-static bucket's reason carries an expiry: those resources survive a rollback only because
a restore rebuilds into the same live `World`, which still holds the map worldgen built. That stops
being true the day a checkpoint becomes a save file loaded into a fresh process.
