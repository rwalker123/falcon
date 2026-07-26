# Delta streaming: decoupling per-turn cost from map size

Design for issue #386. The server sends the Godot client a **complete world every turn**; this
plan replaces that with a baseline-plus-deltas publication model on both halves of the wire.

Companion measurements: `.claude/rules/core_sim/turn-profiling.md` (server) and
`.claude/rules/client/turn-profiling.md` (client). Read both before changing any number here.

---

## 1. The problem, in the units that matter

| Half | Per-turn cost | Scales with |
|---|---|---|
| Server turn total | ~17 ms (80×52) · ~72 ms (160×104) | **tile count**, ~4.2 µs/tile |
| ├ `encode.flat_snapshot` | 7.3 ms — **44% of the turn** | tile count |
| Client applied snapshot | **~170 ms** | tile count + content |
| ├ `decode` / `decode.native` | ~80 ms — FlatBuffers → Godot `Dictionary` | tile count |
| └ `apply` | ~91 ms — `display` + `hud` + `inspector` + `selection` | tile count + content |

**The client is ~10× the sim.** Any design that fixes only `encode.flat_snapshot` buys 7.3 ms of a
~190 ms player-visible turn — a 4% win presented as a 44% one. This plan is explicitly not that:
the server work is phase 1 because it is the *prerequisite* (there is no delta on the wire to
consume), not because it is the prize.

Both costs are linear in tile count and **independent of what the player did**. That is the
property being removed: a turn where one band moved one hex should cost what one band moving one
hex costs.

---

## 2. Why the existing delta path cannot simply be switched on

A full delta pipeline already exists and is computed every turn (`SnapshotHistory::update` →
`history.diff`, ~1.5 ms). Three things stop `broadcast_latest` from just sending it.

**2.1 The client's delta decoder is a stand-in, not an incremental apply.**
`native/src/snapshot/delta.rs`'s `DeltaAggregator` collects whatever sections a delta carried and
re-enters `snapshot_dict` to synthesize a **full-snapshot-shaped dictionary with zeros for
everything the delta did not carry**. Its own comments say so — `pasture_capacity: &[]`,
`forage_capacity: &[]`, "the live stream is full snapshots". Switching the turn broadcast to
deltas today blanks the pasture and forage channels on the first quiet turn, and every raster the
delta happened to omit.

**2.2 The client keeps only the newest frame per poll.**
`SnapshotLoader.poll_stream` loops the batch assigning `last_stream_snapshot = snapshot_dict` and
returns only the last one — it even counts the waste (`last_poll_discarded_frames`). Discarding a
*snapshot* is free; discarding a *delta* silently loses that turn's changes forever. **This is a
correctness bug the moment deltas become the steady state**, and it is invisible: the world just
drifts from the server's.

**2.3 `has()`-means-unchanged is load-bearing and asymmetric.**
`Main._apply_snapshot` guards ~18 HUD calls with `snapshot.has(key)`, documented as "a delta
carries a field only when it CHANGED, so absence means unchanged, never cleared". That is correct
today *because deltas are rare*. It stays correct under streaming — but the reverse (a cached full
dictionary that always has every key) would make every consumer re-render every turn, converting
a decode win into an apply loss. See §5.

**2.4 `campaign_profiles` has no `WorldDelta` field at all.**
`WorldSnapshot.campaign_profiles` exists; `WorldDelta` has no counterpart (`sim_schema/src/world.rs`).
Today that is harmless — the client only ever sees it on a full snapshot. Under streaming a change
to it would never reach the client. Closed in phase 1.

---

## 3. The publication model

### 3.1 Every published frame is sequenced

Append two fields to `SnapshotHeader` (append-only; slots are positional):

```
frameSeq:ulong = 0;      // monotonic per-world publication counter
baseFrameSeq:ulong = 0;  // delta only: the frameSeq this delta applies to
```

`frameSeq` counts **publications**, not ticks, and resets with `world_epoch`. It exists because
tick is not unique per frame: `recapture_and_broadcast` publishes mid-tick on every world-mutating
command, so several frames share a tick and tick-continuity cannot detect a gap.

