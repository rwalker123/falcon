---
paths:
  - "clients/godot_thin_client/src/scripts/TurnProfile.gd"
  - "clients/godot_thin_client/src/scripts/Main.gd"
  - "clients/godot_thin_client/src/scripts/SnapshotLoader.gd"
  - "clients/godot_thin_client/src/scripts/MapView.gd"
  - "clients/godot_thin_client/src/scripts/SnapshotSections.gd"
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
| │  ├ `display.layers.culture` | ~32 | **was the single largest sub-block** — `duplicate(true)` per culture layer; the copy is gone (see below) |
| │  ├ `display.sites.forage` | ~10 | same pattern: deep-copied forage patches; also gone |
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

## Snapshot sub-trees are HELD BY REFERENCE — and must never be written to

`display.layers.culture` (~32 ms) and `display.sites.forage` (~10 ms) were `duplicate(true)` loops
over snapshot sub-trees — together ~25% of the client's per-turn cost (#389). Those copies are gone;
`MapView` now holds the frame's row dictionaries.

**A row belongs to the DECODER, not to the client, and it outlives the frame it arrived on.** The
older reading here — "the decoder builds a fresh Dictionary tree per frame and nothing shared
survives into the next one" — stopped being true when delta streaming landed. `SectionCache` merges
a delta by shallow-duplicating the cached array and writing only the changed slots
(`native/src/snapshot/cache.rs`: *pointer copies of the entry Variants — never a deep copy of a row
dictionary*), and `decode_frame` republishes the merged dict as the next baseline. So an unchanged
row is **literally the same `Dictionary` object frame after frame**, and a reference the client keeps
is a reference into the decoder's world.

That cuts both ways, and the two halves are why this is a rule rather than a preference:

- **Holding is free, and it is the default.** A changed row arrives as a NEW dictionary in the
  republished array, and every ingest is gated on its section being *named* in `changed_sections`, so
  the ingest that would have re-copied is exactly the one that re-reads. A copy buys nothing.
- **Writing into a held row edits the decoder's baseline**, and the edit survives into every later
  delta that does not replace the row. Nothing errors; the decoder's world simply carries a key the
  server never sent.

**Two ingests must stamp a derived key, and they take a SHALLOW `duplicate()` first** —
`_ingest_food_modules` (`terrain_id`, resolved off `terrain_overlay`) and `_ingest_population_sites`
(`module_label`). Shallow is the right depth in both cases: the stamp is a top-level key, so nothing
nested needs its own copy, and the nested values stay shared rather than re-allocated. That is the
whole exception — **`duplicate(true)` on a snapshot sub-tree needs a consumer that mutates it
nested-deep, and there is currently no such consumer.**

The forage patch is the measured reason the deep copy hurt: ~25 scalars *plus* a nested
`composition` array of per-species dictionaries (the flora roster), re-allocated for every patch on
the map on every frame that carried the section. The HUD half of the same data
(`HudBandLaborState.set_forage_patches` / `set_food_modules`, fed the same arrays by `Main`) has
always held these rows by reference — `MapView` was the outlier.

**Measured, live, 80×52, `earthlike` / seed 0 / `late_forager_tribe`, release server** — the frame
that CARRIES the sections (the full snapshot; a steady-state delta is already gated out of all of
them and is unchanged at `display` ≈ 6.1 ms either way):

| Phase | before | after |
|---|---|---|
| `display` | 37.18 | **26.49** |
| ├ `display.sites` | 9.33 | **0.86** |
| │  └ `display.sites.forage` | 9.22 | **0.76** |
| └ `display.layers.culture` | 2.50 | **1.03** |

So the win lands on exactly the frames that pay for the section — the first frame of a world, a
resync, and any turn whose diff moves forage or culture — which is also why the ~42 ms in #389's
title no longer reads off a steady-state turn: #388's gates had already taken those frames to zero.

The five ingests are named seams on `MapView` (`_ingest_culture_layers`, `_ingest_food_modules`,
`_ingest_discovered_sites`, `_ingest_forage_patches`, `_ingest_population_sites`) so that
`tools/snapshot_alias_guard.gd` can drive them headlessly with no tree and no rendering, the way
`marker_field_guard` drives `_rebuild_unit_markers`. The gate and the profile span stay at the call
site in `display_snapshot`; the clear and the refill stay together inside the helper.

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

## Skipping what did not change: the `changed_sections` manifest

**`snapshot.has(key)` is no longer a change signal, and every block that used it as one now runs
every turn.** Delta streaming ends with the decoder patching its cached world and republishing it
whole, so a merged delta carries every key — which is exactly what makes it indistinguishable from a
full snapshot, and exactly what destroys presence as evidence. The replacement is
`changed_sections`, a `PackedStringArray` the native decoder puts on every delta frame naming the
sections that actually MOVED (see `.claude/rules/client/native-extension.md`).

