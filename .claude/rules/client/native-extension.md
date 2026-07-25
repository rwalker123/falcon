---
paths:
  - "clients/godot_thin_client/native/src/**"
  - "clients/godot_thin_client/native/Cargo.toml"
---

<!-- Extracted verbatim from lines 243-336 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Native extension — the GDExtension module map

## Native Extension
`native/` contains GDExtension bindings for FlatBuffers decoding (generated from `sim_schema/schemas/snapshot.fbs`).

### Module map (`native/src/`)
The decoder was one 5,617-line `lib.rs`; it is now split along **the same nine domain
sections `snapshot.fbs` uses**, mirroring the `sim_schema/src/{state,codec}` split on the
server side, so the two ends of the wire have the same shape.

| Module | Holds |
|--------|-------|
| `lib.rs` | The gdextension entry point (`ShadowScaleExtension` + `entry_symbol`) and the crate's public re-exports. Nothing else — no decode logic |
| `bridge/command.rs` | `CommandBridge` (`#[godot_api]`), the command worker thread, `command_sender`, `resolve_entry_path` |
| `bridge/script_host.rs` | `ScriptHostBridge` (`#[godot_api]`) over the embedded script runtime |
| `bridge/decoder.rs` | `SnapshotDecoder` (`#[godot_api]`) + the free `decode_snapshot` / `decode_delta`. **The only entry into the decode path** (`SnapshotLoader.gd` is its one caller) |
| `bridge/variant.rs` | `Variant` ↔ `serde_json` marshalling shared by the bridges |
| `snapshot/mod.rs` | The two top-level assemblers: `snapshot_dict` (rasters + sections → the client dict) and `snapshot_to_dict` (walks a `WorldSnapshot`) |
| `snapshot/raster.rs` | `GridSize`, `OverlaySlices`, `TerrainSlices`, `OverlayChannelParams`, `packed_from_slice`, `insert_overlay_channel`, `normalize_overlay` |
| `snapshot/delta.rs` | `DeltaAggregator` + `CrisisAnnotationRecord` — a delta carries only changed sections, so it accumulates them into full-snapshot shape and re-enters `snapshot_dict` |
| `dict/mod.rs` | ONLY the leaf helpers with consumers in two or more sections: `strings_to_variant_array`, `string_vector_to_packed`, the `u16/u32/u64_vector_to_packed_*` packers, `fixed64_to_f32` / `fixed64_to_f64` |
| `dict/{map,economy,population,subsistence,knowledge,governance,culture,campaign}.rs` | The ~60 `*_to_dict` / `*_to_array` / `*_label` converters, one module per `snapshot.fbs` section |

There is deliberately **no `dict/vision.rs`** — the vision section is only the
fog/visibility/military rasters, which `snapshot/raster.rs` and the assemblers already
own (`sim_schema` makes the same call: a `codec/vision.rs`, no `state/vision.rs`).

**The rule for a new snapshot field: its converter goes in its section's `dict/` module** —
the section is whichever `.section()` accessor `snapshot_to_dict` reaches it through. Put a
helper in `dict/mod.rs` only once a *second* section needs it, and hoist rather than
duplicate. Fixed-point (`Scalar`, 1e6) fields go through `fixed64_to_f32`/`_f64`, never an
inline divide — and a new `Scalar` **cohort** field belongs in `CohortScalars`
(`dict/population.rs`), which is the one part of this crate `cargo test` can reach
(`VarDictionary` cannot be built outside a live engine).

`population_to_dict` decodes two **Predators Phase 3** cohort keys (appended after `fodderStore` in
the schema): `raid_radius` ← `cohort.raidRadius()` (a plain `uint` reach, `as i64` — like `work_range`,
NOT a Scalar), the odd-r hex distance within which an aggressive carnivore herd raids this band's
larder; and `raid_forfeit` ← `cohort.raidForfeit()` (`float`, `as f64`), the food this band lost to
raids THIS turn — the raid twin of `pen_feed_upkeep`. Both are consumed client-side by the band panel:
`raid_radius` derives the "Predator nearby" Warrior alert (the DANGER itself is derived on the client
from visible-herd telemetry, never a wire flag), `raid_forfeit` is the "Lost to raids" food-ledger row.

