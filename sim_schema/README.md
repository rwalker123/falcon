# sim_schema

Pure data contracts for the Shadow-Scale simulation stack. This crate defines
snapshots, deltas, axis bias payloads, and generation metadata. It is consumed
by both the runtime (`sim_runtime`, `core_sim`) and tooling (the Godot thin
client plus external consumers) and purposely avoids Bevy or other heavy
dependencies.

## Module map

The crate is partitioned along the **same ten domain sections `schemas/snapshot.fbs`
uses**, so each feature arc appends into its own file instead of colliding in one
6k-line module. `src/lib.rs` is module declarations plus glob re-exports, so every
item is still reachable as `sim_schema::Foo` — consumers never name a submodule.

| Path | Contents |
|---|---|
| `src/state/map.rs` | `TileState`, `TerrainType`/`TerrainTags`/`TerrainSample`, `MountainKind`, terrain & elevation overlays, `ClimateBandsState`, `TemperatureSurvivabilityState`, `StartMarkerState`, `RiverClass`/`RiverChannel`, `ScalarRasterState`/`FloatRasterState` |
| `src/state/economy.rs` | faction inventories, `KnownTechFragment` (the logistics/trade-link states were demolished in arc #527 — `docs/plan_contact_and_logistics.md` §As-built; their `.fbs` tables survive as `(deprecated)` slots) |
| `src/state/population.rs` | cohorts, demographics, generations, labor assignments, harvest/scout tasks, stockpiles |
| `src/state/subsistence.rs` | herds + herd telemetry, forage/graze registries, forage patches, food modules, sedentarization, intensification knowledge, `GRAZE_PHASE_*` |
| `src/state/knowledge.rs` | the leak ledger + countermeasures/infiltrations/modifiers, knowledge timeline & metrics, great discoveries, discovered sites |
| `src/state/governance.rs` | power nodes/incidents/telemetry, corruption ledger, crisis gauges + overlay |
| `src/state/culture.rs` | culture layers/traits/tensions, influential individuals, influence domains, sentiment telemetry |
| `src/state/campaign.rs` | campaign profiles, command events, victory, and the whole Telling family (beats, voice, forks, stance) |
| `src/state/connections.rs` | `ConnectionState` — the directed band-to-band tie contact leaves behind (arc #527, `docs/plan_contact_and_logistics.md` §Q2). Deliberately carries **no** faction column and no rider vocabulary: faction is a property of the endpoints, and logistics/culture/knowledge each own their own state |
| `src/world.rs` | the deliberately **flat** `WorldSnapshot`/`WorldDelta`, `SnapshotHeader`, `hash_snapshot`, `MapExport`, and the **JSON** codecs (`encode_/decode_snapshot_json`, `_delta_json` — the only pair here with both directions; `MapExport` rides JSON too). **There is no bincode codec** — `finalize`/`hash_snapshot` call `bincode::serialize` inline purely to get bytes to hash, and nothing anywhere bincode-*decodes* these structs: the bincode snapshot socket was retired in #388, and the frames were never decodable anyway (`skip_serializing_if` omits fields a non-self-describing format still expects back) |
| `src/codec/mod.rs` | `encode_snapshot_flatbuffer`/`encode_delta_flatbuffer` and their inverses `decode_snapshot_flatbuffer`/`decode_delta_flatbuffer`/`decode_frame_flatbuffer` (→ `FramePayload`, errors as `DecodeError`), the `build_*_flatbuffer` / `decode_*_table` envelope assembly, and helpers shared by two or more sections in both directions (`create_scalar_raster`/`decode_scalar_raster`, `create_float_raster`/`decode_float_raster`, `create_known_fragments`/`decode_known_fragments`, the `map_rows`/`decode_rows` vocabulary). Its `round_trip_tests` are the decoder's definition of done: the saturated fixture must survive encode → decode → encode byte for byte |
| `src/codec/<section>.rs` | that section's `serialize_<section>_section` + `_delta` **and** `decode_<section>_section` + `_delta`, plus the `create_*`/`to_fb_*` helpers and their `decode_*`/`to_state_*` inverses, side by side so a field is added in one file for both directions. Every decoded struct is an **exhaustive** literal (no `..Default::default()`), so an appended field fails to compile until decoded; an unknown enum discriminant is a `DecodeError`, never a default. `vision` is codec-only — its state is the rasters in `state/map.rs` |
| `src/apply_delta.rs` | `WorldSnapshot::apply_delta` / `ApplyDeltaError` — the consumer of `core_sim`'s three diff shapes (`diff_indexed` → upsert-by-key then sweep `removed_*`, `diff_whole` → `Some` replaces and `Some(empty)` clears, `diff_appended` → append by new `seq` then trim by tick window), gated on `base_frame_seq`/`world_epoch`, destructuring the delta exhaustively so a new field has to be given a rule. `core_sim/tests/apply_delta_producer.rs` proves it against the shipped publication path |
| `src/fixture.rs` | `saturated_snapshot()` — the one `WorldSnapshot` with every section, repeated field and scalar leaf populated. Consumed by this crate's codec round trip **and** by `cargo xtask decode-fixture` for the Godot decode guard; `assert_no_empty_arrays` refuses to build it with an unseeded `Vec`, which is what makes "every section is covered" a checked claim rather than a hope |

**The rule when you add a snapshot field:** append it to your section's
`state/` file *and* that section's `codec/` file — serializer **and** decoder, which
the exhaustive decode literal will insist on (and to your section table in
`schemas/snapshot.fbs`, which is append-only — see the FlatBuffers slot-order
discipline). If it is a `Vec`, seed it in `fixture.rs` or the fixture refuses to
build. Nothing else should need to change. If a codec helper gains a second
section as a consumer, hoist it to `codec/mod.rs` rather than duplicating it.

**Then run `cargo xtask decode-guard`** — "nothing else should need to change" is
true of *this* crate, but a field the client never decodes is invisible from here,
and the Godot decoder has silently dropped an appended field six times (see
`clients/godot_thin_client/CLAUDE.md` → Native Extension). The guard decodes a
synthetic snapshot in which every section is populated and diffs the resulting
dictionary against a golden; your new field should show up as a new line whose
value is its own wire path. Two things will stop you first if you skip it: a new
**repeated** field fails the fixture build until it is seeded, and a field added to
one of the state structs without a `Default` fails to compile
(`src/fixture.rs` holds exhaustive literals for those on purpose, and every decoder
literal in `codec/` is exhaustive for the same reason).

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
