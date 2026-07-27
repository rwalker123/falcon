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

| ring | depth | holds | used for |
|---|---|---|---|
| `CheckpointHistory` | `checkpoint_history_turns / checkpoint_interval` | `SimState` | rebuilding the world |
| `SnapshotHistory` | `PUBLICATION_RING_DEPTH` = **1** | the latest `WorldSnapshot` | export, resync, the live delta baseline |

It briefly was two rings of whole worlds — a checkpoint ring beside a 256-deep snapshot ring — and
that is worth knowing because it is what got the snapshot ring measured for the first time: **1.68 GB
resident at 80×52**, a cost it had always carried. The client view does not need archiving, because
it is *derivable*: `handle_rollback` restores the checkpoint and **recaptures** the frame from the
restored world, which `a_rollback_produces_the_world_that_tick_had` asserts directly. Deriving it
also removes the failure mode where two histories disagree about the same tick — there is no second
history to disagree.

`CheckpointHistory` deliberately lives on the **turn thread**, not in the publisher's ring. A
`SimState` is a full world clone and is never published; putting it in a `StoredSnapshot` would send
the largest object in the system across the publisher channel every turn to a consumer that never
reads it, re-adding the per-turn publisher cost that moving publication off the turn thread removed.

## The three construction rules

**No `Entity` crosses a checkpoint.** Restoring despawns and respawns everything, so bevy hands back
fresh generations and `Entity::to_bits()` names a different thing before and after. Every reference
is a stable sim id: tiles and settlements by `(x, y)`, logistics links by their endpoint pair, power
nodes by `y * width + x`, bands by `BandId`. Cloned components have their `Entity` fields set to
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

A component's *existence* is state. Worldgen spawns `LogisticsLink` bare, and `capture_snapshot`'s
query asks for `(&LogisticsLink, &TradeLink)` — so a link without a `TradeLink` is invisible to the
published `logistics` section entirely. A restore that helpfully inserted a default `TradeLink` made
**728 links appear on the wire that the original world never published**. `LinkRecord::trade` and
`BandRecord::labor` are `Option` for that reason.

## Ordering a simulation by entity allocation order is a bug

`simulate_logistics` sorted its links by `Entity::to_bits()` and then moved mass along them in that
order — a chain, where a later link moves what an earlier one already moved. Entity ids are stable
*within one process run*, which is why the forward-determinism tests never saw it, and are
renumbered by every restore. It sorts on endpoint positions now, the same natural key the checkpoint
stores links under.

The change moved **no** world: worldgen spawns links in `(y, x)` order against sequential entity
allocation, so the two orderings coincide until a renumber. The orders differ only in the case that
was already broken.

## A ring of 256 full worlds was the shape of the thing all along

`SnapshotHistory` kept 256 full `WorldSnapshot`s from long before checkpoints existed. Nobody had
put a number on it. When `CheckpointHistory` added a second ring of the same depth, the measurement
that followed found **both**: 1.68 GB for the snapshot ring, 1.50 GB for the checkpoint ring, on an
80×52 map at the standard recipe. The arc did not create the memory problem — it added a second one
the same size and caused the first to be measured.

Measured RSS, release, 300 turns so the rings are full:

| grid | snapshot ring 256-deep | + checkpoint ring | after collapsing the snapshot ring |
|---|---|---|---|
| 40×26 | 0.48 GB | 0.92 GB | 0.46 GB |
| 80×52 | 1.68 GB | 3.18 GB | **1.68 GB** |

Both rings are **linear in tile count** (3.5× for 4× the tiles), so the exponent, not the current
figure, is what governs. Measured on the shipped configuration: **0.18 GB at 80×52 and 0.61 GB at
160×104** — 3.4× for 4× the tiles, the same slope.

The pre-fix figure at 160×104 is not in this table because it could not be run: two 256-deep rings
of whole worlds at that size exceeded available memory, which is a more useful statement about the
old shape than any ratio would have been.

The snapshot ring collapsed to a single entry because rollback was its only historical reader, and
that read was redundant — recapturing the client frame from the restored world yields the same
bytes, which `a_rollback_produces_the_world_that_tick_had` asserts. `checkpoint_history_turns` now
governs `CheckpointHistory` alone: **one depth knob, one history of worlds.** Net resident memory is
within 0.4% of where the arc started, with a checkpoint ring where a snapshot ring used to be.

**The transferable part is not either figure.** It is that a per-turn ring of whole worlds is an
easy thing to write and an invisible thing to pay for, and it went unmeasured for as long as it did
because nothing in the system reported a total. Estimating it structurally is not a substitute —
`size_of` does not see the `Vec` and `HashMap` heap that is most of an entry, and a structural
estimate of this ring was wrong by 5× in the direction that mattered.