`SnapshotSections` is the only thing that reads it, and its three states are the whole risk:

| `changed_sections` | means | `changed()` |
|---|---|---|
| **absent** | a FULL snapshot, or a pre-manifest native build | `true` — everything changed |
| present, non-empty | a delta; these moved | membership |
| present and **EMPTY** | a delta in which nothing moved | `false` |

Reading *absent* as "nothing changed" freezes the world on the one frame that must repaint — first
frame, resync, new game. Reading *empty* as absent un-gates every block on the quiet frames that are
the entire point; and GDScript's `if array:` is false for an empty array exactly as for `null`, so
the natural-looking test collapses the two. Hence the explicit `has()` before the membership test.

**Pair the manifest with `has()`, do not replace it.** `has()` is still the right outer guard: an
absent key means the frame never carried the section, and where a comment calls that guard
load-bearing (`pending_forks` — absence means "unchanged", never "cleared") it stays load-bearing.

**A gate must cover the CLEAR as well as the refill.** `MapView`'s sites blocks get erasure for free
by wiping their lookup before refilling it; gating the refill while leaving the clear outside
publishes an empty world. That is why gating `sites.food` / `sites.discovered` / `sites.forage`
forced the untangle of their accidental nesting inside `if food_variant is Array` — a block must
never be gated on a key it does not read.

Measured per-turn cost of what is now gated (80×52 live, and *before* the deep copies came out —
`sites.forage` and `layers.culture` are ~12× and ~2.4× cheaper now, see the section above):
`sites.forage` 10.0 ms on a section a steady-state delta never carries, `shader` 7.7 ms (its inputs are terrain / fog / elevation / the
river masks and nothing else — which is why the decoder reports `tiles.rivers` apart from `tiles`),
`tiles` 6.9 ms, `layers.culture` 2.4 ms. **`markers` is deliberately NOT gated**: `herds` and
`populations` are named on essentially every turn, so there is nothing to win, and unit markers are
the most visible thing on screen. **`_invalidate_map_cache()` is deliberately NOT gated either**,
though the minimap beside it is: `CachedMapRenderer._draw` paints `_tile_color`, which follows the
ACTIVE OVERLAY channel, so a gate on terrain/fog would freeze the overlay-tinted map. The minimap's
own image is a full-grid loop over terrain + visibility ONLY — read off `_rebuild_image` rather than
assumed — so it gates cleanly on those two.

## `display.tiles` incremental, and the erasure trap

Once `tiles` is honest, the tile loop can walk the ~600 changed rows instead of all 4,160. Both
paths funnel through one `_ingest_tile`, which is what stops them drifting; the full rebuild is
taken whenever the incremental one cannot be proved safe — no manifest (a full snapshot), a grid
resize, lookups not yet built, or no sparse `tile_updates` list to walk.

**Clear-and-refill gets erasure for free; incremental does not.** Every conditional insert in
`_ingest_tile` therefore has an explicit `erase` on its else branch — a tile whose graze capacity
falls to 0, whose river mask clears, or which stops reporting habitability must LOSE its entry, or
the lookups accumulate stale rows silently. After a clear, `erase` on an absent key is a no-op, so
the two paths produce identical lookups from identical rows. This was mutation-tested: deleting one
`erase` fails the delta probe on three assertions.

## Verifying a gating change

`map_preview` is a **bit-identity reference** (61/61 across runs) but it feeds only full snapshots,
so it proves the refactor did not change full-snapshot rendering — and can say nothing about the
gates, which are all no-ops there. The delta path needs its own probe: instantiate `MapView`, feed a
full snapshot, then feed a frame carrying `changed_sections` plus DELIBERATELY EMPTIED arrays for
the sections the manifest does not name. Emptying them is the trick — it is what distinguishes "the
gate skipped" from "the gate ran and got the same answer".

## Key scripts

| Script | Purpose |
|--------|---------|
| `SnapshotSections.gd` | The ONE reader of the delta frame's `changed_sections` manifest: `changed()`, `any_changed()`, `has_manifest()`. All-static, no node state, `class_name`d (used from `MapView` and `Main`). Absent manifest → `true` for everything, so a full snapshot / resync / older native build is never gated. See the section above for the three states |
| `TurnProfile.gd` | The client's per-snapshot profiler: ordered `label=ms` accumulation (`start`/`begin`/`end`/`record_ms`/`absorb`/`render`/`emit`), the `SHADOW_SCALE_CLIENT_PROFILE` flag, and the shared `LINE_PREFIX`/`USEC_PER_MSEC`/`ENTRY_FORMAT` constants `Main`/`MapView` reuse. Mirrors `core_sim/src/turn_profile.rs` — flat labels, parent includes children, first-open ordering |
