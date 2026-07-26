---
paths:
  - "clients/godot_thin_client/src/scripts/TurnProfile.gd"
  - "clients/godot_thin_client/src/scripts/Main.gd"
  - "clients/godot_thin_client/src/scripts/SnapshotLoader.gd"
  - "clients/godot_thin_client/src/scripts/MapView.gd"
  - "clients/godot_thin_client/native/src/bridge/decoder.rs"
---

# Client turn profiling — where a snapshot's cost goes

The client half of issue #384. The server's `turn.profile` covers the sim; this covers everything
between a frame arriving on the socket and the HUD being current. **The client is the expensive
half by an order of magnitude** — this rule exists so nobody optimizes the sim again on a hunch.

## Measured shape of one applied snapshot (80×52, live stack)

`decode` happens in `SnapshotLoader.poll_stream`, **before** `_apply_snapshot`, so it is a sibling
of `apply`, not a child. Everything else nests under `apply`.

| Phase | ms | note |
|---|---|---|
| `decode` / `decode.native` | ~80 | full FlatBuffers → Godot `Dictionary` tree, `snapshot_to_dict` |
| `apply` | ~91 | `display` + `hud` + `inspector` + `selection` + `scripting` |
| ├ `display` | ~66 | `MapView.display_snapshot` |
| │  ├ `display.layers.culture` | ~32 | **the single largest sub-block** — `duplicate(true)` per culture layer |
| │  ├ `display.sites.forage` | ~10 | same pattern: deep-copied forage patches |
| │  ├ `display.shader` | ~7.5 | six full-grid `PackedByteArray` splatmaps |
| │  ├ `display.tiles` | ~7 | the full-grid GDScript per-tile loop |
| │  └ `display.markers` | ~7 | unit + herd marker rebuild |
| ├ `inspector` | ~20 | **while hidden** — the un-skippable prefix; see below |
| └ `hud` | ~5 | the ~18 `_hud_invoke` fan-out |

For scale: the whole server turn is ~17 ms. **The client costs roughly ten times the sim.**

Two things this table is good for and one it is not. It is good for aiming an optimization and for
noticing a regression. It is **not** a partition — see the nesting rule below.

## The `TurnProfile` contract

`TurnProfile.gd` is the one helper. New per-turn timing goes through it rather than a fresh
`print` — that is what keeps the client's line readable next to the server's and keeps one flag in
charge of both.

- **Same model as `core_sim/src/turn_profile.rs`**: flat labels, dotted names carry the hierarchy,
  **a parent's number includes its children**, and entries order by first *open* so a parent prints
  ahead of its children. The phases **do not sum** to `apply`.
- **One flag: `SHADOW_SCALE_CLIENT_PROFILE`** (`TurnProfile.ENV_FLAG`), read once per launch,
  default from `DEFAULT_ENABLED`. It governs both the per-snapshot line and `MapView`'s rolling
  `_draw` average, so the two cannot disagree. Set `0`/`false`/`off`/`no` to silence it.
- Unlike the server's profiler, this one **is** gated, because the client prints to a console a
  player can see and a per-turn line in normal play is noise.
- `MapView` publishes its sub-phases on `last_display_profile` and `Main` splices them in as
  `display.*`; `SnapshotLoader` publishes `last_poll_*`. Neither prints — `Main` emits the single
  line. Keep it that way, or the halves drift apart.
- `decode.native` is the decoder's own `Instant`; `decode` is the GDScript wall time around it. The
  gap is Variant marshalling, and having both is what tells you which side to attack.
- The native getter is probed with `has_method`, so a stale extension degrades to `0` rather than
  erroring.

## Deep copies are the client's dominant cost

`display.layers.culture` (~32 ms) and `display.sites.forage` (~10 ms) are both
`duplicate(true)` loops over snapshot sub-trees — together ~25% of the client's per-turn cost.

The decoder builds a **fresh** Dictionary tree per frame and nothing shared survives into the next
one, so a deep copy of a sub-tree is only defensible if some consumer mutates it in place. Before
adding another `duplicate(true)` on a snapshot-derived structure, establish that a consumer
actually mutates it — the default should be to hold the reference.

## An offline fixture will mislead you here

A probe against `tests/fixtures/snapshot_envelope.bin` predicted `display.tiles` and
`display.shader` would dominate. Live, `display.layers.culture` is ~4× either of them — because the
fixture carries no culture layers, no orders and no annotations, so every block whose cost is
proportional to *content* reads ~0. **Measure against a live stack before believing a client
number.**

The live stack does not come up on its own: the packaged entry point is `res://src/ui/LandingScreen.tscn`,
a menu, and `_apply_snapshot` never runs until a world is loaded. Drive it with

```bash
scripts/run_stack.sh --port-base <base>          # or a release server + xtask new_game
STREAM_PORT=<base+2> COMMAND_PORT=<base+1> LOG_PORT=<base+3> \
  SHADOW_SCALE_CLIENT_PROFILE=1 \
  godot --path clients/godot_thin_client res://src/Main.tscn
```

going straight to `Main.tscn` to skip the landing screen, then `cargo xtask command --port <base+1> turn N`
and grep `[TurnProfile]`. Note `--port` must **precede** the verb.

## Key scripts

| Script | Purpose |
|--------|---------|
| `TurnProfile.gd` | The client's per-snapshot profiler: ordered `label=ms` accumulation (`start`/`begin`/`end`/`record_ms`/`absorb`/`render`/`emit`), the `SHADOW_SCALE_CLIENT_PROFILE` flag, and the shared `LINE_PREFIX`/`USEC_PER_MSEC`/`ENTRY_FORMAT` constants `Main`/`MapView` reuse. Mirrors `core_sim/src/turn_profile.rs` — flat labels, parent includes children, first-open ordering |
