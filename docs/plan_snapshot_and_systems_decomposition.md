# Decomposition: snapshot pipeline + systems/god-file split

**Status:** COMPLETE. The four numbered steps landed, and every deferred
follow-up below is now resolved — 1/2/3 as their own extraction PRs (#310/#312/#314,
`MapView.gd` **5,430 → 3,858**), 5 as the native `lib.rs` split (#295), 6/7 in
`8a55f06`, and **4 was absorbed by the separate HUD-decomposition arc** rather than
done here (see follow-up 4). Nothing in this plan remains open.
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
| `CultureSection` | cultureLayers, cultureTensions, cultureRaster, influencers, axisBias, sentiment, sentimentRaster | removedInfluencers, removedCultureLayers |
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

---

## Status (PR #122)

Steps 1–3 landed in full. Step 4 landed **partially** — the cleanly-verifiable
extractions shipped; several `_draw_*` families were deliberately left in place
(see below). Verified: `cargo fmt`, `clippy -D warnings`, 303 `core_sim` tests,
and the `ui_preview`/`map_preview` PNG harnesses (47/47 map frames byte-identical).

Extracted in Step 4:
- Hud: `ui/hud/CommandFeedController.gd`, `ui/hud/LegendController.gd`.
- MapView: `ui/MinimapController.gd`, `ui/BandMarkerRenderer.gd`,
  `ui/SecondaryMarkerRenderer.gd` (each holds a `_view: MapView` back-ref; shared
  geometry/glyph/pill/fog primitives, marker source arrays, and selection state
  stay on MapView).

### `sim_schema` façade split (issue #274) — landed

Step 1 repartitioned the *wire* into nine sections but left `sim_schema/src/lib.rs`
a single 6.1k-line flat module, so it stayed the same append-target hotspot the
`.fbs` had been. It is now a module tree along the **same nine sections**:
`state/{map,economy,population,subsistence,knowledge,governance,culture,campaign}.rs`,
`world.rs` (the deliberately flat `WorldSnapshot`/`WorldDelta` + bincode/JSON), and
`codec/{mod,map,economy,population,subsistence,knowledge,governance,culture,vision,campaign}.rs`.
`lib.rs` is module declarations plus glob re-exports, so every `sim_schema::Foo`
path still resolves and no caller outside the crate changed. Pure code motion:
verified byte-identical `encode_snapshot_flatbuffer` / `encode_delta_flatbuffer`
output on a snapshot populated across all nine sections.

The remaining deferred items below are now tracked as issues **#295–#299**
(native `lib.rs` split; the three `MapView.gd` families; the snapshot
clippy-allow and per-capture `tracing` cleanups).

### Native `lib.rs` split (issue #295) — landed

Deferred follow-up 5 is done. `clients/godot_thin_client/native/src/lib.rs`
(5,617 lines: three `GodotClass` types, the whole snapshot/delta decode path, and
~60 converters) is now a module tree partitioned along the **same nine sections**,
so both ends of the wire look alike: `bridge/{command,script_host,decoder,variant}.rs`,
`snapshot/{mod,raster,delta}.rs`, and
`dict/{map,economy,population,subsistence,knowledge,governance,culture,campaign}.rs`.
`lib.rs` keeps only the gdextension entry point and the public re-exports (25 lines).
Like `sim_schema`, there is no `dict/vision.rs` — the vision section is rasters,
owned by `snapshot/raster.rs`.

Pure code motion, and verified as such **mechanically**, because the usual PNG
harnesses do not reach this file at all (neither `ui_preview.gd` nor `map_preview.gd`
references `SnapshotDecoder`; they feed hand-written GDScript fixture dicts straight
to `Hud`/`MapView`, so a green PNG run proves nothing here). Instead: all 5,159
substantive lines reconstruct in their original order with **zero** mismatches, the
only altered lines being 107 `pub(crate)` visibility promotions; all 736 distinct
string literals (989 occurrences) — every dictionary key and label — are identical;
plus `clippy -D warnings`, `cargo test`, `cargo xtask godot-build`, and a live
`ClassDB` check that all three classes still register and instantiate.

### `MapView.gd` terrain / shader split (issue #296) — landed

Deferred follow-up 1 is done. `clients/godot_thin_client/src/scripts/ui/TerrainRenderer.gd`
(858 lines) takes both implementations of *textured base terrain* — the Approach-B blend
shader (`setup` / `update_shader_quad` / `hide_shader_quad` / `rebuild_shader_maps` /
`shader_active`) and the per-hex texture path (`_build_hex_texture_cache` /
`_render_hex_texture` / `draw_hex_textured_direct` / the hex alpha mask) — plus the
blend-class helpers, the texture toggles, the `ShaderMaterial` + its ~40 uniforms, the six
raster textures, and all eight shader-uniform const families with their tuning commentary.
Same `_view: MapView` back-ref as the Step-4 marker renderers. `MapView.gd`: **5,430 → 4,609**.

Two boundaries were **measured** rather than assumed, and one went against the follow-up's own
framing above:

- **The `_cache_*` SubViewport STAYS on MapView.** It is named here as part of the family, but
  `_invalidate_map_cache()` has 11 call sites and only 2 are terrain — the rest are overlay
  channels, the FoW toggle, grid lines, pan-wrap, zoom and the reserved inset — and
  `CachedMapRenderer` reads `_tile_color` / `_visibility_state_at` /
  `_fow_texture_tint_for_state` / `_show_grid_lines` / `GRID_LINE_COLOR`. It caches the whole
  *non-shader base render*, not the terrain. Only its `_hex_texture_cache` read repointed.
  **`_draw_terrain_direct` stays with it** for the same reason: it is the frame's base-pass loop
  (textured hex vs `_tile_color`, per-tile FoW classification, the shared grid overlay), not a
  terrain function.
- **All eight const families moved wholesale** (`EDGE_BLEND_*`, `WATER_BLEND_*`, `SHORE_*`,
  `CANOPY_*`, `PEAK_*`, `RIVER_*`, `BASE_DEFAULT_*`) — every executable reference to them was
  inside the three shader functions; the only outside hits were comments. **`FOW_*` stayed**
  (12+ references across the visibility and tile-card paths), aliased into the helper as
  `const X = MapView.Y` — the idiom `HudLayer` uses for `SourceForecast`.

`MapView._draw_hex_textured` was deleted: callerless (static and dynamic), a stale duplicate of
`CachedMapRenderer`'s own copy.

Verified by **PNG byte-diff, 286 frames compared, 0 differing** across `map_preview` and
`blend_probe`. Getting there took making the baseline deterministic first: 4 frames vary
run-to-run *in the unmodified code*, and the river-flow shader scroll (`TIME *
river_flow_speed`) made the whole `map_rivers*` family incomparable — so `flow_speed` was
temporarily zeroed to bring those frames into the comparison rather than excluding them
(reverted after). The 4 residual frames' hashes group **by run, not by code version**, which is
the signature of harness nondeterminism, not drift; root cause is the documented
window-maximize race — **`tools/map_preview.gd` lacks `blend_probe`'s `_pin_canvas`
re-assertion**, so its earliest states render at the wrong resolution. Fixing that would make
the whole `map_preview` set a strict bit-identity reference; it is a pre-existing harness bug
and was left untouched.

### `MapView.gd` selected-band overlays (issue #297) — landed

Deferred follow-up 2 is done. `clients/godot_thin_client/src/scripts/ui/BandOverlayRenderer.gd`
(525 lines) owns the selected-band and selected-herd overlay family — work highlights, the
three range borders, the deferred yield labels, the optimistic pending overlay, the travel
destination, the herd grazing range and the corral pen footprint — behind four public entry
points (`draw_band_work_highlights` / `draw_herd_range_highlights` /
`draw_pen_footprint_highlight` / `flush_yield_labels`). Same `_view: MapView` back-ref as
`TerrainRenderer` and the Step-4 marker renderers. `MapView.gd`: **4,609 → 4,131**.

**The follow-up's stated difficulty did not exist, and that is the finding.** It warned that
the yield-label batch "queues mid-`_draw` and flushes at the very end, so extracting cleanly
means splitting that batch lifecycle across MapView and a helper." Measured, all three phases
were already inside the family: the clear sits at the top of `_draw_band_work_highlights`
(before its early-outs), the appends come only from `_queue_yield_label` called within that
same function, and the flush drains. The *only* thing crossing the boundary was where `_draw`
calls flush — so the helper took the whole lifecycle and MapView calls two ordered entry
points, exactly as `_draw` already does for `SecondaryMarkerRenderer.compute_slots()` before
its draw calls. Nothing split. The three invariants that made the batch look fragile are
preserved verbatim, each guarding a real past bug: the clear precedes the early-outs (a
deselected band must leave no stale labels), the far-zoom LOD gate stays at the **queue** site
(so deferral cannot bypass suppression), and the flush stays the **last** draw call in `_draw`
(the layers above it used to scribble across the numbers).

`set_labor_pending` keeps its exact name as a MapView pass-through: `Main.gd` connects the
HUD's `labor_pending_changed` signal to it *by name* via `has_method`/`Callable`, and
`tools/map_preview.gd` calls it at 13 fixture sites — a rename would have broken the wiring
and the harness at once.

All 41 family consts moved with **no `const X = MapView.Y` alias needed** (unlike
`TerrainRenderer`): each was grepped individually and none had a reference outside the family.
Six ambiguous leaves were measured rather than assumed and moved because their only callers
were inside it (`_in_range_disk`, `_draw_dashed_line`, `_draw_dashed_hex`,
`_labor_assignments_of_marker`, `_format_yield_signed`, `_entry_realized_yield`); the shared
primitives and `_is_player_unit` / `_draw_reticle` / the axial converters stayed, reached
through `_view`.

Verified by PNG byte-diff: **56 frames compared, 0 differing** — and this time with no
excluded frames, because PR #310 made `map_preview` deterministic (canvas pinned +
`Engine.time_scale = 0.0`, 56/56 across runs). Baseline reproducibility was confirmed on the
clean tree first. `blend_probe`'s bit-identity references are likewise untouched; its 25
`BANK_*` frames vary run-to-run for a **pre-existing** reason — `blend_probe` does not freeze
`Engine.time_scale`, and BANK is its only state carrying a `TIME`-scrolled navigable-river
channel. Applying map_preview's freeze there would make that set a strict reference too.