## Checkpoints are sparse, and a rollback replays the gap

`SimulationConfig::checkpoint_interval` sets how often `record_checkpoint` runs, and
`checkpoint_history_turns` sets how far back a rollback can reach — a window in **turns**, so the
number of entries is the one divided by the other and raising the interval buys memory without
shortening the reach. A rollback restores the newest checkpoint **at or before** the target tick and
replays forward to it, which is exact rather than approximate — that is what
`a_restored_world_simulates_forward_identically` proves of a restored world, and what
`rolling_back_to_a_non_checkpoint_tick_reproduces_that_tick` proves end to end through the rollback
path itself.

Measured on the standard recipe, 300 turns so the ring is full:

| interval | ring RSS | entries | worst-case replay |
|---|---|---|---|
| 1 | 1.79 GB | 256 | 0 turns |
| 4 | 0.51 GB | 64 | 3 turns — 2.6 ms |
| 8 | 0.28 GB | 32 | 7 turns — 6.0 ms |
| **16 (default)** | **0.18 GB** | **16** | **15 turns — 12.9 ms** |
| 32 | 0.11 GB | 8 | 31 turns — 26.7 ms |

A replayed turn costs **0.86 ms** against a normal turn's **4.61 ms**, because the two systems replay
gates off — `capture_snapshot` and `record_checkpoint` — are most of a turn's cost. So a rollback is
cheaper than the turns it undoes at any interval on this table. 16 is the knee: 90% of the memory
gone, and doubling again buys 0.07 GB for another 13.8 ms.

## Replay may only cross turns. Anything that is not a turn forces a checkpoint

Replay-forward reproduces the original world **only if every step between the checkpoint and the
target is a turn** — something `run_turn` can re-execute from world state alone. Two things are not
turns, and both would otherwise be silently skipped:

- **World-mutating commands** mutate between turns. With checkpoints at 16 and 32, a player
  assigning labor at tick 20 and rolling back to 25 would restore 16 and replay forward *without the
  assignment*, producing a world that never existed.
- **Config reloads** re-read JSON at runtime, and a checkpoint deliberately carries no config, so a
  replay across one would run turns under whatever tuning is live rather than that tick's.

Both are closed the same way: `recapture_and_broadcast` — the one seam **every** dispatched command
already passes through — takes a checkpoint before it recaptures. `CheckpointHistory::record`
replaces any checkpoint already at that tick, because the post-command state is what "the world at
tick T" means once a command has landed there. So no unreproducible event can sit between a
checkpoint and a rollback target, and replay-forward is exact **by construction rather than by
assumption**.

It is cheap for the reason sparse checkpointing was worth doing at all: commands are **human-paced**,
so these scale with player actions rather than with turns.

**Checkpointing is on the uniform seam, not a curated list of mutating commands.** That is the same
judgment `recapture_and_broadcast` already made for the same reason — a hand-maintained "which
commands mutate" list is a thing to forget, and re-checkpointing a genuinely non-mutating command is
merely slightly wasteful.

**This defect shipped, briefly, and the reason no oracle caught it is worth keeping.** All five
oracles drive the world with `app.update()` and issue **zero commands** — exactly the case where
replay-forward is trivially correct. A test that never exercises the thing that breaks an invariant
cannot report the invariant broken, however thoroughly it exercises everything else.
`a_rollback_across_a_command_reproduces_the_world_that_tick_had` (in `bin/server.rs`, where the
command handlers live) is the one that would have.

**What replay must not do**, and what `Replaying` exists to prevent: republish frames the client has
already applied, and push entries into the ring the rollback is rewinding — a rollback that grew its
own history could not terminate. `collect_metrics` and `advance_tick` share that stage and are
deliberately **not** gated: the tick has to advance, and `SimulationMetrics` is checkpoint state the
next turn reads.

## Reading this arc's numbers

`integration_tests/tests/replay_determinism.rs` reports **differing leaves out of compared leaves**,
per field-path group, plus a separate count of leaves it could *not* compare (present on one side
only, a length mismatch, `null` against a value). A bare difference count is unreadable: it moves
when correctness changes and when the set of comparable leaves changes, and one integer cannot tell
those apart.

**Restore-loss is the headline metric.** It asks a structural question — did restoring reproduce the
checkpoint — whose answer does not depend on which world is running, so it is comparable across any
two commits.

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
`Expedition` and `TradeLink` have no instances in a fresh world and an archetype walk would miss
exactly the state a rollback is most likely to drop.

The world-static bucket's reason carries an expiry: those resources survive a rollback only because
a restore rebuilds into the same live `World`, which still holds the map worldgen built. That stops
being true the day a checkpoint becomes a save file loaded into a fresh process.