### 3.2 Four publication sites, three behaviours

| Site | Today | Under streaming |
|---|---|---|
| `SnapshotHistory::update` (turn) | full flat snapshot | **flat delta**, `baseFrameSeq` = previous frame |
| `refresh_latest` (recapture, per world-mutating command) | full flat snapshot | **flat delta against the uncommitted turn baseline** |
| `update_axis_bias` / `update_influencers` / `update_command_events` | flat delta (already) | unchanged |
| World rebuild (`world_epoch` change) | full flat snapshot | **full flat snapshot** — the baseline |

**The recapture deltas are cumulative, and that is what makes them safe.** `refresh_latest`
deliberately does not commit the delta baselines (`self.tiles`/`populations`/…) — documented at
its definition, so the next turn's delta re-sends those structural changes idempotently. Diffing
against that *uncommitted* baseline means every intra-turn recapture delta is
`baseline(turn N) → now`, i.e. each one is a superset of the last. Applying them in order is
idempotent, and **missing an intermediate one is harmless**. Turn deltas, which do commit the
baseline, are genuinely incremental and must not be missed — which is what §3.3 is for.

This requires factoring the diff out of `update()` into a reusable
`diff_against_baseline(&self, &WorldSnapshot) -> WorldDelta` that `update()` and `refresh_latest`
both call; only `update()` then commits the baseline.

### 3.3 The resync contract

Client state: `applied_frame_seq`, plus the existing `_world_epoch_applied`.

- **Full snapshot** — always applicable. Sets `applied_frame_seq = frameSeq`.
- **Delta** — applied iff `baseFrameSeq == applied_frame_seq`. Then `applied_frame_seq = frameSeq`.
- **Anything else** — the frame is dropped and the client sends `resync` on the command socket.
  The server answers with a full flat snapshot (the `refresh_latest` full path, retained for
  exactly this).
- **No baseline yet** (`applied_frame_seq` unset) — every delta is dropped until a full snapshot
  arrives. This is already the client's behaviour (`_try_reveal_world` ignores deltas before the
  first full snapshot) and needs no new code.

**Resync is retried until answered, not until sent** — the same reasoning as
`NEW_GAME_ANSWER_TIMEOUT` in `.claude/rules/core_sim/world-handoff.md`: a client stuck without a
baseline is unrecoverable for the player, while a redundant `resync` costs one full encode.

### 3.4 What does NOT change

**No connect-time frame replay.** `.claude/rules/core_sim/world-handoff.md` establishes that a
newly accepted client is sent nothing until the next broadcast, because a cached frame may belong
to a world the client did not ask for and the client cannot tell. That stays. A connecting client
gets its baseline from the **world-epoch-change full snapshot** its own `new_game` triggers, not
from a replay. The `resync` command in §3.3 is the answer for any future "attach to a running
world" flow — it is client-initiated, so it cannot hand back a world nobody asked for.

**Rollback still needs a per-tick encoded snapshot** for every ring entry
(`.claude/rules/core_sim/turn-profiling.md`). `encoded_snapshot` (bincode) is unaffected.

---

## 3.5 MEASURED: one field defeats the whole premise

Phase 2 shipped and was measured on a live release server (80×52, `earthlike`, seed 12345,
`late_forager_tribe`, steady state). The result contradicts this plan's own premise and is the most
important thing on this page.

```
encode.flat_snapshot   7.3 ms   (before — full world every turn)
encode.flat_delta      6.0 ms   (after  — "delta" every turn)
```

**An 18% saving, not 44%.** The reason, straight off the instrumented turn:

```
DELTA_SIZE tiles_changed=4160 of 4160
```

**Every tile is in every delta.** The delta *is* a full world, in delta clothing. Diffing a world
where every entity changed buys only the cost of the sections that did not.

The culprit is a single field. Two consecutive states of the same **DeepOcean** tile — no life, no
graze, no forage, nothing a player could act on:

```
TILE_OLD  … mass: 1101644, temperature: -1009804, graze_biomass: 0.0, forage_capacity: 0.0 …
TILE_NEW  … mass: 1131380, temperature: -1009804, graze_biomass: 0.0, forage_capacity: 0.0 …
```