### `MapView.gd` annotation family (issue #298) — landed

Deferred follow-up 3 is done, in two parts and strictly in that order — **the fixtures were
built and proven before the extraction touched anything**, because unlike follow-ups 1 and 2
this family had no pixel safety net to fall into and one had to be built first.

**Part 1 — four new `map_preview` states** (`map_trade_overlay`, `map_crisis_annotations`,
`map_terrain_highlight`, `map_routes`). Each was written after reading its draw function, so
the frame exercises the real branches rather than rendering an empty overlay that would sail
through a byte-diff proving nothing: the trade frame drives the exact TradePanel call sequence
and includes a *selected* link plus a leak pip and an unresolvable endpoint (the skip guard);
crisis covers all four annotation shapes including an unknown-severity fallback; terrain
highlight shows matched and unmatched tiles in one frame; routes covers int-keyed, string-keyed
and unknown-faction colours plus a one-waypoint order the draw must bail on.

The check was falsifiable and held: **all 56 pre-existing frames byte-identical, 4 added,
nothing else moved**, and the new frames are themselves deterministic across three runs. The
states are appended last in `_ready`, which makes that guarantee structural rather than
empirical — a state leak from a new fixture can only reach another new fixture.

**These four fixtures were written after the code they cover, so they encode current behaviour
including any bugs. They prove "unchanged", not "correct".** That is the right tool for a
decomposition safety net and the wrong one to later mistake for a correctness test.

