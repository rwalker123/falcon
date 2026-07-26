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

## 4. What this does NOT try to do

**Do not design this around the Inspector.** Standing decision (Ray, 2026-07-26): the Inspector is
legacy scaffolding, expendable, and slated for rework after this arc. #391's hidden-Inspector gate
assumes deltas are rare; this inverts that. Both consequences — a hidden Inspector resuming its
fan-out, and the show-time catch-up replaying a cached *full* snapshot while later deltas are lost
— are **accepted**, not worked around. The eventual fix (queue deltas while hidden, replay
`cached full + queued deltas`, capped, falling back to `resync`) is recorded in #386 and is not a
prerequisite here. #390 is parked behind this for the same reason.

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