Only `mass` differs. It drifts every turn on every tile including open ocean, so it alone puts all
4160 tiles into the diff and pins per-turn cost to tile count — the exact property this arc exists
to remove.

**`mass` has one consumer in the entire client:** `TerrainPanel.gd`, an Inspector panel, which
renders it as `"Mass: %.1f"`. It is fixed-point (1e6), so that sample is `1.10 → 1.13`. Nothing
else — no renderer, no HUD module, no overlay — reads it.

**So the next lever is not a better diff, it is a narrower payload.** In rough order of value:

1. **Quantise `mass` on the wire** to the precision its only reader displays. A field diffed at
   1e-6 and rendered at 1e-1 is generating five digits of pure delta traffic.
2. **Or drop it**, given the standing decision that the Inspector is expendable (§4). That makes
   this the first concrete case where the Inspector's cost is not hypothetical.
3. **Then re-measure and repeat** — `mass` is the field that dominates *today*; there is no reason
   to assume it is the only one, and the instrumentation above (a `DELTA_SIZE` count plus a
   first-differing-tile dump) is four lines and should be re-run, not re-derived.

**The generalisable lesson, which outlives `mass`:** a delta pipeline is only as good as the
stability of the fields it diffs, and **wire precision is a performance decision, not a fidelity
one**. Any per-tile field that drifts every turn costs the full map every turn, forever, no matter
how good the delta encoder is. That belongs in the review checklist for every new `TileState`
field.

## 3.6 The comparison rule, and where two decimals is the wrong answer

Acting on §3.5: the "did this change?" test for the two collections large enough to matter is now
`same_published_state` rather than `PartialEq` — **ignore fields the client does not receive, and
compare the rest at hundredths**. `PartialEq` stays exact, because rollback and the determinism
tests compare whole snapshots and must keep seeing every bit.

Measured effect, same setup: **tiles in a steady-state delta went 4160/4160 → ~600/4160.**

### Two decimals is a grid, not a band — and that is what makes it safe

The comparison **rounds to an absolute grid** (`(v * 100).round()`), it does not test
`|a − b| < 0.005`. That distinction is the whole safety argument. A relative epsilon band is
defeated by exactly the input this system produces: a value creeping by less than the band every
turn is never "changed", and the client's error grows without bound. Rounding cannot do that —
a creeping value crosses a grid line on its own, so the client is never more than half a hundredth
behind, whatever the step size. Pinned by
`drift_below_the_deadband_still_publishes_as_it_accumulates`.

### Where hundredths would be WRONG

Two decimals is right for everything the UI renders as a human-scale quantity. It is **wrong for
any value whose meaningful range sits below 0.01**, where it does not coarsen the signal, it
deletes it:

- **Crisis telemetry gauges.** Live values from the same run: `PhageDensity raw=0.0031406`,
  `ema=0.0030595`, `trend_5t=0.00022678`, against a `warn_threshold` of 0.35. Rounded to
  hundredths every one of these is `0.00`, and the trend — a *difference* of two such numbers —
  is identically zero forever. **Not quantised.**
- **Any rate or per-turn increment** small enough to be an accumulator rather than a reading
  (`regrowth_rate` 0.05, ecology `r ≈ 0.09`, knowledge progress increments). These are inputs to
  running totals; rounding the increment biases the total.

The rule of thumb that separates them: **quantise a READING, never an INCREMENT.** A reading is
compared against a threshold or drawn on a ramp, and hundredths is finer than either. An increment
is summed, and rounding it accumulates the error you just hid.

I did not blanket-apply hundredths to the whole wire for that reason — it is applied to
`TileState` and `CultureLayerState`, the two collections where the measurement showed it pays.

### Culture layers: measured, and NOT a precision problem

`CultureLayerState` got the same treatment (its `last_updated_tick` is a per-turn timestamp that
guarantees every layer is "changed", and **nothing in the client reads it** — it is decoded in
`dict/culture.rs` and no GDScript consumes the key). That did **not** move the number:
**4201 of 4201 layers still ride every delta**, and there are more culture layers than tiles.

