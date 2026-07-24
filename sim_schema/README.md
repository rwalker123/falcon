# sim_schema

Pure data contracts for the Shadow-Scale simulation stack. This crate defines
snapshots, deltas, axis bias payloads, and generation metadata. It is consumed
by both the runtime (`sim_runtime`, `core_sim`) and tooling (the Godot thin
client plus external consumers) and purposely avoids Bevy or other heavy
dependencies.

## Module map

The crate is partitioned along the **same nine domain sections `schemas/snapshot.fbs`
uses**, so each feature arc appends into its own file instead of colliding in one
6k-line module. `src/lib.rs` is module declarations plus glob re-exports, so every
item is still reachable as `sim_schema::Foo` — consumers never name a submodule.

| Path | Contents |
|---|---|
| `src/state/map.rs` | `TileState`, `TerrainType`/`TerrainTags`/`TerrainSample`, `MountainKind`, terrain & elevation overlays, `ClimateBandsState`, `StartMarkerState`, `RiverClass`/`RiverChannel`, `ScalarRasterState`/`FloatRasterState` |
| `src/state/economy.rs` | logistics links, trade links + knowledge, faction inventories |
| `src/state/population.rs` | cohorts, demographics, generations, labor assignments, harvest/scout tasks, stockpiles |
| `src/state/subsistence.rs` | herds + herd telemetry, forage/graze registries, forage patches, food modules, sedentarization, intensification knowledge, `GRAZE_PHASE_*` |
| `src/state/knowledge.rs` | the leak ledger + countermeasures/infiltrations/modifiers, knowledge timeline & metrics, great discoveries, discovered sites |
| `src/state/governance.rs` | power nodes/incidents/telemetry, corruption ledger, crisis gauges + overlay |
| `src/state/culture.rs` | culture layers/traits/tensions, influential individuals, influence domains, sentiment telemetry |
| `src/state/campaign.rs` | campaign profiles, command events, victory, and the whole Telling family (beats, voice, forks, stance) |
| `src/world.rs` | the deliberately **flat** `WorldSnapshot`/`WorldDelta`, `SnapshotHeader`, `hash_snapshot`, `MapExport`, and the bincode/JSON codecs |
| `src/codec/mod.rs` | `encode_snapshot_flatbuffer`/`encode_delta_flatbuffer`, the `build_*_flatbuffer` envelope assembly, and helpers shared by two or more sections (`create_scalar_raster`, `create_float_raster`, `create_known_fragments`) |
| `src/codec/<section>.rs` | that section's `serialize_<section>_section` + `_delta` plus the `create_*`/`to_fb_*` helpers only those two use. `vision` is codec-only — its state is the rasters in `state/map.rs` |

**The rule when you add a snapshot field:** append it to your section's
`state/` file *and* that section's `codec/` file (and to your section table in
`schemas/snapshot.fbs`, which is append-only — see the FlatBuffers slot-order
discipline). Nothing else should need to change. If a codec helper gains a second
section as a consumer, hoist it to `codec/mod.rs` rather than duplicating it.

## Terrain Overlay Channel
- `WorldSnapshot` now carries a `terrainOverlay` table (width, height, packed
  samples of `TerrainType` + `TerrainTags`).
- `WorldDelta` mirrors the same table whenever the raster changes so clients can
  redraw map biomes without re-deriving from component state.
- Consumers should prefer the overlay for large renders while keeping tile-level
  data for debugging.

## Map Export (offline inspection & test fixtures)

`MapExport` bundles a full `WorldSnapshot` with the resolved worldgen `seed`,
`preset`, and grid `width`/`height` so a running game's exact map can be dumped
to a single self-describing JSON file — reproducible and inspectable offline.

- Written by the server's `export_map` command (see `core_sim` server) into the
  gitignored `exports/` scratch dir; the Godot Terrain tab has an **Export Map**
  button that triggers it.
- Round-trip helpers: `encode_map_export_json` / `decode_map_export_json`.
  `MapExport::from_snapshot` derives `width`/`height` from the terrain overlay so
  they can never desync from the sample buffer.
- `MapExport::tile_at(x, y)` resolves a terrain sample by **row-major `(x, y)`** —
  the same coordinate the Godot inspector shows as `@x,y` — so tests (and agents)
  can reference a hex by coordinate. See `integration_tests/tests/map_fixture.rs`
  for the round-trip + per-hex assertion pattern.

## Pending Culture Payload Additions

To stay ahead of the culture subsystem work, the FlatBuffers schema will pick up
new enums and tables so downstream code can rely on stable contracts:

- `CultureLayerScope` (Global/Regional/Local) and `CultureTensionKind`
  (DriftWarning/AssimilationPush/SchismRisk) describe layer granularity and
  forecast buckets.
- `CultureTraitAxis` enumerates the 15 culture axes captured in the game manual
  (passive↔aggressive, open↔closed, … , pluralistic↔monocultural). Tooling can
  drive overlays without hard-coded strings.
- `CultureTraitEntry` bundles baseline, modifier, and resolved values (scaled
  `long`) for each axis so clients can separate inherited weight from local
  adjustments.
- `CultureLayerState` carries the serialized layer (id/owner/parent/scope,
  trait vector, divergence metrics, last update tick).
- `CultureTensionState` records pending drift events surfaced to the Cultural
  Inspector (layer id, scope, severity, timer, tension kind).
- `WorldSnapshot`/`WorldDelta` will export `cultureLayers`,
  `removedCultureLayers`, and `cultureTensions` sequences once the schema change
  lands.

These definitions live in `schemas/snapshot.fbs`; once merged, regenerate the
bindings via `make flatbuffers` (or the `shadow_scale_flatbuffers` helper) so
`sim_runtime`, `core_sim`, and client crates pick up the new payloads.