**The plan's premise was again partly stale:** it listed *targeting* among the overlays with no
canned data, but `map_quarry_targeting` had since been added for the quarry-picker work. Four
fixtures were needed, not five.

**Part 2 — the extraction.** `clients/godot_thin_client/src/scripts/ui/AnnotationRenderer.gd`
(521 lines) owns the trade overlay, crisis annotations, terrain highlight, routes and targeting.
`MapView.gd`: **4,131 → 3,858**.

**Five public seams keep their exact names** as MapView pass-throughs, and this is the sharpest
naming constraint in the arc so far because every one is reached **reflectively** — `set_targeting`
(`Main.gd`, signal connect by name), `update_trade_overlay` / `set_trade_overlay_enabled` /
`set_trade_overlay_selection` (`TradePanel.gd` via `has_method`/`call`), and `set_terrain_highlight`
(`TerrainPanel.gd`, same). A rename here does not error: `has_method` returns false and the call is
**silently skipped**. The two setters that redrew *conditionally* return a bool so that condition
survived the move.

Nothing outside MapView reads or writes `crisis_annotations` or `routes` by name (the external
`crisis_annotations` hits are the snapshot *dict key*, routed to `CrisisPanel` — a different thing),
so both moved cleanly. `_draw_reticle` was re-grepped rather than inherited from follow-up 2's
conclusion: its old reason evaporated when `_draw_targeting` moved, but `BandOverlayRenderer`'s
travel-destination reticle still uses it, so it stays.

Verified by byte-diff: **60 frames compared, 0 differing**, plus `blend_probe` 230/230. The final
render was repeated with the native extension loaded, since the baselines predated building it.

One GDScript trap worth remembering, found while authoring the crisis fixture:
`const X := PackedInt32Array([...])` is **not** a constant expression, so the paths are authored as
plain int Arrays and converted in the builder — and that conversion is load-bearing, because the
draw branches on the exact type and a plain Array of flat ints falls into the pairs branch and
renders nothing at all.

## Deferred follow-ups

These were consciously scoped out (not missed). Each is a candidate for its own
verified pass; the `MapView.gd` remainder is also summarized in
`clients/godot_thin_client/CLAUDE.md` (key-scripts table, `MapView.gd` row).