Because the culture really is changing. Two consecutive states of one layer:

```
trait AsceticIndulgent   baseline: 0.040205 -> 0.058625      (+0.018)
trait RationalistMystical baseline: 0.045142 -> 0.065870     (+0.021)
```

That is the simulation doing work, not noise, and no diff can compress it. So the remaining
`encode.flat_delta` ≈ 5.4 ms is now **dominated by culture**.

### …and culture does not settle, ever — measured

Traced one local layer over 40 turns (`max_step` = its largest per-turn trait movement,
`max_gap_to_parent` = how far it sits from the parent it is chasing):

| tick | 0 | 7 | 15 | 23 | 31 | 39 |
|---|---|---|---|---|---|---|
| `max_step` | 0.035 | 0.037 | 0.071 | 0.104 | 0.114 | 0.089 |
| `max_gap_to_parent` | 0.061 | 0.104 | 0.111 | 0.154 | 0.186 | 0.168 |

**Both grow.** `CultureLayer::resolve_against` is an exponential relaxation
(`value += (target − value) × elasticity`), which converges geometrically *if the target holds
still*. It does not: the step size rises by 3× over 30 turns and the layer ends up **further** from
its parent than it started. A layer chasing a fixed target cannot do that — so the target is moving
faster than elasticity can track, and the layers oscillate instead of settling.

The driver is **not** the global layer, which was the obvious suspect and is wrong: traced over 40
turns it is flat zero on every axis (`resonance.global` is zero — no influencer culture resonance
at this stage). The movement enters at the regional layer, whose target is
`global(0) + modifier + regional_resonance`, and `regional_resonance` is redistributed influencer
resonance that changes as influencers rise and fall.

**Two questions for design, in order:**

1. **Should culture move this fast at all?** Trait values here have magnitude ~0.05–0.13 and are
   moving up to **0.12 per turn** — a trait can invert in a couple of turns. Whatever culture is
   meant to model, something that re-rolls every few turns is not a *culture*; it reads as a
   tuning bug (elasticity, or the resonance magnitude) rather than intended slow drift. Fixing it
   would also collapse the delta as a side effect — a settled culture diffs to nothing.
2. **Does the client need per-layer, per-axis traits every turn** when what it renders is a culture
   *raster*? 4201 layers × 15 axes × 3 values is the payload; the overlay is one number per tile.

### Resolved: the client never wanted the numbers

Both questions turned out to have the same answer, found by asking what the client actually reads.
`MapView` consumes exactly four keys from a culture layer — `id`, `owner`, `parent`, `scope` — and
uses them only to walk the tree and resolve a tile's **province**. It reads no trait, no
divergence, no threshold. The sole consumer of those was the Inspector's Culture tab.

So the layer's 45 numbers (15 axes × baseline/modifier/value) plus divergence, the thresholds and
`lastUpdatedTick` came off the client stream; their FlatBuffers slots are `(deprecated)`. What
remains is topology, which changes only when the culture tree restructures.

**And no amount of quantisation could have substituted for this.** Each of those 45 numbers drifts
~0.0025/turn — a quarter of a hundredth, comfortably inside the deadband on its own. But across 45
of them, *something* crosses a grid line essentially every turn, so the layer was always "changed".
**Per-entity width defeats per-field precision**: the more numbers an entity carries, the closer its
change probability gets to 1 regardless of how coarsely each is compared. That is the sharper
version of §3.5's lesson and the one to carry forward.

    encode.flat_delta   5.5 ms  ->  0.62 ms
    snapshot phase     14.3 ms  ->  ~9.7 ms

Against the 7.3 ms full snapshot this arc started from, publishing a turn is now **~12x cheaper**.

The culture *drift* fix is separate and still stands on its own (the global layer was integrating
its resonance — see the commit); it is a gameplay correctness fix, not the performance one.

## 4. What this does NOT try to do

**Do not design this around the Inspector.** Standing decision (Ray, 2026-07-26): the Inspector is
legacy scaffolding, expendable, and slated for rework after this arc. #391's hidden-Inspector gate
assumes deltas are rare; this inverts that. Both consequences — a hidden Inspector resuming its
fan-out, and the show-time catch-up replaying a cached *full* snapshot while later deltas are lost
— were **accepted**, not worked around. #390 is parked behind this for the same reason.

