# Decomposition: snapshot pipeline + systems/god-file split

**Status:** in progress (single PR, four steps)
**Goal:** Cut cross-workflow merge conflicts by giving each subsystem arc its own
edit surface. Today a handful of shared "hero" files sit on the path of nearly
every feature arc, so concurrent PRs collide constantly.

## Why (measured)

Over the last 200 commits (125 of them merges), co-change coupling with
`sim_schema/schemas/snapshot.fbs` is extreme — every arc that adds a snapshot
field edits the same six files in lockstep:

| File | co-changes w/ `.fbs` | lines |
|---|---|---|
| `sim_schema/schemas/snapshot.fbs` | 72 | 1.3k |
| `sim_schema/src/lib.rs` | 72 | 4.8k |
| `core_sim/src/snapshot.rs` | 66 | 6.1k |
| `clients/godot_thin_client/native/src/lib.rs` | 49 | 4.8k |
| `core_sim/src/systems.rs` | 42 | 8.6k |
| `MapView.gd` / `Hud.gd` | 38 / 31 | 5.8k / 5.8k |

Two structural causes:
1. **Flat append-target tables.** `WorldSnapshot` (~46 fields) and `WorldDelta`
   (~50) are single flat tables. FlatBuffers field slots are positional and
   append-only (see memory `flatbuffers-append-only-merges`), so two branches that
   both append a field collide on slot order *by construction*.
2. **God-files.** `systems.rs` (76 top-level items across 6 unrelated
   subsystems), `snapshot.rs` (74 items), `MapView.gd` (224 funcs), `Hud.gd`
   (273 funcs) each aggregate work that belongs to different arcs.

`no-back-compat-yet` applies: no shipped saves or external clients, so the wire
format may be restructured cleanly — **no migration/fallback code**.

---

## Step 1 — Repartition the snapshot schema into domain sections

Replace the two flat root tables with root tables that nest one **section table
per subsystem**. Each arc then appends fields to *its* section table; two arcs
touch different tables and never collide on slot order.

### Section partition

| Section table | Fields (from `WorldSnapshot`) | Delta `removed*` lists |
|---|---|---|
| `MapSection` | tiles, terrainOverlay, elevationOverlay, moistureRaster | removedTiles |
| `EconomySection` | logistics, tradeLinks, logisticsRaster, factionInventory | removedLogistics, removedTradeLinks |
| `PopulationSection` | populations, demographics, generations | removedPopulations, removedGenerations |
| `SubsistenceSection` | herds, foragePatches, sedentarization, intensificationKnowledge, foodModules | — |
| `KnowledgeSection` | greatDiscoveryDefinitions, greatDiscoveries, greatDiscoveryProgress, greatDiscoveryTelemetry, knowledgeLedger, knowledgeTimeline, knowledgeMetrics, discoveredSites, discoveryProgress | removedKnowledgeLedger |
| `GovernanceSection` | power, powerMetrics, corruption, corruptionRaster, crisisTelemetry, crisisOverlay | removedPower |
| `CultureSection` | cultureLayers, cultureTensions, cultureRaster, influencers, axisBias, sentiment, sentimentRaster | removedInfluencers, removedCultureLayers, removedCultureTensions |
| `VisionSection` | fogRaster, visibilityRaster, militaryRaster | — |
| `CampaignSection` | campaignProfiles, commandEvents, victory | — |

`header:SnapshotHeader` and `capabilityFlags:uint` stay at the root of both
`WorldSnapshot` and `WorldDelta`.

The `removed*` lists live inside their section table (nullable; the snapshot
builder leaves them unset, the delta builder populates them). One set of section
tables serves both root types.

New root shape:

```
table WorldSnapshot {
  header:SnapshotHeader;
  capabilityFlags:uint;
  map:MapSection;
  economy:EconomySection;
  population:PopulationSection;
  subsistence:SubsistenceSection;
  knowledge:KnowledgeSection;
  governance:GovernanceSection;
  culture:CultureSection;
  vision:VisionSection;
  campaign:CampaignSection;
}
// WorldDelta identical (sections carry the removed* lists).
```

Append discipline **within** a section table still holds: new fields go strictly
after shipped ones. The win is that arcs are now partitioned across nine tables.

### Layer-by-layer work

The snapshot is a 4-layer stack; touch each:

1. **`sim_schema/schemas/snapshot.fbs`** — define the nine section tables, rewrite
   `WorldSnapshot`/`WorldDelta` to nest them. Keep every existing leaf table
   (`TileState`, `HerdTelemetryState`, …) unchanged.
2. **Generated bindings** — regenerate via `cargo xtask` (builds
   `shadow_scale_flatbuffers`); do not hand-edit generated code.
3. **`sim_schema/src/lib.rs` (façade)** — **keep the public `WorldSnapshot` /
   `WorldDelta` Rust structs FLAT** (fields stay top-level) so `core_sim` is
   unaffected. Only their `serialize`/`deserialize` impls change: route each
   field through its section table. **Split serialize/deserialize into one helper
   fn per section** (`serialize_map_section`, `deserialize_map_section`, …) so
   future field additions localize to one helper.
