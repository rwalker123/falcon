---
paths:
  - "core_sim/src/turn_profile.rs"
  - "core_sim/src/snapshot/capture.rs"
  - "core_sim/src/snapshot/publish.rs"
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
with no meaningful unit activity. **This is the #384 "before" baseline, kept because the
*proportions* are the lesson** — both full-snapshot encodes in it have since left the turn path
(`encode.flat_snapshot` in #386; **both** bincode encodes in #388, socket and all — see below):

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

## The capture walks the tile query ONCE, then walks the patch subset

`capture_snapshot` used to make **three** full passes over `tiles.iter()` — one building
`tile_states`/`tile_tags`/`seasonal_weights`, one for `sow_site_refusals`, one for
`flora_compositions` (issue #387). The last two both *filtered to forage-patch tiles*, so two
whole-map walks were being spent to reach a subset.

Now there is **one** full sweep, which also collects the `patch_tiles: Vec<&Tile>` both readouts
are about (so `forage_registry.patch()` is asked once per tile, not once per readout), and **one**
walk of that subset that builds both. Collecting `&Tile` out of the query is sound because the
query is read-only and outlives the vec.

**The second pass cannot fold into the first, and that is a property of the rule, not of the
code.** `forage::tile_is_fresh_watered` reads a tile's *neighbours'* tags, so the `plant:field`
site refusal needs the tag grid **finished** before the first refusal is judged. Anything else
per-tile and self-contained belongs in sweep 1.

> ### A `HashMap<UVec2, _>` read once per hex NEIGHBOUR is a real cost at map scale
>
> `tile_tags` was a `HashMap<UVec2, TerrainTags>` whose only consumer probes **six** neighbours per
> patch tile — ~14k SipHash lookups a turn on an 80×52 map, which was most of what the refusal
> sweep cost. It is now a flat `TerrainTagGrid` (`Vec<Option<TerrainTags>>` indexed by grid
> position, sized from `config.grid_size`). Flattening it made even the sweep that *writes* the
> grid measurably faster, and that is the transferable finding: **per-tile coord-keyed hashing is
> cheap until something reads it per adjacency.**
>
> The cell stays `Option<TerrainTags>` deliberately — "no tile there" and "a tile carrying no tags"
> are different readings, the same reason `seasonal_weights` stores only what it has.

**Measured** (80×52, release, `late_forager_tribe` on `earthlike`, steady state, mean of 30 turns
after 5 warm-ups, `map_seed` pinned):

| | before | after |
|---|---|---|
| `snapshot.build.tiles` | 0.33 | **0.25** |
| `snapshot.build.sow_refusals` + `.flora` | 0.40 + 1.58 | — |
| `snapshot.build.patches` | — | **1.40** |
| `snapshot.build` | 4.09 | **3.62** |
| `snapshot` | 9.64 | **9.19** |

> **Both columns predate #388, and the two aggregate rows have moved since.** On post-#388 `main`
> the same measurement reads `snapshot.build` **3.41** and `snapshot` **7.83** — the whole-`snapshot`
> drop is #388 retiring both bincode encodes, and a further ~0.17 ms came out of `build`'s
> *unlabelled* remainder, which #398 also touched. The per-sweep rows are unaffected and reproduce
> as published (`tiles` 0.24, `patches` 1.37). **Read the two sweep rows as this change's result;
> read the aggregates as a same-baseline before/after that no longer matches current absolute
> numbers.** This is the ordinary hazard of quoting an aggregate in a file two other arcs are also
> optimizing — prefer the narrowest label that shows the effect.

**The two labels became one.** `snapshot.build.sow_refusals` and `snapshot.build.flora` are now
`snapshot.build.patches`, because one loop does both jobs and two labels would be a fiction. The
cost is real and worth stating: flora dominates that number (~1.4 of 1.4 ms), so a flora
regression no longer has its own line — it shows up as `patches` growing.
`core_sim/tests/turn_profile_wiring.rs`'s `EXPECTED_CAPTURE_PHASES` pins the new set.

### Profiling this path: pin the seed, and don't compare hashes

Two traps, both hit while measuring #387:

- **`map_seed: 0` in the shipped `simulation_config.json` means "roll from entropy"**, so a
  before/after profile run against the default config measures **two different worlds**. Pin the
  seed (a `SIM_CONFIG_PATH` copy) or the numbers are noise wearing a table. **Rebuild that copy
  from the current shipped file each time** rather than reusing an old one — a `SIM_CONFIG_PATH`
  naming a file that is stale against the *schema* is a boot panic, not a silent fallback
  (`config-loading.md`), and the keys do move: #388 renamed `snapshot_bind` → `port_base_bind`.
- **`header.hash` is not reproducible run-to-run even on unchanged code** — the `influencers` list
  is nondeterministic, which is exactly why `integration_tests/tests/determinism.rs` clears it
  before hashing. To prove a capture refactor is output-neutral, diff the *structures* it touched
  (dump `forage_patches` + `tiles` coord-sorted across several turns), never the snapshot hash.

## What the turn path actually broadcasts

Publication sends **one** thing per frame: on the flat socket — the only snapshot socket, the one the
Godot client consumes (`.claude/rules/core_sim/ports.md`) — the **flat delta**, except for a world's
*first* publication, which is the full flat snapshot the client baselines on.

That rule now lives where the frame is produced: `PublishState::publish` **returns** the bytes to put
on the wire. It used to be `network::broadcast_latest`, reading `encoded_snapshot_flat` /
`encoded_delta_flat` back off the history after the fact and choosing between them — a rule stated in
a different place from the code that decided which field was `Some`. One publisher owning both is
what stops a later turn re-broadcasting a stale full snapshot.

**The client is no longer sent a complete world every turn** (#386,
`docs/plan_delta_streaming.md`). A full flat snapshot is now encoded only for a world's first
frame, for **rollback**, and in answer to a client **`resync`** — encoded on demand rather than every
ring entry paying for the few ever asked for.

**Both of those re-encode rather than broadcasting the ring entry's stored bytes, because a full
frame must claim a FRESH publication sequence number** — `SnapshotHistory::publish_full_frame` is the
one seam for it, and `StoredSnapshot::encode_flat()` (which returns *stored* bytes) is consequently
test-only now. The counter is never rewound: it numbers publications, not ticks, and `reset_to_entry`
rewinds the baselines but deliberately not the sequence. A frame carrying a stale number leaves the
client baselined behind the server, so the next delta's `base_frame_seq` names a frame the client
never applied and the client drops it.

The stale numbers are easy to reach. Rollback's ring entry was stamped when that tick was
*originally* published. A **recapture** refreshes `history.back().snapshot` but **not** its cached
`encoded_snapshot_flat`, and an **auxiliary delta** (`update_axis_bias` and friends) claims a number
without touching the ring at all — so `latest_entry()`'s bytes can lag by either route. On rollback
that costs one wasted round trip, because resync heals it. **On resync it is worse: resync *is* the
recovery path**, so a stale answer reopens the gap it was sent to close and the client cannot
converge until some later publication refreshes the entry. Guarded by
`core_sim/tests/delta_streaming.rs::{a_rollback_frame_is_the_base_the_next_delta_names,
a_resync_frame_is_the_base_the_next_delta_names_after_a_recapture}`, both reading `frameSeq` off the
published envelope.

`refresh_latest` (the mid-tick recapture after a world-mutating command) publishes a delta too, and
shares `publish()` with the turn path. It deliberately does **not** commit the delta baseline, which
is what makes those deltas *cumulative*: each is `baseline(last turn) → now`, so each is a superset
of the last and dropping an intermediate one is harmless.

> ### The stored flat delta now has a reader — that is what changed
>
> This file used to record the stored flat delta as **dead work**: `StoredSnapshot::new` computed
> `encode_delta_flatbuffer` every turn and nothing read it, costing 24% of a turn to be discarded.
> The rule then was "if you find yourself adding a stored flat delta back, **wire a reader first**".
>
> A reader was wired: `broadcast_latest` broadcasts it. The rule stands and is what made the
> reintroduction legitimate rather than a regression — the cost is now paid *for* something.
>
> **`encode.flat_snapshot` is correspondingly absent from a steady-state turn's profile**, and
> after #388 the flat delta is the *only* encode a steady turn pays for at all.
> `core_sim/tests/turn_profile_wiring.rs` pins that from both sides — `EXPECTED_CAPTURE_PHASES`
> lists `encode.flat_delta`, `RETIRED_CAPTURE_PHASES` asserts the full-snapshot encodes stay off
> the turn — and it is the alarm that would catch the turn path silently losing, or regaining, an
> encode.

### Measured effect

Same conditions as the table above (80×52, release, `late_forager_tribe` on `earthlike`):

| | before | after |
|---|---|---|
| flat encode on the turn path | `encode.flat_snapshot` 7.3 ms | `encode.flat_delta` **0.62 ms** |
| `snapshot` phase | 15.6 ms | **~9.7 ms** |

Getting there took three payload fixes, not just the delta pipeline — the delta was initially
*larger* than expected because every tile and every culture layer changed every turn. See
`docs/plan_delta_streaming.md` §3.5–3.6; the short version is that **a field nobody reads, or one
compared more precisely than anything renders it, costs the whole map every turn forever**.

**Both bincode encodes are gone (#388), and with them the socket they fed.** The plan was to make
the snapshot lazy the way the flat one already was — the ring holds the `Arc<WorldSnapshot>`, so
retaining 256 encoded copies to serve the handful rollback ever asks for was this file's usual
dead-work shape, worth **~0.91 ms of every turn**. Writing the round-trip test for it found the
larger fact: **a `WorldSnapshot` cannot be bincode-decoded at all.** `SnapshotHeader::campaign_label`
and ~14 fields under `sim_schema::state::campaign` carry `#[serde(skip_serializing_if =
"Option::is_none")]`, and a field omitted from a **non-self-describing** format desynchronises the
reader — `UnexpectedEof`, or, with bytes still to come, *silent garbage*. Those frames had never been
readable by anything, which is why retiring the socket needed no deprecation: the delta encode
(`encode.bincode_delta`, the rest of the 8% line above) went with it, `broadcast_latest` now writes
one socket, and `base+0` is a reserved slot. `sim_schema`'s `encode_snapshot`/`encode_delta`
wrappers went with them (nothing called either); bincode survives there only *inline* in
`finalize`/`hash_snapshot`, as bytes to hash rather than a codec. `turn_profile_wiring.rs`'s
`RETIRED_CAPTURE_PHASES` is what stops either coming back — a retired label reappearing in a
steady-state profile fails that test.

## The per-frame content hash is gone — `WorldSnapshot::finalize` no longer exists

`finalize` bincode-serialized the **entire world** on every published frame to stamp
`header.hash` — ~1.0 ms of an 80×52 frame. #393 deleted it, and the deletion is the point: **the
value had no reader anywhere.**

That was established by tracing every consumer, not by assuming:

| candidate reader | what it actually does |
|---|---|
| the Godot client | never touches `hash` — no hit in the native decoder or any `.gd` |
| rollback / the ring | restores from the snapshot; never compares a hash |
| `integration_tests/tests/determinism.rs` | **zeroes** `header.hash` on both snapshots and calls `hash_snapshot` itself |
| `sim_schema/src/world.rs` tests | tested the stamping mechanism, i.e. themselves |

So this is the same shape this file already records twice — the retired bincode socket (#388) and
the stored flat delta — and the rule those established is what makes the deletion obvious rather
than risky: **if you find yourself stamping a content hash again, wire a reader first.**

**Retired outright, not relocated.** #393 moved publication off the turn thread, and it would have
been easy to carry `finalize` along as `publish.finalize_hash`. Moving dead work still pays for it,
so the label has no publisher twin, and `turn_profile_wiring.rs` asserts `snapshot.finalize_hash`
appears on *neither* side.

**What survives.** `hash_snapshot` stays public with exactly one caller, `determinism.rs`, and is
now off every publication path — pinned by `world.rs`'s
`hash_snapshot_is_deterministic_and_ignores_the_stored_hash`, which replaces the two `finalize`
tests and covers the two properties that caller depends on. `SnapshotHeader::hash` and its
`snapshot.fbs` slot also stay, always `0`: FlatBuffers slots are positional and this repo's merges
are append-only, so retiring a wire slot is its own change, and an always-zero `u64` costs 8 bytes.

## Publication is not on the turn thread — and `turn.profile` no longer describes all of a turn

`capture_snapshot` walks the ECS world, assembles the `WorldSnapshot`, and hands it to a **publisher
thread** (`snapshot/publish.rs`, #393). Hashing, diffing, encoding and the socket write happen there.
The turn thread's last act is a move onto a bounded channel, and that is what `snapshot.handoff`
measures — single-digit **microseconds** in steady state.

**The seam was already there.** Everything after the capture reads only the assembled snapshot and
publication's own state; nothing touches the world, and the simulation does not depend on any of it
having finished. What #393 changed is who runs it, not what it does.

### The label move, and the two lists that pin it

| was, on `turn.profile` | is, on the publisher |
|---|---|
| `snapshot.finalize_hash` | `publish.finalize_hash` |
| `snapshot.history.diff` | `publish.diff` |
| `encode.flat_delta` | `publish.encode.flat_delta` |
| `encode.flat_snapshot` (first frame) | `publish.encode.flat_snapshot` |
| `snapshot.history`, `broadcast` | — (no longer a phase; the publisher owns both) |

The old names moved into `turn_profile_wiring.rs`'s `RETIRED_CAPTURE_PHASES`, beside the genuinely
retired bincode encodes, because the assertion wanted is the same: **a turn that pays for any of them
again has regressed.** The complement, `PUBLISHER_PHASES`, asserts the same work is present on the
publisher — absence-only would pass just as well if publication had quietly stopped happening.

**A publisher scope is `turn_profile::publish_scope`, and its accumulator is thread-local** — the
opposite choice from the turn's global, for two reasons that would both be bugs the other way. The
publisher runs concurrently with the next turn by construction, so a span folded into the global
would land in whatever profile happened to be open; and `cargo test` runs many worlds at once, each
with its own publisher. Being thread-local means nobody else can read it, which is why the publisher
drains its own breakdown per frame into `SnapshotHistory::last_publish_profile()`.

**The rare inline paths deliberately carry no scope.** Rollback, `Resync` and the auxiliary feed
deltas (`update_axis_bias` and friends) run on the *caller's* thread after draining the queue, so a
`publish_scope` there would accumulate into a thread-local nothing drains. They are human-paced and
already log their frame's tick and size.

### Measured, and the honest shape of the win

80×52, release, `late_forager_tribe` on `earthlike`, `map_seed` pinned, mean of 30 turns after 5
warm-ups — the standard recipe above:

| | before | after |
|---|---|---|
| `run_turn`, publisher **idle** | 8.48 ms | **4.55 ms** |
| `run_turn`, publisher **concurrently busy** | 8.48 ms | **5.98 ms** |
| `snapshot.build` | 3.27 | 3.16 / 4.75 (idle / busy) |
| publisher, per frame | — | `diff` 1.72 · `encode.flat_delta` 0.61 |

**The busy row in that table has since closed** — see "The diff is O(changed)" below, which took
`publish.diff` from 1.72 ms to ~0.2 and left the busy and idle rows equal. The gap it describes is
kept because the *mechanism* is the lesson.

**Read both rows: the gap between them is the finding.** The work genuinely left the turn thread —
4.55 ms is what a turn now executes. But when turns resolve **back to back**, the publisher's ~2.3 ms
overlaps the *next* turn's capture and the two compete, and nearly all of that lands in
`snapshot.build`'s allocation-heavy remainder while its labelled per-tile sweeps (`tiles`, `patches`,
`rasters`) do not move at all. Interactive play is the idle row — a player's turns are seconds apart,
so the publisher is long finished before the next capture starts. A batched `turn 100`, a benchmark,
or an integration-test loop is the busy row.

**Reproduce the idle row with `SnapshotHistory::shutdown()`**, not with a `sleep` between turns.
Shutting the publisher down makes `update()` return immediately and is shipped API; a sleep lets the
core idle down and distorts everything (see the warning below).

**Do not read the busy row as a reason to shrink the queue or to drop frames.** The queue was never
the constraint: `snapshot.handoff` stays at ~0.002 ms in every one of these runs, so no turn ever
waited on it. The lever on the busy row is making the publisher's per-frame work smaller, which is
what retiring the content hash (below) did to the largest single piece of it — not making the
handoff tighter.

> **Beware the profiler when measuring this.** Two traps cost real time here: a spin-loop "control"
> thread that calls `Instant::now()` in its inner loop contends on the macOS timebase and fabricates
> ~2.5 ms of phantom slowdown in the *other* thread; and inserting a `sleep` between turns to
> simulate interactive pacing lets the core idle down, which made **every** phase, on both threads,
> ~2.3× slower. Compare back-to-back runs against back-to-back runs, and vary one thread's work with
> a register-only, clock-free loop if you must vary it at all.

## The diff is O(changed), not O(world) — and that is what made the busy row go away

`publish.diff` was **1.72 ms of every frame on a still world**, and the reason was structural: the
diff rebuilt its own inputs from scratch every turn. Broken down (the five sub-labels were
measurement scaffolding and are **not** in the code — the single `publish.diff` scope is):

| section of `publish.diff` | ms | share |
|---|---|---|
| build the `tiles` index | 0.08 | 4% |
| build the other ~12 indexes | 0.29 | 16% |
| the ~35 whole-section comparisons | 0.33 | 18% |
| `diff_new_tiles` + `diff_new_culture_layers` | 0.85 | 48% |
| assemble the `WorldDelta` | 0.24 | 13% |

**The shape that matters is not "tiles is the big one" — it is that the body is FOUR map-sized
collections.** `tiles` (4160 entries on an 80×52 map), `power` (one node per tile, 4160),
`culture_layers` (4195), and the rasters (one `Vec` per map, compared and cloned as whole sections)
are ~95% of it; the other ten indexed collections hold **≤ 6 entries between them**. A partition
that treats "tiles" as the heavy case and "everything else" as the tail is wrong twice over.

### An unchanged entry now costs one hash probe

It used to cost **two clones and two hash inserts, every turn, forever, to produce no output**:
`publish` cloned all 4160 `TileState`s into a fresh `HashMap`, `diff_new_tiles` consumed that map
and built *another* fresh one as the new baseline (cloning `prev` for every unchanged tile), and
`diff_removed` then walked the old baseline again.

The baselines are now **mutated in place** (`diff_indexed` in `snapshot/mod.rs`): walk the captured
`Vec`, probe the baseline, and on the unchanged path **do nothing at all** — no clone, no insert,
no rewrite. Removal keeps its own rule: the walk counts how many baseline entries the capture still
carries, and only when that count falls short does `diff_removed` sweep for the keys that vanished,
so the common path never pays for the rare one.

> **"Do nothing" is the deadband's requirement, not an optimization.** `same_published_state` is a
> *rounded* comparison, and the baseline has to keep the value the client actually holds. Storing
> the fresh value for an entry judged unchanged would re-zero the deadband every turn, so an
> accumulating sub-hundredth drift would never cross the grid and the client would hold a stale tile
> forever. The rewrite makes that unwritable — there is no store on the unchanged path — and
> `snapshot::indexed_diff_tests` asserts it on the baseline map, which is the only place it shows.

The ~35 **whole-section** comparisons had the mirror-image bug: `let x = snapshot.x.clone(); if
self.x == x {…}` cloned the section *before* deciding whether it had changed, then assigned it to
the baseline either way. `diff_whole` compares first and clones only on the changed branch, and
leaves the baseline alone when nothing moved. Measured on a steady turn, **12 of the 37 sections
differ** — so 25 full-section clones per frame, several of them whole-map rasters, were being taken
to be thrown away.

### `Baseline::Advance` / `Baseline::Hold` — because in-place mutation cannot be "not stored"

A mid-tick **recapture** must not advance the baselines (that is what makes its deltas cumulative).
While the diff returned a freshly-built map that was simple: the recapture arm returned early and
never stored it. In-place mutation has nothing to withhold, so the intent is now an argument
threaded through every diff helper, decided once at the top of `publish` from `Publication`. A
`Hold` diff produces exactly the same delta and writes nothing.

### ECS change detection is the WRONG tool here — do not re-propose it

Bevy's `Changed<T>` marks on mutable **access**, which is a superset of "the value changed", which
is itself a superset of "changed at **rendering precision**". The deadband already filters at
rendering precision, so change detection is strictly *weaker* than the comparison that is already
there: it would mark tiles a system merely touched, and the delta would grow. `O(changed)` here has
to come from value comparison, because value comparison is the only kind that can respect the
deadband.

## The fan-out: a semantic section registry on a bounded pool

The diff runs as nine tasks over `rayon::scope` — tiles · culture · power · rasters · knowledge &
great discoveries · crisis & victory · campaign & telling · subsistence (herds, forage, food
modules) · people & networks. Each is a `*Parts` output struct, an optional `*Baselines` borrow
bundle, and a `diff_*` function, all in `snapshot/capture.rs`.

**The partition is semantic, not cost-balanced.** A cost-balanced partition has to be re-measured
every time a subsystem grows or a raster is added; a semantic one keeps its meaning as the weights
move, and `rayon::scope` work-steals, so an unequal section is absorbed by the scheduler rather than
baked into the design. **Adding a snapshot collection is: a baseline field, a line in its section's
`*Parts`, a line in its `diff_*`, a line in the assembly.** That registration property is the
deliverable; the milliseconds are a consequence.

Two rules hold for every section:

- **A section reads and writes only its own baselines.** `publish` destructures `&mut *self` into
  per-section field borrows, so the compiler proves the disjointness the partition claims. A section
  that needs another's baseline means the partition is wrong, not that a lock is needed.
- **No `publish_scope` inside a section.** The publisher profiler's accumulator is thread-local, so
  a span opened on a pool worker folds into an accumulator nothing drains — the label would vanish
  from the frame and leak into the next frame on that worker. The single `publish.diff` scope is
  opened and closed on the publisher thread, around the whole fan-out.

**The pool is the publisher's own and is deliberately narrow** (`DIFF_POOL_THREADS = 4`, a process-
wide `OnceLock` so parallel test worlds share one budget rather than one pool each). Rayon's global
pool is `num_cpus` wide; the publisher runs *concurrently with the next turn* by construction, so a
publisher that grabs the whole machine wins its own milliseconds by taking cores from the simulation
it is racing. Today the sim is ~5% of a turn and that trade is invisible — bounding it now is what
stops it being discovered later, in a profile nobody takes. A pool that fails to build falls back to
the caller's thread; publication continues, serially.

### Measured

Standard recipe (80×52, release, `late_forager_tribe` on `earthlike`, `map_seed` pinned, mean of 30
frames after 5 warm-ups), three points so the structural win and the parallel win are separable:

| | before | O(changed) | + fan-out |
|---|---|---|---|
| `publish.diff` | 1.63 | **0.386** | **0.208** |
| `run_turn`, publisher **busy** | 5.82 | 4.52 | **4.53** |
| `snapshot.build`, publisher **busy** | 4.57 | 3.33 | **3.30** |
| `run_turn`, publisher **idle** | 4.53 | 4.47 | 4.45 |

**The busy row is the finding: it is now the idle row.** Publisher contention used to cost the turn
thread ~1.3 ms of `snapshot.build` on back-to-back turns; with the publisher's per-frame work down
to ~0.8 ms (`diff` + `encode.flat_delta`) there is nothing left to contend with, and a batched
`turn 100` now runs at interactive speed. **Nearly all of that came from step one** — the fan-out
halves an already-small number. It is in for what it does to the *next* ten subsystems, not for the
0.18 ms.

`encode.flat_delta` is unchanged at ~0.6 ms and is now the largest thing the publisher does.

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