> **Superseded on measurement — see §8.6.** Both consequences turned out to be one bug with a
> six-line fix, not the queue-and-replay machinery sketched here, because a merged frame is
> self-contained. Parking it would have left ~60% of the client's remaining per-turn cost on the
> table. The paragraph above stands as the reasoning at the time; the Inspector still did not get
> to *shape* this design, which is what the decision was actually protecting.

---

## 5. The client architecture

Phase 1 leaves the client's observable behaviour identical: the native extension merges each
delta into a **cached decoded world** and emits the same full dictionary it emits today. That is
correct-by-construction (the dict is byte-identical to the full-snapshot dict for the same state)
and wins the server + wire while touching no renderer. It wins **nothing** on the client.

Phases 2–3 are where the ~170 ms goes, and they hinge on one decision:

**The cache is the produced `Dictionary`, not the decoded `WorldSnapshot`.** Caching a Rust-side
`WorldSnapshot` and re-running `snapshot_to_dict` each turn still pays the full ~80 ms conversion
— the conversion *is* the cost, not the FlatBuffers read. Caching the `Dictionary` and mutating
only the changed sub-trees in place makes decode proportional to what changed.

That makes every key permanently present, which breaks `has()`-means-unchanged (§2.3). So the
returned dictionary carries a **change manifest** — the set of section keys this frame actually
touched — and `Main`/`MapView`/`Hud` switch their guards from "is the key present" to "is the key
in the manifest". The manifest is what lets `apply` (~91 ms) become incremental too:
`display_snapshot`'s clear-and-refill blocks, the ~18 `_hud_invoke` fan-out, and the deep-copy
hotspots (`display.layers.culture` ~32 ms, `display.sites.forage` ~10 ms) each skip when their
section is untouched.

---

## 6. Phasing

Each phase is independently shippable and independently verifiable.

**Phase 1 — server + schema.** `frameSeq`/`baseFrameSeq`; `diff_against_baseline` factored out;
per-turn and recapture broadcasts become flat deltas; `campaign_profiles` added to `WorldDelta`
and both codecs; `resync` command. Client: `poll_stream` applies **every** frame in order (§2.2),
and the native extension merges deltas into a cached world, still emitting a full dict.
*Win: `encode.flat_snapshot` 7.3 ms → ~1 ms; wire bytes collapse.*

**Phase 2 — client decode.** Cache the produced `Dictionary`; mutate changed sub-trees in place;
emit the change manifest.
*Win: `decode` ~80 ms → proportional to change.*

**Phase 3 — client apply.** `MapView.display_snapshot`, the HUD fan-out, and the deep-copy
hotspots consume the manifest and skip untouched sections.
*Win: `apply` ~91 ms → proportional to change.*

---

## 7. Guards

The failure modes here are **silent** — a dropped delta looks like a quiet turn, and a stale cache
looks like a world where nothing happened. Every phase needs a guard that fails loudly.

