---
paths:
  - "core_sim/src/turn_profile.rs"
  - "core_sim/src/snapshot/capture.rs"
  - "core_sim/src/network.rs"
  - "core_sim/tests/turn_profile_wiring.rs"
  - "sim_schema/src/world.rs"
---

# Turn profiling and the snapshot serialization budget

Where a turn's time actually goes, how it is measured, and which of the snapshot's
serializations are load-bearing. Written from issue #384, which started as "turn resolution
feels slow even though nothing is happening".

## The shape of a turn: the sim is not the cost

Measured on an 80×52 grid, release build, `late_forager_tribe` on `earthlike`, steady state,
with no meaningful unit activity:

| Phase | ms | share |
|---|---|---|
| all 11 `TurnStage` sets combined | 1.3 | 5% |
| `snapshot` | 15.6 | 94% |
| ├ `snapshot.build` | 3.7 | 22% |
| ├ `snapshot.finalize_hash` | 1.1 | 6% |
| └ `snapshot.history` | 10.5 | 63% |
|   ├ `history.diff` | 1.5 | 9% |
|   ├ `encode.flat_snapshot` | 7.3 | 44% |
|   └ `encode.bincode_snapshot` + `.bincode_delta` | 1.4 | 8% |
| `broadcast` | 0.3 | 2% |

**The simulation is ~5% of a turn. Publishing the result is ~94%.** Every optimization
instinct that reaches for the ECS systems is aimed at the wrong 5%.

**Turn cost is linear in tile count, and independent of what the player is doing** — ~4.2 µs
per tile across 1k–17k tiles. That is the property to keep in mind when a change makes the
snapshot wider: the cost lands on every tile of every turn, forever, whether or not anything
about that tile changed. It is also why a bigger map is the thing that will break this first
(a 160×104 map is ~72 ms/turn), not more bands or more herds.

## What the turn path actually broadcasts

`broadcast_latest` (`network.rs`) sends exactly two things:

- the **bincode delta** on the legacy bincode socket, and
- the **flat (FlatBuffers) snapshot** on the flat socket, which is the one the Godot client
  connects to (`.claude/rules/core_sim/ports.md` — the stream port is `snapshot_flat`).

So **the client is sent a complete world every turn.** A full delta pipeline exists and is
computed (`history.diff`), but the client does not consume it. Closing that gap is the single
largest remaining win and is its own arc, not a local optimization.

> ### A stored flat *delta* is dead work — do not reintroduce one
>
> `StoredSnapshot::new` used to call `encode_delta_flatbuffer` every turn and stash the result.
> Nothing ever read it: the turn path broadcasts the bincode delta and the flat *snapshot*, and
> the three on-demand feed paths (`update_axis_bias`, `update_influencers`,
> `update_command_events`) each build a flat delta locally and **return** it for immediate
> broadcast rather than reading the stored field. It cost **24% of every turn** — more than the
> entire simulation — purely to be dropped.
>
> `encode_delta_flatbuffer` still exists for those three on-demand paths. What was removed is
> the per-turn call and the `encoded_delta_flat` field on `StoredSnapshot` / `SnapshotHistory`.
> **If you find yourself adding a stored flat delta back, wire a reader first** — this is the
> exact shape of thing that is invisible in behavior and expensive forever.

The two bincode encodes that remain **are** live and must not be removed on the same reasoning:
`encoded_delta` is what the bincode socket broadcasts, and `encoded_snapshot` is read by
`recapture_and_broadcast` and by **rollback**, which needs a per-tick encoded snapshot for every
entry in the ring. Rollback is why it is computed per turn rather than on demand.

## `finalize` hashes in place — it deliberately does not call `hash_snapshot`

`WorldSnapshot::finalize` takes `self` **by value**, so it zeroes `header.hash` on itself,
serializes, and stamps the result back. The free-standing `hash_snapshot` takes `&self` and must
therefore **clone the entire `WorldSnapshot`** just to zero one `u64` before serializing — a deep
copy of every tile, raster, and registry, once per turn, discarded immediately. That clone was
~45% of the hash cost.

The two are **byte-equivalent by construction**: both hash the bincode encoding of the snapshot
with `header.hash == 0`. `sim_schema/src/world.rs` pins this
(`finalize_stamps_exactly_the_free_standing_hash`) because the optimization is otherwise
invisible in behavior, and pins re-`finalize` idempotency because the on-demand feed paths
finalize an already-stamped snapshot — dropping the explicit zeroing would silently hash the
stale value in.

`hash_snapshot` survives as a public function: `integration_tests/tests/determinism.rs` calls it
directly on two normalized snapshots. That test does **not** depend on the per-turn stamping.

## Reading a `turn.profile` line

The server emits two events per turn: `turn.completed` (the existing headline `duration_ms`) and
`turn.profile`, whose `phases` field is one flat string of `label=ms`, chronological.

**Nesting is flat and parents include their children.** There is no tree — every label is one
slot, so `snapshot.build.tiles` is counted in `snapshot.build` *and* in `snapshot`. The dotted
names carry the hierarchy. **The phases do not sum to the turn duration**, and reading them as a
partition will double-count. A `snapshot.build.rasters=3.10(x2)` suffix means the label was
entered twice (the raster section is genuinely two non-contiguous blocks, re-entered rather than
leaving the second one unmeasured). A label entered exactly once carries no suffix, and `(x0)`
means the phase was still open when the profile was taken, so nothing was folded in.

Entries order by when a label was first **opened**, not when its time was folded in — a scope
closes before the stage enclosing it, so close-ordering would print children ahead of parents.

Two things to know before trusting a single line:

- **`recapture_snapshot_in_place`** re-runs the whole capture outside turn resolution, on every
  world-mutating command. Its scopes land in whatever profile is currently open, so a
  command-heavy turn shows inflated `snapshot.build` / `encode.*`. Compare steady-state turns.
- **`turn.profile` fires only from `resolve_ready_turn`.** Benchmarks and tests accumulate into
  the global and never drain it; `begin_turn` clears, so nothing grows without bound.

## Adding a `TurnStage` means adding a marker

The stage boundaries are timed by zero-param marker systems scheduled `.after(prev).before(next)`
between the chained sets (`lib.rs`). They are **deliberately not capability-gated**, unlike the
stages themselves, so a gated-off stage records ~0 rather than vanishing from the log — "this
stage was skipped" and "this stage is not instrumented" must not look identical.

A marker is an ordinary system whose absence breaks nothing at runtime, so a new stage without
one would silently never appear in any operator's log. `core_sim/tests/turn_profile_wiring.rs`
resolves a real turn and asserts all stage labels appear **in schedule order** — add the new
stage to its `EXPECTED_STAGES` alongside the marker.

## Config files

None. The profiler is always on and has no levers: at a couple dozen spans against a ~17 ms turn
the mutex traffic is below measurement noise, and a flag would only create a second code path
whose numbers nobody has.
