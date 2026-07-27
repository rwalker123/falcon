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

Two rings, two jobs, read at the same tick by `handle_rollback`:

| ring | holds | used for |
|---|---|---|
| `CheckpointHistory` | `SimState` per turn | rebuilding the world |
| `SnapshotHistory` | `WorldSnapshot` per turn | re-baselining the client |

They agree by construction, and two tests say so: `checkpoint_restore_is_lossless` proves
recapturing a restored world reproduces the snapshot published at that tick, and
`the_rollback_ring_and_the_snapshot_ring_agree_tick_for_tick` proves the rings file under matching
ticks.

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
figure, is what governs: a 160×104 map is 4× the tiles again.

The snapshot ring collapsed to a single entry because rollback was its only historical reader, and
that read was redundant — recapturing the client frame from the restored world yields the same
bytes, which `a_rollback_produces_the_world_that_tick_had` asserts. `snapshot_history_limit` now
governs `CheckpointHistory` alone: **one depth knob, one history of worlds.** Net resident memory is
within 0.4% of where the arc started, with a checkpoint ring where a snapshot ring used to be.

**The transferable part is not either figure.** It is that a per-turn ring of whole worlds is an
easy thing to write and an invisible thing to pay for, and it went unmeasured for as long as it did
because nothing in the system reported a total. Estimating it structurally is not a substitute —
`size_of` does not see the `Vec` and `HashMap` heap that is most of an entry, and a structural
estimate of this ring was wrong by 5× in the direction that mattered.

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

The component half is not optional. `PowerNode.base_generation` / `.base_demand` is component-level
omission that had already happened — restore set `base = current`, so the next turn re-applied
modifiers to an already-modified base — and a resource-only guard would have missed the very bug
that motivated the guard. Registered components are walked rather than live ones, because
`Expedition` and `TradeLink` have no instances in a fresh world and an archetype walk would miss
exactly the state a rollback is most likely to drop.

The world-static bucket's reason carries an expiry: those resources survive a rollback only because
a restore rebuilds into the same live `World`, which still holds the map worldgen built. That stops
being true the day a checkpoint becomes a save file loaded into a fresh process.
