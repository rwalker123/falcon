---
paths:
  - "core_sim/src/lib.rs"
  - "core_sim/Cargo.toml"
  - "core_sim/tests/schedule_parallelism.rs"
---

# The turn schedule: ordering, parallelism, and what it costs

How systems are ordered in `build_headless_app`, why the ordering is declared rather than
chained, and what the multi-threaded executor actually buys. From issue #402, the foundation
slice of the #392 multi-core arc.

## A new system declares the edges it has — it does not join a chain

**Appending to a `.chain()` is not the default gesture.** A system added to a `TurnStage` declares
the orderings it genuinely needs (`.after(that_system)` / `.before(that_system)`), and nothing
more. Whatever it does not declare, bevy is free to overlap.

This is enforced, not merely encouraged. The `Update` schedule is built with
**`ambiguity_detection: LogLevel::Error`** (set at the end of `build_headless_app`), so any pair of
systems with **conflicting data access and no ordering edge between them** is a **boot panic** with
both system names and the conflicting types. The practical consequence for anyone adding a system:

- If the panic names your system and one you don't care about, you have found a real shared
  resource. Declare the edge — the order the panic forces you to think about is one the turn's
  result already depends on.
- If it names nothing, your system is genuinely independent and now runs alongside its stage.

**The gate is what makes de-chaining safe.** A pair with conflicting access cannot run in parallel
regardless — the executor serializes it — so leaving such a pair unordered never buys a core, it
only lets the executor pick a winner and breaks `integration_tests/tests/determinism.rs`. With the
gate armed, the pairs where order is observable *cannot* be left unordered, so dropping an edge is
provably behavior-preserving.

`Commands` are the one hazard the gate cannot see, because `Deferred` access is not a conflict. Two
systems that both use `Commands`, or one that spawns and one that reads the result, register as
independent while their auto-inserted `apply_deferred` sync point depends on the edge between them.
Only `advance_band_movement` and `advance_expeditions` take `Commands`, and both sit inside the
`Population` chain. **A new `Commands`-using system needs its ordering reasoned about by hand.**

Deferred buffers themselves are applied in **system-index order** (`apply_deferred` walks a
`FixedBitSet` with `.ones()`), not completion order, so command application is deterministic under
the multi-threaded executor.

## Which stages are chained, and why that is a finding

`.chain()` survives where a stage is **serial by data** — every pair conflicts, so an explicit edge
list would be the same total order written longhand:

| Stage | Shape | Shared state that forces it |
|---|---|---|
| `Influence` | fully serial | the culture layers each writes |
| `Knowledge` | fully serial *in effect* | the one free system must precede the rest anyway, so its freedom is already implied |
| `Finalize` | fully serial | `CorruptionLedgers` |
| `Snapshot` | fully serial | `SimulationTick` → `SimulationMetrics` |
| `Population` | 7-chain + 1 free | every pair conflicts on `PopulationCohort`; `publish_trade_telemetry` is pure telemetry |
| `Visibility` | 5-chain + 1 free | everything funnels through `VisibilityLedger`; `prune_sweep_tracker` touches only `VisibilitySweepTracker` |
| `GreatDiscovery` | 6-chain + 1 free | `apply_capability_effects` writes `CapabilityFlags`, which nothing else in the stage reads |
| `Logistics` | **7-chain + 5 free** | the fauna backbone shares `HerdRegistry`; the flora/pasture half is disjoint |

`Logistics` is the only stage with real intra-stage width: 12 systems, critical path 7. The
flora/pasture systems (`ForageRegistry`/`GrazeRegistry`) run alongside the fauna run.

**The 11-set `configure_sets(...).chain()` across stages is deliberate and stays.** `turn_profile`'s
stage markers pin themselves between neighbouring stage sets (`.after(X).before(Y)`) so
`enter_stage` can close one stage and open the next; overlapping stages would make the per-stage
profile meaningless. Parallelism here is *intra*-stage by design.

## The executor pays off above ~10–30 µs of work per system

`core_sim/Cargo.toml` takes bevy with `default-features = false`, so **`multi-threaded` is an
explicit feature in that list**. Without it `bevy_ecs` compiles the single-threaded executor and no
schedule shape uses a second core. Nothing in the crate references the feature by name, which is
why `tests/schedule_parallelism.rs` asserts on `Schedule::get_executor_kind()` rather than on the
manifest.

The executor dispatches **every** system through the task pool, chained or not, at roughly **2 µs
each**. On ~61 systems that is a fixed **~0.12 ms per turn**, paid whether or not anything overlaps.

Measured on a 12-system stage with critical path 7 (ceiling 12/7 = 1.71×), per-system work swept:

| per-system work | 1-thread | multi-threaded | speedup |
|---|---|---|---|
| ~1.8 µs | 0.021 ms | 0.035 ms | 0.61× |
| ~5.1 µs | 0.062 ms | 0.077 ms | 0.80× |
| ~34 µs | 0.405 ms | 0.314 ms | 1.29× |
| ~314 µs | 3.763 ms | 2.301 ms | 1.64× |

**Below ~10 µs per system the dispatch overhead dominates and the multi-threaded executor loses;
above it the speedup climbs toward the stage's critical-path ceiling.** Today's stages sit at or
below that line — `Logistics` averages ~18 µs per system and is near break-even, while
`Knowledge` (~3 µs) and `GreatDiscovery` (~2 µs) lose — so on the current workload the executor
costs about **0.12 ms of an 8.4 ms turn**. It is on regardless, because the alternative is a
determinism guarantee that no test ever exercises: with the single-threaded executor the ambiguity
gate and every ordering edge are theory until someone flips the feature.

**This is not where a turn's time is.** `snapshot` is ~7.6 ms of that 8.4 ms and is essentially one
system (`capture_snapshot`), which no amount of schedule-level parallelism can divide — see
`turn-profiling.md`. Widening the schedule addresses how latency *grows as systems multiply*, not
the present cost. The lever for the present cost is data parallelism **inside** the heavy systems;
`rayon` is a declared dependency of `core_sim` and does not need bevy's executor, since it carries
its own pool.

## Guard

`core_sim/tests/schedule_parallelism.rs` pins both halves: that the executor is `MultiThreaded`, and
that named independent pairs are mutually unreachable in the flattened dependency graph. It
flattens the graph the way bevy does — a dependency edge may terminate on a **`SystemTypeSet`**
rather than a system node, because `.after(some_system)` records its edge against that system's type
set, so every leaf under each endpoint has to be expanded before reachability means anything. The
`MUST_BE_ORDERED` cases are the negative control: two of them cross a type-set boundary, so a
regression in that expansion fails the test instead of silently making everything look concurrent.

Re-`.chain()`-ing a stage costs a core and fails nothing else, which is precisely why the assertion
is a test and not a comment.