1. ~~**Terrain / raster / shader draw family** (`_draw_terrain_direct`,
   `_update_terrain_shader_quad`, `_rebuild_terrain_shader_maps`,
   `_draw_hex_textured*`, blend-class helpers). *Highest remaining churn, so
   highest conflict-reduction value — but the hardest.* It is **not** a read-only
   draw family: it owns a large mutable state surface (the `_cache_*` SubViewport,
   a `ShaderMaterial` + ~40 uniforms, `_terrain_blend_*`, id/vis/river map
   textures, `_hex_texture_cache`) written across `_ready`/`_process`/
   `display_snapshot`/`_draw`. A mechanical move risks subtle visual drift that the
   PNG harness only weakly covers. Needs a dedicated pass, not a bounded one.~~
   **Done** (issue #296) — see "`MapView.gd` terrain / shader split" in Status above.
   The `_cache_*` SubViewport and `_draw_terrain_direct` were **measured out** of the
   family and stayed on MapView; the drift risk was answered with a 286-frame byte-diff.
2. ~~**Selected-band overlays** — work-highlights, yield-labels, herd-range,
   pending, travel-destination. Reads `_selected_player_band`/`selected_unit_id`/
   `_labor_*` and, critically, **queues the yield-label batch mid-`_draw` and
   flushes it at the very end** (`_deferred_yield_labels` → `_flush_yield_labels`);
   extracting cleanly means splitting that batch lifecycle across MapView and a
   helper. Verifiable (work/herd_range/travel fixtures exist) — just larger than a
   marker move.~~ **Done** (issue #297) — see "`MapView.gd` selected-band overlays"
   in Status above. **The split-lifecycle premise above was measured FALSE**: all
   three phases already lived inside the family, so the helper took the whole
   lifecycle and nothing split.
3. ~~**Trade / crisis / terrain-highlight annotations + targeting / routes**
   (`_draw_trade_overlay`, `_draw_crisis_annotations`, `_draw_terrain_highlight`).
   Cohesive, but **no `map_preview` fixture exercises them** (no canned
   trade-link/crisis/highlight/targeting data), so there is no before/after pixel
   comparison. **Add fixtures first**, then extract under the revert-on-drift rule.~~
   **Done** (issue #298) — see "`MapView.gd` annotation family" in Status above.
   Fixtures were built and proven FIRST, then the extraction ran under them.
   **Targeting already had a fixture** (`map_quarry_targeting`), so four were needed, not five.
4. ~~**Hud selection-panel builders** (`_build_allocation_*`, `_herd_summary_lines`).
   Read shared `_selected_*` state; defer until a selection-panel PNG fixture can
   verify them.~~ **Done — absorbed by the separate HUD-decomposition arc**
   (`docs/plan_hud_decomposition.md`), not this one. By the time this arc reached
   follow-up 4, that arc had already lifted every named builder out of the
   `Hud.gd` god-file and verified them under `ui_preview`: `_build_allocation_panel`
   / `_build_expedition_panel` → `ui/hud/SubjectDrawerController.gd`,
   `_unit_summary_lines` → `ui/hud/BandDetailLines.gd`, `_herd_summary_lines` →
   `DetailFormat.herd_summary_lines` (a static). `Hud.gd` is down from ~5,800 to
   ~1,400 lines and holds no selection builders; there is no residual work and the
   deferred PNG-fixture prerequisite is moot. The two arcs shared the god-file-split
   goal for this surface, and the HUD arc got there first — recorded here so the
   ledger reflects reality rather than implying an open item.
5. ~~**`native/src/lib.rs`** remains a single-file hotspot (Step 1 re-routed ~90
   read sites through section accessors but did not split the file). Splitting it
   by domain is a candidate if it stays a conflict source.~~ **Done** (issue #295)
   — see "Native `lib.rs` split" in Status above.

### Pre-existing cleanups surfaced by PR #122 review

These are code-quality items the reviewer flagged on lines this PR only *moved
verbatim* from the old monolithic `snapshot.rs` — they pre-date the refactor and
were left untouched to keep the split behavior-preserving. Worth a small
follow-up PR:

6. ~~**`#[allow(clippy::too_many_arguments)]`** on `fog_raster_from_discoveries`
   (`snapshot/vision.rs`) and `population_state` (`snapshot/population.rs`).
   Replace each with a parameter struct so the allow can be dropped, rather than
   suppressing the lint.~~ **Done** (commit `8a55f06`) — both take a `*Inputs`
   parameter struct (`FogRasterInputs` / `PopulationStateInputs`) and the allows
   are gone.
7. ~~**Per-capture `tracing::info!`** in `visibility_raster_from_ledger`
   (`snapshot/vision.rs`) fires once per snapshot capture (every tick). Downgrade
   to `debug!` or gate behind a flag.~~ **Done** (commit `8a55f06`) — now `debug!`.
