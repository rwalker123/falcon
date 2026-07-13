# Godot Thin Client

Inspector and visualization client for the Shadow-Scale simulation. Renders the map, streams snapshots, and exposes the tabbed inspector.

## Quick Reference

```bash
# Build native extension
cargo xtask godot-build

# Build terrain texture atlas (if out of date)
scripts/build_terrain_textures.sh

# Run client (auto-builds textures if needed)
scripts/run_stack.sh --client-only

# Regenerate FlatBuffers bindings
cargo build -p shadow_scale_flatbuffers && cargo xtask godot-build
```

**Sockets**:
- Snapshot stream: `127.0.0.1:41002` (FlatBuffers via `SimulationConfig::snapshot_flat_bind`)
- Command socket: `127.0.0.1:41001` (Protobuf `CommandEnvelope`)
- Log stream: `127.0.0.1:41003` (length-prefixed JSON tracing frames)

---

## Key Scripts Reference

| Script | Purpose |
|--------|---------|
| `Main.gd` | Scene orchestration, streaming toggle |
| `MapView.gd` | Terrain rendering, overlays, hex selection (select-then-cycle through a tile's band stack), navigation (WASD/QE/mouse), 2D minimap, and the **layered hex-marker system** (see Map markers below) |
| `Inspector.gd` | Inspector coordinator: streaming fan-out, capability gating, typography; hosts per-tab panels |
| `ui/inspector/PowerPanel.gd` | Power tab panel — reference for the tab-panel extraction contract (`apply_update`/`reset`) |
| `ui/inspector/CrisisPanel.gd` | Crisis tab panel — adds command hooks (`set_command_hooks`) and `apply_typography` to the contract |
| `ui/inspector/KnowledgePanel.gd` | Knowledge tab panel — adds `set_command_connected` (connection-gating), `ingest_log_entry` (log-path telemetry), and `append_events` (Trade→Knowledge feed) |
| `ui/inspector/TradePanel.gd` | Trade tab panel — `set_map_view` (overlay), owns the Map-tab overlay toggle, and emits `knowledge_events_produced` (the coordinator forwards it to KnowledgePanel — panels stay decoupled) |
| `ui/inspector/SentimentPanel.gd` | Sentiment tab panel — display; axis bias is coordinator-owned and pushed in via `set_axis_bias` |
| `ui/inspector/VictoryPanel.gd` | Victory tab panel — display + one-shot "victory achieved" log via `set_log_hook` |
| `ui/inspector/FaunaPanel.gd` | Fauna tab panel — **display-only** herd list/detail + estimated hunt yields. The follow-herd command it used to emit was retired with the single-task fauna commands (Early-Game Labor slice 3a; hunting is now HUD labor allocation), so it issues no command; `set_command_connected` is a contract no-op |
| `ui/inspector/GreatDiscoveriesPanel.gd` | GreatDiscoveries tab panel — large, self-contained (ledger + progress + definition catalog + details); capability-gated (`CAP_MEGAPROJECTS`), no command/log/MapView coupling |
| `ui/inspector/LogsPanel.gd` | Logs tab panel — owns the LogStreamClient + polling + filters + tick sparkline; emits `log_entry_received` (coordinator dispatches to Knowledge/Trade); fed synthetic lines via `append_entry` |
| `ui/inspector/InfluencerPanel.gd` | Influencers tab panel — owns the influencer roster; capability-gated (`CAP_INDUSTRY_T1`/`T2`) via `set_available`; exposes `aggregate_resonance()` (coordinator feeds it into the Culture tab) and `get_influencers()` (coordinator's still-inline influencer command controls read the roster back). The influencer *command* controls stay coordinator-owned |
| `ui/inspector/CorruptionPanel.gd` | Corruption tab panel — display-only ledger (reputation modifier, audit capacity, incidents); not capability-gated |
| `ui/inspector/CommandsPanel.gd` | Commands tab panel — the designer/debug console (axis-bias, influencer/channel/spawn, corruption inject, heat, config reload, autoplay row, command status/log; the scenario scout/follow rows were removed with the retired single-task commands). Outbound: issues verbs via `set_command_hooks` and logs via the sink; the command transport + autoplay timer + turn-sending stay in the coordinator. Couplings are coordinator-mediated: emits `axis_bias_apply_requested` (coordinator owns `_axis_bias`, pushes back via `set_axis_bias`), `autoplay_toggled`/`autoplay_interval_changed` (coordinator drives the timer, mirrors via `set_autoplay_active`); fed the roster via `set_influencer_roster` and gated via `set_command_connected`. NOT in `_tab_panels` (no snapshot inputs) |
| `ui/inspector/OverlayPanel.gd` | "Map Overlays" section (nested inside the Map tab, attached to `OverlaySection`) — owns the overlay-channel selector (built at runtime), channel metadata, and the culture/military readouts; drives `MapView.set_overlay_channel`. Fed via `set_map_view` + `ingest(overlay_dict, terrain_tag_labels)` (the coordinator re-homes the palette → Terrain and crisis_annotations → Crisis side-routes that share the `overlays` key, and passes Terrain's tag labels since the terrain-tags channel depends on them). NOT in `_tab_panels` |
| `ui/inspector/MapPanel.gd` | Map tab panel — map-size controls, start-profile (scenario) controls, and the hydrology rivers toggle. Snapshot-driven (in `_tab_panels`): `apply_update` consumes `grid`/`campaign_profiles`/`campaign_label`/`faction_inventory`. Issues `map_size`/`start_profile` via `set_command_hooks`, gated by `set_command_connected`, and drives `MapView.set_highlight_rivers` via `set_map_view`. The nested Map-Overlays section keeps its own `OverlayPanel` script |
| `ui/inspector/CulturePanel.gd` | Culture tab panel — culture layers, divergence list + detail, tension readout; drives `MapView.set_culture_layer_highlight`. Snapshot-driven (in `_tab_panels`): `apply_update` ingests `culture_layers`/`culture_layer_updates`/`culture_layer_removed`/`culture_tensions`, but rendering is driven by the coordinator via `render(resonance)` — the influencer-resonance "pushes" line is coordinator-mediated (`InfluencerPanel.aggregate_resonance()` passed in). `set_map_view` (highlight) + `set_log_hook` (new tensions log to the Logs feed) |
| `ui/inspector/TerrainPanel.gd` | Terrain tab panel — the largest: biome list + drill-down, tile list/detail, the runtime terrain-highlight dropdown, and the **Export Map** button (the tile Scout button was retired with the single-task `scout` command). Snapshot-driven (in `_tab_panels`): `apply_update` ingests `tiles`/`tile_updates`/`tile_removed`/`food_modules` and renders. Owns the inbound MapView hex-selection (`focus_tile_from_map`, coordinator forwards) and drives `set_terrain_highlight` / `relative_height_at` via `set_map_view`. The biome palette + tag labels arrive on the `overlays` key (coordinator routes them in via `set_terrain_palette`/`set_terrain_tag_labels`; `get_terrain_tag_labels()` feeds OverlayPanel). Export sends via `set_command_hooks`, gated by `set_command_connected` |
| `Hud.gd` | HUD layer, legend (the right-dock **TerrainLegendPanel**; `update_overlay_legend` renders rows `{color,label,value_text}` and, for the base terrain legend (`key == "terrain"`) only, shows a runtime-built **sort header** — `Name`/`Count` toggle buttons with a ▲/▼ arrow on the active field. Sort mode is display-only HUD state — field ∈ {name,count} × per-field direction, default **Count desc** — held in `_legend_sort_*` and re-applied via `_sorted_terrain_rows` on every legend push, so the chosen order persists across map regen; MapView's `_build_terrain_legend` supplies a numeric `count` per row for the count sort. Non-terrain (overlay/tag) legends hide the control and render in the given order), the split **Tile card** (`TilePanel`/`%TileDetail` — terrain + the `%ForageAssignControls` "assign foragers" stepper) + **Occupants roster card** (`OccupantsPanel`/`%RosterList`/`%OccupantDetail` — selectable bands+wildlife roster with a per-occupant detail drawer for **herds/expeditions**; a herd shows the `%HerdAssignControls` "assign hunters" stepper+policy picker, an expedition the `%AllocationPanel` Recall/Move panel). **Player-band detail relocated into the dockable `BandCityPanel`** (summary + `%AllocationPanel`-style labor UI render there via `_render_band_into_panel`; the Occupants card keeps only the roster row) — see "Band/City dockable panel". Turn readout (the standalone band Alerts panel was folded into the turn-orb attention model — see "Turn orb & attention model"). Both cards + all selection state (`_selected_tile_info`/`_selected_unit`/`_selected_herd`) + the snapshot-captured `_player_band` (and `_player_bands`, the full player-faction list backing the band-picker + the panel cycler) live here; roster selection emits `roster_occupant_selected`; labor edits emit `assign_labor_requested` / `move_band_requested` / `cancel_order_requested` (clear-all) |
| `ui/BandCityPanel.gd` / `.tscn` | The dockable **Band/City command center** CanvasLayer — persistent whenever ≥1 player band exists, dockable to any of the 4 edges (default left, persisted to `user://band_city_dock.cfg`) + collapse-to-rail. Header (stage glyph/name/label + `◀ n/N ▶` cycler + 2×2 dock chooser + collapse), body hosts the relocated band detail as **section blocks** via `set_band_sections` (tall = vertical stack that fits its width to the content, wide = manual balanced-column packing that fits its height to the content). Reserves its edge via `reservation_changed(edge, size)` → `Main._apply_reservation(&"band_panel", …)`. See "Band/City dockable panel" + `docs/plan_band_city_dock.md` |
| `ui/BandFoodStatus.gd` | Single source of truth for band food-supply thresholds (`band_status_config.json`) + the days→green/amber/red color / BBCode-hex mapping (plus the parallel morale warn/critical thresholds + `color_for_morale`/`hex_for_morale`), shared by MapView's band dot and Hud's food/morale lines + alerts |
| `ui/TileHabitability.gd` | Single source of truth for the Tile-card Habitability rating: buckets `TileState.habitability` (band-independent per-turn morale drain) into Hospitable/Fair/Harsh/Hostile via `tile_habitability_config.json` thresholds, with the HEALTHY/INK/WARN/DANGER color / `hex_for_rating` mapping. Consumed by `Hud._tile_terrain_lines` + `_format_detail_bbcode` |
| `ui/TileClimate.gd` | Single source of truth for the Tile-card Climate band: maps `TileState.temperature` (°, a latitude+elevation climate, equator-in-the-middle) into Tropical/Warm/Temperate/Cool/Polar via `tile_climate_config.json` cutoffs. INFORMATIONAL only — deliberately no HEALTHY/WARN/DANGER tint (renders neutral ink), so it doesn't compete with the Habitability row's semantic palette. Consumed by `Hud._tile_terrain_lines` |
| `SnapshotStream.gd` | Consumes length-prefixed FlatBuffers snapshots |
| `CommandBridge.gd` | Issues Protobuf commands to server |
| `ui/MinimapPanel.gd` | Minimap component for the 2D map view (click-to-pan, aspect ratio sizing) |
| `ui/TurnOrb.gd` / `ui/TurnOrb.tscn` | The bottom-right **turn orb** (replaces the old "Advance Turn" button): calm cyan pulse when the attention registry is empty, else a severity-tinted count badge + a reasons popover (see "Turn orb & attention model"). Re-emits `focus_requested` (jump) / `advance_requested` so Main's advance/jump wiring is unchanged; palette from `HudStyle`, all geometry/severity/kind as named constants |
| `ui/MagnifierButton.gd` | Zoom-rail in/out button that `_draw`s a crisp magnifier icon (lens + handle + inner `+`/`−`, `zoom_sign` picks which) — font magnifier glyphs render as tofu/blobs. Monochrome `HudStyle` ink → `SIGNAL` on hover |
| `ui/AutoSizingPanel.gd` | Shared helper for panels that expand to fit content |
| `ui/HudStyle.gd` | Single source of truth for the dark HUD console look: palette (cyan `SIGNAL`, amber `WARN`, ink/line neutrals), `card_stylebox()`, `header_stylebox()`, `banner_stylebox()`, and `apply_button(btn, "primary"/"ghost"/"armed")`. Every HUD surface styles through here |
| `ui/FoodIcons.gd` | Shared map-marker emoji glyphs — food modules (`for_site`) and fauna herds (`for_herd`, species keyword matched in the herd label, longest-first). Covers migratory species plus wild game (deer/boar/rabbit/fowl). Used by the Harvest/Hunt button (`Hud.gd`) and the map's food-site / herd markers (`MapView._draw_food_site` / `_draw_herd`) so a source always reads the same |
| `tools/ui_preview.gd` / `.tscn` | Dev-only preview harness: instances the real `HudLayer` with canned selection/targeting data, renders each state, and saves PNGs to `ui_preview_out/` (gitignored). Iterate on HUD styling without a server: `godot --path . res://tools/ui_preview.tscn` |
| `tools/map_preview.gd` / `.tscn` | Dev-only **MapView** preview harness (HUD-only ui_preview's companion): instances the real `MapView`, feeds a canned `display_snapshot` + selects a band, and dumps PNGs (`map_*.png`) to `ui_preview_out/`. Verifies the selected-band labor highlights (work-range ring / worked forage tiles / hunted-herd ring+link; scouting draws no disc — it extends sight in the fog) without a server: `godot --path . res://tools/map_preview.tscn` |
| `tools/blend_probe.gd` / `.tscn` | Dev-only **edge-blend probe rendered at the GAME's on-screen hex radius** — the other harnesses *fit* their grid to the window (r ≈ 83–178) and the blend look is radius-relative, so every judgement made in a fitted frame was wrong. Pins a 1:1 1920×1080 canvas + a grid sized so `_fit_map_to_view` lands on the target radius (it prints the achieved radius and warns if it drifts). **Two states:** (1) a **band strip** of flat biomes at r≈45 (desert · prairie · scrub · alluvial · tundra · salt flat — every adjacent pair is a flat↔flat seam) → `blend_bands_*.png`; (2) **ISOLATED prairie hexes surrounded on all six sides by dark rocky soil** at **r≈75** (the user's on-screen size) → `blend_isolated_shipped.png` + one full frame & native-res close-up per tuning variant + a labelled contact sheet (`V6_*.png`). **State 2 is mandatory for any blend change**: a straight band seam looks fine even when the blend is tearing holes in hex interiors — only a surrounded hex exposes it (that is how the shredding regression shipped). **Two more states (V7, water↔water):** (3) an irregular **deep-ocean region embedded in continental shelf** (plus isolated deep hexes) at r≈77 → `V7_water_W1.png` (water on the shared LAND levers — still a soft-edged hexagon) vs `V7_water_W2.png` (the shipped `water_blend` block — the silhouette dissolves); (4) a ragged **coast** frame with a single water id → `V7_coast_unchanged.png`, the **bit-identical reference** any blend-eligibility change is pixel-diffed against (it must not move the shoreline). **Two more states:** (5, V8) the water patch rendered **FoW OFF vs FoW ON** (a mix of active + discovered hexes, nothing unexplored) → `V8_water_fow_off.png` / `V8_water_fow_on.png` — the FoW tint comes from a **per-hex, NEAREST-sampled vis-map**, which used to make every discovered↔active adjacency a **hard hex-shaped tint boundary that is not a terrain seam**. Any "hard straight edges are back" report must be checked against this pair BEFORE the blend is touched. This is also the frame the **FoW boundary softening** is judged on (see Fog-of-war softening: the steps must be gone, pure states unchanged); (6, V10) the shipped **shoreline profile** on the ragged coast at r≈75, rendered against TWO land biomes → `V10_shore.png` + `V10_shore_closeup.png` (prairie) and **`V10_shore_dark_land.png` + `V10_shore_dark_land_closeup.png`** (rocky_regolith). The close-ups are where the "is there a hard line anywhere on land→sand→foam→water?" call is made (the downscaled full frame hides a 1px line; see Shoreline), and **the DARK-land one is decisive** — prairie's tan hides sand-vs-land contrast and masked an invisible-beach bug through several passes, so never judge the beach on prairie alone. `_render_variant(overrides, name, crop…)` overrides any `terrain_config` lever (incl. the nested `water_blend` / `shore` blocks) live, which is how the shipped values were swept. **One more state (8, W): the FoW hex-step BEFORE vs AFTER the boundary softening** — one camera, one terrain, one visibility map, only `fow_softness` varying → `W_fow_off.png` (FoW off, the terrain-only reference: the deep-ocean blob's edges are already soft, which **exonerates the blend**), `W_fow_on.png` (softness `0` — reproduces the **unsmoothed per-hex tint**, i.e. the hard hexagonal brightness steps), `W_fow_fixed.png` (the shipped softness — steps gone, mist preserved). Each also dumps a `_closeup` and, decisively, a **`_same_terrain`** crop straddling hexes **(4,3) Active / (3,3) Discovered — BOTH continental shelf**, so the only thing that can draw an edge between them is the FoW tint. That crop answers any "hard straight edges in open water, even between hexes of the same terrain" report. **One more state (9, X): the DARK-WATER report on REAL game terrain** → `X_dark_water.png` + `X_dark_water_closeup.png`, rendered from a **verbatim 14×10 window of a LIVE snapshot's id-map** (`X_WATER_IDS`), FoW OFF, r≈75. The synthetic water states (3/5/8) never reproduced the "dark patches of open water with hard full-hexagon edges" report because their deep-ocean region is ONE clean ragged blob; the real ocean is **salt-and-pepper** shelf/deep, and a lone deep hex ringed by shelf can only read as a dark HEXAGON. **Any "dark water hexagons" report must be rendered on THIS state** — a synthetic blob will not show it. It is the frame the water **depth field** (see Edge Blending → water) was verified against. **One more state (10, L): the PER-WATER-TERRAIN shore profile on a SMALL INLAND SEA** → `L1_current.png` / `L2_no_wisp.png` / `L3_half.png` / `L4_tenth.png` (+ `*_full.png`), a 7-hex `inland_sea` lake in a field of **dark rocky_regolith** (prairie's tan camouflages both sand and foam) at r≈75, one camera/crop across all four. `_render_lake_variant` overrides the inland_sea entry's `shore_profile` in the live config and calls `TerrainTextureManager.rebuild_layer_shore_map()` — the sweep for choosing a lake's coast (now in the three-scale scheme; **L3 IS the shipped lake**, `sand 0.5 / foam 0.5 / wisp 0`, and L4 = the whole profile scaled so its OUTERMOST reach, `wisp_center + wisp_half` = 0.68·r, lands at ~0.10·r → 0.147). **The harness disables `MapView._unhandled_input`** — it renders in a REAL window, so the OS cursor otherwise drew a faint HOVER hex outline into the frames, a run-to-run difference of a few thousand pixels that silently defeats the pixel-diff the coast states exist for. With it off, consecutive runs are **byte-identical**, so `V7_coast_unchanged.png` / `V10_shore*.png` are usable as strict bit-identity references. **One more state (11, H): ROLLING HILLS "cut off at the hex edge"** → `H_*.png`, a `rolling_hills` (24) blob + **isolated** hills hexes + an **isolated alpine (26)** hex in a field that is dark `rocky_reg` west / tan `prairie` east, at r≈75 with the **hex grid overlay OFF** (a drawn hexagon would answer the very question under test). Frames: `H_before` (the artifact), **`H_base_only`** (peaks skipped by pushing `peak_min_radius` above the render radius — isolates the BASE floor, and is what proved the cut is the rugged base hexagon, **not** a weak mound overhang), `H_peaks_only` (the amplified `before − base_only` pixel diff = the peak pass's exact footprint: it shows the mounds DO overhang, and that the peak **cast shadow darkens the whole neighbour hex**, a second hard hexagon), and the candidate fixes `H_fix_overhang` / **`H_fix_base`** (`blend_rugged_land`) / `H_fix_both`. Each renders a full frame + a seam close-up + the **isolated-hex** and **alpine** close-ups (the mandatory shred checks). `H_gate_bands_full` / `H_gate_coast` re-render the flat↔flat strip and the coast with the rugged gate ON — they must byte-compare **identical** to `blend_bands_full` / `V7_coast_unchanged`. **One more state (12, R): the RUGGED-GATE SWEEP** — `blend_rugged_land` is GLOBAL, so shipping it lets EVERY rugged biome's base floor blend, and the failure mode is SHREDDING. R renders **each rugged biome as an ISOLATED hex** (even col + even row ⇒ never adjacent to another subject) in TWO fields, each **gate OFF vs gate ON** so every biome is a controlled A/B: `R_flatoff_*` / `R_flat_*` (dark `rocky_reg` west, tan `prairie` east) and `R_ruggedoff_*` / `R_rugged_*` (a field of `canyon_badlands` — the rugged↔rugged case), plus `R_*_field_full`. **The gate-OFF pair is not optional**: several biomes' own art (e.g. `karst_highland`'s semi-transparent overhanging spires) *looks* like neighbour texture leaking into the hex, and only the A/B tells art from tear. **One more state (13, S): the PEAK CAST-SHADOW HEXAGONS** — an alpine massif + an isolated `rolling_hills` hex in a light prairie field, grid OFF → `S_shadow.png` + `_closeup` + `_iso`, and decisively **`S_shadow_footprint*.png`**, the amplified diff against a `shadow_strength = 0` render (the cast shadow **in isolation** — the only frame on which "is it hex-shaped? is it still directional?" can actually be answered, since the semi-transparent mound fringe contaminates every other measurement). **Two harness bugs were fixed here and must not regress:** (a) `project.godot` opens the window **MAXIMIZED** (`window/size/mode=3`) and the WM applies that a few frames into the run — *after* `_ready` sized it — so the viewport became the whole monitor and every state after the second silently rendered at **r ≈ 154, not the game's 75** (and the taller states overflowed the canvas, clipping the close-ups). `_pin_canvas` re-asserts WINDOWED + 1920×1080 on every `_refit`. (b) Lever overrides now go through **`_override_config`/`_restore_config`**, which **ERASE** a key that was absent instead of writing `null` back: MapView reads levers as `bool(config.get(key, DEFAULT))`, the default only applies when the key is **missing**, and a present-but-null key reaches `bool(null)` — a **runtime error that aborts `_update_terrain_shader_quad` before it pushes a single uniform**, so every later frame renders with STALE uniforms and lies. **One more state (14, G): the REAL NEIGHBOURHOOD from the user's screenshot** — the "hills are STILL cut off, with the rugged gate ON" report → `G_*.png`. State H could not see why: its hills blob sits in FLAT fields only, so every peak edge in it is a peak↔non-peak one (which the overhang feathers). G rebuilds the screenshot — a `rolling_hills` blob against `canyon_badlands` (rugged, **no** peak asset), **`alpine_mountain` (which HAS one → the peak↔PEAK case)**, `high_plateau` (a peak at ~the SAME elevation as the hills → the near-zero-Δ case), `alluvial_plain`, `rocky_reg` and an `inland_sea` lake hex — at r ≈ 75, grid OFF. It is the **only** probe state that ships a real **elevation raster** (`G_ELEVATION_BY_ID` + `elevation_sea_level`): every other snapshot omits the channel, so MapView falls back to `PEAK_ELEV_FALLBACK` for EVERY hex and **no elevation asymmetry can be judged in them**. Frames: `G_before` (shipped), **`G_no_peaks`** (peak pass skipped — it renders the same seam as a soft ecotone, which **exonerated the base blend** and convicted the peak overlay), `G_no_shadow` (cast shadow off, peaks on — attributes a residual line to the shadow vs the art), `G_peaks_only` (the amplified diff = the peak pass's exact footprint), each with native-res crops `_peakpeak` (hills↔alpine, big Δelev), `_sameelev` (hills↔plateau, Δ≈0 → must stay a soft symmetric cross-fade), `_canyon` (peak↔non-peak — the control), `_lake` (the shoreline — hard BY DESIGN), `_iso` + `_iso_alpine` (the mandatory isolated-hex shred checks; both sit on the LEFT of the frame because MapView's minimap CanvasLayer is NOT hidden and a bottom-right crop captures IT). **A `--only=` state filter** (`godot --path . res://tools/blend_probe.tscn -- --only=G`, or `--only=1,4,G`; keys are `<number>/<letter>`, no filter = every state) renders one state instead of all 14 — a diagnosis loop re-renders one state many times. **A third harness bug was fixed here and must not regress:** `project.godot` opens the window **MAXIMIZED** and macOS applies — and **RE-applies** — that asynchronously, many frames in, so a fixed pair of `process_frame`s is a RACE that does not stay won. A filtered run puts a radius-critical state FIRST and it fitted at **r ≈ 154, not the game's 75**; a re-maximize BETWEEN two frames of one state rendered them at different resolutions (the pixel-diff then dies on a size mismatch); and one DURING a crop sequence made the captured image the monitor's while the viewport still reported the pinned size (`content_scale_size` pins the viewport, so **only `get_window().size` can see the maximize**) — the crop then landed off-frame as a 686×1 sliver. `_ensure_canvas` (called from `_settle`) re-pins and WAITS on the window; `_capture` re-draws until the captured geometry is the canvas's (or an integer HiDPI multiple) instead of silently saving a bad frame. **One more state (15, D): the THREE-SCALE shore profile — CLIFF vs BEACH vs LAKE, and the MIXED coast** → `D*.png`, the ragged coast against **dark `rocky_reg`** (prairie's tan camouflages both sand and foam) at r≈75, **grid overlay OFF**, one camera/crop per comparison set. `_snapshot_coast(shore_id, water_id)` now takes the SEA's id, which is what selects the `shore_profile` under test. Frames: **`D1_cliff`** (`deep_ocean` meeting land — NO sand anywhere, big surf, and the full-strength surf peak must still conceal the base's own step at the waterline, since there is no sand out there to hide it); **`D2_shelf_C1/C2/C3`** (the shelf's muting ladder, `foam_scale` 0.85/0.75/0.65 × `wisp_scale` 0.5 — the surf's measured footprint falls 18.0k → 15.8k → 13.9k → 12.2k px against the cliff's; **C2 ships**); **`D3_mixed_coast`** — THE DECISIVE FRAME: a `deep_ocean` hex and a `continental_shelf` hex **adjacent along ONE coastline**, both touching the same land (`_snapshot_mixed_coast` swaps the sea by row), where a nearest-water PICK would jump the profile at their bisector and make the sand appear along a **hard line**; the weighted-mean profile field must instead **fade the beach in** along the shore (measured: the land-pixel difference vs `D1_cliff` ramps from 0.00 over ~220px ≈ 3 hex radii — not a step); and **`D4_lake_unchanged`** (the lake, shipped config — the two-lever → three-scale migration must be a no-op). `godot --path . res://tools/blend_probe.tscn` |
| `tools/band_panel_preview.gd` / `.tscn` | Dev-only preview harness for the **Band/City dockable panel**: instances the real `BandCityPanel` + `HudLayer`, injects the panel into the HUD, pushes a seeded player band through `update_band_alerts`, and dumps the panel docked left/right/top/bottom + collapsed (`band_panel_*.png`) so the chrome + the relocated band detail + the HUD reflow can be eyeballed without a server: `godot --path . res://tools/band_panel_preview.tscn` |
| `tools/marker_field_guard.gd` / `.tscn` | Headless **regression guard** for the "unit marker drops a panel-consumed field" bug class (twice hit: `hunt_mode`, then `working_age`/`idle_workers`). Feeds one realistic population entry through the real `MapView._rebuild_unit_markers` and asserts the produced marker is a superset of `PANEL_CONSUMED_KEYS` (the keys `Hud._unit_summary_lines` + `_build_allocation_panel` read off `_selected_unit`) and that the drop-prone fields round-trip (not defaulted). Exits non-zero on failure (CI-usable). No rendering, so headless: `godot --headless --path . res://tools/marker_field_guard.tscn`. When the panel starts reading a new marker field, add it to `PANEL_CONSUMED_KEYS`. |
| `assets/terrain/TerrainTextureManager.gd` | Autoload singleton for terrain texture loading |
| `assets/terrain/TerrainDefinitions.gd` | Single source of truth for terrain definitions |

---

## Architecture

### Scene Structure
- `Main.tscn` - Root `Node2D` scene with a `Camera2D`, the `MapView` map layer, and `CanvasLayer`s for HUD/inspector/Band-City panel
- The client is **2D-only**; an experimental 3D relief view was permanently removed (see `docs/architecture.md` → "Removed: 3D Relief Rendering")
- Toggle: `I` hides/shows inspector, `L` collapses legend

### Data Flow
```
Server (FlatBuffers) -> SnapshotStream.gd -> parsed snapshot
                                          -> MapView (terrain/overlays)
                                          -> Inspector (panels)
                                          -> Hud (legend, selection)
```

### Native Extension
`native/` contains GDExtension bindings for FlatBuffers decoding (generated from `sim_schema/schemas/snapshot.fbs`).

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

## Minimap System

The 2D minimap lives in the HUD **bottom-left** `NavCluster` (an HBox in `BottomBar`,
`HudLayer.tscn`) — a `MinimapContainer` (the map thumbnail with its viewport indicator
rectangle) with a docked **zoom rail** to its right. `MapView._setup_2d_minimap` finds the
container via `Hud.get_minimap_container()`, so the container abstracts the move.

### Zoom rail — the on-screen map-zoom control
The rail (`ZoomRail` VBox) is `＋` (`MagnifierButton`, zoom in) / a live `1.0×` readout /
`－` (`MagnifierButton`, zoom out) / `▣` fit ("Fit map to view (C)"). It rides the **one**
map-zoom path: the buttons emit `Hud.map_zoom_step(±1)` / `map_zoom_fit` → `Main` →
`MapView.zoom_step()` / `fit_to_view()` (thin wrappers over `_apply_zoom`, pivoting on the
map center), and `MapView.zoom_changed(zoom_factor)` → `Hud.set_zoom_readout` renders the
readout (so it also reflects the wheel and `Q`/`E`). The old top-right **interface-scale**
widget (which drove `content_scale_factor` — it scaled the whole canvas uniformly, so map
icons never crossed the icon-LOD threshold) was **removed**; map zoom is now solely
`MapView._apply_zoom`. Interface scale returns later via an Options menu. See
`docs/plan_hud_nav_turn_orb.md`.

The map view displays this minimap showing the full map with a viewport indicator rectangle.

### Component (`ui/MinimapPanel.gd`)
Reusable minimap UI component handling:
- CanvasLayer hierarchy setup (layer 102)
- Aspect ratio sizing from grid dimensions
- Click-to-pan with drag support
- Viewport indicator overlay with draw callbacks

### 2D Minimap (MapView.gd)
- Renders terrain at 1 pixel per hex as an `ImageTexture`
- Viewport indicator uses pointy-top hex coordinate math:
  - Screen corners → axial coords (q,r) → offset coords (col,row) → normalized [0,1]
- Click-to-pan converts normalized position → hex grid coords → pan_offset

### Configuration
Minimap sizing parameters live in `heightfield_config.json` (the file also holds fog-of-war appearance tunables; its 3D-relief sections were removed):
```json
"minimap": {
  "base_height": 220,
  "min_width": 140.0,
  "max_width": 520.0,
  "margin": 16.0
}
```

---

## Terrain Texture System

Optional terrain texture graphics for the 2D map view.

### Asset Structure
```
assets/terrain/
  textures/
    base/                        # 37 terrain textures (512x512 PNG); forest bases are grass FLOOR (no trees)
      00_deep_ocean.png
      ...
      36_aquifer_ceiling.png
    canopy/                      # RGBA tree-crown overlays (transparency); one per canopy biome (3 today: 07/12/13)
    peaks/                       # RGBA mountain-relief overlays (transparency); one per relief biome (5 today: 24/25/26/27/29)
    edges/                       # 6 edge masks for blending (optional)
    wang/                        # Wang tile variants (future)
  terrain_config.json            # Configuration
  TerrainTextureManager.gd       # Autoload singleton for centralized texture loading
  TerrainDefinitions.gd          # Single source of truth for terrain definitions
  TerrainTextureGenerator.gd     # CLI script to generate placeholder textures
```

### Enabling Terrain Textures
1. Generate placeholder textures from command line:
   ```bash
   godot --headless --path clients/godot_thin_client --script assets/terrain/TerrainTextureGenerator.gd
   ```
2. Replace placeholders in `assets/terrain/textures/base/` with AI-generated or hand-crafted textures
3. Set `"use_terrain_textures": true` in `terrain_config.json`

Textures are loaded at runtime from individual PNGs and combined into a `Texture2DArray`.

### Configuration (`terrain_config.json`)
```json
{
  "use_terrain_textures": true,
  "use_edge_blending": true,
  "texture_scale": 4.0,
  "blend_width": 0.25,
  "blend_soft": 0.35,
  "blend_height_influence": 0.25,
  "blend_noise_scale": 0.25,
  "blend_noise_amount": 0.3,
  "feature_noise_cell": 6.0,
  "water_blend": { "blend_width": 0.45, "blend_soft": 0.45, "blend_noise_amount": 0.45 },
  "lod_near_distance": 50.0,
  "lod_far_distance": 200.0
}
```
Every terrain entry also carries a `"blend_class"` (`flat` | `water` | `rugged`) — the single
source of truth for edge-blend eligibility, which is **same-class** (flat↔flat and water↔water blend;
land↔water and rugged stay hard — see Edge Blending below). The top-level `blend_*` keys are the
**seam** levers, tuned for LAND (`blend_width` = the ecotone's reach, `blend_soft` = the feather
softness, `blend_height_influence` = the detail-following nudge, `blend_noise_scale`/`blend_noise_amount`
= the boundary wobble); the `water_blend` block **overrides width/soft/noise_amount for water↔water
only** (smooth low-variance water needs a wider, softer, wobblier seam). All documented under Edge
Blending below. `feature_noise_cell` is the value-noise cell size
(**raw px**) for the **other** noise-driven features — the shoreline reach/wisp, the canopy treeline and
the peak footline. The blend noise and the feature noise are deliberately **decoupled** (one uniform each)
so retuning the seam can never move a coastline, treeline or footline. **The units differ on purpose:**
`blend_noise_scale` is a **fraction of the hex radius** (→ `blend_noise_cell = blend_noise_scale · radius`
px) so the seam's character is identical at every zoom (a fixed px cell drifted — a hex is ~45px on screen
in-game but several times that in a zoomed-in preview frame, so the same 6px cell read very differently in
the game than in the preview it was judged in), while the shore/treeline/footline look is tuned in
absolute pixels. **Judge any blend change at the GAME's hex radius (~45px)** — use
`tools/blend_probe.tscn`, which pins it.