- **Convergence (the core property).** Resolve N turns; assert the client-side state reached by
  `full snapshot + N deltas` is identical to the state reached by decoding the full snapshot at
  turn N. This is the one test that catches a section missing from `WorldDelta`
  (§2.4's `campaign_profiles` class of bug) and it is what `cargo xtask decode-guard` becomes for
  the delta path.
- **Gap detection.** Feed a delta whose `baseFrameSeq` does not match and assert the frame is
  dropped and `resync` is sent — not applied against the wrong baseline.
- **Recapture idempotency.** Two intra-turn recapture deltas applied in order equal the second
  applied alone (§3.2).
- **No silent frame discard.** Pin that a poll delivering three frames applies three frames.
- `cargo xtask decode-guard` after any schema change, per
  `.claude/rules/client/native-extension.md`.

### As built

| Guard | Where | Pins |
|---|---|---|
| Convergence, sequence numbering, recapture supersession | `core_sim/tests/delta_streaming.rs` | §7 items 1 and 3 |
| Whole-section `None` vs `Some(vec![])` on the encoded envelope | `sim_schema/src/codec/mod.rs` | §8.4 |
| A merged delta frame is an honest complete world | `tools/decode_guard.gd` + the delta fixture | §8.2 — the bug class |
| Supersession, no dropped delta, gap → `resync` | `tools/stream_frame_guard.gd` | §7 items 2 and 4 |
| A hidden Inspector skips both frame kinds and catches up on the newest | `tools/inspector_hidden_guard.gd` case 5 | §8.6 |

**Every one of these was mutation-tested** — the defect reintroduced, the guard observed failing
with a message that names the mechanism, the defect removed again. That is not ceremony here: three
of the four bugs this arc produced were found by a *measurement*, not by a test, precisely because
the tests that should have caught them had never been made to fail. `decode-guard` had no delta
fixture at all while the delta path was carrying every turn.

The one §7 item deliberately left unbuilt is a **chained** multi-delta fixture (frame N → N+1 → N+2).
`stream_frame_guard` pins the supersession rule with one delta, which is what catches
keep-only-the-newest; a second chained delta would additionally pin ordering across a batch of
three. Worth adding when the fixture generator next needs touching, not worth a bespoke pass now.

---

## 8. MEASURED: the client half, and the bug the server half planted

Phases 1 + the server payload work shipped and were measured against a live release server
(80×52, `earthlike`, `late_forager_tribe`, steady state, `SHADOW_SCALE_CLIENT_PROFILE=1`).

| | §5's estimate | measured after Phase 1 |
|---|---|---|
| `decode` / `decode.native` | ~80 ms | **0.36 ms** on a quiet delta, ~12 ms when a raster moves |
| `display.layers.culture` | ~32 ms | **2.4 ms** |
| `apply` | ~91 ms | **~44 ms** |
| client total | ~171 ms | **~45 ms** |

**Phase 2 was already paid for by Phase 1.** §5 assumed the ~80 ms decode had to be attacked with
in-place sub-tree mutation and a manifest. It did not: the decode was ~80 ms *because the frame was
a full world*. Once the frame is a delta, the conversion is proportional to the delta, and the
`DeltaAggregator`'s re-entry into `snapshot_dict` — the thing §5 warned would "still pay the full
~80 ms" — costs 0.36 ms because there is almost nothing in it. The manifest is still needed, but
for §5's *other* reason (apply), not for decode.

### 8.1 What a steady-state delta actually carries

Instrumented in `decode_delta_against`, one line per frame:

```
tiles=508 herds=21 populations=1 influencers=3 cultureRaster sentimentRaster corruptionRaster militaryRaster
tiles=642 herds=21 populations=1 influencers=3 demographics=1 command_events=5 cultureRaster …
tiles=750 herds=21 populations=1 influencers=3 cultureRaster sentimentRaster corruptionRaster militaryRaster
```

**Absent from every steady-state delta**: `forage_patches`, `food_modules`, `discovered_sites`,
`culture_layers`, `terrainOverlay`, `elevationOverlay`, `moistureRaster`, `visibilityRaster`. The
client rebuilds all of them every turn anyway, because `display_snapshot` clears and refills from
the merged dict unconditionally. That is where the remaining `apply` cost lives:

| block | ms | reads | in a steady delta? |
|---|---|---|---|
| `display.sites.forage` | 10.0 | `forage_patches` | **absent** |
| `display.shader` | 7.7 | terrain / visibility / elevation / river masks | **all absent** |
| `display.tiles` | 6.9 | `tiles` (4160 rows) | ~600 changed |
| `display.markers` | 4.8 | `units`, `herds` | present |
| `display.layers.culture` | 2.4 | `culture_layers` | **absent** |
| `display.overlays` | 0.95 | `overlays` (rasters) | present |

### 8.2 The bug: every diff-list section freezes at the baseline

`decode_delta_against` inserts `tile_updates`; `snapshot_dict` — the assembler the aggregator
re-enters — does **not** insert `tiles`. Only `snapshot_to_dict`, on the full-snapshot path, does.
So after the first frame `merged["tiles"]` is the baseline array and never moves again:

```
[TileProbe] turn=1 tiles=4160 grazesum=165813.449 updates=0
[TileProbe] turn=5 tiles=4160 grazesum=165813.449 updates=533 updsum=55098.063
[TileProbe] turn=9 tiles=4160 grazesum=165813.449 updates=605 updsum=62457.249
```

Byte-identical for nine turns while 400–600 tiles changed each turn. `TerrainPanel.gd` was
unaffected because it already applies `tile_updates` incrementally, keyed by entity; that is the
pattern the fix follows.

**And `tiles` was one instance of nine.** The delta publishes every diff-list section under a
`*_updates` key — `population_updates`, `culture_layer_updates`, `trade_link_updates`,
`influencer_updates`, `power_updates`, `generation_updates`, `discovery_progress_updates` — while
the client reads the *base* key, which the merge never writes. The worst of them is not tiles:

```
Main.gd:518   _hud_invoke("update_band_alerts", [snapshot["populations"]])
```

**Band alerts — food warnings, idle workers, predator-nearby — were frozen at the baseline**, along
with the harvest-site and scout-site map overlays (`MapView.gd:919`, `:1834`), the culture-layer map
(`:834`) and the trade overlay (`:1031`). The server confirms these are genuine diffs rather than
whole-section emits (`capture.rs:686`, `populations: diff_new(…)` with `removed_populations:
diff_removed(…)`), so the fix is merge-then-remove, keyed by each section's identity field.

Fixing only `tiles` would have left eight instances of a bug class this very page had just
documented. The decoder therefore gets **one generic keyed-section cache**, parameterised by
identity key, with `tiles` as a configured instance rather than a bespoke type.

**Why it shipped: `cargo xtask decode-guard` has no delta fixture.** The guard decodes a full
snapshot and a headerless one — the entire delta path, which is now the path that runs every turn,
had zero coverage. The fix adds a delta envelope to `decode_fixture.rs` and asserts the merged
frame carries the delta's values, not the baseline's.

**A guard that has never failed is not yet a guard.** Each assertion here was mutation-tested in
both directions before being believed: delete the patch and confirm the failure message names the
stale value; delete the fixture's river mutation and confirm the `tiles.rivers` name goes missing.
Writing the guard *first* is what made the difference between finding this class of bug and
documenting it.

**The generalisable lesson, and it is the §3.5 lesson's twin:** §3.5 was *a field nobody reads
costs the whole map every turn*. This one is **a field everybody reads, silently not arriving**.
Both are invisible in a running client — the world simply looks calm — and both were found only by
measuring the shipped representation rather than reasoning about the code. A cached-baseline
decoder makes "absent means unchanged" load-bearing for correctness, not just for bandwidth: every
key the merge does not write is a key asserting *nothing happened*, and it must be true.

### 8.3 The change manifest

A delta frame carries `changed_sections: PackedStringArray`. **Absent on a full snapshot, and
absence means "everything changed"** — so a consumer that has never heard of the manifest keeps
working, and a full snapshot is never gated.

The dangerous direction is an **under-complete** manifest: a consumer skips a section that really
did change and the world goes quietly stale, which is exactly §8.2's failure mode. So the manifest
is not a hand-maintained list — the delta path inserts through a helper that names the key and
writes it in one call, making it impossible to add a delta-carried key without naming it. The
aggregator's raster channels are re-derived from cache and therefore always *present*, so presence
cannot be their signal; they push an explicit name at each `apply_*` call site.

Two derived names carry information no section key can: `tiles.rivers` and `tiles.culture_layer`,
set by comparing each changed tile against the value it replaced. They are what let the client skip
the terrain splatmap rebuild (`display.shader`, 7.7 ms) — a tile changing its graze biomass must not
force six full-grid `PackedByteArray`s to be rebuilt, but a tile changing its river mask must.

**A name means "this moved", not "this was transmitted".** The codec emits several diff vectors as
always-present-but-empty, so naming them on presence would have had the manifest assert that eight
sections changed every turn — true of the wire, useless to a consumer. Measured on the live stack, a
steady-state delta names:

```
overlays.{sentiment,corruption,culture,military}, tiles, populations, influencers, power_nodes,
victory, stance_axes, herds, sedentarization, axis_bias, sentiment, crisis_telemetry, power_metrics
```

and **never** names `forage_patches`, `food_modules`, `discovered_sites`, `culture_layers`,
`trade_links`, `orders`, `overlays.{terrain,elevation,visibility,moisture}`, `tiles.rivers` or
`tiles.culture_layer` — which is exactly the list of blocks the client can stop rebuilding.

### 8.4 A third bug: `culture_tensions` cannot say "now empty"

Found while generalising the section cache. `culture_tensions` is a **whole-section** field typed as
a bare `Vec`, and `capture.rs` encodes "unchanged" as `Vec::new()`:

```rust
let delta_culture_tensions = if self.culture_tensions == culture_tensions_state {
    Vec::new()          // "unchanged"
} else { culture_tensions_state.clone() };
```

So "nothing changed" and "the last tension just resolved" are **the same bytes**, and the receiver
must guess. Read it as *replace* and every delta blanked `CulturePanel`; read it as *unchanged* and a
genuinely-emptied list stays stale until the next full snapshot. The decoder took the second reading
as the lesser evil, and the representation was fixed at the source: `Option<Vec<_>>`, matching the
idiom the rest of `WorldDelta` already uses.

**`WorldDelta` has exactly two conventions and every field must pick one**: a *diff list* is
`Vec<T>` **plus** a `removed_*` companion, where empty unambiguously means "no changes" because
removals are explicit; a *whole section* is `Option<Vec<T>>`, where `None` is unchanged and
`Some(vec![])` is now-empty. A bare `Vec` with no `removed_*` companion is neither, and is the shape
to grep for when the next one of these surfaces.

### 8.5 A fourth: two sections never decoded on the delta path at all

`food_modules` and `faction_inventory` were passed as `None` by `decode_delta_against`, so the
merged frame republished the baseline's food modules and **stockpiles** for the life of the world.
Same staleness class as §8.2, reached from the opposite direction: not a diff left unpatched but a
whole-section field never read. The HUD's stockpile line was frozen alongside the band alerts.

Four bugs, one arc, all invisible in a running client. The through-line: **when a decoder starts
caching, every key a consumer reads must be audited against every key the producer writes.** Before
the cache, that audit was free — the frame was the whole world by construction.

### 8.6 Result, and one parked decision revisited

| | before the arc | after the server half | after the client half |
|---|---|---|---|
| `decode` | ~80 | 0.36 | 0.36 |
| `display` | ~66 | 35.6 | **6.9** |
| ├ `sites.forage` | 10 | 10.3 | **0.0** |
| ├ `shader` | 7.5 | 7.9 | **0.0** |
| ├ `tiles` | 7 | 7.1 | **1.2** |
| ├ `layers.culture` | ~32 | 2.4 | **0.0** |
| └ `markers` | 7 | 4.7 | 4.7 (not gated — `herds` moves every turn) |
| `hud` | ~5 | 5.8 | 4.5 |
| client total | **~171** | ~45 | **~13 + inspector** |

**§4 parked the hidden Inspector; the measurement inverted the reasoning.** That decision accepted
"a hidden Inspector resuming its fan-out" when it was ~20 ms of ~171 (12%). After the work above it
is 16–30 ms of ~30 — **~60% of what the client still spends** — and it is spent rendering a panel
that ships hidden.

The fix §4 sketched was elaborate ("queue deltas while hidden, replay `cached full + queued deltas`,
capped, falling back to `resync`"). It is no longer needed, because the same property that makes
this arc work makes it unnecessary: **the skip only ever depended on self-containment, not on
payload kind**, and every merged frame is now self-contained. So the gate simply widens to both
kinds and `_cached_snapshot` holds the newest frame of either.

That couples two things that must stay coupled: the hidden skip is safe **only** while the decoder
keeps every base key patched (§8.2). If that regresses, the Inspector silently serves a stale panel
on show — so the code comment says so, and `decode_guard`'s section assertions are what hold the
other end.