4. **`core_sim/src/snapshot.rs`** — unchanged by the nesting (it builds the flat
   façade struct). (It is split separately in Step 3.)
5. **`clients/godot_thin_client/native/src/lib.rs`** — the ~45 read sites read the
   generated accessors directly (`snapshot.tiles()`, `delta.herds()`). Update each
   to go through its section: `snapshot.map().and_then(|m| m.tiles())`,
   `delta.subsistence().and_then(|s| s.herds())`, etc. Preserve the existing
   `Option`/empty handling exactly.

**Behavior contract:** byte-for-byte the same data reaches the client; only the
wire nesting changes. Verify snapshot/delta round-trip tests still pass.

**Decision (blast-radius bound):** the façade struct stays flat rather than
nesting to match the wire. Rationale: a flat Rust struct field-append is a
line-level git merge (not a positional-slot collision), so the flat struct is not
a real conflict source, and keeping it flat leaves `core_sim/snapshot.rs` and its
callers untouched. `native/lib.rs` remains a single-file hotspot; splitting it is
noted as a possible follow-up, out of scope here.

---

## Step 2 — Split `systems.rs` into a `systems/` module

Pure code motion. `core_sim/src/systems.rs` → `core_sim/src/systems/mod.rs` +
submodules, grouped by the clusters already present in the file:

| Submodule | Contents (approx. current line ranges) |
|---|---|
| `systems/worldgen.rs` | spawn_initial_world, tag-budget solver, biome palette clamp, coastal shelf, population cluster/profile spawn, starting knowledge/inventory (191–2960) |
| `systems/trade.rs` | simulate_materials, simulate_logistics, trade_knowledge_diffusion, publish_trade_telemetry (3000–3200) |
| `systems/population.rs` | demographics, morale pressure, discontent, output/migration fractions, simulate_population (3200–3650) |
| `systems/expeditions.rs` | advance_band_movement, advance_expeditions, hunt_* , simulate_hunt_trip, HuntTripForecast (3648–4640) |
| `systems/labor.rs` | advance_labor_allocation, advance_population_migration, step_toward (4641–5300) |
| `systems/power.rs` | simulate_power, process_culture_events, advance_tick, process_corruption (5305–end) |

- Keep shared param structs (`LogisticsSimParams`, `PowerSimParams`, etc.) and
  small helpers with their primary consumer; if used across submodules, hoist to
  `systems/mod.rs`.
- **`mod.rs` re-exports every previously-public item** (`pub use worldgen::*;` …)
  so no external caller (`lib.rs`, `server.rs`, tests) changes. External call
  sites must remain identical.
- Cargo fmt/clippy -D warnings/test all green with no behavior change.

---

## Step 3 — Split `snapshot.rs` into a `snapshot/` module

Pure code motion, mirroring the Step 1 sections. `core_sim/src/snapshot.rs` →
`core_sim/src/snapshot/mod.rs` + submodules:

- `snapshot/capture.rs` — `capture_snapshot`, the broadcast/update/delta driver,
  `restore_world_from_snapshot`.
- `snapshot/map.rs`, `snapshot/economy.rs`, `snapshot/population.rs`,
  `snapshot/subsistence.rs`, `snapshot/knowledge.rs`, `snapshot/governance.rs`,
  `snapshot/culture.rs`, `snapshot/vision.rs`, `snapshot/campaign.rs` — the
  per-domain `*_state` / `snapshot_*` / raster builder helpers.
- Shared diff helpers (`diff_new`, `diff_removed`) and raster math to
  `snapshot/mod.rs` or a `snapshot/raster.rs`.
- `mod.rs` re-exports the public surface; external callers unchanged.
- Tests move with their targets (or stay in `mod.rs`); all green.

---

## Step 4 — Extract `MapView.gd` and `Hud.gd`

Composition over one god-object (Godot). Behavior/visuals unchanged; verify with
the `ui_preview` PNG harness.

- **`MapView.gd`** (224 funcs): extract per-overlay rendering into dedicated
  helper scripts (one per overlay family — terrain/rasters, fauna/pasture,
  vision/fog, culture, governance), owned by MapView and called from it. Keep
  MapView as the coordinator (input, camera, selection, tile picking).
- **`Hud.gd`** (273 funcs): extract cohesive panel/widget controllers (command
  feed, resource/food readouts, turn-orb wiring, nav rail) into separate scripts
  attached to their nodes; Hud retains top-level layout + signal routing.
- Reuse `AutoSizingPanel.gd` for any panel that sizes to content (per CLAUDE.md).
- No snapshot-dict key changes (native output keys are stable), so this step is
  independent of Step 1.

---

## Sequencing & verification

- **Rust track** (server-dev, sequential, verify between): Step 2 → Step 1 →
  Step 3. Each gate: `cargo fmt` + `clippy -D warnings` + `cargo test`.
- **GDScript track** (client-dev, parallel — disjoint files): Step 4. Gate:
  `cargo xtask godot-build` + `ui_preview` PNG harness.
- The two tracks touch disjoint file sets (Rust vs `.gd`), so they run
  concurrently in the worktree without collision.
- Every step is behavior-preserving; the combined verification bar is: workspace
  builds, all tests pass, `ui_preview` frames match pre-refactor rendering.