### Texture Loading (TerrainTextureManager)
- Autoload singleton loads textures once at startup for the 2D map renderer
- Builds `Texture2DArray` from individual PNGs in `textures/base/`
- Exposes: `terrain_textures` (Texture2DArray), `terrain_config`, `use_terrain_textures`, `use_edge_blending`
- Also computes each base layer's **mean luminance** at build time (`layer_mean_luma` /
  `get_layer_mean_luma()`, measured on a 16² Lanczos downscale of the retained CPU-side Image) and packs it
  into `layer_luma_texture` (a 1×N single-channel `ImageTexture`, one texel per terrain id). This is the
  zero-point of each texture's pseudo-height for the shader's flat↔flat **height blending** (see Edge
  Blending); MapView binds it once as the `layer_luma_map` uniform. The Rec.709 weights here MUST match the
  shader's `luma()` helper
- Also builds `canopy_textures` (a second Texture2DArray of RGBA crowns from `textures/canopy/`) +
  `canopy_layer_by_id` / `canopy_layer_for(id)` (`terrain_id → canopy array layer`, -1 = none) for the
  blend shader's canopy overlay (see Edge Blending → Canopy overlay), and `peak_textures` (a third
  Texture2DArray of RGBA mountain relief from `textures/peaks/`) + `peak_layer_by_id` / `peak_layer_for(id)`
  for the blend shader's peak overlay (see Edge Blending → Peak overlay)

### 2D Rendering Pipeline
- `MapView` gets textures from `TerrainTextureManager` and pre-renders hex-masked textures on startup
- Cached as `ImageTexture` per terrain ID for efficient drawing
- Falls back to solid colors when overlay mode is active
- Textures only displayed in base view (empty overlay key)
- Fog of War keeps textures: the draw loop classifies each tile once via
  `_visibility_state_at()` — Active tiles draw full-brightness, Discovered tiles
  are tinted toward the mist color (cloudy) via `_fow_texture_tint_for_state()`,
  Unexplored tiles fill with the fog color.
- Runtime toggle: `T` key (`enable_terrain_textures` / `_toggle_terrain_textures`)
- Edge blending: a flat↔flat **per-pixel biome blend shader** at biome seams (see Edge Blending below)

### Edge Blending — per-pixel biome-blend shader (Approach B)
When `use_edge_blending` is enabled, biome **seams** blend per-pixel in a **fragment shader**
(`assets/terrain/terrain_blend.gdshader`): a symmetric **height blend** (texture splatting) where the two
biomes interlock across the boundary — each texture competes on its own per-pixel height, so one settles
into the *cracks* of the other. It is neither a gradient blur (blur ghosts on detailed textures) nor a
dither (see below). It is deliberately narrow in scope: a biome blends only against biomes of its **own
blend_class**, and only the *flat* and *water* classes blend at all; every other seam — rugged, and every
class change (notably the land↔water shoreline) — stays a **crisp hard edge**. Approach B replaced the earlier baked-overlay
dither (Approach A), fixing its three caveats: **symmetric** mutual intrusion (a tie at the exact edge via
signed distance), **no tiling** (world-space noise varies per hex), and **cleaner grain**.

**Eligibility — SAME CLASS (`blend_class`, config `terrain_config.json`):** every terrain carries a
`blend_class` of `flat` | `water` | `rugged` (id-map G channel: 0 water / 1 flat / 2 rugged, named
`CLASS_WATER`/`CLASS_FLAT`/`CLASS_RUGGED` in the shader). Blend fires for an edge **only** when both
sides share the **same blendable class** and their terrain ids differ:
- **flat↔flat** (grass↔soil ecotones) → blends.
- **water↔water** (deep_ocean ↔ continental_shelf ↔ inland_sea …) → **blends**. Two adjacent ocean
  depths are a gradient, not a cliff; before this rule the `water` class forbade *all* water blending
  and deep-vs-shelf showed razor-sharp hexagon silhouettes.
- **land↔water** (a CLASS CHANGE) → **hard**. That seam is the **shoreline**, owned by the foam/beach
  pass; softening it would wash the coastline out. This is the whole reason `water` is its own class —
  but the old gate over-reached and also banned water↔water.
- **rugged↔anything** → hard (forests/hills/mountains/volcanic — never bleed discrete-object textures),
  **unless `blend_rugged_land` is on** — see below.

`MapView._terrain_is_flat` / `_blend_class_code` read a cached `_terrain_blend_class` map
(`_build_terrain_blend_class_map`); `TerrainTextureManager.blend_class_for` mirrors it.

