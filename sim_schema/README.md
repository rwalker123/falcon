# sim_schema

Pure data contracts for the Shadow-Scale simulation stack. This crate defines
snapshots, deltas, axis bias payloads, and generation metadata. It is consumed
by both the runtime (`sim_runtime`, `core_sim`) and tooling (the Godot thin
client plus external consumers) and purposely avoids Bevy or other heavy
dependencies.

## Terrain Overlay Channel
- `WorldSnapshot` now carries a `terrainOverlay` table (width, height, packed
  samples of `TerrainType` + `TerrainTags`).
- `WorldDelta` mirrors the same table whenever the raster changes so clients can
  redraw map biomes without re-deriving from component state.
- Consumers should prefer the overlay for large renders while keeping tile-level
  data for debugging.

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