**The whole path is gated by `tools/decode_guard.gd`** (see its Key Scripts row) — the answer to
"`VarDictionary` cannot be built outside a live engine", which is why the coverage here was a single
`cohort_decode_tests` module for so long. Run it from the workspace root:

```bash
cargo xtask decode-guard                  # regenerate fixture → build native → diff the golden
cargo xtask decode-guard --write-golden   # re-record after an INTENDED decode change
```

**When you append a snapshot field, that command is what tells you the decoder actually emitted
it.** The golden gains a line carrying the field's own wire path as its value; if the new key does
not appear, the converter was never wired up — the "decoded in `native/src/lib.rs`" bug this file
records **six** times. Two forcing functions sit under it, both in `xtask/src/decode_fixture.rs`:
appending a **repeated** field fails the fixture build until it is seeded (`assert_no_empty_arrays`
names the path), and appending to one of the state structs that has no `Default` fails the *compile*
(those blanks are exhaustive literals on purpose).

**Those two forcing functions reach CI; the golden diff does not.** CI has no Godot and the decoder
returns a `VarDictionary`, so the diff is a **local** gate — but `xtask`'s own `cargo test` builds
the fixture, which means the unseeded-repeated-field alarm and the fixture's determinism are checked
on every PR. Run `cargo xtask decode-guard` yourself for the part CI cannot.

**A MALFORMED snapshot must DEGRADE, never panic — and `snapshot_to_dict` returns `Option` to say
so.** `snapshot.fbs` marks nothing `required` and `root_as_envelope` verifies table STRUCTURE only,
so a verifiable payload can still be missing a field the decoder needs. Today that is the `header`:
absent, it answers `None`, which reaches the loader as an empty dictionary and the frame is skipped
(`SnapshotLoader.poll_stream` already had that branch). **Dropping the frame is deliberate and is
the rule for any field the decoder cannot do without** — the header carries the frame's identity
(`tick`, `worldEpoch`) and the grid's topology (`wrapHorizontal`), each with a plausible-looking
zero, so filling in defaults publishes a coherent-looking world that is quietly wrong instead of one
that is honestly absent. The delta path reaches the same "never unwrap" outcome its own way
(`if let Some(header)` in `bridge/decoder.rs`) and is what the snapshot path was inconsistent with.
Both halves are gated: the headerless fixture pins the empty-dictionary contract, and the xtask
runner fails the run on a caught Rust panic (see the `decode_guard.gd` Key Scripts row).

> Doc references elsewhere in this file of the form "decoded in `native/src/lib.rs
> `*fn*`" predate the split — the named function now lives in its section's module above
> (e.g. `herds_to_array` → `dict/subsistence.rs`, `tile_to_dict` → `dict/map.rs`,
> `population_to_dict` → `dict/population.rs`). The function names did not change.

> **Note:** Elevation is not rendered as 3D relief. A shallow-3D heightfield view was
> prototyped and permanently removed; elevation is surfaced as the 2D **Elevation
> Heatmap** overlay and as a per-tile **Height** readout in the tile panels (the HUD
> selection panel via `MapView._tile_info_at` → `Hud._tile_summary_lines`, and the
> Inspector Terrain tab). All read the same normalized `ElevationOverlay.samples` raster —
> there is no per-tile elevation on `TileState`. **Height is a relative 0..100 indicator**
> (a number + filled/empty bar), NOT meters: it exists so a player can reason about line of
> sight — a higher tile can occlude the tile behind it (matching the LOS raycast in
> `visibility_systems.rs`). `MapView.relative_height_at` rescales the above-sea-level span
> into 0..100 (at/below sea level reads 0, since open water occludes nothing). The sea level
> is the **active map's** `sea_level`, streamed per-snapshot as `ElevationOverlay.seaLevel`
> (pre-normalized server-side to the raster's [min,max] scale) and read into
> `MapView._elevation_sea_level` — no hardcode; `HEIGHT_DEFAULT_SEA_LEVEL` is only the
> pre-first-snapshot fallback. `MapView.format_height` is the single source of truth for the
> number+bar formatting. The
> raster still streams from the core for the heatmap and for gameplay (LOS), but the
> per-vertex `normals` field (3D-only) was dropped from the schema. See
> `docs/architecture.md` → "Removed: 3D Relief Rendering".

---