**`blend_rugged_land` — the RUGGED-LAND gate (config bool, `terrain_config.json`, **SHIPPED `true`**;
`EDGE_BLEND_DEFAULT_RUGGED_LAND` in `MapView.gd` → the shader's `blend_rugged_land` uniform).** Under
the bare same-class rule a rugged biome's BASE FLOOR never blends, so it ends in a razor-straight
hexagon against its neighbour — and for a **peak** biome that floor is the *whole* ground under the
relief overlay (`rolling_hills`' base is plain grass; the mounds are a `peaks/` overlay), which is the
"rolling hills look CUT OFF at the hex edge" report. This gate widens the **land** half of the rule from
*same class* to *both sides are land*: flat↔rugged and rugged↔rugged blend through the **existing** flat
levers (no new tuning), so a hills/alpine hex feathers into its neighbour instead of cookie-cutting.
**land↔water is untouched** (still hard — that seam is the shoreline) and water keeps its depth field,
so it is **bit-identical** on every frame with no rugged hex (verified: `blend_bands_full`,
`blend_isolated_shipped`, `V7_coast_unchanged` and `V10_shore_dark_land_closeup` all byte-compare equal
with it on).
It shipped only after the **whole rugged roster** was swept for SHREDDING (the height term tearing holes
in a structured texture's interior — high-contrast rugged art is exactly what is at risk): **`blend_probe`
state R** renders EVERY rugged biome as an **ISOLATED hex surrounded by a contrasting one**, in a flat
field *and* in a rugged field, gate OFF vs ON. All held — interiors stay solid, only the rim feathers,
including the extreme-contrast cases (white `fumarole_basin` on dark rocky_reg; black `basaltic_lava_field`
and white `karst_highland` on orange `canyon_badlands`). **A straight band seam cannot show shredding —
never judge this gate on one.** What it *does* cost is that a high-contrast rugged pair (bright karst
against orange canyon) now reads as a wide hazy ecotone rather than crisp geology; that is a look call,
not a tear.

**WATER IS A DEPTH FIELD, NOT A SEAM** (the fix for the "dark patches of open water with hard
full-hexagon edges, FoW off" report). A hex's water id is a **quantized sample of a continuous seafloor**,
and the real map's ocean is **salt-and-pepper**, not clean blobs: a live 80×52 snapshot's id-map carried
**2332 deep_ocean↔continental_shelf hex adjacencies** and **16 deep hexes whose six water neighbours were all
a different id**. Under the flat↔flat *nearest-edge* seam blend such a hex can only ever read as a **dark
hexagon** — the rim feathers, but the interior keeps the (far darker) deep texture and the silhouette IS the
hex. That artifact is **TERRAIN, not the FoW tint**: with FoW off the shader never reads the vis-map at all
(`fow_enabled` gates the whole block) and `_rebuild_terrain_shader_maps` writes vis = 255 everywhere, so
**fog off already means every hex renders fully lit** — no mist, no dim, nowhere in the client (the CPU path's
`_visibility_state_at` returns `""` → `Color.WHITE`, and the overlay-color path is `_fow_enabled`-gated too).
So water takes its **own branch**: every qualifying water neighbour (same class, different id) contributes
**at once**, weighted by how close the fragment is to **that** shared edge — the same 6-neighbour cross-edge
weighting the FoW softening uses — and the result is the **normalized weighted mean** of the water textures.
The weight reaches 1 exactly **at** a shared edge, so the mean there is `(own + nb)/2` read from BOTH sides:
continuous across every boundary by construction. The flat↔flat interlock is **untouched** and water no longer
takes it. Verify with `blend_probe` **state 9 (X)** below.

**The three water levers** (`terrain_config.json` → `water_blend` block:
`blend_width` **0.45** / `blend_soft` **0.45** / `blend_noise_amount` **0.45**, vs the land
0.25/0.35/0.30; fallbacks are `WATER_BLEND_DEFAULT_*` in `MapView.gd`, pushed as the
`water_blend_band`/`water_blend_soft`/`water_blend_noise_amount` uniforms). They keep their names but, under
the depth field, they mean:
- `blend_width` → the field's **REACH**: how far into the own hex a neighbour's depth still bleeds.
- `blend_soft` → the **PLATEAU**, as a fraction of that reach: how far back from the shared edge a neighbour
  already carries FULL weight. **This is the lever that dissolves the hexagon** (a pure ramp only softens its
  rim). Capped in-shader by `WATER_FIELD_PLATEAU_MAX` (0.5) so a ramp always survives — a plateau spanning the
  whole reach would put a hard step at the reach's outer edge.
- `blend_noise_amount` → the amplitude of the world-noise displacement of the depth boundary (in reach units),
  so the depth contour meanders organically instead of tracing hex geometry. Sampled in map space, so a world
  point reads the same value from both sides of an edge — continuity survives it.

The wobble **cell** (`blend_noise_scale`) and the height nudge stay **shared with land** — a finer cell would
speckle, and the height term is a no-op on smooth low-variance water anyway.

**Mechanism — whole-map shader quad + hex splatmap:**
- `terrain_blend.gdshader` (canvas_item) is drawn as **one whole-map rect** by a dedicated child
  node `TerrainBlendQuad` (`show_behind_parent = true`, so it renders BEHIND MapView's grid/markers —
  a separate node is required because a canvas item's ShaderMaterial applies to *all* its draw
  commands). `MapView._setup_terrain_blend_shader` builds it once; `_update_terrain_shader_quad`
  pushes uniforms each frame. Per fragment the shader **inverts the pointy-top odd-r hex layout**
  (MUST match `MapView._hex_center`/`_axial_center`/`_offset_to_axial` + the `hex_origin`/`hex_radius`
  uniforms exactly — this is the alignment contract with grid lines/selection/markers), reads its
  hex's biome from the **`sampler2DArray`**, and — if its class is blendable (flat or water) — checks the
  6 neighbours (wrap-aware) for a **same-class, different-id** biome; near the nearest qualifying shared
  edge it **height-blends** the neighbour's
  array sample in. The seam weight is **symmetric**: `p = clamp(0.5 + signed_dist_to_edge /
  (2·blend_band), 0, 1)` is 0.5 at the edge on both sides.
- **THE INVARIANT: the seam is ALWAYS a continuous weighted mix, never a 1-bit pick.** The splat weight
  `p` **leads**; two enveloped perturbations only **bend** it; a `smoothstep` feathers the result into a
  mix factor:
  ```glsl
  float env = 4.0 * p * (1.0 - p);                          // dies at both ends of the band
  float h   = (luma(own) - mean_luma(own_layer)) - (luma(nb) - mean_luma(nb_layer));
  float pw  = clamp(p + ((noise - 0.5) * blend_noise_amount - h * blend_height_influence) * env, 0.0, 1.0);
  result    = mix(own, nb, smoothstep(0.5 - blend_soft, 0.5 + blend_soft, pw));
  ```
  The **wobble** (world `vnoise`, cell `blend_noise_cell`) gives an organic, meandering boundary instead
  of the straight hex line, and carries **low-variance pairs** (smooth sand ↔ smooth soil) where there is
  little detail to follow. The **height term** is a *detail-following NUDGE*: with no height maps each
  texture's **zero-centred luminance** is its pseudo-height (`luma(rgb) − mean_luma(layer)`, Rec.709; the
  per-layer means come from `TerrainTextureManager.layer_luma_texture`, a 1×N single-channel texture
  fetched by layer index — **zero-centring is essential**, or a bright biome always out-heights a dark one
  and the seam collapses to one side), and it bends the boundary toward the darker/lighter side so it
  follows the textures' own tufts and grains. `blend_height_influence` **must stay small** (≤
  `EDGE_BLEND_MAX_HEIGHT_INFLUENCE` = 0.5) so it can never out-vote the distance weight. The `4·p·(1−p)`
  envelope guarantees no perturbation can leak neighbour texture into a hex interior nor leave a straight
  discontinuity at the band's outer edge.
- **Rejected alternatives (do NOT reintroduce)** — the first two are the SAME BUG (a 1-bit pick) in two
  disguises: (1) the **dither** (`result = neighbour if p > vnoise(...)`) — a binary pick makes every pixel
  100% one biome, so the seam can only ever be **discrete hard-edged blobs**; the user's verdict on the live
  game was "the blobs are too big… I shouldn't really even notice the blending, but it is very obvious". No
  noise tuning fixes it (coarse noise → chunky blobs, fine noise → pixel shimmer) — *the approach was the
  bug*. (1b) **Height blending with `blend_height_influence` 4.0 + a small overlap depth** (`blend_depth`,
  now gone): the luma term (±0.3 × 4 = ±1.2) **dwarfed** the 0..1 distance weight, so it degenerated into
  winner-takes-all-by-luminance — wherever prairie was dark, soil won outright, *deep inside the hex*. The
  user's verdict: prairie hexes looked **shredded**, "this isn't a blend at all". A straight band seam looks
  fine under this bug — **only an isolated hex surrounded by the other biome exposes it**, which is exactly
  what `blend_probe`'s isolated-hex state renders. (2) A plain
  linear crossfade — it ghosts two detailed textures over each other. (3) A 3-octave "wander" noise +
  an S-curve on `p` (tried under the dither) — big smooth lobes.
- **Base biome UV — CONTINUOUS world space** (like the canopy pass, NOT per-hex-normalized): the base
  biome is sampled at `base_uv = v_map / (2·hex_radius) · base_scale` (`v_map = v_world - hex_origin`,
  pan/zoom-anchored), so **one texture tile spans ~`1/base_scale` hex-rows** and adjacent hexes show
  DIFFERENT regions of it. This kills the **per-hex identical-repeat grid** (with diagonal seams) that
  any *detailed* (non-homogeneous) base texture used to show when each hex was mapped to one whole
  centered copy — invisible on homogeneous grass/water, obvious on a rocky/alpine texture. The
  **flat↔flat height blend samples the neighbour biome at the SAME `base_uv`** (only the array layer differs),
  so the cross-edge interlock stays continuous (two world-sampled biomes at one world point). `repeat_enable`
  tiles the array. The canopy pass already sampled this way; the base now matches it.
- **id-map splatmap** (`_rebuild_terrain_shader_maps`, per snapshot): a `grid_w × grid_h` **RGBA8**
  texture, R = terrain id, G = `blend_class` code (0 water / 1 flat / 2 rugged), B = canopy code
  (0 none, else canopy layer + 1), A = 255, NEAREST-sampled. A
  companion **R8 vis-map** carries FoW state (0 unexplored / 0.5 discovered / 1 active).
- **Config levers (all fallbacks mirrored as `EDGE_BLEND_DEFAULT_*` consts in `MapView.gd`):**
  - `blend_width` (**0.25** → `blend_band = blend_width · radius`, the half-band in px) — the **REACH**, i.e.
    the width of the ecotone. The user wants a **shallow** transition confined to the hex edge, so it is
    small: `0.25·radius` ≈ 19px at the on-screen r≈75, a band that never reaches a hex interior.
  - `blend_soft` (**0.35**, capped at `EDGE_BLEND_MAX_SOFT` = 0.5) — the **FEATHER SOFTNESS**: the
    smoothstep's half-width in seam-weight units. **Small** (≈0.03) ⇒ the mix snaps wherever the
    noise/detail carries the weight past 0.5 → a fine crisp **stipple**; **large** (≈0.35) ⇒ a smooth
    **gradient** the noise only leans. Floored in-shader (`BLEND_SOFT_MIN`) so it can never become a hard step.
  - `blend_height_influence` (**0.25**, hard-capped at `EDGE_BLEND_MAX_HEIGHT_INFLUENCE` = 0.5) — the
    detail-following **NUDGE** (see the invariant above). Typical zero-centred luma deviations are ±0.3, so
    0.25 moves the weight by ≤ ~0.08 — a fraction of the 0..1 distance weight it perturbs. `0` = a pure
    distance+noise feather. **Never raise it past the cap**: at 4.0 it out-voted the distance weight and
    shredded hex interiors (see Rejected alternatives).
  - `blend_noise_scale` (**0.25**, a **fraction of the hex radius** → the `blend_noise_cell` px uniform) —
    the **WAVELENGTH** of the boundary wobble: ≈19px at r=75, i.e. a few organic lobes per hex edge, which
    is what stops the seam reading as the straight hex polyline. Very fine (≈0.05) turns it into a
    per-pixel speckle instead (which only reads as a boundary at all when `blend_soft` is also tiny).
  - `blend_noise_amount` (**0.3**) — the wobble's amplitude, **added to** the seam weight (never
    thresholded against it — this is not a dither) and enveloped so it dies at both ends of the band.
  - `blend_rugged_land` (**true**, shipped) — the rugged-land eligibility gate (see the gate above). It
    changes only *which* seams blend, never *how*: rugged land reuses the five levers above verbatim.
  - The blend look is **zoom-invariant** (band + wobble are both radius-relative), so a preview frame is an
    honest proxy for the game *only if it is rendered at the game's on-screen hex radius* (**r ≈ 75px**;
    hexes read ~150px across on the user's screen). `tools/blend_probe.tscn` pins that, and — critically —
    renders **isolated hexes surrounded by another biome**, the only state that exposes hex shredding.
    `tools/map_preview.gd` *fits* (r ≈ 83–178) and only ever shows straight band seams, so judgements made
    in it are not trustworthy for the blend.
  - `feature_noise_cell` (default `6.0`, the world-noise cell **px** for the
    shoreline reach/wisp + canopy treeline + peak footline; **decoupled** from the blend noise — it
    drives the shader's `noise_cell` uniform, so the seam can be retuned without moving any
    coastline/treeline/footline; verified by pixel-diff).
  - Top-level `base_texture_scale`
  (→ `base_scale`, default `0.25` = one base texture spans ~4 hex-rows; smaller covers MORE hexes,
  larger fewer — `BASE_DEFAULT_TEXTURE_SCALE` in `MapView.gd`). **LOD:** below `EDGE_BLEND_MIN_RADIUS`
  (`= ICON_MIN_DETAIL_RADIUS`) the shader renders base-only (no shimmer at far zoom). **FoW:** the
  shader applies the same discovered-mist multiply / unexplored-fog fill as the per-hex path
  (`_fow_texture_tint_for_state` semantics) via the vis-map — it dims, never drops, the blend. It also
  **softens the mist across hex boundaries** — see Fog-of-war softening below.

**Fog-of-war softening — the hex steps (shader path only).** The vis-map is **per-hex, NEAREST-sampled**
(0 unexplored / 0.5 discovered / 1 active), so reading it raw made every **active↔discovered adjacency a
hard HEXAGONAL brightness step** — straight edges cutting across even *uniform water*, where no terrain
seam exists at all. (This is why "hard straight edges are back" reports must be checked against
`blend_probe` state 5 *before* the blend is touched: the culprit was usually the FoW tint, not the blend.)
Fixed in two halves, both in the shader's `fow_enabled` block:
1. **Smooth the visibility SCALAR across hex boundaries**, reusing the same 6-neighbour
   signed-distance machinery as the blend/shore: each neighbour's visibility is weighted by
   `smoothstep(-fow_soft, 0, d)` — how close the fragment is to **that** shared edge — and the weighted
   mean replaces the raw per-hex value. At a shared edge the neighbour's weight → 1, so `vis → (own+nb)/2`
   from **both** sides — equal, hence **continuous across the boundary**; deep inside a hex all six weights
   → 0 and `vis → own`, so interiors are untouched.
2. **Map `vis` to the tint CONTINUOUSLY** (the old per-state `if` chain was itself a step function):
   `fog_amt = 1 − smoothstep(FOW_UNEXPLORED, FOW_DISCOVERED, vis)` and
   `mist_amt = (1 − smoothstep(FOW_DISCOVERED, FOW_ACTIVE, vis)) · mist_blend`, composited with the
   **existing** `mist_color`/`fog_color`/`mist_blend` uniforms. At the pure states this reproduces today's
   look **exactly** (verified bit-identical: vis 1 = clear, 0.5 = the same mist multiply, 0 = fog fill) —
   only the boundaries change.
- **Optional wispiness:** the smoothed scalar is perturbed by world `vnoise` (reusing `noise_cell`) so the
  fog line reads cloudy rather than a clean arc. It is **enveloped by `|smoothed − own|`** (normalized by
  `FOW_NOISE_EDGE_PEAK`, the 0.25 that a 6-neighbour average can shift the scalar across one state gap), so
  it bites **only at boundaries** and can never tint a pure Active/Discovered/Unexplored interior.
- **Config levers** (`heightfield_config.json` → `fog_of_war`, beside the existing mist/fog colours —
  FoW appearance stays in one place): `fow_softness` (**0.6**, a **fraction of the hex radius** → the
  `fow_soft` px uniform, like `blend_width`, so the gradient is zoom-invariant) and `fow_noise_amount`
  (**0.15**; `0` disables the wisps). Fallbacks are `FOW_DEFAULT_SOFTNESS` / `FOW_DEFAULT_NOISE_AMOUNT` in
  `MapView.gd`. The **per-hex CPU path is unaffected** (it is hard-edged by construction).
- **Verify** with `blend_probe` state 5 → `V8_water_fow_on.png`: on uniform shelf water the mist boundary
  must read as a soft cloudy gradient with **no hexagonal brightness steps**, while pure Active and pure
  Discovered areas are unchanged. State **8 (W)** makes the before/after explicit —
  `W_fow_on_same_terrain.png` (softness `0` = the unsmoothed tint) vs `W_fow_fixed_same_terrain.png`, on two
  adjacent **shelf** hexes across an Active/Discovered boundary. **This is the FIRST thing to render on any
  "hard straight full-hexagon edges are back in open water" report**: the tone-only steps in water are the
  FoW tint, NOT the blend (which `W_fow_off.png` shows already dissolving the deep-ocean silhouette). The
  mist multiply lands exactly on the hex boundary, so it **re-imposes a hard hexagonal edge on water the
  blend has just softened** — and it does so between hexes of the SAME terrain id, where no seam exists.
- **Integration:** the shader is the base-terrain renderer whenever `use_terrain_textures` and no
  overlay and `use_edge_blending` (`_shader_terrain_active`); it **bypasses the CPU map cache** (a
  single cheap GPU draw, so the cache's per-hex-loop purpose is moot). With `use_edge_blending` off,
  the **per-hex texture path** (`_build_hex_texture_cache` / `_draw_hex_textured_direct` +
  `CachedMapRenderer`) renders crisp hard hexes — that is the blend-OFF reference. Overlay/solid
  modes are unchanged.

**Shoreline — ONE continuous coastal profile straddling the coast (universal for now):** separate from the
flat↔flat interlock, every **land↔water** edge gets a coastal treatment in the same shader, reusing the
signed-distance-to-shared-edge machinery. It fires for any edge where **exactly one side is water**
(`blend_class` code 0) — so it's independent of the land side's class (**both flat-land and rugged-land**
coasts get it) and never touches inland edges (flat↔flat interlock and rugged↔* inland edges stay exactly
as before — both sides non-water → skipped). Seaward read: **land → sand → surf → open water**, and the
requirement is that **NO boundary in that chain is a hard line** — not sand↔land, not sand↔foam, not
foam↔water.
- **THE SIGNED COAST COORDINATE `u` — why this can't step at the hex edge.** The shore pass computes
  `dist_in` = distance from the shared land↔water edge INTO the own hex, which tends to **0 on BOTH sides**
  at that edge. Negating it on the land side gives one coordinate running continuously through the
  coastline: `u < 0` inland · `u = 0` **exactly at the waterline** · `u > 0` seaward. Every shore weight is
  a `smoothstep` **of `u` alone**, so its value at `u = 0` is identical whether the fragment belongs to the
  land hex or the water hex — the profile is continuous across the boundary **by construction**, and no
  term can pop there. (The world-noise wobble that meanders the reaches is sampled in **map space**, so it
  too is the same value on both sides of the edge at a given world point.)
- **The three rejected passes — all the same bug class** (a term saturating AT the hex edge, or sand where
  the user does not want it). (1) A **two-sided** pass (tan beach on the land, foam on the water) with
  LINEAR fades `1 − dist_in/reach`, which are **≈1 AT the shared edge on BOTH sides**: the land went solid
  tan, the water solid white, and they met along the boundary — a **hard tan↔white line TRACING THE
  HEXAGON**. (2) The fix for *that* pushed everything onto the **water side** (`land_beach_width = 0`),
  which killed the sand↔foam line but left the sand **stopping dead at the hex edge against the raw land
  texture** — a **new hard sand↔land line**. (3) Sand on **BOTH** sides (`sand_land_band` + `sand_sea_band`)
  straddling the edge: every hard line was gone, but the beach then read **TWICE AS WIDE** — **sand in the
  water hex is not wanted at all**. Hence the shipped shape: sand is **LAND-ONLY**, and the sand↔foam blend
  is bought by letting the **surf wash INLAND over the beach** instead of by putting sand in the sea.
- **Sand — LAND SIDE ONLY** (`u ≤ 0`; the water hex gets **zero** sand, by construction — the term is
  ternary-gated on the sign of `u`). It is **FULL from the waterline across the surf's inland wash** (the
  **plateau**), then `smoothstep`-fades inland into the biome art over the rest of `sand_band`. Capped at
  `SHORE_SAND_OPACITY` (< 1) so the land art reads through and the beach never looks like flat paint, and
  its reach is deliberately SHORT (0.25·r) so it tints rather than buries the biome.
  **The plateau is anchored to `foam_inland_band`, and that anchor is load-bearing** (`SHORE_SAND_PLATEAU_MAX`
  caps it at 0.6 of the sand reach, so a fade window always survives): the surf is composited **over** the
  sand and peaks at ~1 at the waterline, so wherever the wash is strong the sand is whitewashed and
  contributes nothing. A sand that *also* decayed from the waterline (a plain `1 − smoothstep(0, sand_reach,
  −u)`) was down to ~30% opacity by the time the foam cleared and gone entirely a hair further inland — the
  beach was **invisible** and the coast read **land → surf → water with NO SAND AT ALL** (caught against a
  dark rocky-regolith coast, where white foam met bare rock; **prairie's tan hides this** — always judge the
  beach on a DARK land biome). Holding the sand full across the wash means the **retreating surf uncovers a
  full-strength beach** — that IS the sand↔foam crossfade.
- **Surf — peaks AT the waterline and washes BOTH ways.** Inland over the sand across `foam_inland_band`
  (the crossfade that kills the sand↔foam line) and seaward into open water across `foam_band`. **Its peak
  must stay at ~1 — never scale it down:** with no sand in the water, the *base* itself steps at `u = 0`
  (sand-tinted land on one side, open water on the other), and the full-strength foam is the only thing
  concealing that flip.
- **Wisp — the faint SECOND surf line out over open water.** Its geometry is **its own pair of
  radius-relative levers** (`wisp_center_width` / `wisp_half_width` → the `wisp_center_band` /
  `wisp_half_band` px uniforms), **not** a multiple of `foam_band` as it once was — that chaining meant the
  surf could not be shortened without dragging the wisp in with it (and the wisp could not be pulled in at
  all). Config is responsible for keeping the wisp band **clear of the surf** (`wisp_center − wisp_half >
  foam_width`) so the two read as two lines; overlap just merges them into one wide white smear.
  `wisp_half_width = 0` turns the wisp off. Only its opacity (`SHORE_WISP_STRENGTH`) stays a shader const.
- **Every falloff is a `smoothstep`** (no linear ramp's slope kink, no hard cutoff anywhere). All reaches
  are noise-modulated by the SAME world-noise wobble (`mix(SHORE_REACH_NOISE_MIN, 1, noise)`, reusing
  `noise_cell`), so the sand's inland edge, the surf line and the wisp meander together as organic fingers
  rather than concentric clean stripes.
- **Config levers** (`terrain_config.json` → `shore` block): `sand_width` (**0.25** — sand reach INLAND of
  the coastline; **land-only**) / `foam_inland_width` (**0.15** — how far the surf washes UP the beach) /
  `foam_width` (**0.41** — surf reach SEAWARD) / `wisp_center_width` (**0.55**) / `wisp_half_width`
  (**0.13**) — the second surf line's centre and half-thickness, so it spans 0.42–0.68·r, clear of the surf
  that dies at 0.41·r. **All five are fractions of the hex radius** → the `sand_band` / `foam_inland_band` /
  `foam_band` / `wisp_center_band` / `wisp_half_band` px uniforms (computed in
  `MapView._update_terrain_shader_quad` like `blend_width`), plus `foam_color` / `beach_color` (RGB 0–255,
  parsed by `MapView._shore_color` into normalized `vec3` uniforms). Fallbacks are the `SHORE_DEFAULT_*`
  consts in `MapView.gd`; the fixed feel-tuning (`SHORE_SAND_PLATEAU_MAX` / `SHORE_SAND_OPACITY` /
  `SHORE_WISP_STRENGTH`) is named consts in the shader. The `land_beach_width` / `sand_land_width` / `sand_sea_width` keys of the rejected passes are
  **gone**. Note the visible beach is intrinsically narrow: the surf covers the inner `foam_inland_width` of
  it, so only the `sand_width − foam_inland_width` annulus (0.10·r) reads as open sand — that is the
  specified geometry, not a bug. LOD-suppressed and FoW-tinted like the rest of the shader (shares the
  `blend_enabled` gate + the vis-map).
- **Verify at the game's hex radius** with `tools/blend_probe.tscn` **state 6 (V10)** — the shipped profile
  on the ragged coast at r≈75 → `V10_shore.png` + `V10_shore_closeup.png` **and `V10_shore_dark_land.png` +
  `V10_shore_dark_land_closeup.png`** (the same coast against **rocky_regolith**). **The dark-land frame is
  the decisive one** — prairie is tan and hides sand-vs-land contrast, which masked the invisible-beach bug
  through several passes. **Judge on the close-ups**: the full frame is downscaled when viewed, which hides
  exactly the 1px line this pass exists to prevent. A coast rendered in a *fitted* harness frame is not a
  trustworthy proxy either (the look is radius-relative — same caveat as the blend). `_render_variant` can
  still sweep the three width levers.
- **PER-WATER-TERRAIN shore profile (`shore_profile`) — A COAST IS NOT ONE THING.** The five levers above are
  the GLOBAL profile, tuned for OCEAN coasts. But the worldgen's water sequence is **deep_ocean →
  continental_shelf → land**: deep ocean *never* meets ordinary land, so where it DOES touch land it is a
  **CLIFF** (no beach at all, full dramatic surf), the **shelf** is the ordinary **beach** (sand, a muted
  wave), and an **`inland_sea`** is a handful of hexes that the ocean profile swamps (its offshore **wisp**
  reads as noise across the middle of a lake). So a WATER terrain entry in `terrain_config.json` may carry an
  optional block scaling the profile of **its own** coastline, along **three independent axes**:
  `{ "id": 1, "name": "continental_shelf", …, "shore_profile": { "sand_scale": 1.0, "foam_scale": 0.75, "wisp_scale": 0.5 } }`
  - `sand_scale` multiplies the beach's INLAND reach (`sand_band`). **`0.0` = no beach at all** (the cliff).
  - `foam_scale` multiplies the MAIN WAVE's reaches **both ways** (`foam_inland_band` = the wash up the beach
    **and** `foam_band` = the surf's seaward reach). **REACH only — the surf's PEAK is never scaled**, because
    that peak (~1 at the waterline) is the only thing concealing the base's own step there, and on a cliff
    coast (no sand) that step is raw land → open water. `foam_scale 0` is not a legal profile.
  - `wisp_scale` multiplies the secondary offshore disturbance — its **centre distance, its half-width AND its
    strength** — so it recedes toward the shore and fades as one gesture; `0.0` removes it cleanly.
  - **A water terrain with no `shore_profile` gets the neutral default (1, 1, 1)** —
    `SHORE_PROFILE_DEFAULT_{SAND,FOAM,WISP}_SCALE` in `TerrainTextureManager` — a bit-exact no-op (a partial
    block is legal too: a missing key is neutral on that axis).
  - **Plumbing mirrors the per-layer mean-luminance table** (`layer_luma_texture`): `TerrainTextureManager`
    packs the profiles into `layer_shore_texture`, a **1×N FORMAT_RGBAF** image (R = sand_scale, G =
    foam_scale, B = wisp_scale, one texel per terrain id), bound once by MapView as the `layer_shore_map`
    uniform and fetched in-shader by layer index (`shore_profile(layer)` → `vec3`).
    `rebuild_layer_shore_map()` is public and **updates the ImageTexture in place** (so the binding survives)
    — that is how `blend_probe` sweeps profiles live.
  - **THE PROFILE IS KEYED ON THE WATER, on BOTH sides of the waterline.** A *correctness* requirement, not a
    style choice: every shore weight is one smoothstep of the signed coast coordinate `u` evaluated on both
    sides of the shared edge, so if the two sides read different scales the profile would be discontinuous
    **at the hex edge** — reintroducing exactly the hard line `u` exists to prevent. The water is also the only
    side both fragments can agree on (the land biome varies along a coast; the body of water does not) and the
    meaningful one ("cliff, beach or lake?" is a property of the water).
  - **AND IT IS A CONTINUOUS FIELD, NEVER A NEAREST-PICK** (the fix for what used to be filed here as a "known
    limitation"). One land hex can border a deep_ocean hex **AND** a continental_shelf hex along the SAME
    coastline. Taking the *nearest* water neighbour's profile makes the profile **JUMP at the bisector**
    between them — and with `sand_scale` 0 on one side and 1 on the other that is a **HARD LINE of sand
    appearing out of nowhere along the beach** (it was only a faint seam while all the profiles were similar;
    the cliff/beach split makes it glaring). So **every water hex in `{own + 6 neighbours}` contributes at
    once**, weighted by proximity to **that** shared edge, and the profile is their **normalized weighted
    mean** — the water depth field's discipline. A cliff coast **transitions into** a beach coast over ~a hex
    instead of switching.
    * **The weight** is `smoothstep(−reach, 0, d)` on the signed distance `d` to that neighbour's shared edge
      (own water = weight 1 by construction; land contributes nothing — it has no profile), with `reach` =
      `SHORE_PROFILE_REACH_APOTHEMS` (**1.0**) × the hex **apothem** (the `half_dist` the loop already
      computes). It is deliberately **unwobbled** — a noise displacement here would break the cross-edge
      agreement below.
    * **Why 1.0 apothem is the cap, and why the mean is EXACTLY continuous across every hex edge** (including
      the land↔water one, where it must be, per `u` above). On the shared edge of hexes A|B: (i) a water hex
      **C that neighbours BOTH** reads the *same* signed distance from A's frame and from B's frame — the
      three bisectors meeting at that corner are symmetric under the 120° rotation about it — so both frames
      give C the same weight; (ii) a water hex enumerated from **only one** frame has signed distance
      `≤ −apothem` there, so its weight is exactly **0** and the frame that cannot see it agrees. Raising the
      reach past 1.0 apothem breaks (ii) and re-introduces a step at the hex boundary.
    * **The beach fades out with its own reach** (`sand_fade`): `SHORE_REACH_MIN_PX` floors every reach so no
      fade divides by ~0, but on a cliff coast (`sand_scale → 0`) that floor would keep a **1px, full-strength
      tan hairline** alive at the waterline — and worse, the beach would **POP** into existence at full
      opacity the instant `sand_scale` left 0 as a cliff profile blended into a beach one. So the sand's
      opacity is scaled by `min(sand_reach_raw / SHORE_REACH_MIN_PX, 1)`: exactly **1.0 (a bit-exact no-op)
      for any beach wider than the floor**, and a continuous grow-in from nothing below it.
  - **Shipped:** `deep_ocean` **(0, 1, 1)** — the cliff · `continental_shelf` **(1, 0.75, 0.5)** — the ordinary
    beach, main wave muted, disturbance halved · `inland_sea` **(0.5, 0.5, 0)** — the approved lake. Every
    other water terrain (coral_shelf, hydrothermal_vent_field) is neutral. Per-**LAND**-biome shore gating (a
    grassy shore vs a wooded shore) is still deliberately NOT built — all coasts render the same beach+foam
    art. Verify via `tools/map_preview.gd` State Q (`_biome_band_terrain` carves an ocean bay so the ocean
    borders BOTH prairie and woodland) → `map_biome_blend.png` + `map_biome_shore_seam.png` (coast close-up),
    the lake via `blend_probe` **state 10 (L)**, and the cliff/beach/mixed coasts via **state 15 (D)** below.
  - **NOTE for the next pixel-diff:** because the shipped `continental_shelf` profile is no longer neutral,
    `V7_coast_unchanged` / `V10_shore*` / `H_gate_coast` (whose sea IS the shelf) **moved** when it landed —
    that is the shipped muting, not a regression. They remain the bit-identical reference for any blend
    **eligibility** change; re-baseline them after a deliberate `shore_profile` edit. Frames with no ocean hex
    (`blend_bands_*`, every `H_*`/`S_*`/`G_*`, the `L*` lake) must stay byte-identical through both.

**Canopy overlay — forest = grass floor + overhanging tree crowns:** a forest biome is split into a
**ground layer** that blends like any flat land and a **canopy overlay** of whole crowns that overhang
the hex boundary and thin out, so a forest edge is a natural treeline instead of a razor-cut hex
silhouette. Today the only canopy biome is **12 (mixed_woodland)** — its `blend_class` is now **`flat`**
(the grass floor flat↔flat-blends with prairie and gets a shoreline at coasts, like any flat land); 13
(boreal_taiga) stays `rugged` (no canopy asset yet).
- **Assets:** `textures/base/NN_name.png` is the **forest-floor grass** (trees removed);
  `textures/canopy/NN_name.png` (**new dir**, RGBA crowns on transparency) is the canopy.
- **Second Texture2DArray:** `TerrainTextureManager` builds `canopy_textures` (a companion
  `Texture2DArray` from `textures/canopy/`, same once-only `Image.load_from_file` pattern as the base)
  plus `canopy_layer_by_id` (`terrain_id → canopy array layer`, `canopy_layer_for()` returns -1 for
  none). Only biomes with a canopy file get a layer. Two `sampler2DArray`s in **one** canvas shader work
  fine (base `biome_array` + `canopy_tex`).
- **Canopy code in the splatmap:** the id-map is now **RGBA8** (was RG8) — R=terrain id, G=blend_class
  code, **B=canopy code** (`0` none, else canopy layer + 1), A unused (`MapView._canopy_code`). This
  reuses the per-neighbour id-map fetch the shader already does rather than a separate id-indexed uniform
  array, so both own and neighbour canopy state come from one texture read.
- **Overhang density D (shader):** using the same signed-distance-to-shared-edge machinery vs the
  **canopy↔non-canopy** boundary (`s` = signed distance, + inside the forest): D = 1 deep inside, **~0.5
  at the exact edge**, ramping to 1 over `canopy_softness` px inside and down to 0 at `canopy_overhang` px
  **outside** the forest (crowns overhang the neighbour, then fade). The treeline is world-noise
  perturbed (`CANOPY_TREELINE_NOISE`, reusing `noise_cell`) so it's bumpy, not a clean arc. Interior
  forest hexes (all-canopy neighbours) → D=1. Composited **after** blend+shoreline, before FoW:
  `result = mix(result, crown.rgb, crown.a · D)`.
- **Map-space canopy UV:** `cuv = v_map / (2·hex_radius) · canopy_scale`, where `v_map = v_world -
  hex_origin` is the pan/zoom-anchored MAP coordinate (raw `v_world` is the quad-LOCAL/screen-fixed
  coord and would slide against the grid on pan/zoom — all map-space terms, canopy UV + the
  blend-wobble/shore/treeline noise, use `v_map`). Continuous across hexes (a crown straddling a boundary
  reads as one tree). The base biome now samples in the same continuous world space (see **Base biome
  UV** above), so `canopy_scale` and `base_scale` are the two independent world-UV density knobs (a
  crown tile per hex at `canopy_scale = 1.0`; a base tile per ~`1/base_scale` hexes). FoW-tinted like the rest.
- **Canopy LOD is DECOUPLED from the blend LOD** (own `canopy_lod_enabled` uniform, `radius ≥
  canopy_min_radius`, NOT the flat↔flat `blend_enabled`/`EDGE_BLEND_MIN_RADIUS` gate). `canopy_min_radius`
  sits WELL BELOW `EDGE_BLEND_MIN_RADIUS` (3.0 vs 16.0) so the canopy pass keeps running at far zoom:
  interior forest density (D=1) persists into a **distinct darker-green forest mass** (a forest region no
  longer reads as bare grassland when zoomed out); the edge overhang naturally shrinks to nothing as hexes
  shrink. The crown array (`canopy_textures`) is built **with mipmaps** and the `canopy_tex` sampler uses
  **trilinear** (`filter_linear_mipmap`) filtering, so far-zoom crowns AVERAGE into a smooth tone instead of
  shimmering/aliasing. (The base biome array has no mipmaps — `filter_linear` only; the canopy is the layer
  that visibly aliases at far zoom because whole crowns tile many times per tiny hex. If the base ever
  shimmers it can take mipmaps the same way.)
- **Config levers** (`terrain_config.json` → `canopy` block): `overhang_width` / `softness_width`
  (fractions of the hex radius → `canopy_overhang` / `canopy_softness` px uniforms, like `blend_width`),
  `texture_scale` (→ `canopy_scale`), and `canopy_min_radius` (the decoupled canopy LOD floor in px, ≪
  `EDGE_BLEND_MIN_RADIUS`). Fallbacks are the `CANOPY_DEFAULT_*` consts in `MapView.gd`.
- **Caveat — canopy is shader-only:** the blend-OFF **per-hex CPU path** (`use_edge_blending = false`,
  `map_biome_hard.png`) renders only the base, so forests there read as the **bare grass floor** (no
  crowns). The live client runs blend-on, so this affects only the reference/fallback path.
- Verify via `tools/map_preview.gd` State Q → `map_biome_blend.png` + `map_biome_woods_edge_seam.png`
  (the forest block borders prairie floor left + ocean top/right): whole crowns overhang + thin into a
  treeline, interior stays dense, the prairie↔forest floor blends softly, and the forest coast shows
  beach/foam with canopy overhanging the water. Far-zoom decoupled-canopy LOD via State Q-far →
  `map_biome_farzoom.png` (same four bands on a large grid so hexes go tiny): the woodland band reads as a
  distinct darker-green forest mass vs the prairie grass, smooth (mipmapped), not shimmering.

**Peak overlay — highland/volcanic relief = flat rocky floor + overhanging faceted peaks + cast shadow:**
the mountain-drama analog of the canopy overlay, built on the exact same machinery (DRY). A relief biome
keeps its flat rocky base floor and gets an RGBA **peaks overlay** of faceted mountains composited on top:
they overhang the hex boundary and thin to a footline (like the treeline), have an **elevation-driven
prominence**, and **cast a shadow** onto neighbouring hexes, so mountains read as raised relief on the 2D
map. Five relief biomes carry real AI-gen peak art today — **24 (rolling_hills)**, **25 (high_plateau)**,
**26 (alpine_mountain)**, **27 (karst_highland)**, **29 (active_volcano_slope)** — each a magenta-keyed,
offset-blend-seamless RGBA overlay in `textures/peaks/`. (28 canyon_badlands is intentionally NOT a peak
biome — its drama is incision, handled at the base-floor level, not raised relief.)
- **Assets + third Texture2DArray:** `textures/peaks/NN_name.png` (**new dir**, RGBA relief on
  transparency). `TerrainTextureManager` builds `peak_textures` (a THIRD `Texture2DArray`, same once-only
  `Image.load_from_file` + **mipmaps** pattern as the canopy) plus `peak_layer_by_id` /
  `peak_layer_for()` (`terrain_id → peak array layer`, -1 = none). Only biomes with a peak file get a
  layer. Three `sampler2DArray`s in one canvas shader (base + canopy + peaks) work fine.
- **Peak code in the splatmap A channel:** the id-map A channel (previously the unused `255`) now carries
  the **peak code** (`0` none, else peak layer + 1, `MapView._peak_code`) — the peak analog of B=canopy
  code, so both own and neighbour peak state come from the one id-map read the shader already does.
- **New elev-map (R8):** a companion `grid_w × grid_h` R8 texture (parallel to the vis-map), each texel =
  the hex's relative height (`MapView.relative_height_at` 0..100 → 0..255; `PEAK_ELEV_FALLBACK = 200` when
  a snapshot lacks an elevation raster, so relief still renders in preview/rehydrated frames). Drives the
  shader's per-hex `prominence` (`mix(peak_min_prominence, 1, elev)`) and shadow length.
- **Peak pass (shader), after canopy, before FoW:** mirrors the canopy signed-distance-to-boundary scan
  vs the **peak↔non-peak** boundary to get `s` (+inside relief) + `peak_code` (own, else nearest
  peak-neighbour's for the overhang/shadow region). Where `peak_code > 0`: (1) a multi-tap **cast shadow**
  looks back toward `peak_light_dir` (TOWARD the light; top-left = `(-0.7,-0.7)`, canvas +y DOWN) and
  darkens the ground by up to `peak_shadow_strength` where a peak occludes; (2) a **peak composite** over
  the shadowed ground using the shared `canopy_density(s, overhang, softness)` × prominence and the
  world-noise `CANOPY_TREELINE_NOISE` bumpy footline (reused, not duplicated). Peak UV = the same
  continuous map-space `v_map / (2·hex_radius) · peak_scale` as the canopy.
- **PEAK ↔ PEAK IS A SEAM TOO — and the TALLER relief overhangs the LOWER one** (the fix for the "rolling
  hills STILL have hard straight edges, even with `blend_rugged_land` on" report). A peak↔**non**-peak edge is
  a footline (the relief overhangs it and thins away), but an edge between two hexes that BOTH carry relief
  used to be **no boundary at all** — the scan skipped it (`own_is_peak == (ncode > 0) → continue`), so each
  hex composited its OWN peak layer at full density right up to the shared edge and the art switched **1-bit
  ON the hex line**: rolling_hills' green mounds ended in a razor-straight diagonal and alpine_mountain's
  spires began. **The base floor under them was blending correctly the whole time** — it is simply invisible
  under near-opaque relief art, which is why the `blend_rugged_land` gate did not help this seam (`blend_probe`
  state **G** proves it: `G_no_peaks` renders the same seam as a soft organic ecotone).
  So a relief↔relief edge now **cross-fades the two peak layers**, as a CONTINUOUS WEIGHTED MIX (never a pick),
  with the seam's **centre — not its shape — driven by elevation** (the `elev_map` the pass already reads):
  * `asym` = a smooth ODD function of Δelev (`2·smoothstep(−D, D, Δ) − 1`, `D = PEAK_BLEND_FULL_DELTA` = 0.25
    of the 0..1 relative-height scale): +1 when the neighbour towers over us, −1 when we tower over it, **0 at
    equal height**.
  * the 50/50 line sits at depth `m = (peak_overhang − peak_softness) · asym` **into our hex**, and the
    neighbour layer's coverage is `w = 1 − smoothstep(m − peak_softness, m + peak_softness, depth)`. So the
    taller relief spills across the edge and dies exactly `peak_overhang` px in — **the same reach it has onto
    flat ground, and no further** (offsetting by the *full* overhang stacks the feather on top of it and pushes
    the alpine art a whole hex radius into the hills, swallowing them), while the lower relief does **not climb
    uphill**. At Δelev → 0 it degrades to `m = 0`: a symmetric cross-fade of half-width `peak_softness`.
  * **CONTINUITY** (the shoreline's signed-coast-coordinate discipline): the neighbour computes the same edge
    with `asym`, hence `m`, **negated**, and smoothstep is symmetric about its centre — so at the shared edge
    (depth = 0 from **both** sides) the two hexes assign the **same** coverage to the **same** layer, for every
    elevation pair. The seam-centre **wobble** (world noise, so the cross-fade meanders instead of tracing the
    straight hex line) must therefore be applied **ANTISYMMETRICALLY**, signed by the peak **layer index** — a
    total order both sides agree on and that never ties.
  * Neighbours contribute **all at once**, weighted, and the result is their **weighted mean** (own weight =
    what the neighbours have not claimed; denominator `max(1, Σw)`, continuous) — the water depth field's
    discipline, so a hex meeting two different reliefs cannot seam along the bisector. Elevation is averaged
    with the same weights, so **prominence follows the art actually showing** (a tall neighbour's spires
    overhang at THEIR prominence, not faded down to ours). No new config levers: the reach and feather reuse
    `peaks.overhang_width` / `softness_width`.
  * The mean is taken in **PREMULTIPLIED alpha** (`premultiplied`/`unpremultiplied` helpers). Relief art is
    RGBA with large transparent regions, and a straight-alpha mean lets a transparent texel's keyed-out RGB
    pollute the colour wherever the other layer is opaque — it drew bright dotted fringes **along** the seam.
  * **Every `peak_tex` fetch uses `textureGrad`** with gradients taken **before any branch** (`puv_dx`/`puv_dy`,
    hoisted above `if (!in_map)`). The peak pass's fetches all sit in divergent control flow, where implicit-LOD
    `texture()` has **UNDEFINED derivatives**: on a 2×2 quad straddling a relief↔relief seam the lanes take
    different branches, the driver picks a garbage mip, and the fetch returns the wrong resolution — which drew
    a **1-pixel dark column exactly along the hex edge**, i.e. a razor line hiding inside an otherwise correct
    cross-fade. This was invisible before only because the seam it hid in was already hard.
  * A snapshot with **no elevation raster** (preview/rehydrated frames) writes `PEAK_ELEV_FALLBACK` for every
    hex, so Δelev = 0 everywhere and every relief↔relief seam is the symmetric cross-fade.
  * **Still a nearest-edge pick:** a **ground** hex touching two DIFFERENT reliefs picks the *nearest* one's
    layer for the overhang/shadow (`nb_peak_code`). Same 1-bit bug class, not yet hit in a frame; the
    accumulator above is the shape of the fix if it ever shows.
- **THE CAST SHADOW MUST DIE OFF WITH DISTANCE FROM THE RELIEF, NOT WITH THE HEX GRID** (the fix for the
  "dark hexagons in the rocky field next to the hills" report). `peak_code > 0` is true for any hex merely
  **ADJACENT** to relief, and the peak art is near-opaque wherever the occlusion taps sample it — so the
  raw occlusion term is roughly **CONSTANT across the whole neighbour hex** and then terminates on **that
  hex's own boundary**: a flat, hex-shaped dark patch painted into the neighbouring biome, on all six sides
  at once (not even directional). Fix: the occlusion is multiplied by a **`shadow_env` envelope** built from
  the very signed distance to the peak↔non-peak boundary the overhang already computes —
  `env = 1 − smoothstep(0, reach, out_dist)`, where `out_dist` is the distance **beyond the (noise-wobbled)
  footline**, so the envelope is FULL at the footline and 0 within `reach`. `reach` is
  `peak_shadow_len` × an **elevation** factor (`PEAK_SHADOW_ELEV_FLOOR`… 1 — a high massif throws a longer
  shadow) × a **DIRECTIONAL** factor: full length where the relief lies TOWARD the light (we are down-light
  of it → in its cast shadow), shrinking to `PEAK_SHADOW_UPLIGHT_REACH` of it on the LIT side. It stays a
  *directional cast shadow*, **not a symmetric halo** — but the lit side keeps a short **contact skirt**
  rather than a hard angular cutoff, because a hard `dot(light, normal) > 0` gate would step to zero right
  at the footline, where the art is only ~half opaque and the shadowed ground shows through: that trades the
  hexagon for a dark crescent.
  **Continuity, the same discipline as the shoreline's signed coast coordinate `u`:** the envelope is
  evaluated per boundary edge from quantities that read **identically on both sides of a shared edge**
  (the signed distance is 0 there from both hexes, and the relief-normal is the same vector), so nothing
  pops at the hex line. It is a **MAX over the qualifying edges, never a sum** — a hex touching relief on two
  sides takes the deeper of the two and cannot **double-darken into a seam**. (Enveloping by the *single
  nearest* edge only — the discipline `peak_code`/prominence still follow — would have been **discontinuous
  along the bisector** where the nearest edge switches, since the two edges' light alignments differ; a max
  of continuous functions is continuous everywhere.) Verify on **`blend_probe` state S**, and judge it on
  **`S_shadow_footprint*.png`** — the amplified diff against a `shadow_strength = 0` render, i.e. the cast
  shadow **in isolation**. That frame is necessary because the relief art overhangs the footline and is
  semi-transparent out there, so neither the eye nor a pixel sample can separate "shadow" from "dark mound
  fringe" in the composited frame.
- **Peak LOD is DECOUPLED from the blend LOD** (own `peaks_lod_enabled`, `radius ≥ peak_min_radius`,
  default 3.0 ≪ `EDGE_BLEND_MIN_RADIUS`), so the mountain mass persists at far zoom; trilinear-mipmapped
  peak array keeps it smooth (no shimmer).
- **Config levers** (`terrain_config.json` → `peaks` block): `overhang_width` / `softness_width`
  (→ `peak_overhang` / `peak_softness` px, like canopy), `texture_scale` (→ `peak_scale`),
  `peak_min_radius` (LOD floor px), `shadow_length` (→ `peak_shadow_len` px) / `shadow_strength`,
  `min_prominence`, and `light_dir_x` / `light_dir_y` (normalized → `peak_light_dir`). Fallbacks are the
  `PEAK_DEFAULT_*` consts in `MapView.gd`. Peaks are shader-only (same caveat as canopy).
- Verify via `tools/map_preview.gd` **State swatch** with `SWATCH_BIOME_ID = 26` (alpine) →
  `map_swatch.png` (+ `map_swatch_farzoom.png`): faceted peaks composite with light-left/dark-right
  self-shading, overhang the alpine↔prairie seam + cast a darkening shadow onto the prairie, and the
  far-zoom alpine band reads as a raised mountain mass. Restore `SWATCH_BIOME_ID = 2` after.

**Texture readback fix (kept from A):** `TerrainTextureManager` retains the CPU-side layer Images
(`_layer_images`) captured once at build time; `get_terrain_image` serves duplicates from it and
**never** calls `Texture2DArray.get_layer_data()` again (a second readback returned a blank image on
some drivers, whitening the base). The `sampler2DArray` uniform is the same `terrain_textures`.

Verify via `tools/map_preview.gd` State Q → `ui_preview_out/map_biome_hard.png` (blend off, the
reference) vs `map_biome_blend.png` (Approach B on), plus `map_biome_blend_seam.png` (desert↔prairie
close-up): the flat pair blends symmetrically, prairie↔forest / forest↔ocean stay crisp, and terrain
stays aligned with the grid. **State S** (`map_repetition_after.png` + `..._zoom.png`) renders a large
detailed-rugged field (alpine id 26) beside a flat prairie band: the continuous world-space base
sampling means NO per-hex identical-repeat grid on the alpine (each hex shows a different region of the
texture, features flow across boundaries), while the prairie↔alpine seam stays a hard edge.

**Fallback considered:** a MultiMesh (one instance per hex) was the fallback if whole-map inverse-hex
alignment couldn't be matched; the splatmap alignment held, so the single-quad path was chosen (fewer
moving parts, no per-frame instance transforms). **Future:** blue-noise sample instead of hash value
noise. A **per-hex UV rotation+offset for rugged biomes** (hard-edged, so cross-edge rotation
discontinuities hide) was speced to break the texture's *own* tiling-period repeat, but the continuous
world-space base sampling alone removed the objectionable per-hex grid (verified on alpine id 26 at
`base_scale = 0.25`), so it was NOT needed. Do NOT rotate flat biomes — it would break their cross-edge
blend continuity.

---

## HUD Panel Framework (Docked PanelCards)

The HUD (`HudLayer.tscn`) owns the screen regions with one layout authority — a
`RootColumn` VBox split into `TopBar` / `ContentRow(LeftDock · center · RightDock)`
/ `BottomBar`. No panel positions itself with absolute offsets into a region;
everything is container-sized so regions never collide.

### Reserved-edge docking (4-edge, multi-reserver registry)
A docked panel does not overlap or rearrange gameplay panels — it *reserves* a
strip of one screen edge, shrinking the game area to fit beside it, as if the
window were that much smaller. The mechanism is a **reservation registry** keyed
by reserver id, so multiple panels can reserve (possibly different) edges at once:

- **`MapView.set_reserved_inset(id: StringName, edge: int, size: float)`** and
  **`Hud.set_reserved_inset(id, edge, size)`** — `edge` is a Godot `Side` const
  (`SIDE_LEFT/SIDE_TOP/SIDE_RIGHT/SIDE_BOTTOM`); `size <= 0` releases the reserver.
  Each stores `{edge, size}` under `id` and recomputes four per-edge totals
  (`left/right/top/bottom` = Σ of sizes whose edge matches).
- **`Main._apply_reservation(id, edge, size)`** fans a reserver's contribution out
  to both surfaces. Two reservers today: the **Inspector** (`&"inspector"`,
  `SIDE_LEFT` — `reserved_width()` / `reserved_width_changed` on show/hide + live
  drag-resize) and the **Band/City panel** (`&"band_panel"`, its currently-docked
  edge — see below).
- **`MapView`** applies the totals via three coordinated pieces:
  1. `_get_adjusted_viewport_size()` subtracts `left+right` on x and `top+bottom`
     on y, so fit, pan-clamp, draw extents, hit-testing and the minimap indicator
     all treat the remaining rect as the whole viewport.
  2. The node is translated by the **leading** insets only (`position =
     Vector2(left, top)`; trailing right/bottom just shrink the viewport), so the
     reduced coordinate space renders beside the panel(s). Because
     `get_local_mouse_position()` accounts for the node transform, clicks stay
     correct without touching any screen↔hex math.
  3. `_apply_view_clip()` (in `_draw`, via `RenderingServer.canvas_item_set_clip`)
     clips every draw command to the usable rect whenever **any** inset > 0. The
     map is **cover-fit**, so its content is larger than the reduced viewport and
     would otherwise overflow into a reserved strip; clipping confines it.
  - `_is_local_point_in_view()` bounds hit-testing to the full adjusted-viewport
    rect on **both** axes (`0 ≤ local ≤ adjusted` in x and y), so a click under a
    left/top/right/bottom strip is rejected, not just a left one.
- **`Hud`** applies the four totals to `LayoutRoot` offsets: `offset_left = left`,
  `offset_top = top`, `offset_right = -right`, `offset_bottom = -bottom`, so every
  bar and dock lives in the smaller rect.

Because the HUD, reservers, and map all sit under the same `content_scale`
transform, each reservation is a single canvas-space value that applies to all
surfaces with no per-surface scaling. Panels keep their natural docks.

### PanelCard (`ui/PanelCard.gd`)
The single building block for every dock panel. It is a `PanelContainer` (never a
bare `Panel`) that owns the chrome — styled background + title header — and hosts
caller content in a plain `VBoxContainer`. Because it is container-sized, it
always reports a correct minimum size, so the dock reflows automatically.

- **Content contract:** author one child `VBoxContainer` named `CardContent`. The
  card inserts its title header as that container's first row and **never
  reparents the authored widgets** — reparenting them into a runtime wrapper
  silently clears `unique_name_in_owner`, so `%Name` references from the owner
  script break. Reference inner widgets by unique name (`%Name`).
- **Rule:** no anchor-positioned children inside a card. Anchor layout inside a
  container parent is what made the legacy `Panel`s overlap.
- API: `card_title` / `set_card_title()`, `get_content()`, and `hotkey_hint`
  (renders the toggle key in the header, e.g. `"Terrain Types (L)"`; leave empty
  for panels with no show/hide hotkey).
- Replaces the bespoke `ui/AutoSizingPanel.gd` height math — the dock's own
  `ScrollContainer` owns overflow, so cards only size to content.

### PanelDock (`ui/PanelDock.gd`)
Ordered controller for one dock region's `VBoxContainer`. Panels `add(panel,
priority)` to register; the dock reparents them in priority order. Visibility is
data-driven — `set_relevant(panel, false)` (or `panel.visible = false`) removes a
panel from layout flow and the stack reflows with no gap. Hud builds `left_dock`
and `right_dock` in `_ready()`.

**Scroll behaviour:** on construction the dock disables **horizontal** scrolling
on its enclosing `ScrollContainer` and zeroes the stack's horizontal minimum, so
the stack always fills the dock width and content wraps to fit rather than
spilling under a sideways scrollbar (which reads as unpolished for a game HUD).
**Vertical** scroll mode is *not* set by PanelDock — it is configured per dock in
the scene (`HudLayer.tscn`); both docks use `AUTO`, so a scrollbar appears only
when the stack actually overflows.

**Migration status:** `TilePanel`, `OccupantsPanel` (the split selection cards),
`CommandFeedPanel`, and
`TerrainLegendPanel` are now `PanelCard`s (the last two dropped the bespoke
`AutoSizingPanel` height math and the legend's absolute `PRESET_TOP_RIGHT`
positioning that used to overlap the Victory panel). `StockpilePanel` and
`VictoryPanel` are still plain `PanelContainer`s (correctly container-sized, but
not yet cards). `AutoSizingPanel.gd` remains only for the Inspector.

---

## Map markers (MapView hex-icon stack UX)

Co-located hex markers no longer overlap at the hex center. Markers split into two
classes by their source array (not a predicate): **PRIMARY** = player bands, drawn by
`MapView._draw_primary_bands` over the `units`/`populations` array; **SECONDARY** = herds /
food sites / wondrous sites, placed by `MapView._compute_secondary_slots`. (Tuning consts
are grouped near the top of `MapView.gd`, after the FoW/height consts.)

- **PRIMARY — player bands** own the **center spotlight** as an offset card-stack
  (`_draw_primary_bands`/`_draw_band_stack`/`_draw_band_token`). Each band's token is its
  **settlement-stage glyph** — the opaque `settlement_stage_icon` string the sim resolves from
  `settlement_stage_config.json` (⛺ nomadic / 🛖 camp / 🏘️ village today) — drawn via the shared
  `_draw_marker_glyph` drop-shadow helper (`BAND_STAGE_GLYPH_SIZE_FACTOR`), **no faction ring or
  disc**. Ownership is carried by a **faction-colored nameplate banner** (`_draw_band_banner`,
  `BAND_BANNER_*` consts) — a short rounded bar under the token filled with the band's faction
  color, drawn for the **active (primary) card only** and LOD-suppressed below
  `ICON_MIN_DETAIL_RADIUS`. The banner is intentionally sized as the substrate for an optional
  faction/band **name label** later (text on the bar). When `settlement_stage_icon` is empty
  (pre-stage / missing snapshot — rare) the token draws a small **neutral non-circular** fallback
  marker (gray square, `BAND_FALLBACK_MARKER_*`) instead of the glyph, never a disc. The stage
  label (`settlement_stage_label`) surfaces as the Occupants roster row's hover tooltip.
  Multiple bands on one hex fan up-right: up to `BAND_STACK_MAX_CARDS` (3) cards,
  back cards **darkened** (glyph multiplied by `BAND_STACK_BEHIND_TINT` so they recede/shadow),
  the **active** band (the one whose `entity == selected_unit_id`, else the first) drawn
  full-brightness on top. The active band reads by brightness alone — there is **no per-token
  selection ring** (the hex selection outline marks the tile); `BAND_STACK_BEHIND_TINT` is the
  single lever for the recede effect (RGB<1 darkens, alpha<1 fades — swap between the two there).
  Beyond 3, a `×N` count pill folded onto the **right end of the banner** (nameplate-with-count).
  Food-days dot + the travel arrow draw on the active card only.
- **SECONDARY — herds / food sites / wondrous sites** ring the hex in **fixed edge slots**
  (`SECONDARY_SLOT_OFFSETS`, near the hex corners), computed once per frame in
  `_compute_secondary_slots` by category priority **wonder → food → herd** (sequential fill,
  so icons never jump frame-to-frame). Cap `SECONDARY_VISIBLE_CAP` (3) visible icons; extras
  collapse into a `+N` overflow chip (`_draw_secondary_overflow`). Glyphs drop the old dark
  backing disc for a 1px drop shadow (`_draw_marker_glyph`). Herd migration arrow is thinner
  and only drawn on the hovered/selected herd tile. The `×N`/`+N` pills share `_draw_count_pill`.
- **Selected + hovered hex outline** (`_draw_tile_selection_highlight`, reusing `_outline_hex`):
  a solid white hex outline on `selected_tile`, a faint one on `_hovered_tile` (skipped when
  hover == selection) — this replaces the old selection-as-marker-ring feel.
- **Select-then-cycle** (`handle_hex_click` + `cycle_index`): re-clicking the current
  `selected_tile` with >1 band advances `cycle_index` (mod band count) so the stack surfaces the
  next band on top; a fresh tile resets to the top band. `select_occupant` (roster click) syncs
  `cycle_index` to the picked band's stack position via `_cycle_index_for_unit`.
- **Zoom LOD**: below `ICON_MIN_DETAIL_RADIUS` (far zoom, tiny hexes) secondary icons + all
  count/overflow chips are suppressed; only primary tokens draw.

Verify visual changes via `tools/map_preview.gd` (`godot --path . res://tools/map_preview.tscn`
→ `ui_preview_out/map_band_stack.png` / `map_mixed_hex.png` / `map_far_zoom.png` /
`map_stage_glyphs.png` (the ⛺→🛖→🏘️ progression + empty-stage neutral non-circular fallback marker) + the existing
labor-highlight states).

## Command Targeting

Labor allocation is source-centric (assign workers to a source/role, see the **Labor
allocation UI** bullet below). The one remaining **targeting mode** is **move-band** —
picking a destination tile — replacing the old easy-to-miss "select a band…" line.

- **Selection split — Tile card + Occupants roster** (`Hud.gd`): the old single
  selection panel is now **two left-dock `PanelCard`s driven by one script**. The
  **Tile card** (`TilePanel`/`%TileDetail`, priority 10) is the *place* — terrain
  rows (Biome/Height/Tags + the gather module relabeled `Forage:`) and, on a
  food-module tile, the `%ForageAssignControls` "assign foragers" stepper. The
  **Occupants card** (`OccupantsPanel`, priority 12,
  hidden via the dock on an empty hex) is a **selectable roster** of the bands +
  wildlife on the hex, built at runtime into `%RosterList` as two sub-groups
  (`Bands (N)` / `Wildlife (N)`); each row is a `Button` hosting a mouse-transparent
  HBox — a selection accent, a **vitality dot**, name, size, and (bands) an
  activity glyph. Below the roster, `%OccupantDetail` is the selected occupant's
  **detail drawer** for **herds/expeditions** (`_herd_summary_lines` +
  `%HerdAssignControls`; expedition → `_build_expedition_panel` into
  `%AllocationPanel`). **Player-band detail relocated out of the Occupants card into
  the dockable `BandCityPanel`** (see **Band/City dockable panel** below): the roster
  still lists the band, but its summary + labor allocation render in the panel, not
  the card. Selecting a row (`_on_roster_row_selected`) re-homes the
  selection and emits `roster_occupant_selected(kind, id)`; **Main forwards it to
  `MapView.select_occupant`, which moves the map selection ring** (sets
  `selected_unit_id`/`selected_herd_id`) with no hex click. A fresh tile click
  auto-selects the first occupant through the same path. The **vitality dot is
  unified** across map/roster/drawer: a band's dot uses `BandFoodStatus.color_for_days`
  (`days_of_food` → green/amber/red), a herd's uses `_ecology_tier_color`
  (`ecology_phase` → thriving green / stressed amber / collapsing red), sharing the
  exact `HudStyle` HEALTHY/WARN/DANGER constants. Non-player bands list with a neutral
  dot and no allocation panel (their larder/orders aren't ours to see). (The Tile card
  has no camp action — the `found_camp` command was removed end-to-end.)
- **Labor allocation UI** (`Hud.gd`, Early-Game Labor slice 3b — `docs/plan_early_game_labor.md`):
  the band is a **labor pool** whose working-age workers are assigned source-centrically to
  in-range sources/roles. There is **exactly one player band today**, captured each snapshot
  into `_player_band` (first player-faction cohort in `update_band_alerts`); assign/move/clear
  all target it. Every player band is also collected into `_player_bands`, which backs the
  **band-picker dropdown** on the herd/tile assign controls (see `%HerdAssignControls` /
  `%ForageAssignControls` below) — an assignment explicitly names WHICH band supplies the
  workers (built for N even though only one exists live). Three runtime-built control sets replace the retired single-task Scout/Cancel,
  Hunt/policy, and Forage buttons:
  - **`%AllocationPanel`** (band drawer, player band only, `_build_allocation_panel`): reads as a
    "current actions" report — a `Population <size> · Workers <working_age> (Idle <n>)` header (spells
    out that only the ~16 working-age labor, not the 30 people — children/elders are dependents), a
    **Current actions** section with one `−/+` **worker-stepper** row per staffed Forage tile / Hunt
    herd (from the cohort's `labor_assignments`; an empty-state hint when none). **Both source kinds
    tag their take policy** like `[sustain]` (Forage `(x, y) [sustain]`, Hunt `<herd> [sustain]`),
    read off the assignment's `policy` field (now populated for forage too); an older snapshot with no
    forage policy falls back to no tag. **Each source row headlines its per-turn food yield**
    (`… +0.31 /turn`, the assignment's `actual_yield`), with a WARN-tinted `⚠` **overdraw flag** when
    `actual > sustainable + ε` (`OVERHUNT_EPSILON`). A Sustain source gathers at its renewable ceiling
    (`actual == sustainable` → no flag, reads `… · renewable`); a Surplus/Market/Eradicate **forage
    patch** or an over-hunted herd pushes `actual` above `sustainable` → the flag trips (forage is no
    longer hardcoded renewable now that the policy axis can decline a patch). A
    `tooltip_text` spells out actual-vs-sustainable.
    `actual_yield`/`sustainable_yield` are decoded per assignment in `native/src/lib.rs` (inside
    `labor_assignments`); the band-level food flow (net rate + Gathered/Hunted/Eaten breakdown) lives
    on the **Food summary line**, not here — see "Band food status". Then a **Band roles**
    section with the always-shown **Scout** + **Warrior** rows (even at 0), each with a one-line hint so
    the `−/+` steppers read as "this is how you staff this standing role" (Scout's hint reads "Extends
    the band's sight — more scouts see further"; more staffed scouts extend the band's actual sight
    range, so the effect shows directly in the fog, not as a map-action or a reveal disc). Then
    **Move** / **Clear all**.
    Each stepper re-sends `assign_labor_requested` with the new count (0 removes); `+` is gated on idle.
  - **Optimistic pending feedback** (slice 3b UX): assigning workers or moving the band shows
    immediately, before the next snapshot. `_emit_assign_labor` / `_try_dispatch_pending_move_band`
    record a HUD-local **pending** entry per band entity (`_pending_labor[entity] = {turn, assign:{key→…},
    move:{x,y}}`) and re-render. In the panel, a pending source row reads **amber with a "· pending"
    suffix** and the header **Idle** counts optimistically (`_effective_idle` = working-age − effective
    assigned, overlaying pending). **Reconciliation is turn-based:** each pending entry is tagged with the
    snapshot `turn` (header tick, set in `update_overlay`); `_reconcile_pending` (called from
    `update_band_alerts` each snapshot) drops entries issued on an OLDER turn — a newer-turn snapshot is
    authoritative confirmation and cleanly absorbs server-side clamping (the snapshot shows the real
    count). Pending is emitted to MapView via `labor_pending_changed` → `set_labor_pending`.
  - **Selected-band map highlights** (`MapView._draw_band_work_highlights`, drawn when a player band
    is selected, cleared on deselect): the **worked forage tiles** (strong green fill on each
    `forage` assignment's `target_x/y`), the **work-range ring** (thin cyan outline on every tile
    within `work_range`, replicating the sim's true **odd-r hex distance** `hex_distance_wrapped`
    via `MapView._hex_distance` — a real hexagonal ring of 19 tiles at range 2, so highlighted ==
    actually-assignable; the old Chebyshev square wrongly lit its diagonal corners, which are 3
    hex-steps away), and the **hunted
    herds** (red ring on the herd tile + a band→herd link, drawn wherever the herd is since hunt reach
    = `work_range` + leash). **Per-source yield annotations** (`_draw_yield_label`): each staffed forage
    tile / hunted herd is labeled with its `actual_yield` (food/turn, from the assignment inside
    `labor_assignments`) as a small drop-shadow number above the tile center (reusing `_draw_marker_glyph`),
    food-income **green**; a source that overdraws (`actual_yield > sustainable_yield + ε`, reusing the
    panel's overdraw test) reads **WARN amber + a `⚠`** — an over-hunted herd, or a non-Sustain forage
    patch now that the forage policy axis can decline one (a Sustain forage gathers at regrowth, so it
    stays green). The
    label font scales with the hex radius (clamped) and the whole annotation is **LOD-suppressed below
    `ICON_MIN_DETAIL_RADIUS`** (like the secondary markers) so far zoom stays clean. Scout/Warrior
    produce no food → no label. **Scouting draws no map highlight** — staffed scouts extend the band's
    real sight range (visible directly in the fog as a wider Active radius); the old faint-blue scouted
    disc was removed because `scout_reveal_radius` no longer means a reveal-disc radius — it now carries
    the band's effective sight-range bonus (extra tiles beyond base, `0` when no scouts), which the
    client can't turn into a true ring without the server-side `base_range`. New snapshot fields
    `work_range` / `scout_reveal_radius` are decoded in `native/src/lib.rs population_to_dict` and flowed
    onto the MapView unit marker in `_rebuild_unit_markers` (alongside `labor_assignments`);
    `scout_reveal_radius` is still carried (it documents the field) but no longer drawn. **Optimistic pending**
    actions for the selected band draw in a distinct **dashed-amber** style (`_draw_band_pending`, fed by
    `set_labor_pending`) — the pending forage tile, the pending hunted herd (dashed ring-hex + dashed
    band→herd link), and the pending move destination (dashed hex + dashed link) — clearly apart from the
    solid confirmed styles, cleared when the snapshot confirms.
  - **Travel destination** (`MapView._draw_travel_destination`, drawn for the selected traveling unit —
    band OR expedition — from `_draw_band_work_highlights`): when the unit reports `is_traveling`, a
    thin cyan line runs from its tile to the destination hex plus a steady (non-pulsing) cyan target
    reticle on it. The target coords (`travel_target_x` / `travel_target_y`, `uint`, `0,0` and ignored
    unless `is_traveling`) are decoded in `native/src/lib.rs population_to_dict` and flowed onto the
    marker in `_rebuild_unit_markers`. **Wrap-aware:** the target is brought into the band's effective
    column frame via `_wrapped_col_delta`, so the line follows the SHORT (possibly seam-crossing)
    wrapped path the sim actually takes rather than shooting the long way across the map. Only the
    selected unit's destination draws (no clutter). Covered by `marker_field_guard`
    (`travel_target_x`/`travel_target_y`/`is_traveling`) and `map_preview` states `map_travel_band` /
    `map_travel_seam` (seam-crossing) / `map_travel_expedition`.
  - **Band-picker dropdown** (`_build_band_picker`, on BOTH assign controls, above the worker
    stepper so it reads "which band → how many workers"): a `Band:` `OptionButton` listing every
    `_player_bands` cohort by positional name ("Band N", via `_band_display_name`; the cohort has
    no label field), item metadata = the band `entity`. The selection is the **actor band**:
    `_hunt_assign_band` / `_forage_assign_band` hold the picked entity (defaulting to
    `_resolve_assign_band()` when the selected source changes, else persisted across re-renders);
    the worker stepper's cap is that band's `_assignable_hunt_workers` / `_assignable_forage_workers`
    (its `idle_workers` + any it already staffs on that source, so re-editing isn't capped below
    current staffing), and the Assign emit + optimistic pending key off the picked band. Switching
    the dropdown re-caps the stepper and re-renders. Always shown (single-item with one band, so the
    actor is explicit). Lists **all** player bands — in-range filtering (Forage `work_range` / Hunt
    `work_range` + leash) is deferred to the multi-band slice (needs hunt-leash reach in the snapshot).
  - **`%HerdAssignControls`** (herd drawer, huntable herds, `_build_herd_assign_controls`): the
    band-picker, then a **distance-aware** "Assign hunters" **compose** control — a `−/+` worker/party
    count (`_hunt_assign_count`) + a sustain/surplus/market/eradicate **policy picker**
    (`_build_policy_picker`, `_hunt_assign_policy`, `LABOR_HUNT_POLICIES`, default `sustain`). The
    button + command switch on the **wrap-aware hex distance** from the **SELECTED band's** own tile
    to the herd vs that band's **`hunt_reach`** (= `work_range` + hunt leash, decoded as `hunt_reach`
    and flowed onto the marker): **within reach** → a `Hunters` stepper + **"Assign Local Hunt"** →
    `assign_labor hunt <herd_id> <policy> <workers>`; **beyond reach** → a `Party` stepper (cap
    `min(idle_workers, max_expedition_party_size)`) + a distance hint + **"Send Hunting Expedition"** →
    `send_hunt_expedition <faction> <band> <party_workers> <fauna_id> <policy>` (emitted directly, no
    herd-targeting step — the herd is already selected). Every part of the decision (distance, reach,
    band-entity target) keys off the band the picker selects, explicitly threaded — never the faction's
    default band. Distance uses Hud-local mirrors of MapView's odd-r `_hex_distance` /
    `_wrapped_col_delta`, fed grid width + wrap via `Hud.set_grid_dimensions` (Main forwards the
    snapshot `grid` key). Compose state re-seeds from current staffing when the selected herd changes.
    Covered by ui_preview states `herd_verbs` (local) / `herd_hunt_expedition` (single far band) /
    `herd_hunt_band_near` + `herd_hunt_band_far` (two bands, one herd — picker flips local↔expedition).
  - **`%ForageAssignControls`** (Tile card, food-module tiles, `_build_forage_assign_controls`): the
    band-picker, then a sustain/surplus/market/eradicate **policy picker** (`_build_policy_picker`,
    `_forage_assign_policy`, `LABOR_HUNT_POLICIES`, default `sustain`) with a **forage-appropriate**
    behaviour hint (`FORAGE_POLICY_HINTS` — "gather at the patch's regrowth" etc., NOT the herd-cull
    hints), an "Assign foragers" Foragers `−/+` count (`_forage_assign_count`), and a
    **range-aware** **Forage** button → `assign_labor forage <x> <y> <policy> <workers>` (the policy is
    the optional token the sim accepts before the worker count; the policy persists across re-renders
    and re-seeds from the tile's current forage policy via `_policy_for_forage` when the tile changes).
    Mirrors `%HerdAssignControls`' policy affordance. Foraging is
    **stationary** gathering — there is **no forage-expedition fallback** — so the button gates on the
    **wrap-aware hex distance** from the **SELECTED band's** own tile to the forage tile vs that band's
    **`work_range`** (the plain `workRange` field, NOT `hunt_reach`; already decoded/on the marker):
    **within range** → enabled **Forage**; **beyond range** → the button is **disabled** + an
    out-of-range hint (`"(x,y) is N tiles away — beyond this band's forage range (R)"`), no alternative.
    Reuses the same `_hex_distance_wrapped` / `_band_tile` / grid-dim plumbing and explicit
    selected-band threading as the herd hunt. Covered by ui_preview states `food_tile` (in range) /
    `food_forage_out_of_range` (single far band) / `food_forage_band_near` + `food_forage_band_far`
    (two bands, one tile — picker flips enabled↔disabled).

  All emit `assign_labor_requested(payload)` (payload: `faction/band/kind/workers/x/y/herd_id/policy`);
  `Main._on_hud_assign_labor` formats the `assign_labor …` text command. **Clear all** emits
  `cancel_order_requested` (the repurposed `cancel_order` = clear-all → fully idle). The roster
  glyph keeps reading the still-populated `activity` (now the largest-worker
  kind: `idle|forage|hunt|scout|warrior`) and `hunt_mode`. `harvestTask`/`scoutTask` are always
  null server-side and no longer decoded. **Convenience shortcut:** double-clicking a herd on the
  map (`MapView.herd_quick_hunt_requested` → `Main._on_map_herd_quick_hunt` → `Hud.quick_assign_hunters`)
  assigns the player band's idle workers to hunt that herd at Sustain — a no-op with a command-feed
  note when there are no idle workers (never silently nothing).
- **Herd ecology readout** (`Hud.gd` `_herd_summary_lines`): the selection panel shows
  the group's `ecology_phase` (snapshot `HerdTelemetryState.ecologyPhase`) as an
  **Ecology** row — a neutral "Thriving", or a warned "⚠ Stressed" / "⚠ Collapsing"
  that `_format_detail_bbcode` tints amber / red (`_ecology_value_hex`, `HudStyle.WARN_HEX`
  / `DANGER_HEX`). A `Collapsing` herd has been overhunted past the point of no return and
  is crashing to local extinction (see `core_sim` Fauna & Wild Game — depensation collapse).
- **Clear-all / move-band** (`Hud.gd`, Early-Game Labor slice 3b): the single-task
  Scout/Cancel affordance + its optimistic `_pending_transition_bands` machinery were
  **retired** with the labor-allocation model. There is no longer a band-global task to
  cancel — you staff a source down to 0 (`assign_labor … 0`). The **Clear all** button on
  `%AllocationPanel` emits `cancel_order_requested`; `Main._on_hud_cancel_order` sends the
  **repurposed** `cancel_order <faction> <band_bits>` (now clears ALL assignments → fully
  idle). **Move band** is the one remaining targeting flow: the panel's **Move** button
  (`_on_move_band_pressed`) enters tile-targeting (`_pending_move_band` → `_current_targeting_info`
  returns `command: "move", need: "tile"`), the top-centre banner reads "MOVE … click a
  destination tile", and the destination click (`_try_dispatch_pending_move_band`, via
  `show_tile_selection` / `notify_hex_selected`) emits `move_band_requested(payload)` →
  `Main._on_hud_move_band` → `move_band <faction> <band> <x> <y>`. Esc/right-click cancel
  via `cancel_active_targeting` → `_cancel_pending_move_band`.
- **Herd husbandry readout** (`Hud.gd` `_herd_summary_lines`): when a herd's
  `domestication` (snapshot `HerdTelemetryState.domestication`, 0–1) is above 0, a
  **Husbandry** row shows "Domesticating N%" while it's being tamed and "🐄 Domesticated"
  (SIGNAL tint via `_husbandry_value_hex`) once fully domesticated. Progress builds while a
  band Sustain-follows a Thriving herd; the `domesticate` command claims it early (see
  `core_sim` Fauna & Wild Game — Domestication / husbandry).
- **Sedentarization meter** (`Hud.gd` `update_sedentarization`, dispatched from `Main.gd`):
  the player faction's `SedentarizationState.score` (snapshot `sedentarization[]`) shows as a
  compact top-bar block-glyph meter (`▰▰▰▰▰▱▱ 62/100 · soft`, `SedentarizationLabel` in
  `TurnBlock`), tinted amber (soft) / cyan (hard) by stage and hidden until the score is
  meaningful. The soft/hard threshold prompts themselves arrive in the command feed
  (`CommandEventKind::SedentarizationPrompt`). See `core_sim` Campaign Loop — Sedentarization.
- **Demographics readout** (`Hud.gd` `update_demographics`, dispatched from `Main.gd`): the player
  faction's age structure from `PopulationDemographicsState` (snapshot `demographics[]`) shows as a
  top-bar line (`Pop 100  👶34 🛠51 🧓15  dep 96/100`, `DemographicsLabel` in `TurnBlock`) — total
  head-count, the three brackets, and the **dependency ratio** `(children+elders)/working` per 100
  workers, tinted amber when dependents outnumber workers / cyan on a healthy labor surplus. Hidden
  until the faction has population. See `core_sim` Campaign Loop — Population & Demographics.
- **Wondrous Sites (discovered)** (snapshot `discovered_sites[]`, per-faction like
  `sedentarization`/`demographics`; each entry `{faction, sites:[{x,y,site_id,category,display_name,
  glyph}]}` with `category`/`display_name`/`glyph` resolved server-side — client renders the provided
  glyph/name, no client-side site config; undiscovered sites are never sent). Decoded in
  `native/src/lib.rs discovered_sites_to_array` into both the full-snapshot and delta dicts under
  `discovered_sites`. Surfaced three ways, all filtered to `PLAYER_FACTION_ID`:
  (1) **Top-bar readout** (`Hud.gd update_discoveries`, dispatched from `Main.gd`): a compact
  `◈ Discoveries N  <distinct glyphs>` line (`DiscoveriesLabel` in `TurnBlock`, cyan), hidden when 0.
  (2) **Map glyph markers** (`MapView.gd`): ingested into `discovered_sites` + a `discovered_site_lookup`
  (`Vector2i → site`) mirroring `food_modules`; `_draw_discovered_site` draws the site's `glyph` (drop-shadow,
  no backing disc) in a fixed **edge slot** via the shared secondary-marker system (see Map markers below),
  gated on `_visibility_state_at != "unexplored"` (persists on any known/remembered tile — Discovered OR
  Active — since a site is permanent geographic knowledge, unlike the Active-only food-site/herd markers).
  (3) **Tile card** (`Hud._tile_terrain_lines`): a `Site: <display_name>` row (from `_tile_info_at`'s
  `discovered_site_lookup` cross-ref → `site_name`), shown before the FoW discovered early-return since
  it's known knowledge. The server also pushes a `SiteDiscovered` command-feed entry, which renders
  generically via the server-provided `kind`/`label` (no client kind→label map needed). See
  `core_sim` — Wondrous Sites.
- **Band food status** (snapshot `PopulationCohortState.daysOfFood` / `activity` / `supplyNetworkId` /
  `stores[]`, decoded in `native/src/lib.rs` `population_to_dict` as `days_of_food` / `activity` /
  `supply_network_id` / `stores{item:qty}`): the green/amber/red warn·critical thresholds and the
  day→color mapping live in one place, `ui/BandFoodStatus.gd` (config `src/config/band_status_config.json`,
  key `food_days.{warn,critical}`; `999` = not food-limited → ∞). Surfaced three ways:
  (1) `MapView._draw_band_status` draws a food-days dot on each **player** band
  (`_is_player_unit`); (2) `Hud._band_food_line` adds a `Food  <N>  (<D> days)`
  row to the band selection panel, tinted by the thresholds via `_format_detail_bbcode`;
  (3) `MapView._draw_supply_links` faint-chains player bands sharing a `supply_network_id` (`0` = solo).
  **Band food flow on the Food line** (snapshot `PopulationCohortState.foodIncome`/`foodConsumption`,
  decoded as `food_income`/`food_consumption`, flowed onto the MapView unit marker + guarded by
  `marker_field_guard`): for a **player** band with real flow, `_band_food_line` appends the **net
  per-turn rate** — `Food 15 (19 days) · −0.77 /turn` — where net = `food_income − food_consumption`,
  tinted green (≥0) / red (<0). The days-to-empty stays only in the `(N days)` figure; it is not
  repeated. The `Food` label is a **click-to-expand disclosure** (a `▸/▾` caret) toggling a
  **category breakdown** beneath it — indented `▲ +X  Gathered` / `▲ +Y  Hunted` / `▼ −Z  Eaten`
  sub-lines (Gathered/Hunted = Σ per-source `actual_yield` by kind, Eaten = `food_consumption`),
  rendered through the **shared morale-breakdown path** in `_format_detail_bbcode` (income ▲ green,
  eaten ▼ amber). The breakdown **auto-shows when food is concerning** (`_food_is_concerning`:
  net-negative OR runway below the warn threshold, mirroring `_morale_is_concerning`), else it's
  collapsed but reachable via the click. Older snapshot / no flow → the bare `Food N (D days)` line,
  no net/disclosure. **The Food + Morale rows share ONE disclosure mechanism** (see "Band morale
  readout" for the shared helpers) — see `_register_disclosure` / `_on_detail_meta_clicked` /
  `_breakdown_open_for` / `_breakdown_expanded`. (The label + click are wired on BOTH the Occupants-card
  drawer's `%OccupantDetail` and the dockable Band/City panel's `get_band_detail_label()`.)
- **Band morale readout** (snapshot `PopulationCohortState.morale`, decoded in `native/src/lib.rs`
  `population_to_dict` as `morale`, a 0–1 float on each cohort dict; flowed into the MapView unit marker
  in `_rebuild_unit_markers`): a band can shrink while well-fed when a harsh tile erodes morale until
  births fall below elder mortality. `BandFoodStatus.gd` owns the morale thresholds too (config key
  `morale.{warn,critical}` = `0.40`/`0.25`, just above the ~0.20 birth floor) and the mirrored
  `color_for_morale`/`hex_for_morale` helpers (same green/amber/red palette, but a plain scalar — no
  "unlimited" sentinel). `Hud._band_morale_line` adds a `Morale: <N>%` row to the drawer **for player
  bands only** (`_is_player_unit`), tinted by `hex_for_morale` via `_format_detail_bbcode` (same
  stash-then-tint pattern as the Food row, using `_selected_band_morale`).
- **Morale trend + named cause** (snapshot `PopulationCohortState.moraleDelta` / `moraleCause`, decoded in
  `native/src/lib.rs` `population_to_dict` as `morale_delta` (raw Scalar/1e6, signed) / `morale_cause`
  (int; `0=None,1=Terrain,2=Cold,3=Unrest`), flowed into the MapView unit marker): "low morale" named the
  symptom, not the cause — the morale drivers live server-side and were discarded each turn until the
  cohort started exporting the per-turn trend + dominant negative driver. `Hud._band_morale_line` appends
  a trend arrow (`▼` falling / `▲` rising / none when `|morale_delta| < MORALE_TREND_EPSILON`) and, when
  falling, the plain-language cause via `_morale_cause_label` — `Terrain`→"harsh terrain", `Cold`→"harsh
  climate" (the server penalty fires on hot **or** cold deviation, so not literally "cold"),
  `Unrest`→"unrest". `Terrain` appends the band's `_selected_tile_info.terrain_label` in parens
  (`Morale: 22% ▼ — harsh terrain (Karst Cavern Mouth)`) — the "it's the hex you're on" payload. A
  rehydrated save reports `morale_delta 0 / cause None` for one turn (the sim doesn't persist them); the
  row degrades to a bare percentage.
- **Civilization Wellbeing — productivity, itemized morale, recovery** (see
  `docs/plan_civ_wellbeing.md`; snapshot `PopulationCohortState.outputMultiplier` /
  `discontentFraction` / `lastEmigrated` / `lastImmigrated` / `grievance` + the four signed
  Layer-1 contributions `moraleSettling` / `moraleTerrain` / `moraleClimate` / `moraleUnrest`,
  decoded in `native/src/lib.rs population_to_dict` as `output_multiplier` / `discontent_fraction`
  / `last_emigrated` / `last_immigrated` / `grievance` (telemetry only, not displayed in P1) /
  `morale_settling` / `morale_terrain` / `morale_climate` / `morale_unrest`, all flowed onto the
  MapView unit marker in `_rebuild_unit_markers`). Player-band drawer only (`_unit_summary_lines`):
  - **Output row** (`_band_output_line`): `Output: N%` shown when `output_multiplier < OUTPUT_FULL`
    (1.0), placed just under Morale. Tinted ink → amber → red by `BandFoodStatus.hex_for_output`
    (config `band_status_config.json` `output.{warn,critical}` = `0.85`/`0.60`; near-full reads
    neutral ink, *not* green — it's a productivity note, not a "good"). Ties productivity to morale.
  - **Itemized morale breakdown** (`_morale_breakdown_lines`): the four signed contributions
    (their sum IS `morale_delta`) as indented sub-lines (e.g. `    ▲ +1.0%  settling`). Only
    contributions above `BandFoodStatus.morale_breakdown_epsilon()` (config `morale.breakdown_epsilon`
    = `0.002`) list. Labels: `settling`, `harsh terrain (<terrain_label>)` (matches the headline cause
    treatment), `harsh climate`, and `unrest`/`culture` by sign. `_format_detail_bbcode` tints each
    row two-tone by its sign glyph (▲ = HEALTHY green, ▼ = WARN amber — deliberately not a rainbow);
    the indented breakdown lines are intercepted before the KV split. The **Morale row is a
    click-to-expand disclosure identical to Food** (the `▸/▾` caret + `meta_clicked` toggle share
    `_register_disclosure` / `_on_detail_meta_clicked` / `_breakdown_open_for` / `_breakdown_expanded`,
    keyed `"morale:<entity>"`): **auto-shown when concerning** (`_morale_is_concerning`: below warn
    **or** falling past `MORALE_TREND_EPSILON`), else collapsed but expandable via the click. The
    contributions always compute so the good state can be manually expanded; the disclosure is offered
    only when there's actually something to show (a contribution above epsilon, or the concerning
    recovery line).
  - **Recovery guidance** (`RECOVERY_GUIDANCE_TEXT`): a dim `↑ Recover: move to Hospitable ground ·
    Scout · Hunt` line (the real levers, NOT harvest), appended under the breakdown **only when
    morale is concerning** (a healthy band that manually expands its breakdown is not told to
    "recover"). `_split_detail_kv` skips lines beginning with `↑` so it renders as a dim sentence.
  - **Action morale hints**: the Scout button tooltip (`MORALE_HINT_SCOUT`, "(+morale)") and the four
    persistent Hunt/Follow policy tooltips (Sustain/Surplus/Market/Eradicate get `MORALE_HINT_PERSISTENT`
    appended, "(+morale/turn)") advertise the positive levers; the one-shot Single policy does not.
- **Tile-card Habitability** (snapshot `TileState.habitability`, decoded in `native/src/lib.rs`
  `tile_to_dict` as `habitability` (raw Scalar/1e6; band-independent per-turn morale drain of the tile's
  terrain + temperature, ≥0, bigger = harsher), stored in `MapView.tile_habitability` keyed by
  `Vector2i` and copied onto the `_tile_info_at` dict): `Hud._tile_terrain_lines` adds a
  `Habitability: <rating>` row (before the FoW discovered/unexplored returns — it's terrain-intrinsic, so
  fine on a remembered tile; only shown when the field is present). `ui/TileHabitability.gd` is the single
  source of truth — config `src/config/tile_habitability_config.json` (`habitability.{hospitable_max,
  fair_max,harsh_max}` = `0.02`/`0.05`/`0.09`) buckets the drain into Hospitable/Fair/Harsh/Hostile,
  tinted HEALTHY/INK/WARN/DANGER via `hex_for_rating` in `_format_detail_bbcode` (mirrors the
  `BandFoodStatus` bucketing pattern). The Karst Cavern Mouth (~0.0825) reads "Harsh" (amber).
  With the latitude climate + cold-morale tolerance dead-band (see `core_sim`), temperate
  mid-latitudes read "Hospitable", the equator "Hospitable/Fair", and poles/high-alt/caverns
  "Harsh/Hostile" — the config buckets (`0.02`/`0.05`/`0.09`) spread cleanly across that range,
  so no re-tune was needed.
- **Tile-card Climate** (snapshot `TileState.temperature`, decoded in `native/src/lib.rs`
  `tile_to_dict` as `temperature` (°); temperature is now a **latitude + elevation** climate
  (equator-in-the-middle, poles cold) with a small element jitter, NOT the old element
  checkerboard — see `core_sim`), stored in `MapView.tile_temperature` keyed by `Vector2i` and
  copied onto the `_tile_info_at` dict): `Hud._tile_terrain_lines` adds a `Climate: <band>` row
  next to Habitability (before the FoW discovered/unexplored returns — it's terrain-intrinsic, so
  fine on a remembered tile; only shown when the field is present so rehydrated tiles degrade
  gracefully). `ui/TileClimate.gd` is the single source of truth — config
  `src/config/tile_climate_config.json` (`climate.{tropical_min,warm_min,temperate_min,cool_min}`
  = `26`/`20`/`12`/`3`) maps the temperature into Tropical/Warm/Temperate/Cool/Polar, making the
  latitude gradient legible ("far south → Polar"). The row is **informational** — neutral ink, no
  HEALTHY/WARN/DANGER tint, so it doesn't overload the Habitability row's warning semantics.
- **Band alerts → the turn orb** (`Hud.gd` `update_band_alerts`, dispatched from `Main.gd` on the
  snapshot `populations`): the standalone left-dock **Alerts panel was removed** and its alerts folded
  into the turn-orb attention model (see next bullet) — the single player-faction loop now builds the
  orb's `attention` array instead of a separate alerts array. NOTE: cohorts carry no top-level band label
  in the snapshot — names fall back to a positional "Band N"; a server-side band-label field would make
  names authoritative.
- **Turn orb & attention model** (`ui/TurnOrb.gd` + `ui/TurnOrb.tscn`, last `BottomBar` child;
  `docs/plan_hud_nav_turn_orb.md`): the bottom-right orb replaces the "Advance Turn" button and
  is a **generic attention hub**. Readiness = the attention registry is **empty** → a calm cyan
  `SIGNAL` pulse ("nothing needs you"); any entries → the pulse stops and a **count badge** tinted
  by the highest severity shows. **The orb face always advances the turn** (`_on_face_pressed`): with
  an **empty** registry the click emits `advance_requested` directly (no popover — an empty popover has
  nothing to review, and once mis-stretched to full height it pushed its own `Advance ▸` footer
  off-screen, trapping the player); with **entries** it toggles a **reasons popover** (built at
  runtime, `HudStyle.card_stylebox()`) — one row per entry (severity stripe + kind icon + label +
  detail + right-aligned `Jump →`), highest-severity first, plus an `Advance ▸` footer. The orb
  knows nothing about producers; it renders a list of generic **Attention** dicts:
  `{kind, severity ("info"|"warn"|"critical" → SIGNAL/WARN/DANGER), label, detail, x, y}` where
  `x < 0` = non-locating (renders `Open ▸`, a no-op stub for now). Kind→icon (in `TurnOrb.gd`):
  `starving`→🍖, `losing_population`→📉, `idle_workers`→🛠, unknown→●. Wiring stays stable via Hud
  relays: a row's jump → `focus_requested` → `alert_focus_requested` → `MapView.focus_on_tile`
  (the same centering the retired Alerts panel used); the footer → `advance_requested` →
  `next_turn_requested(1)`; `update_overlay` pushes the turn number via `set_turn`. The **three live
  producers** (all in `Hud.update_band_alerts`, one loop over the player faction, each pushed with the
  band tile `current_x`/`current_y` so Jump locates it) — the folded-in Alerts panel:
  - **`starving`** (critical) — `BandFoodStatus.is_critical(days)`; label `"<band> starving"`, detail = `_food_days_text(days)`.
  - **`losing_population`** (warn) — shrank vs the previous snapshot (`_prev_band_sizes`); label `"<band> losing population"`, detail = `_decline_reason(days, morale, morale_cause, last_emigrated)` (`— starving` / `— people leaving` / `— harsh terrain|climate|unrest` / `— low morale`).
  - **`idle_workers`** (warn) — `idle_workers > 0`; label `"N idle workers"`, detail = band name. Supersedes the old `activity == idle` alert (a worker count is more actionable).

  The orb severity-sorts (critical floats up), so a starving band tops the popover. Future producers
  (`war` / `decision` / `expedition_awaiting`) are stubs the model already fits — one producer each,
  **no orb changes**.
- **Targeting: move-band + send-expedition + send-hunt-expedition** (`Hud.gd`): the single-task
  forage/scout/hunt/follow `_pending_*` flows were retired with labor allocation. Three targeting
  flows remain, all built on the same `_pending_*` → `_current_targeting_info()` →
  `_refresh_targeting()` machinery: `_pending_move_band` (`command: "move"`, `need: "tile"`),
  `_pending_send_expedition` (`command: "expedition"`, `need: "tile"`, carries the outfitted band +
  party size), and `_pending_send_hunt_expedition` (`command: "hunt_expedition"`, `need: "herd"`).
  `_current_targeting_info()` returns a descriptor (`{active, command, need, origin_x/y,
  context_label}`) for whichever is set; `_refresh_targeting()` shows the floating **targeting
  banner** (top-centre, `HudStyle.banner_stylebox()`: cyan reticle + command + instruction + Cancel)
  and emits `targeting_changed(info)`. `show_tile_selection` + `notify_hex_selected` dispatch all
  three pending flows on the click (the tile click carries `tile_info.herds`, which the hunt flow
  resolves its target from).
- **Main forwards** `hud.targeting_changed → map_view.set_targeting` and
  `map_view.targeting_cancel_requested → hud.cancel_active_targeting`.
- **MapView draws** the overlay (`_draw_targeting`): `need == "tile"` draws a reticle on the
  hovered hex (the `need == "band"` path is now unused). Esc / right-click during targeting emit
  `targeting_cancel_requested` instead of panning; the pulse is animated from `_process`.
- **Resolution**: the destination tile click (`_try_dispatch_pending_move_band`) emits
  `move_band_requested` → `Main._on_hud_move_band` → `move_band …`; the expedition-target click
  (`_try_dispatch_pending_send_expedition`) emits `send_expedition_requested` →
  `Main._on_hud_send_expedition` → `send_expedition …`.
- **Scouting expedition** (`docs/plan_exploration_and_sites.md` §2; snapshot
  `PopulationCohortState.isExpedition`/`expeditionMission`/`expeditionPhase`, decoded in
  `native/src/lib.rs population_to_dict` as `is_expedition`/`expedition_mission`/`expedition_phase`,
  flowed onto the MapView unit marker in `_rebuild_unit_markers`; `homeBandEntity` is decoded as
  `home_band_entity` (the outfitting band — powers the Band panel's Active-expeditions section),
  while the persistence-only `expeditionAnnounced`/`pendingReveal*` fields stay undecoded). A
  detached party is a `PopulationCohort` tagged `Expedition` that flows through the same
  `populations[]` array as a band. Surfaced four ways:
  (1) **Distinct map marker** (`MapView._draw_unit` → `_draw_expedition_body`): a hollow,
  faction-tinted **flag disc** (⚑) instead of a resident band's solid dot; when
  `expedition_phase == "awaiting"` a **pulsing amber (WARN) ring** signals idle-at-objective needing
  an order (animated from `_expedition_time` in `_process`, gated on `_has_awaiting_expedition` set
  at marker-rebuild). Resident-band rendering is untouched.
  (2) **Expedition drawer panel** (`Hud._render_occupant_drawer` → `_build_expedition_panel`):
  replaces the labor-allocation panel for a selected expedition (no labor in v1). Drawer text
  (`_expedition_summary_lines`) shows Mission / humanized Phase / Party / Provisions (`daysOfFood`);
  the panel hosts **Recall** (→ `recall_expedition_requested` → `Main._on_hud_recall_expedition` →
  `recall_expedition …`) + **Move** (reuses `_on_move_band_pressed`; `_resolve_assign_band` returns
  the selected expedition since it's a player unit — Move retargets it via `move_band` unchanged, no
  un-gating needed).
  (3) **Outfit UI** (`Hud._build_allocation_panel` → `_build_send_expedition_controls`): on a
  selected resident band, a "Send scouting expedition" party-size stepper (max =
  `min(idle_workers, max_expedition_party_size)`; the server's hard cap comes from the
  `maxExpeditionPartySize` snapshot field, decoded as `max_expedition_party_size`, defensively
  falling back to idle when absent/0) + a button entering `_pending_send_expedition` targeting.
  (4) The `marker_field_guard` covers the four new marker keys (`is_expedition`,
  `expedition_mission`, `expedition_phase`, `max_expedition_party_size`). The server still rejects
  a genuinely over-cap request with a feed message as a backstop.
- **Hunting expedition** (PR 2, `docs/plan_exploration_and_sites.md` §2b; snapshot
  `PopulationCohortState.expeditionTargetHerd` (string fauna_id) / `expeditionHuntPolicy` (string
  `sustain|surplus|market|eradicate`) / `expeditionCarryCap` (float), decoded as
  `expedition_target_herd` / `expedition_hunt_policy` / `expedition_carry_cap` and flowed onto the
  marker; `expedition_mission` also takes `"hunt"`, `expedition_phase` also takes
  `"hunting"`/`"delivering"`). A hunt party follows a migratory herd, accumulates food up to a carry
  cap, and drops it at the band — the second verb on the same expedition machinery. Surfaced:
  (1) **Distinct map marker** (`MapView._draw_expedition_body`): a hollow 🏹 **bow disc** (vs the
  scout's ⚑ flag), keyed on `expedition_mission == "hunt"`. Phase read: `hunting` (gathering) draws a
  small red "working" cue ring; `delivering`/`returning` (hauling home) draw a green food pip.
  (2) **Hunt drawer panel** (`Hud._expedition_summary_lines` branches on mission): Mission "Hunting
  expedition", **Target** herd (`expedition_target_herd`, species via `_herd_label_for_id` → raw id
  fallback), **Policy** (`expedition_hunt_policy`, capitalized), humanized **Phase**
  (Hunting/Delivering/Returning), Party, and **Carried X / cap** (`stores` total vs
  `expedition_carry_cap`, days from `daysOfFood`) with a **· FULL** badge at the ceiling. Reuses
  `_build_expedition_panel` (Recall + Move, "Returning"-when-returning treatment — mission-agnostic,
  so hunt parties get it too).
  (3) **Outfit UI** (`Hud._build_send_expedition_controls`): under the shared "Send expedition"
  section (party stepper + "Send scouting expedition"), a **hunt policy radio**
  (`_build_policy_picker(…, _send_hunt_policy)`, Sustain/Surplus/Market/Eradicate, default Sustain)
  with a one-line behaviour hint (`SEND_HUNT_POLICY_HINTS`), then "Send hunting expedition". It enters
  a HERD-targeting pending mode (`_pending_send_hunt_expedition`, `command: "hunt_expedition"`,
  `need: "herd"`) carrying band + party + policy; the target click resolves to a huntable herd on the
  clicked hex (`_huntable_herd_id_on_tile` reads `tile_info.herds`) and emits
  `send_hunt_expedition_requested` → `Main._on_hud_send_hunt_expedition` →
  `send_hunt_expedition <faction> <band> <party_workers> <fauna_id> [policy]` (trailing policy;
  server defaults Sustain). No huntable herd on the hex → a command-feed nudge, stays in targeting.
  `MapView._draw_targeting` glows huntable herds + reticles the hovered hex for `need == "herd"`.
  (4) `marker_field_guard` covers `expedition_target_herd` / `expedition_hunt_policy` /
  `expedition_carry_cap`. Recall is the unchanged `recall_expedition` (works for hunt parties too).
- **Retired verbs (Early-Game Labor slice 3a):** the server now parses-but-ignores
  `follow_herd` / `scout` / `forage` / `hunt_fauna` / `hunt_game`. Every client control that
  emitted them was removed or repointed so nothing is silently dead: the map double-click
  `scout` shortcut was dropped and `follow` repointed to quick-assign hunters; Main's
  `_issue_*`/`_on_hud_follow_herd`/`_on_hud_unit_scout` builders are gone; the Fauna tab's
  follow button, the Terrain tab's Scout Tile button, and the Commands tab's scenario
  Scout/Follow rows were removed (script + `InspectorLayer.tscn` nodes). No code path in
  `Main.gd`/`Hud.gd`/`MapView.gd`/`Inspector.gd` builds any of those five lines.

## Band/City dockable panel

`ui/BandCityPanel.gd`/`.tscn` — a CanvasLayer that is the **persistent band/city
command center**: shown whenever ≥1 player band exists, always displaying a
"current band" (`_panel_band`). Design/roadmap: `docs/plan_band_city_dock.md`.

- **Dockable + persisted.** The user docks it to any of the 4 edges (default
  `SIDE_LEFT`) or collapses it to a thin rail; the choice (+ collapsed bool)
  persists to `user://band_city_dock.cfg` via `ConfigFile` (loaded in `_ready`,
  saved on change — the client's first user-pref file). It reserves its edge
  through the registry above: `reservation_changed(edge, size)` →
  `Main._apply_reservation(&"band_panel", edge, size)` (size = the cross-axis
  width/height, `COLLAPSED_SIZE` when railed, or 0 when hidden), so the map + HUD
  reflow off the reserved edge. All geometry/typography are named constants +
  `HudStyle`; the map-facing edge gets a `SIGNAL_DEEP` accent seam.
- **Header chrome.** Settlement **stage glyph + name + stage label**
  (`set_header` — glyph/label from the band marker's `settlement_stage_icon` /
  `settlement_stage_label`, neutral glyph fallback), a `◀ n/N ▶` **cycler**
  (`set_cycler`) over `_player_bands`, a 2×2 **dock chooser** (active edge
  highlighted), and a **collapse** toggle. `cycle_requested(delta)` → Main relays
  to `Hud.cycle_panel_band`.
- **Content relocation (from the Occupants card).** The **player-band** branch of
  `Hud._render_occupant_drawer` now renders into the panel via `_render_band_into_panel`,
  which assembles an ordered array of **section blocks** — a summary block
  (`_unit_summary_lines`), the Active-expeditions block, then the allocation sections
  (`_build_allocation_sections`) — and hands them to `BandCityPanel.set_band_sections`
  (see "Responsive body"). `_build_allocation_sections` returns the discrete Workers /
  Current actions / Band roles / Orders / Send-expedition VBoxes; the legacy
  `_build_allocation_panel(band, target)` wrapper still exists and fills the flat
  `%AllocationPanel` (the no-panel `ui_preview` fallback) by appending those same blocks.
  Herd/expedition detail stays in the Occupants card (`%OccupantDetail` / `%AllocationPanel`
  — still the expedition host **and** the no-panel fallback).
- **Live + persistent.** `_refresh_panel_band()` (called each snapshot from
  `update_band_alerts`) hides the panel when there are zero player bands, else
  re-resolves `_panel_band` against the fresh snapshot (by entity, falling back to
  the first band) and re-renders so steppers/idle stay current. Selecting a
  herd/empty tile leaves `_panel_band` intact — the panel persists across selection
  changes. `cycle_panel_band(delta)` walks `_player_bands`, **recenters the map**
  on the band (`alert_focus_requested` → `MapView.focus_and_select_tile`), then
  pins the exact band so ring/Tile card/roster/panel all agree.
- **Bands vs expeditions.** `update_band_alerts` splits the player faction into
  `_player_bands` (resident bands — NOT `is_expedition`) and `_player_expeditions`
  (detached scout/hunt parties). The cycler + band-picker read `_player_bands`
  only, so a band + 2 expeditions reads **1/1**, not 1/3. Expeditions surface
  instead as an **Active expeditions** section on their home band (see below).
- **Active expeditions section.** `_render_band_into_panel` → `_build_panel_expeditions_block`
  builds a self-contained expeditions **section block** (handed to the panel in the section
  array, so it's its own flow item / stack row) with one ghost-button
  row per `_player_expeditions` entry whose `home_band_entity == _panel_band.entity`
  (correct for N bands; omitted when none). Row summary: hunt `🏹 <herd> · <Phase> ·
  <Policy>`, scout `⚑ → (x,y) · <Phase>`. A row click reuses the cycler's routing —
  `alert_focus_requested`→`focus_and_select_tile` + `roster_occupant_selected`→
  `MapView.select_occupant` — so the map ring moves to the expedition and the
  **Occupants card** (not the band panel) renders its `_build_expedition_panel`
  drawer; `_panel_band` stays put. `home_band_entity` is decoded in
  `native/src/lib.rs population_to_dict` from the snapshot's `homeBandEntity`,
  flowed onto the MapView unit marker, and covered by `marker_field_guard`.
- **Responsive body — section blocks (tall stack vs wide column-flow).** The band
  content is a list of discrete **section blocks** Hud hands the panel via
  **`set_band_sections(blocks: Array)`** (replacing the old
  `get_band_alloc_container()`/`get_band_detail_label()`/`get_band_expeditions_container()`
  fill-a-container contract): the summary RichTextLabel block, the Active-expeditions
  block, then the allocation sections (Workers / Current actions / Band roles / Orders /
  Send expedition). Hud builds them in `_render_band_into_panel` (allocation sections from
  `_build_allocation_sections` — the per-row stepper/band-picker/pending/expedition wiring
  is unchanged, only each row's *parent* is its section VBox now; the legacy flat
  `%AllocationPanel` fallback still fills by appending the same blocks). The panel **owns**
  the blocks (frees the prior set on each call) and arranges them by dock aspect
  (`_relayout_body`/`_arrange_sections`, hooked off `_apply_dock_layout`, reparenting the
  **same** block nodes on a tall↔wide flip — no Hud re-render): **tall** (LEFT/RIGHT) = a
  vertical `ScrollContainer` stack whose reserved **WIDTH fits the content** (`_measure_tall_width`,
  the mirror of the wide height fit): the cross-axis width is `maxf(PANEL_WIDTH, content-min)` (the
  PanelContainer's combined min width — margins + widest section), floored at `PANEL_WIDTH`, so
  `_root`, the seam (`_position_seam`), and the reservation all track the **true card edge** — a wide
  section (a long Hunt row, the send-expedition button) no longer overflows a fixed-380 `_root` and
  freezes the seam mid-card. Re-measured (deferred one frame, `is_equal_approx`-guarded — the content
  min is width-independent so there's no resize feedback) on `set_band_sections`, dock/collapse change,
  and viewport resize. **Wide** (TOP/BOTTOM) = **manual balanced-column packing** (`_pack_wide_columns`):
  column count from the
  available width (`num_cols = clamp(avail / (SECTION_COLUMN_WIDTH + WIDE_FLOW_SEPARATION), 1,
  #blocks)`), blocks distributed **greedily into the shortest column** so the tallest column
  is minimized, columns in an HBox. The panel then **sizes its T/B height to the content** —
  the reservation it reports (`reservation_changed`) is `header + tallest-column + margins`,
  so the map/HUD reflow to exactly fit and **nothing clips** (fit-to-content, not a fixed
  `PANEL_HEIGHT`). Re-packs on dock change, `set_band_sections` (content change), and window
  `size_changed`; a deferred re-measure (`await process_frame`) lets the `fit_content` summary
  RichTextLabel settle before the height is finalized. Safety net: reserved height is capped
  at `MAX_WIDE_HEIGHT_FRACTION` of the window, past which the columns' ScrollContainer
  re-enables vertical scroll. (Earlier `VFlowContainer` / fixed-height wide layouts were
  replaced — VFlowContainer can't do fit-to-content *and* multi-column: unbounded height
  stops it wrapping.)
- Verify chrome + reflow via `tools/band_panel_preview.gd`
  (`godot --path . res://tools/band_panel_preview.tscn` → `ui_preview_out/
  band_panel_{left,right,top,bottom,collapsed}.png`).

## Inspector Panels

See `docs/godot_inspector_plan.md` for full roadmap.

| Tab | Purpose |
|-----|---------|
| Map | Overlay selector, logistics toggle, map size dropdown, Generate Map button |
| Terrain | Full biome histogram, tag histograms, tile drill-down, terrain-type highlight dropdown, **Export Map** button |
| Fauna | Herd registry + density telemetry (display-only; follow-herd command retired) |
| Culture | Layer trait vectors, divergence meters, resonance pushes |
| Military | Readiness heatmaps, cohort summaries |
| Power | Grid metrics, node list, incident feed |
| Crisis | Dashboard gauges, modifier tray, event log |
| Knowledge | Ledger overview, timeline graph, espionage mission queue |
| Logs | Streaming tracing feed, level/target/text filters, duration sparkline |
| Commands | Turn/rollback/autoplay, axis bias, spawn utilities, debug hooks |

**Capability gating** (`Inspector._apply_capability_gating`): most tabs enable only when the matching `CapabilityFlags` bit is set. **Terrain is exempt** — it is an always-available inspection tab with no capability-gated actions (the former Found Camp action + its CAP_CONSTRUCTION gate were removed with the retired `found_camp` command). **Migrated tab panels don't grey out** — instead of disabling the tab (confusing: a dead tab with no explanation), the coordinator calls `panel.set_available(has_flag)` and the panel stays clickable, rendering a "🔒 Locked — unlocks via …" message while gated (see `PowerPanel`). `_set_tab_enabled` is still used for tabs not yet migrated to the panel contract. Its **terrain-type highlight** dropdown lists every defined terrain (via `TerrainDefinitions`), and selecting one calls `MapView.set_terrain_highlight(id)`, which outlines/tints all matching hexes map-wide (ignoring Fog of War) — handy for spotting a biome or confirming one is absent. Selecting "none" (`-1`) clears it.

The overview text draws a **full biome histogram** (`_render_terrain` → `_histogram_bar`): every present biome, sorted by count, with a monospace `[code]` bar scaled to the most common biome plus its tile count and percentage — all computed client-side from the streamed `_terrain_counts`. The **Export Map** button (`_on_export_map_button_pressed`) sends the fire-and-forget `export_map` runtime command; the server writes the current map (terrain snapshot + resolved seed) to its `exports/` scratch dir as JSON (see `sim_schema` `MapExport`). Tile coordinates shown here as `@x,y` (`_format_tile_coords`) index straight into the export's row-major samples, so the same coordinate names a hex in the client, in the export file, and in tests.

### Tab-panel extraction pattern

`Inspector.gd` is being decomposed from a single god-object into per-tab panels;
`Inspector` stays the **coordinator** (streaming, capability gating, typography,
reserved-width/resize) and forwards each update to the tab panels. A tab panel:

- Is a script attached to the tab's own scene node (its `class_name` typed by the
  node's base type — the Power tab is a `ScrollContainer`, so `PowerInspectorPanel
  extends ScrollContainer`). References its widgets by `%UniqueName` (mark those
  nodes `unique_name_in_owner` in `InspectorLayer.tscn`) and wires its own signals
  in `_ready()`. Same model as the pre-existing `scripting/ScriptManagerPanel`.
- Implements the coordinator contract: `apply_update(data: Dictionary,
  full_snapshot: bool)` — the panel reads only the snapshot/delta keys it owns and
  re-renders itself — and `reset()` — drop all panel state so the coordinator can
  re-seed it from a clean slate. `Inspector._apply_update` forwards to
  `panel.apply_update(...)`; `_render_static_sections` calls `panel.reset()` (today
  only on init; it is the hook a future disconnect/full-reinit flow would call). The panel owns its schema keys,
  state, and rendering; the coordinator knows none of them. Panels needing extra
  collaborators add setters (as `ScriptManagerPanel` does with `set_manager()`).
- Capability-gated panels also implement `set_available(available: bool)` — the
  coordinator maps the `CapabilityFlags` bit to it in `_apply_capability_gating`,
  and the panel renders a locked explanation while unavailable (the tab is *not*
  disabled). Always-on tabs (e.g. Terrain) skip this.

Optional contract hooks a panel adds only if it needs them:
- `apply_typography()` — the coordinator's `apply_typography()` calls it so the
  panel styles its own widgets (`CrisisPanel`). `Typography.gd` is currently a
  no-op stub, so this has no visual effect yet — it preserves intent for when
  typography is implemented.
- Collaborator setters for cross-cutting dependencies, kept narrow: `set_map_view`
  (overlay sync), `set_command_hooks(send: Callable, append_log: Callable)` for
  tabs that issue runtime commands (`CrisisPanel` spawn/auto-seed, `KnowledgePanel`
  policy/budget/mission). The panel never reaches back into the coordinator — it
  holds only the Callables/handles it is given.
- `set_command_connected(connected: bool)` — for tabs whose command controls
  enable/disable on the command socket state (`KnowledgePanel`). The coordinator's
  `_update_command_controls_enabled` delegates the panel's own controls to this.
- `ingest_log_entry(entry: Dictionary)` — for tabs fed by parsed *log messages*
  rather than snapshot keys (`KnowledgePanel` knowledge/espionage/counter-intel
  telemetry). The coordinator's log loop calls it per entry.
- Public feeder methods for cross-panel data flow (`KnowledgePanel.append_events`,
  fed by Trade's diffusion records). The two panels never reference each other —
  `TradePanel` emits `knowledge_events_produced(records)` and the coordinator
  forwards the batch to `KnowledgePanel.append_events` (wired in `_ready`).
- Coordinator-owned state pushed into a display panel: `SentimentPanel.set_axis_bias`
  — axis bias belongs to the Commands axis controls (which mutate it optimistically),
  so the coordinator pushes it to the Sentiment view at both the snapshot and the
  optimistic-write sites, instead of the panel owning the key.
- Command-issuing via a signal when the command needs coordinator-only context (pattern
  reference; the Fauna/Terrain examples were retired with the single-task commands — FaunaPanel
  is now display-only and TerrainPanel's Scout button is gone). `set_log_hook(append_log)` is the
  log-only variant of `set_command_hooks` (`VictoryPanel`'s one-shot victory announcement).

The coordinator collects extracted panels in `_tab_panels` and fans `apply_update`
out to them at the **end** of `_apply_update`, after its own key routing (e.g.
`_ingest_overlays`), so a panel's own keys win over coordinator-side feeders on
conflict (see the `crisis_overlay` vs `overlays.crisis_annotations` precedence note).

**Reference implementations:** `ui/inspector/PowerPanel.gd` (Power — pure
snapshot/render), `ui/inspector/CrisisPanel.gd` (Crisis — command hooks +
typography), `ui/inspector/KnowledgePanel.gd` (Knowledge — the fullest: connection
gating, log-path ingestion, and the Trade→Knowledge event feed), and
`ui/inspector/TradePanel.gd` (Trade — map-overlay collaborator + the emit side of
the Knowledge↔Trade seam). **The decomposition is complete** — every inspector tab is
now its own panel (see the key-scripts table). `Inspector.gd` (≈880 lines, down from
~6,500) is purely the coordinator: streaming fan-out, the command hub + autoplay timer,
capability gating, typography, MapView attach, and the cross-panel seams (faction
resolution for Fauna/Terrain, influencer resonance → Culture, the `overlays` fan-out
junction routing palette→Terrain / annotations→Crisis / channels→Overlay).

**Commands tab (designer/debug console).** The `Commands` tab (axis-bias, heat,
config-reload, autoplay row, influencer/corruption command
buttons, command status/log; the scenario scout/follow rows were removed with the retired
single-task commands) is now `CommandsPanel` (see the key-scripts table). Its
subtree once went missing in the 2025-11-21 scene split (`Main.tscn` → instanced
`InspectorLayer.tscn`) and sat dead for months — the coordinator's
`get_node_or_null("RootPanel/TabContainer/Commands/…")` refs silently resolved to
`null` — before it was transplanted back from git history and extracted onto the
tab-panel contract. The **command hub stays in the coordinator**: `_send_command` →
`command_client`, `_ensure_command_connection`, the `autoplay_timer`, and turn-sending
are shared with the turn controls in `RootPanel/CommandToolbar` (outside the
`TabContainer`) and the Terrain tab's Export Map button. The panel issues
verbs through `set_command_hooks` and is connection-gated via `set_command_connected`.
Autoplay is split: the toggle+interval widgets live in the panel (relayed as
`autoplay_toggled`/`autoplay_interval_changed`), while the timer that steps turns and
the toolbar Play/Pause mirroring stay in the coordinator (which calls back
`set_autoplay_active`). Axis bias is coordinator-owned (Sentiment depends on it): the
panel emits `axis_bias_apply_requested` and the coordinator sends + mirrors it back via
`set_axis_bias`. The influencer dropdown is fed `InfluencerPanel.get_influencers()`
through the coordinator (`set_influencer_roster`).

---

## Overlay Channels

Raster overlays streamed from `core_sim`:

| Channel | Color | Source |
|---------|-------|--------|
| `logistics` | Blue | Throughput flow |
| `sentiment` | Red | Morale/agency composite |
| `corruption` | Amber | Ledger intensity + risk weights |
| `fog` | Slate | Inverted knowledge coverage |
| `culture` | Violet | Divergence magnitude |
| `military` | Green | Readiness scalar |
| `terrain_tags` | Blended | Per-tag colors averaged |

Legend rendering: min/avg/max values + channel description.

---

## Typography & Theming

Shared `Theme` resource reads `INSPECTOR_FONT_SIZE`, applies to root `CanvasLayer`. Typography map: `body`, `heading`, `caption`, `legend`, `control`.

Helper: `Typography.gd` provides offset deltas (heading = base + 4, caption = base − 2).

---

## Scripting Capability Model

QuickJS sandbox for user scripts.

### Capability Families
- `telemetry.subscribe` - Read-only snapshot feeds with back-pressure
- `ui.compose` - Declarative widget graph (Panel, VBox, Table, Chart2D, OverlayLayer)
- `commands.issue` - Vetted command endpoints with throttle windows
- `storage.session` - Scoped key/value cache persisted with saves
- `alerts.emit` - Toast/banner notifications with rate caps

### Script Distribution
- `.sscmod` bundles (zip) with `manifest.json`, Ed25519 signature
- Local install: `mods/inspector/` or UI import
- Workshop feeds: JSON index of signed bundles

### Lifecycle
- Manifest validation on load
- Hot reload via esbuild-lite bundling
- Suspension on sandbox violations (memory/instruction limits)

---

## Hotkeys

| Key | Action |
|-----|--------|
| `W/A/S/D` | Pan map |
| `Q/E` | Zoom |
| Mouse wheel | Zoom at cursor |
| Right/middle drag | Pan |
| `C` | Fit map to view |
| `H` | Toggle hex grid lines |
| `F` | Toggle fog of war |
| `T` | Toggle terrain textures |
| `I` | Hide/show inspector |
| `L` | Collapse/restore legend |
| Double-click herd | Quick-assign the player band's idle workers to hunt it (Sustain) |

---

## See Also

- `README.md` - Setup and running instructions
- `docs/godot_inspector_plan.md` - Inspector migration progress
- `core_sim/CLAUDE.md` - Simulation engine (snapshot contracts, commands)
- `docs/architecture.md` - Cross-system data flow
