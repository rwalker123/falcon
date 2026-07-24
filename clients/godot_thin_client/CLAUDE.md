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

**Sockets** (defaults — see the discovery precedence below):
- Snapshot stream: `127.0.0.1:41002` (FlatBuffers via `SimulationConfig::snapshot_flat_bind`)
- Command socket: `127.0.0.1:41001` (Protobuf `CommandEnvelope`)
- Log stream: `127.0.0.1:41003` (length-prefixed JSON tracing frames)

**Endpoint discovery — env var → ports file → hardcoded default** (`src/scripts/ServerPortsFile.gd`).
The packaged playtest build pins the three ports above, but if they are busy at launch the server binds
a different free block and publishes its choice to a **ports file**; the client reads it so the two
halves still find each other. Every resolver (`Main._determine_stream_*` / `_determine_command_*`,
`LogsPanel._determine_host` / `_determine_port`) applies the same three-step precedence:
1. the explicit env var (`STREAM_HOST`/`STREAM_PORT`/`COMMAND_HOST`/`COMMAND_PORT`/`COMMAND_PROTO_PORT`/
   `LOG_HOST`/`LOG_PORT`) — **the env var always wins**, so `scripts/run_stack.sh`, which exports them
   explicitly, is completely unaffected by this feature;
2. the ports file;
3. the hardcoded constant.

**Ports-file path** — derived from the environment only, so it matches the server's derivation with no
shared library: `SIM_PORTS_FILE` (used verbatim if set), else Windows `%LOCALAPPDATA%\ShadowScale\ports.json`,
macOS `$HOME/Library/Application Support/ShadowScale/ports.json`, Linux/other `$XDG_STATE_HOME/ShadowScale/ports.json`
(falling back to `$HOME/.local/state/…`). It is a **real filesystem path, not `res://`/`user://`** — opened
with `FileAccess.open(abs_path, READ)`. Content:
`{"host":"127.0.0.1","snapshot":41000,"command":41001,"snapshot_flat":41002,"log":41003,"pid":1234}`.

**THE STREAM PORT IS `snapshot_flat`, NOT `snapshot`.** `snapshot` is the legacy JSON snapshot socket;
the client consumes the **FlatBuffers** one. Reading the wrong key yields a client that connects to a
live socket and then **silently never renders** — no error, no frames — which is the easiest thing to
get wrong here and the hardest to diagnose.

The helper is a **static-func script, not an autoload** (it holds no node state, is needed by both
`Main.gd` and `LogsPanel.gd` before the tree settles, and both `preload` it like their other
collaborators; the static cache gives the once-per-launch read without an `[autoload]` entry). It reads
and parses **once per launch and caches the result — including the absent/invalid one**. Missing file,
unreadable file, malformed JSON, missing keys and non-integer/out-of-range ports **all degrade silently
to the defaults**: a playtester running a normally-ported server must never see an error because of this.
(It parses via `JSON.new().parse()` rather than the `JSON.parse_string()` static, which pushes an
engine-level ERROR to the console on malformed input.) Exactly one informational line is logged, and only
when the file is actually used. A **stale file from a crashed server is expected and tolerated** — the
existing connect/retry behaviour handles the refused connection. The client is a **pure reader**: it
never writes, deletes, or liveness-checks the file.

---

## Key Scripts Reference

| Script | Purpose |
|--------|---------|
| `Main.gd` | Scene orchestration, streaming toggle. On boot sends the `new_game <preset> <w> <h> <seed> <profile>` command (built from the `GameLaunch` autoload handoff, or a dev default) since the server now boots idle and only generates a world on `new_game`. Owns the `$PauseLayer` ESC overlay: ESC opens/closes the pause `MenuShell`, but yields to MapView's targeting-cancel when `hud.is_targeting_active()` |
| `ui/MenuShell.gd` (`ui/MenuShell.tscn`) | The ONE shared menu surface (DRY) for BOTH the landing screen and the ESC pause menu; `mode` ("landing"\|"pause") re-filters a single registry-driven nav and re-lays-out (full-bleed vs centered card over a scrim). Built in code, styled through `HudStyle`. New Game pane = preset picker (earthlike / polar_contrast) + map-size picker (from `MapSizes.OPTIONS`, Standard default) + seed field. Functional items emit `new_game_requested`/`resume_requested`/`abandon_requested`/`exit_requested`; Map Selection/Load/Save/Options render inert placeholder panes |
| `ui/LandingScreen.gd` (`ui/LandingScreen.tscn`) | The boot main-scene (`project.godot` run/main_scene): a MenuShell in landing mode over a dark ground. `new_game_requested` stashes params in `GameLaunch.pending_new_game` and swaps to `Main.tscn`; `exit_requested` quits |
| `MapSizes.gd` | Canonical 5-entry map-size list (`OPTIONS` + `DEFAULT_KEY`), shared by `MapPanel` and `MenuShell` (DRY) |
| `GameLaunch.gd` (autoload) | Cross-scene handoff: `pending_new_game` dict set by LandingScreen, consumed + cleared by `Main._build_new_game_command` |
| `MapView.gd` | Terrain rendering, overlays, hex selection (select-then-cycle through a tile's band stack), navigation (WASD/QE/mouse), tile picking, and the coordinator for the **layered hex-marker system** (see Map markers below). Three cohesive subsystems are composed out into owned renderer helpers, each holding a `_view: MapView` back-ref and driven from MapView's `_ready`/`_draw` (all shared geometry/glyph/pill/fog primitives + the marker source arrays + selection state stay on MapView): the **2D minimap** (`ui/MinimapController.gd`, `_minimap`), the **primary band markers** (`ui/BandMarkerRenderer.gd`, `_band_markers`), and the **secondary markers** (`ui/SecondaryMarkerRenderer.gd`, `_secondary_markers`). A FOURTH helper owns the terrain rasters: the **terrain textures + Approach-B blend shader** (`ui/TerrainRenderer.gd`, `_terrain`). Still on MapView on the terrain side: the CPU base pass `_draw_terrain_direct` (the frame's base loop, which branches between the helper's textured hex and MapView's solid `_tile_color` fill) and the whole `_cache_*` SubViewport (it caches the non-shader base render and 9 of its 11 invalidation sites are non-terrain). Still on MapView: the `_draw_*` overlay families NOT yet extracted — the selected-band work-highlights + yield-labels + herd-range, supply links, routes, targeting, trade/crisis annotations (see the Step-4 report for why each was left) |
| `ui/MinimapController.gd` | Owns MapView's 2D minimap: the `MinimapPanel` instance, its terrain/FoW image (rebuilt only on grid/data/FoW change), the viewport-indicator overlay and click-to-pan. Holds a `_view: MapView` back-ref; behaviour is identical to the old inlined minimap code |
| `ui/BandMarkerRenderer.gd` | Owns MapView's PRIMARY player-band markers: the offset card-stack of settlement-stage tokens / expedition flag-discs, the faction nameplate banner (+ its reused StyleBoxFlat), the food-days dot, the travel/task arrow, and the ×N over-cap count pill. `_view: MapView` back-ref; `draw_primary_bands()` called during MapView's `_draw`; pixel-identical to the old inlined code (verified via `map_preview` byte-diff) |
| `ui/SecondaryMarkerRenderer.gd` | Owns MapView's SECONDARY markers (herds / food sites / discovered sites / harvest+scout overlays) + the per-frame edge-slot assignment (`compute_slots`) and `+N` overflow chip. Owns only the per-frame slot maps; all draw commands + shared primitives + marker source arrays stay on MapView via the `_view` back-ref. Pixel-identical to the old inlined code (verified via `map_preview` byte-diff) |
| `ui/TerrainRenderer.gd` | Owns MapView's TERRAIN raster/shader subsystem: the Approach-B per-pixel biome-blend shader (the whole-map `TerrainBlendQuad` child + its `ShaderMaterial` + the six per-hex splatmap textures it is fed — id / vis / elev / river-edge / river-channel / navigable-underlying), the blend-OFF per-hex texture cache, the blend-class + canopy/peak code maps, and the `T`-key texture toggle (MapView keeps thin `get_terrain_textures_enabled` / `enable_terrain_textures` pass-throughs for the Inspector/HUD; `CachedMapRenderer` reads the cache via `hex_texture_for`). **All eight tuning-const families live here** (`EDGE_BLEND_*` / `WATER_BLEND_*` / `SHORE_*` / `CANOPY_*` / `PEAK_*` / `RIVER_*` / `BASE_DEFAULT_TEXTURE_SCALE`); `EDGE_BLEND_MIN_RADIUS` + `FOW_EXPLORED/VISIBLE_THRESHOLD` are `const X = MapView.Y` aliases, so each has exactly one definition. `_view: MapView` back-ref — every draw command plus the shared geometry/colour/visibility/river primitives stay on MapView. Pixel-identical to the old inlined code, **verified by byte-diff**: an extracted run matched a pre-extraction run on all 286 `map_preview` + `blend_probe` frames (the 4 `map_band_*` frames that vary run-to-run do so identically before and after — the `map_preview` window-maximize race, not this change) |
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
| `ui/inspector/MapPanel.gd` | Map tab panel — map-size controls, start-profile (scenario) controls, and the highlight-rivers toggle (now a shader uniform — see Edge Blending → Rivers). Snapshot-driven (in `_tab_panels`): `apply_update` consumes `grid`/`campaign_profiles`/`campaign_label`/`faction_inventory`. Issues `map_size`/`start_profile` via `set_command_hooks`, gated by `set_command_connected`, and drives `MapView.set_highlight_rivers` via `set_map_view`. The nested Map-Overlays section keeps its own `OverlayPanel` script |
| `ui/inspector/CulturePanel.gd` | Culture tab panel — culture layers, divergence list + detail, tension readout; drives `MapView.set_culture_layer_highlight`. Snapshot-driven (in `_tab_panels`): `apply_update` ingests `culture_layers`/`culture_layer_updates`/`culture_layer_removed`/`culture_tensions`, but rendering is driven by the coordinator via `render(resonance)` — the influencer-resonance "pushes" line is coordinator-mediated (`InfluencerPanel.aggregate_resonance()` passed in). `set_map_view` (highlight) + `set_log_hook` (new tensions log to the Logs feed) |
| `ui/inspector/TerrainPanel.gd` | Terrain tab panel — the largest: biome list + drill-down, tile list/detail, the runtime terrain-highlight dropdown, and the **Export Map** button (the tile Scout button was retired with the single-task `scout` command). Snapshot-driven (in `_tab_panels`): `apply_update` ingests `tiles`/`tile_updates`/`tile_removed`/`food_modules` and renders. Owns the inbound MapView hex-selection (`focus_tile_from_map`, coordinator forwards) and drives `set_terrain_highlight` / `relative_height_at` via `set_map_view`. The biome palette + tag labels arrive on the `overlays` key (coordinator routes them in via `set_terrain_palette`/`set_terrain_tag_labels`; `get_terrain_tag_labels()` feeds OverlayPanel). Export sends via `set_command_hooks`, gated by `set_command_connected` |
| `Hud.gd` | HUD layer. The **legend card** (right-dock **TerrainLegendPanel**: `update_overlay_legend` rows `{color,label,value_text}` + the terrain-only **sort header** — `Name`/`Count` toggles with a ▲/▼ arrow, display-only field ∈ {name,count} × per-field direction, default **Count desc**, persisted across map regen) and the **command feed card** are each composed out into a controller (`ui/hud/LegendController.gd` / `ui/hud/CommandFeedController.gd`); Hud holds them as `_legend` / `_command_feed` and delegates `update_overlay_legend`/`toggle_legend`/`_on_legend_sort_pressed` and `ingest_command_events`/`reset_command_feed`/`_note_command_feed`. MapView's `_build_terrain_legend` supplies a numeric `count` per row for the count sort; non-terrain (overlay/tag) legends hide the sort control. Also: the **selection card** — ONE card, ONE list, ONE drawer (`TilePanel`, `docs/plan_tile_panel_layout.md`): a pinned `%TileChips` strip, the selectable `%SubjectList` with the LAND as its first row, and the height-capped `%SubjectScroll`/`%SubjectBody` drawer whichever row is lit fills (land → `%TileDetail` + the `%ForageAssignControls` "assign foragers" stepper; herd → `%OccupantDetail` + the `%HerdAssignControls` "assign hunters" stepper+policy picker; expedition → `%AllocationPanel` Recall/Move). **Player-band detail relocated into the dockable `BandCityPanel`** (summary + `%AllocationPanel`-style labor UI render there via `_render_band_into_panel`; the drawer renders a one-line pointer at it) — see "Band/City dockable panel". Turn readout (the standalone band Alerts panel was folded into the turn-orb attention model — see "Turn orb & attention model"). **The cross-cluster selection + labor state is extracted into two `RefCounted` models (Phase 0, `docs/plan_hud_decomposition.md`), held as `_selection` / `_band_labor`**: `HudSelectionState` owns the selection triplet (`tile_info()`/`unit()`/`herd()`), the lit-row kind `subject()`, the roster, and the sticky-selection guard `choice_tile()`; `HudBandLaborState` owns the snapshot-captured `player_band()` / `player_bands()` (the full player-faction list backing the band-picker + the panel cycler) / `player_expeditions()` / `panel_band()` / `world_herds()`, the grid scalars, the turn, the losing-population diff, the forage-patch / food-module lookups, and the optimistic `pending_labor()` overlay (incl. `effective_worker_map` / `effective_idle`). A THIRD model, `ComposeState` (Phase 2c-1), is held as `_compose` and owns the forage/hunt/party compose state + the open sheet's `kind`/`subject` (the sheet NODE stays on HudLayer). All three live in-file as `HudLayer` members; every former `_selected_*` / `_player_*` / `_pending_*` / `_forage_assign_*` / `_hunt_assign_*` / `_send_party_*` field is now an accessor/mutator call. (`_selected_food_module` / `_selected_food_is_hunt` were DELETED in 2c-1 — 7 writes, zero readers, since Phase 2a/2b took the readers and `SelectionCardController` re-derives `tile_info.get("food_module")` locally.) Roster selection emits `roster_occupant_selected`; labor edits emit `assign_labor_requested` / `move_band_requested` / `cancel_order_requested` (clear-all). **The shared forecast/estimate MATH has left this file** for `ui/hud/SourceForecast.gd` (all-`static`, stateless) — every yield readout, pre-commit forecast, worker cap and raid verdict is now a `SourceForecast.*` call, and the forecast vocabulary constants it owns are re-exported here as a labelled block of `const X = SourceForecast.X` aliases. What STAYS on `HudLayer` is the node-building side: the widget factories (`_build_policy_picker` / `_forecast_label` / `_gate_reasons`), the drawer-only compose helpers (`_forage_policy_gates` / `_hunt_policy_gates` / `_tame_stalled_hint` / `_forecast_worker_cap` / `_forecast_yield_row` / the `_flora_entry_*` sub-layer / the two local preview lines …), and the one-line `_hex_distance_wrapped` pass-through that supplies the grid pair |
| `ui/hud/HudSelectionState.gd` | `RefCounted` state model (HUD decomposition Phase 0) — "what the player is looking at": the selection triplet (`tile_info`/`unit`/`herd`), the lit-row `subject` (`SUBJECT_LAND`/`UNIT`/`HERD`, aliased back onto `HudLayer`), the assembled roster, and the sticky-selection `choice_tile`. Encapsulated mutators (`select_tile`/`set_tile_info`/`select_unit`/`select_herd`/`select_land`/`set_subject`/`set_roster`/`note_choice_tile`/`clear`) each emit `changed(reason)` — emitted in Phase 0, consumed by nothing yet (Phase 2). Pure DATA, no node refs; accessors return by reference (matches the HUD's in-place-mutation) |
| `ui/hud/HudBandLaborState.gd` | `RefCounted` state model (HUD decomposition Phase 0) — "the digested per-snapshot player world + optimistic overlay": `player_band`/`player_bands`/`panel_band`/`player_expeditions`, `world_herds`, grid scalars, `current_turn`, the `prev_band_sizes` losing-population diff, the `forage_patch_lookup`/`food_module_by_tile` lookups, and the `pending_labor` optimistic overlay. Ingest mutators (`set_turn`/`set_grid`/`set_world_herds`/`set_panel_band`/`ingest_snapshot_bands`/`set_food_modules`/`set_forage_patches`) + the pending API (`record_pending_assign`/`record_pending_move`/`reconcile_pending`/`pending_assigns_for`/`pending_key`) + the moved-on derived readers `effective_worker_map`/`effective_idle`/`effective_forage_workers`/`effective_hunt_workers` (pure functions of `pending_labor` + a band) + the static `as_schedule`. Also owns the **thin band-labor readers** every consumer reaches through `_band_labor.` — the roster pair `current_player_bands`/`player_band_by_entity`, the per-source lookups `forage_assignment_of`/`hunt_assignment_of` and their `workers_for_forage`/`workers_for_hunt`/`policy_for_hunt`/`policy_for_forage`/`assignable_forage_workers`/`assignable_hunt_workers` — plus the canonical policy-rung consts `HUNT_POLICY_OPTIONS`/`FORAGE_POLICY_OPTIONS`/`DEFAULT_HUNT_POLICY` (the last aliases `SourceForecast`'s; `HudLayer` re-exports all three via `const X = HudBandLaborState.X`). Emits `changed(reason)`, consumed by nothing yet |
| `ui/hud/ComposeState.gd` | `RefCounted` state model (HUD decomposition Phase 2c-1) — "what the player is dialing but has not committed": the tile card's **forage** compose (`forage_key`/`count`/`policy`/`species`/`band` + its autofill one-shot), the herd drawer's **hunt** compose (`hunt_key`/`count`/`policy`/`band` + its own one-shot), the Band panel's PARTIES-zone **party** compose (`party_quarry_id` + its one-shot) on its own clearly-separated accessor group so a later band-panel extraction can take it without unpicking the drawer's, and the open sheet's subject identity (`kind`/`subject`; `COMPOSE_KIND_*` alias to its `KIND_*`). Mutators are named for the transition — `begin_*_source` + `seed_*` (the two-step source-changed re-seed: the caller must resolve the actual band between them), `set_*`, `arm_*_autofill`/`consume_*_autofill`, `reset_*_source` (the harnesses' way to stage a fresh compose), `set_composing`/`clear_composing` — and the three READ-MODIFY-WRITEs get explicit ones so the field is never read and written apart: **`clamp_forage_count`/`clamp_hunt_count`** and **`resolve_forage_species(resolver: Callable)`** (the RMW is the model's; the crop RULES stay with the caller, so it holds no flora knowledge). Pure DATA — which is exactly why **`_compose_sheet` (a scene node) stays on `HudLayer`**. Deliberately **NO `changed` signal**, unlike the Phase-0 pair: nothing subscribes (the compose builders re-render explicitly) and unused API is a liability. **`hunt_policy()` is PUBLIC beyond its builder, but its readers are all HERD-DRAWER ones now** (`_tame_stalled_hint` / `_herd_crew_noun`): `_build_policy_picker`'s `selected` fallback — the one real cross-boundary read, where a work-inspector or party-compose render picked up the DRAWER's rung — was DEAD (every caller passed an explicit, provably non-empty `selected`) and is **deleted**; `selected` is a REQUIRED param, so the shared builder now owns none of its callers' state and the drawer/band-panel boundary is structural rather than conventional |
| `ui/hud/LegendController.gd` | Owns the right-dock legend card: row rendering, the terrain-only Name/Count sort header + its display-only sort state, the suppress toggle, and internal-scroll sizing. `HudLayer.LEGEND_SORT_FIELD_*` alias to its `SORT_FIELD_*` consts. Behaviour identical to the old inlined legend code |
| `ui/hud/CommandFeedController.gd` | Owns the left-dock command feed card: the rolling entry list (`COMMAND_FEED_LIMIT` 6), signature de-duplication (`ingest_events`), client `note()`s, and — via the shared `DockScrollFit` — the internal-scroll sizing, now **capped at `COMMAND_FEED_MAX_DOCK_FRACTION` (0.4) of the dock** so it can never crowd out the selection card it shares the column with, and owning its own `feed_suppressed` (hidden by default, `R`) because `render()` runs on every ingest and would otherwise re-show a card the player closed. It is a **command log again**: `ingest_events` **SKIPS** every kind `TellingPanel.handles_kind()` claims (the narrative ones), so the PR-B `KIND_STYLE` prose branch is gone and every remaining kind takes the original `Turn N` + bold-label / italic-detail receipt shape. The feed **always snaps to newest** (a receipt is worthless once read) — read-position preservation is the Telling panel's concern, not this one's |
| `ui/hud/TopBarReadouts.gd` | `RefCounted` controller (HUD decomposition Phase 1a, `docs/plan_hud_decomposition.md`) owning the **top-bar faction readouts** cluster — the Sedentarization meter, the demographics line, the discovered-Wondrous-Sites strip, the intensification-ladder knowledge strip, and the left-dock stockpile panel. Hud holds it as `_topbar` (constructed after `_command_feed`, so the knowledge-unlock nudge routes straight through it) and keeps thin reflective delegators (`update_sedentarization`/`update_demographics`/`update_discoveries`/`update_intensification`/`update_stockpiles`, reached by `Main._hud_invoke`). `update_overlay` stays a HudLayer **fan-out**: `_topbar.render_overlay(turn, metrics)` for the top-bar labels, then `_band_labor.set_turn` + `_turnorb.set_turn`. Holds PURE data + label nodes only; owns `_intensification_knowledge` / `_knowledge_announced` / `_stockpile_totals` (moved off HudLayer). Its `faction_knowledge()` is PUBLIC because HudLayer's policy-gate reasons (`_forage_policy_gates` / `_hunt_policy_gates`) read this cluster's knowledge back. Three helpers shared beyond the cluster stay on HudLayer and are passed in as Callables (`_meter_bar` — the herd-drawer danger bars use it — `_format_stockpile_label`, `_progress_percent`); `_set_label_tooltip` is a trivial 2-line private copy. Behaviour identical to the old inlined top-bar code |
| `ui/hud/TurnOrbController.gd` | `RefCounted` controller (HUD decomposition Phase 1b, `docs/plan_hud_decomposition.md`) owning the **turn-orb / attention / fork** cluster — the orb wiring, the narrative-fork panel (The Telling), and the attention-registry ASSEMBLY (`_push_attention` folds the band/expedition half + the snapshot-driven `_pending_fork_attention` fork producer into ONE `TurnOrb.set_attention`; the fork row `blocking`s the orb's `Advance ▸` — the client-side end-turn gate). Hud holds it as `_turnorb`, constructed in `_ready` AFTER `_telling` + `_command_feed` (it needs both), handed `turn_orb` + the HUD CanvasLayer (`self`, the host it `add_child`s the fork panel into, since a RefCounted can't). Owns `_pending_forks` / `_stance_axes` / `_band_attention` / `_auto_opened_forks` / `_fork_panel` (all moved off HudLayer). Keeps thin reflective delegators for the five methods Main reaches by reflection (`update_pending_forks`/`update_stance_axes`/`update_voice_medium`/`has_pending_fork`/`note_unanswered_fork`). **Emits its OWN signals; HudLayer RELAYS each** (the controller never emits a HudLayer signal): `answer_fork_requested` → `HudLayer.answer_fork_requested`, `advance_requested` → `next_turn_requested.emit(1)` (after `_telling.reveal_newest()`), `focus_requested(x,y)` → `HudLayer._on_turn_orb_focus`. **Two seams to band/labor stay on HudLayer (Phase 3):** `update_band_alerts` feeds the band half via `set_band_attention(attention)` (the write half of `_band_attention`; the two internal callers push directly), and the orb's "Jump →" band routing (`_on_turn_orb_focus` → `_awaiting_expedition_at` / `_starving_pen_at` / `_focus_labor_source`) stays on HudLayer, reached through the relayed `focus_requested`. `set_turn` forwards to the orb so `update_overlay`'s fan-out no longer touches the node directly. `note_unanswered_fork` routes straight through `_command_feed.note`. Behaviour identical to the old inlined turn-orb code |
| `ui/hud/SelectionCardController.gd` | `RefCounted` controller (HUD decomposition Phase 2b, `docs/plan_hud_decomposition.md`) owning the selection card's **IDENTITY/LIST half** — the tile-card header, the pinned condition-**chip** strip, and the whole **roster / subject list** (LAND row + Bands/Wildlife sub-groups), plus the roster-row clicks and the fresh-hex auto-select. The state-isolated half (zero drawer coupling, zero shared compose/band-tint state), so it split off ahead of the drawer (Phase 2c keeps the whole drawer + compose on HudLayer). Hud holds it as `_selectioncard`, constructed in `_ready` after `_turnorb`, handed the three card nodes (`tile_panel`/`tile_chips`/`subject_list`) + the SAME `_selection`/`_band_labor` model instances (BY REFERENCE) + ONE Callable for a HudLayer method that stays (`_alloc_hint_label`); it reads the forage/hunt worker counts straight off `_band_labor.workers_for_forage`/`workers_for_hunt` (those readers moved onto the labor model), and `_is_player_unit` is a trivial private copy. Owns the moved Phase-2a diff caches `_tile_chip_slots` / `_subject_row_keys`, so the in-place chip/row updaters travel with it and a same-tile restate still patches nodes rather than tearing them down (`tile_panel_no_flash`). **The render seam:** `HudLayer._render_selection_panel` stays the ORCHESTRATOR — it resets the band-tint scalars, calls `_selectioncard.render(tile_info)` (roster + tile-card + chips + auto-select + list), then its own `_render_subject_drawer()`. **The row-click seam:** a roster/land click mutates `_selection` and emits the controller's OWN `subject_changed` → `HudLayer._on_selection_subject_changed` (close compose sheet → re-render), plus `roster_occupant_selected(kind, id)` **RELAYED** onto `HudLayer.roster_occupant_selected` → Main (the TurnOrbController pattern); the auto-pick emits only the relayed `roster_occupant_selected` (it runs mid-render). Publics HudLayer's band/labor navigation calls back into: `render`, `select_roster_occupant`, `find_roster_unit`/`find_roster_herd`, `tile_contents_unseen`. Behaviour identical to the old inlined selection-card code |
| `ui/hud/DockScrollFit.gd` | Shared sizing for a dock card whose content grows without bound (**the command feed** and **the selection card's subject drawer**, which calls the split-out `fit_height(scroll, content_height, …)` because its body is a VBox of live controls rather than a RichTextLabel — `fit(scroll, label, …)` is now written in terms of it, so the height math exists once — the Telling panel grows to fit its own bounded page capped at `PAGE_MAX_HEIGHT` now and no longer uses this): grow to fit the label, capped by the room left in the dock's `ScrollContainer` beneath it, so the card scrolls INTERNALLY rather than dragging the fixed panels through the dock scroll. **"Room left" excludes what the cards BELOW it need** (`_height_reserved_below`, summed over *visible* following siblings): a growing card used to always be the LAST in its dock so the distinction never arose, but the command feed sits above the fixed cards in its dock, and claiming everything beneath it pushed them clean out of the visible dock. Only visible siblings count, so the both-hidden default is unchanged and toggling one on simply hands the room back. **Deliberately not `AutoSizingPanel`** — that one sizes a FREE-FLOATING control against the viewport (`global_position` + anchors + `offset_bottom`, as NarrativeForkPanel and the Inspector do), and a card inside the dock's VBoxContainer has neither: the container overwrites its size every layout pass and the ceiling that matters is the DOCK's remaining height. `PanelCard` + this helper is the container-side equivalent |
| `ui/hud/ComposeSheet.gd` | The selection card's **write state** — the floating **compose sheet** (`docs/plan_tile_panel_layout.md` §10-§15). Composing is MODAL BY NATURE (open, decide, commit, done), so the two ~270px compose blocks (`%ForageAssignControls` / `%HerdAssignControls`) left the drawer for a sheet that borrows space only while in use; the drawer keeps the detail rows, a one-line standing summary and an `Assign … ▸` button. **That button wears `primary` while ITS sheet is open and `ghost` at rest — never `armed`**: `armed` is the destructive/warned treatment (DANGER border), and "its sheet is open" is a LIVE state, which this HUD spells in SIGNAL cyan (the Sight chip, the selection accent, the turn orb's calm pulse). **Its card is an `AutoSizingPanel`, NOT a `DockScrollFit` card** — it floats against the VIEWPORT, which is the opposite of what the drawer above needs, and picking wrong misbehaves silently rather than failing (root CLAUDE.md → UI Panel Sizing). **The node IS the full-screen dismiss catcher with the card as its CHILD**, reusing `NarrativeForkPanel`'s nesting exactly (siblings make the ordering ambiguous and the catcher eats the card's own clicks), pinned to the viewport EXPLICITLY via `_sync_to_viewport` — a hidden Control's anchors never settle, and the full-rect preset would also overwrite the size. **NO SCRIM, and that is the one deliberate departure from the fork panel:** a fork is a story beat demanding attention, an assignment is composed *against* the map (work-range ring, herd position, hunt reach are all live context), so the catcher dismisses without dimming. **And that is also why the catcher dismisses on a real CLICK only, never a wheel tick** (`DISMISS_BUTTONS`, an ALLOWLIST of left/right/middle so a future Godot wheel/extra index stays non-dismissing by default): the catcher is `MOUSE_FILTER_STOP` across the whole viewport, so an idle scroll over the un-scrimmed map lands on it, and dismissing there would throw away the composition mid-read. `NarrativeForkPanel` is deliberately left as-is — a modal scrimmed story beat has no such gesture — so the two diverge here on purpose; do NOT factor out a shared predicate for one differing call site. (**Not** a map-zoom passthrough: the catcher stops the wheel either way, so the map cannot zoom while a sheet is open, and a wheel over the card is absorbed by its own `ScrollContainer`.) Guarded by ui_preview's paired wheel-leaves-OPEN / left-click-CLOSES assertions. The sheet floats BESIDE the selection card (`_place_card`, falling back to the viewport margin) so the list + summary it is editing stay readable. It knows nothing about foraging or hunting: `open(eyebrow, title, subject_key, anchor)` returns the content VBox and the caller fills it. `subject_key` is what lets a per-snapshot refresh tell "the same source, restated" from "a different source, gone" |
| `ui/hud/SourceForecast.gd` | **All-`static`, stateless** shared forecast/estimate layer (HUD decomposition, phase 2c-2 precursor) — the pure "what will this source give me?" math THREE consumers ask for: the drawer's compose blocks, the Band panel's WORK zone, and its PARTIES zone. Three families: POST-HOC `source_yield_readout` (what a worked source actually produced, incl. the ⚠ overdraw + overstaff/wasted notes) · PRE-COMMIT `forecast_inputs` / `max_useful_workers` / `expected_yield` / `hunt_policy_ceiling` · THE RAID `hunt_trip_forecast` → `hunt_forecast_line_bbcode` / `hunt_trip_no_surplus` / `hunt_no_surplus_reason` / `expedition_useful_cap` / `expedition_policy_takes` / `style_send_hunt_button` (`style_send_hunt_button` styles a Button off the raid verdict, so it lives WITH the verdict). Plus the shared leaves those need — `format_magnitude`/`format_signed`/`format_yield`/`extractive_take`, `band_tile`/`hex_distance_wrapped`, `herd_display_name`, `is_managed_hunt_source`, and the two one-off leaks into the read-only detail layer, `flora_basket_entries` / `husbandry_ceiling`. **WHY ITS OWN FILE:** the next phase lifts a `DrawerComposeController` out of `Hud.gd`, but this layer is called by the work + parties zones too, so it cannot travel with the drawer; pure injection was measured at **54 Callables** and a `_hud` back-ref would weld an already-pure layer to the god object (and the band-panel extraction would then need a SECOND back-ref to the same place). All three consumers depend on THIS instead. **STATELESS IS THE INVARIANT** — no node, no `_hud`, no snapshot cache; if a new function needs HUD state, pass it in. The one non-plain-value is the grid-wrap pair (`grid_width`, `wrap_horizontal`), threaded as EXPLICIT PARAMETERS through `hex_distance_wrapped` → `round_trip_travel_turns` → `hunt_trip_forecast` / `expedition_policy_takes` so a stale grid can never be captured; `HudLayer._hex_distance_wrapped` is a one-line pass-through supplying its own members, so there is ONE hex implementation. The **forecast vocabulary constants moved here with the math** (`LABOR_KIND_*` / `LABOR_HUNT_POLICIES` / `DEFAULT_HUNT_POLICY` / `SOURCE_KIND_*` / `FORECAST_*` / `MAX_USEFUL_*` / `HUNT_FORECAST_*` / `SEND_HUNT_*` / `HUSBANDRY_CEILING_*` …) and `HudLayer` **re-exports the still-used ones as aliases** (`const X = SourceForecast.X`, one commented block) rather than redefining them — ONE definition, and every HudLayer call site reads unchanged |
| `ui/BandCityPanel.gd` / `.tscn` | The dockable **Band/City command center** CanvasLayer — persistent whenever ≥1 player band exists, dockable to any of the 4 edges (default left, persisted to `user://band_city_dock.cfg`) + collapse-to-rail. Header (stage glyph/name/label + `◀ n/N ▶` cycler + 2×2 dock chooser + collapse), body hosts **THREE NAMED ZONES AT A FIXED CROSS-AXIS SIZE** via **`set_zones(band, work, parties)`** (keys `&"band"`/`&"work"`/`&"parties"`; the panel OWNS and frees them). Two shells, chosen by the panel's own **WIDTH** (`WIDE_SHELL_MIN_WIDTH` — never a dock-edge test, so a resizable dock needs no special case). **That threshold is DERIVED, never hand-picked**: `ZONE_BAND_WIDTH + ZONE_PARTY_WIDTH + ZONE_WORK_MIN_WIDTH + WIDE_SEPARATOR_SPAN + PANEL_CHROME_H` = 300 + 300 + 380 + 50 + 26 = **1056**. `ZONE_WORK_MIN_WIDTH` (380) MIRRORS Hud's `WORK_COLUMN_MIN_WIDTH` — one readable board column — exactly as `ZONE_WORK_MAX_WIDTH` (1520) mirrors `WORK_COLUMN_MIN_WIDTH × WORK_MAX_COLUMNS`; the two are a PAIR with Hud's column consts and move with them. The chrome term is load-bearing because the threshold is tested against the panel's OUTER `_panel_extent().x` while the zones live in `_interior_size()`. It shipped hand-picked at **900**, which broke the whole 900–1055 band: the work zone came out 224px, Hud clamped to one column, its labels clipped — and the NARROW shell would have given the board the full 874px, so flipping wide early made it ~4× narrower, degrading the thing the wide shell exists to improve. `WIDE_SEPARATOR_SPAN` / `PANEL_CHROME_H` are `const`s (a `const` cannot call `_wide_separator_span()`), shared by the threshold, `_wide_content_cap()`, `_wide_separator_span()` and `_interior_size()` so they cannot drift. **wide** (in practice T/B) = the three zones side by side, band/parties fixed `ZONE_BAND_WIDTH`/`ZONE_PARTY_WIDTH` (300), work EXPAND_FILL, `LINE_SOFT` hairlines between, no tab bar; **narrow** (in practice L/R) = a Band·Work·Parties tab bar under the header + exactly one zone beneath it (active tab = SIGNAL ink + a 2px SIGNAL underline, badges via `set_tab_badge(zone, text, hot)`, selection persisted as `CONFIG_KEY_TAB`). **The cross-axis size is FIXED** — `PANEL_WIDTH` 380 (L/R) / `PANEL_HEIGHT_WIDE` 360 clamped to `MAX_WIDE_HEIGHT_FRACTION` of the window (T/B) — so `current_reservation_size()` changes ONLY on dock/collapse/hide/viewport-resize and a content edit can no longer re-emit `reservation_changed` → `MapView.set_reserved_inset` → cache invalidation (the map flicker on every `+` press). **There is deliberately no `ScrollContainer` anywhere in the panel** (no-scroll by design; the work zone pages itself against **`work_zone_size()`**, the zone's interior after chrome — e.g. 354×1107 in a 380 L dock, 1244×298 in a 1920 bottom dock — and re-pages on the **`zones_resized`** signal). **Zone hosts are plain `Control`s, not containers**, so an over-wide zone content cannot push the card past its fixed cross-axis size; `clip_contents` keeps overflow inside its own zone. Reserves its edge via `reservation_changed(edge, size)` → `Main._apply_reservation(&"band_panel", …)`. See "Band/City dockable panel" + `docs/plan_band_city_dock.md` |
| `ui/BandFoodStatus.gd` | Single source of truth for band food-supply thresholds (`band_status_config.json`) + the days→green/amber/red color / BBCode-hex mapping (plus the parallel morale warn/critical thresholds + `color_for_morale`/`hex_for_morale`), shared by MapView's band dot and Hud's food/morale lines + alerts |
| `ui/PenStatus.gd` | Single source of truth for **"is this pen's herd starving?"** — `FULLY_FED` / `FED_EPSILON` + `fed_fraction(herd)` / `is_starving(fed)`, reading `HerdTelemetryState.penFedFraction` (`< 1` ⇒ the keeper underpaid the pen's feed, so the herd is SHRINKING every turn). Plus `herd_is_starving(herd)` for a caller holding only the herd dict. The ONE test all three surfaces ask — the herd drawer (`Hud._corral_label` + the Pen feed row), the map's distress badge (`MapView._draw_herd`) and the turn orb's `starving_pen` producer — so they can never disagree about which pen is dying |
| `ui/TileHabitability.gd` | Single source of truth for the Tile-card Habitability rating: buckets `TileState.habitability` (band-independent per-turn morale drain) into Hospitable/Fair/Harsh/Hostile via `tile_habitability_config.json` thresholds, with the HEALTHY/INK/WARN/DANGER color / `hex_for_rating` mapping. Consumed by `Hud._tile_terrain_lines` + `_format_detail_bbcode` |
| `ui/TileClimate.gd` | Single source of truth for the Tile-card Climate LABELS + classification: maps `TileState.temperature` (°) into **Polar/Boreal/Temperate/Tropical** using the SIM's PUBLISHED cut points (`MapSection.climateBands`, adopted via `set_cut_points` from MapView's overlay ingest — the client no longer keeps its own `cool_min`, retired with the Climate Authority arc so the shown climate can't disagree with the sim's biome). Mirrors `climate::climate_band_for_temperature` exactly (inclusive upper bounds). `has_bands()` gates the row — until the sim publishes, the Climate row is skipped (no invented threshold). INFORMATIONAL only — neutral ink, no HEALTHY/WARN/DANGER tint. Consumed by `Hud._tile_terrain_lines` |
| `ui/RiverEdges.gd` | Single source of truth for the TEXT reading of hex-EDGE rivers: owns the class vocabulary (Minor/Major), the 6 direction names, and the mask bit-widths as named constants, and formats `TileState.riverEdges` into `Major River: NE, NW` / `Minor River: SW` rows (`summary_lines`, Major first, directions in compass order from NE). Consumed by BOTH `Hud._tile_terrain_lines` (Tile card) and `Hud.show_tooltip` (map hover) — one formatter, two surfaces. See Edge Blending → Rivers |
| `SnapshotStream.gd` | Consumes length-prefixed FlatBuffers snapshots |
| `CommandBridge.gd` | Issues Protobuf commands to server |
| `ui/MinimapPanel.gd` | Minimap component for the 2D map view (click-to-pan, aspect ratio sizing) |
| `ui/TurnOrb.gd` / `ui/TurnOrb.tscn` | The bottom-right **turn orb** (replaces the old "Advance Turn" button): calm cyan pulse when the attention registry is empty, else a severity-tinted count badge + a reasons popover (see "Turn orb & attention model"). Re-emits `focus_requested` (jump) / `advance_requested` so Main's advance/jump wiring is unchanged; palette from `HudStyle`, all geometry/severity/kind as named constants ; the attention contract also carries an optional **`blocking: bool`** (default false) — the **end-turn GATE**: while any entry sets it the popover's `Advance ▸` is `disabled` and wears the reason. A **non-locating** row (`x < 0`) now emits **`panel_requested(kind)`** instead of a jump, so the orb never learns what a fork is |
| `ui/MagnifierButton.gd` | Zoom-rail in/out button that `_draw`s a crisp magnifier icon (lens + handle + inner `+`/`−`, `zoom_sign` picks which) — font magnifier glyphs render as tofu/blobs. Monochrome `HudStyle` ink → `SIGNAL` on hover |
| `ui/AutoSizingPanel.gd` | Shared helper for panels that expand to fit content |
| `ui/HudStyle.gd` | Single source of truth for the dark HUD console look: palette (cyan `SIGNAL`, amber `WARN`, ink/line neutrals), `card_stylebox()`, `header_stylebox()`, `banner_stylebox()`, `apply_button(btn, "primary"/"ghost"/"armed")`, `chip_stylebox(border)` (the selection card's pinned condition pills), `hairline_stylebox()` (a standalone 1px LINE_SOFT rule inside a card — the list ↔ drawer boundary; the caller owns the thickness), and `apply_link_button(btn, base_color)` — the **inline link** treatment for a clickable label inside a row (no box at rest; hover tint + cyan text + pointing hand), used by the band panel's clickable Current-actions rows. Every HUD surface styles through here |
| `ui/FoodIcons.gd` | Shared glyph vocabulary — food modules (`for_site`, which takes an optional tile `terrain_id`: **`riverine_delta` splits fish 🐟 ↔ reeds 🎋** — dry floodplain LAND (`alluvial_plain`/`floodplain`) reads as reeds via `RIVERINE_REED_ICON`, open `navigable_river` keeps 🐟; MapView stamps each food site's `terrain_id` so the map marker + HUD Forage row resolve the same glyph — the resolution itself is factored into the public **`site_key_for(module_key, is_hunt, terrain_id)`**, which returns a stable ART KEY (`"hunt"` / `"reeds"` / a module key verbatim / `"default"`, the three non-module keys deliberately disjoint from `ICONS`) so `SiteSprites` resolves the same site without a second copy of the fish↔reeds branch; `for_site` is written in terms of it, so there is exactly ONE implementation — the twin of `species_key_for` on the herd side), fauna herds (`for_herd`, species keyword matched in the herd label, longest-first — the matching itself is factored into the public **`species_key_for(label)`**, which returns the matched HERD_SPECIES key (`""` when none) so `FaunaSprites` can resolve the same species without a second copy of the matcher; `for_herd` is written in terms of it, so there is exactly ONE implementation), and **take policies** (`for_policy`, `POLICY_ICONS`: the four extractive rungs sustain ♻ / surplus ⬆ / market ⇄ / eradicate 💀, plus the **four investment** rungs of the Intensification Ladder — cultivate 🌱 / sow ▦ / tame ◎ / corral 🐄. Each verb wears the glyph of **the rung it builds** (🌱 the crop, ▦ the plotted Field, ◎ the pastoral herd that now keeps near your camp — the rung's defining effect is proximity — 🐄 the penned livestock; 🐄 is also the herd drawer's Domesticated/Corralled badge, and ▦ the tile card's `▦ Field` badge). Verified legible at picker size in `forage_cultivate.png` / `forage_sow.png` / `two_meter_split.png` / `herd_corral.png`; `""` for unknown). Used by the map's food-site / herd markers (`MapView._draw_food_site` / `_draw_herd`), the Harvest/Hunt button + the **band panel's Current-actions rows** (each row leads with its resource glyph), and — for policies — BOTH the Hud policy-picker buttons (`_build_policy_picker`) and the map's yield labels (`MapView._draw_yield_label` appends the icon: `+0.38 ♻`), so a resource/policy always reads the same on the panel and on the map. **Policy glyphs are deliberately TEXT-PRESENTATION symbols** (♻ ⬆ ⇄ ▦ ◎) plus the high-contrast 💀: pictographic emoji (🪙 coin, 💰 money bag) render as a featureless grey blob at the ~12–13px these are drawn at, and ⚖ renders tiny/faint — same glyph-legibility hazard that forced `MagnifierButton` to hand-draw. Verified in `band_panel_left.png` / `map_band_work.png`. **The mechanism is sharper than "prefer line art", and it decides the choice:** a text-presentation glyph **inherits the label's font colour**, so it renders at the button's full contrast and greys out *with* the button when a rung is disabled; an **emoji carries its own colours and cannot be tinted**, so it renders at whatever contrast its art happens to have and stays stubbornly coloured while disabled. 🐾 was tried for `tame` and rejected on exactly that — at picker size it came out a faint washed-out tan against the dark console, the weakest glyph in a row next to a crisp white 💀 (see the first cut of `two_meter_split.png`) — and ◎ replaced it. Prefer a text-presentation symbol for any NEW policy glyph; the surviving emoji (💀 🌱 🐄) are grandfathered and legible. Also the **action-status** glyphs (`for_status`, `STATUS_ICONS`) the Band panel's Current-actions + Active-expeditions rows use instead of words — `pending ○` (the ORDER isn't acknowledged yet; a modifier that rides on any row, amber) / `working ●` (a confirmed local forage/hunt row, and expedition phase `hunting`) / `outbound ➤` / `awaiting ▮▮` / `delivering ◄` = `returning ◄` (both are "coming home"; the tooltip distinguishes them). Same line-art rule and the same hazard: `◌` (dotted circle) was tried for `pending` and rejected — it renders thin and faint at row size — and `⏸` for `awaiting` carries emoji presentation (tofu/blob), so `▮▮` is used. Verified at true size in `band_panel_status_glyphs.png` |
| `ui/FaunaSprites.gd` | Bundled PNG art for map HERD markers — the sprite half of `FoodIcons`' herd vocabulary, and the reason a rabbit no longer renders white on macOS and pink on Windows: the emoji path draws through `ThemeDB.fallback_font`, so the OS emoji font owned the look. Static-only (same reasoning as `ServerPortsFile.gd`): `SPRITE_PATHS` maps a species KEY (a `FoodIcons.HERD_SPECIES` key, resolved via `FoodIcons.species_key_for` — **never a second matcher**) to a file in `assets/icons/fauna/`, aliasing shared art exactly as HERD_SPECIES aliases emoji (deer/reindeer/caribou/elk → `deer.png`). `for_herd(label) -> Texture2D` returns the cached texture or **`null` when this species has no art yet**, which is the fallback contract: `SecondaryMarkerRenderer.draw_herd` resolves the sprite first and calls `MapView._draw_marker_sprite`, else falls through to the unchanged emoji `_draw_marker_glyph`. **Coverage is COMPLETE** — all 22 HERD_SPECIES keys map to one of 12 PNGs (aliases share art: bison/buffalo → `aurochs.png`, oxen → `cattle.png`, ibex → `goat.png`, reindeer/caribou/elk/gazelle → `deer.png`, grouse → `fowl.png`, **hare → `rabbit.png`**; `seal.png` closed the last gap, and `catfish.png` is the wet-biome roster pass's own art — deliberately unlike `sites/fish.png`, which can share a delta hex with it), so no herd species in the game draws an OS emoji. Adding a species is still: drop the PNG in, add the key here. **The `null` fallback stays load-bearing even at full coverage** — it catches a herd label naming a species the client does not know (`species_key_for` → `""`) and the `HERD_DEFAULT` case, both of which still render emoji. Because every shipped species has art, **the emoji path is no longer exercised by map_preview fixtures at all**; only a fixture herd labelled with an unknown/unmapped species guards it. Loaded with `load()` (not `preload()`) so a missing file degrades to the emoji rather than breaking scene load, with one warning per missing path. **The sprite is drawn UNTINTED**, like the emoji — a starving pen still reads as the distress ring + badge GEOMETRY drawn under/over the marker, never a modulate. **Import options are load-bearing**: the sources are 256px but `MapView.texture_filter` is pinned `TEXTURE_FILTER_NEAREST` (to keep the terrain-cache blit seam-free), so the `.import` files set `process/size_limit=64` to cut a 7:1 nearest minification down to ~1.8:1; `mipmaps/generate=true` is set too but is INERT under NEAREST — it only starts paying if that filter is ever raised to linear-with-mipmaps. Judge any art change at TRUE marker size (10–41px), not in a fitted preview frame, which renders them ~2.5× too big |
| `ui/SiteSprites.gd` | Bundled PNG art for map FOOD-SITE markers — the sprite half of `FoodIcons`' site vocabulary, and the food-module twin of `FaunaSprites` (same reasoning: the emoji path draws through `ThemeDB.fallback_font`, so the OS emoji font owned what a shellfish bed or a nut grove looked like). `SPRITE_PATHS` maps a site ART KEY — resolved via **`FoodIcons.site_key_for`, never a second copy of the fish↔reeds branch** — to a file in `assets/icons/sites/`; `for_site(module_key, is_hunt, terrain_id) -> Texture2D` takes the SAME arguments as `FoodIcons.for_site`, so the sprite and the emoji can never disagree about which site this is. **Coverage is COMPLETE** — all 10 `ICONS` modules plus the three non-module keys map to bundled art (12 PNGs, with **`hunt` reusing the fauna `deer.png`**: a hunted site IS game, and a second copy under `sites/` would be one more thing to keep in sync), so no food site in the game draws an OS emoji and — exactly as on the fauna side — **no map_preview fixture exercises the emoji path any more**. The `null` fallback stays load-bearing: it catches an art key with no art (a new food module added to `ICONS` without a PNG), which still renders the emoji. `SecondaryMarkerRenderer.draw_food_site` resolves the sprite first and calls `MapView._draw_marker_sprite`, else falls through to the unchanged `_draw_marker_glyph`. **Same import options as fauna** (`process/size_limit=64`, `mipmaps/generate=true` — inert under the pinned `TEXTURE_FILTER_NEAREST`, see the FaunaSprites row) and the same judging rule: at true marker size. The **reeds are the busiest icon in the set** — at ~36px the individual blades merge into a mass, though the vertical tuft + brown cattail heads stay unmistakable and unique; it is the first one to re-check on any sizing change. Verify the whole set on `map_preview`'s **`map_site_sprites`** (the SPRITE ROSTER: one site per art key in one row, incl. the hunted-site deer and an unknown module's `default` sprig) + **`map_riverine_split`** (the decisive frame: ONE module, `riverine_delta`, drawing the FISH on open navigable river and the REEDS on dry alluvial plain — the branch `site_key_for` exists for) |
| `ui/WonderSprites.gd` | Bundled PNG art for map **DISCOVERED-SITE (Wondrous Site)** markers — the third art family behind `IconSprites`, after `FaunaSprites` and `SiteSprites` (same reasoning: the emoji path draws through `ThemeDB.fallback_font`, so the OS emoji font owned what a Great Peak looked like, and ⛰/⛲ blob at marker size). **Keyed on `site_id`** — the sim's stable catalog key from `core_sim/src/data/sites_config.json`, **already on the wire** (decoded in `native/src/lib.rs`, already read by `SecondaryMarkerRenderer._wonder_key`), so this needed **no schema or server change**. Deliberately NOT keyed on the `glyph` string: that is presentation the server also happens to send, and two sites may share one glyph (the fixture's `sky_arch` reuses ⛰), so keying on it would collapse distinct sites onto one sprite. `for_site_id(site_id) -> Texture2D` returns the cached texture or `null`. **THE `null` FALLBACK IS GENUINELY LIVE HERE — the one way this table differs from `FaunaSprites`/`SiteSprites`**, whose coverage is complete and whose fallbacks only guard an unknown key. `great_peak` + `verdant_basin` are the whole catalog *today*, but that catalog is **data-driven** and expected to grow: a designer adds a site entry with a glyph and it ships with no art, so falling through to the server-provided emoji is a real, **exercised** path (`map_sites.png`'s `sky_arch` renders it). Adding art stays: drop the PNG in `assets/icons/wonders/`, add the id here. **TWO consumers now, and both key on `site_id`:** the map marker (`SecondaryMarkerRenderer.draw_discovered_site`) and the **HUD top-bar discoveries strip** (`Hud.update_discoveries` — see the Wondrous Sites bullet), which was the last place in the client keying site presentation on the glyph and has been migrated onto this table. Neither builds a second art map; `for_site_id` is the one lookup. A site with art must draw **even if the server sent no glyph**, and that takes BOTH halves of `SecondaryMarkerRenderer`, which is why they share one predicate, `_wonder_renders(site)` = *has a sprite OR a non-empty glyph*: (1) `compute_slots` must admit sprite-only sites to **slot eligibility** — it originally tested the glyph alone, so such a site got no slot and `draw_discovered_site` bailed at its `slot < 0` return long before any sprite check, making the guarantee unreachable; and (2) `draw_discovered_site`'s own early-return must likewise account for the sprite, not just the glyph. Past that guard it calls `MapView._draw_marker_sprite`, else falls through to the unchanged emoji `_draw_marker_glyph`. Latent while every shipped site carries a glyph — keep the two tests on the shared helper so they cannot drift back apart. **Same import options as fauna/sites** (`process/size_limit=64`, `mipmaps/generate=true` — inert under the pinned `TEXTURE_FILTER_NEAREST`) and the same judging rule: at true marker size. At ~36px `great_peak`'s snow-capped silhouette is unmistakable; `verdant_basin`'s leaf fronds merge into the green mass (the `reeds` caveat again) but its green-ring-around-blue-water read stays distinct — re-check it first on any sizing change. Verify on `map_preview`'s **`map_sites`** (both sprites + the unmapped `sky_arch` falling to emoji) and **`map_sites_fogged`** (the case unique to this marker: a site persists on a *remembered* tile under the mist tint — both sprites must still read there) |
| `ui/StageSprites.gd` | Bundled PNG art for **SETTLEMENT-STAGE** tokens — the fourth art family behind `IconSprites`, covering BOTH the map band token (`BandMarkerRenderer._draw_band_token`) and the **Band/City panel header** (`BandCityPanel.set_header`, which swaps a `TextureRect` in for each of its two glyph `Label`s). Same reasoning as the rest: the emoji path draws through `ThemeDB.fallback_font`, so the OS emoji font decided what a camp/village looked like, and ⛺/🛖/🏘️ blob at token size. **THE ONE WAY THIS FAMILY DIFFERS FROM ALL THE OTHERS: its key comes STRAIGHT FROM THE SERVER, with no client-side resolver.** `FaunaSprites` derives a species key from a free-text herd label (`FoodIcons.species_key_for`) and `SiteSprites` from a module+terrain branch (`site_key_for`); here the sim's `settlement_stage_id` (from `settlement_stage_config.json`, already on the wire and decoded in `native/src/lib.rs` — this needed **no schema or server change**, only a GDScript reader) IS the key, so `for_stage(stage_id) -> Texture2D` is a direct table hit. Deliberately NOT keyed on `settlement_stage_icon`: that is presentation the server also happens to send, and keying art on a glyph string is the brittle reverse-mapping this table exists to avoid. **The `null` fallback is LIVE, like `WonderSprites`' and unlike fauna/sites'** — `settlement_stage_config.json` is user-editable, so a game may define stages beyond the three bundled (`nomadic`/`camp`/`village`), and those must keep rendering their configured emoji. **Precedence in `_draw_band_token` is load-bearing:** expedition → **sprite** → glyph → placeholder square. The sprite attempt MUST precede the empty-glyph placeholder branch, which returns early — otherwise a sprite-mapped stage whose glyph happened to be empty would wrongly draw a square. `dim` (a behind card in the band stack) modulates the sprite by `BAND_STACK_BEHIND_TINT`, mirroring what the glyph path does to its colour — the ONE case `MapView._draw_marker_sprite`'s `modulate` param is for; it is structural recession, never a state encoding (see that helper's comment). Same import options as the other families (`process/size_limit=64`) and the same judging rule: at true marker size. Verify on `map_preview`'s **`map_stage_glyphs`** (the ⛺→🛖→🏘️ progression as sprites, plus the empty-stage band still drawing the neutral placeholder square) and `band_panel_preview`'s header (the fixture carries `settlement_stage_id: "camp"`) |
| `ui/IconSprites.gd` | The shared texture cache behind ALL FOUR bundled-art tables (`FaunaSprites`, `SiteSprites`, `WonderSprites`, `StageSprites`): `texture_for(path) -> Texture2D` owns the lazily-populated path→`Texture2D` dictionary, the `load()`-not-`preload()` (so a missing file degrades to the emoji rather than breaking scene load) and the **one warning per bad path** (a failed path caches `null`, so the load is attempted once, not once per marker per frame). Extracted because the tables would otherwise carry that cache verbatim four times; a new art family is now just a `SPRITE_PATHS` table plus a key resolver (`WonderSprites` was exactly that — a table keyed on `site_id`, no cache code — and `StageSprites` is the degenerate case: the server sends the key, so there is no resolver at all). Static-only, same reasoning as `FoodIcons` |
| `ui/NarrativeForkPanel.gd` | **The Telling** (`docs/plan_the_telling.md`) — the narrative fork decision surface. A modal centred overlay: the node IS a full-screen dismiss **catcher** (+ a dim scrim) with the card nested INSIDE it as a child, reusing TurnOrb's catcher-nesting exactly (siblings let the catcher swallow the choice clicks). The card is an **`AutoSizingPanel`** (`fit_to_content`) because wardrobe entries vary a lot in prose length. Content top-to-bottom: the **narration** in the active voice register (the hero element — large, generous leading, wrapped, never truncated), the **choices** in catalog order (the `is_defer` one `ghost`, the rest `primary`; **every choice is always enabled** — defer is the out the gate depends on), the **gloss** collapsed behind a "beneath the telling" toggle (the real sampled numbers, `sedentarization.score = 41.00`, deliberately not prettified), and a **voice-register toggle** built from the registers the fork ACTUALLY carries (`VoiceLine.register` is free-form by design — nothing hardcodes `mythic`/`warm`; an unknown stored preference falls back to the FIRST available). The preference persists to `user://narrative.cfg` (`[narrative] voice_register`) via **static** `load_voice_register`/`save_voice_register`/`text_in_register`, which `Hud` also calls so the orb row's label uses the same register. Opens on `TurnOrb.panel_requested("decision")` and **automatically the first time a beat appears** (tracked by `beat_id`, so a dismissed fork does not re-open every snapshot). Answering emits `answer_selected` → `Hud.answer_fork_requested` → `Main` → `answer_fork <faction> <beat> <choice>`, clearing the local cache optimistically. Its **eyebrow carries the voice MEDIUM's accent** (`set_voice_medium`, pushed by `Hud.update_voice_medium`) — the one place a fork shows the medium, since the narration is medium-independent by design; the accent table + its `oral` fallback live in `TellingPanel.accent_for`, so the two narrative surfaces can never drift to different colours for the same medium |
| `ui/TellingPanel.gd` | **The Telling** (`docs/plan_the_telling_book_ux.md`) — the narrative panel as a **grow-to-fit BOOK** whose capabilities grow with the narrator's medium. A controller (like `CommandFeedController`) over a **right-dock** `PanelCard`, priority **10** — the TOP of that dock, and with Victory + Terrain Types both suppressed by default it effectively owns the whole right column. (It used to sit in the LEFT dock at priority 40, under the selection cards + command feed, where it rendered cramped; the command feed stays left.) **WHY IT EXISTS:** beats used to render in the command feed and **two of them filled it outright**, pushing ordinary receipts off; a receipt is a **transactional acknowledgement** (worthless after seconds), a beat is **the story so far**. Opposite retention, density and reading behaviour, so they got separate surfaces. `handles_kind()` (`narrative_beat` / `narrative_fork`) is the **one definition of the split** — `CommandFeedController` asks it rather than keeping its own list, so a kind can never land in both surfaces or in neither. **THE BOOK MODEL:** the controller keeps its full retention buffer (`_entries`, cap `ENTRY_LIMIT` **40** — the backfill/dedup source of truth, each entry `{tick, bbcode}`) and **derives** `_pages` from it every render — **a page = one speaking turn's beats** (the entries sharing a `tick`; `_rebuild_pages` groups by tick ascending, a beatless turn makes no page). The card **grows to fit the current page's content, capped at `PAGE_MAX_HEIGHT`** (320, additionally clamped to `PAGE_MAX_HEIGHT_VIEWPORT_CEIL` = ½ the viewport) — a single beat gives a short card, a two-beat turn grows to show both with no scrollbar (the playtest fix); only a page past the cap engages the inner `ScrollContainer`. A PAGE is bounded (one turn's beats), so fit-to-content doesn't reopen the dock-sizing problem the old scroll log had — the cap is the backstop. The `DockScrollFit`/`_is_at_tail`/auto-scroll machinery is **gone**; `refit()` re-syncs the page geometry + re-fits the current page so `Hud._refit_right_dock`'s call stays valid. `_fit_page_height` reads `_label.get_content_height()`; `PAGE_MIN_HEIGHT`/`PAGE_FIT_PADDING` floor + pad it. Each beat renders as **PROSE** (wrapped, never truncated) with the gloss as a dim italic secondary line — **no `Turn N` prefix, no bold-label/italic-detail split** (receipt affordances that fight prose); only a fork keeps a glyph (`?`, the mark the orb's decision row wears). **THE VISIBLE PAGE NEVER MOVES ON ITS OWN:** new beats grow `_pages` and set the **unread** mark (derived: `_page_index < last` for a retaining medium), they do NOT turn the page (the page-turn twin of the feed's tail-scroll-yields-to-reader rule). The player turns via the leaf controls (`leaf(±1)`); **`reveal_newest()`** catches them up — `Hud._on_turn_orb_advance` calls it on turn-advance ("catch me up"). **A page turn PLAYS A MEDIUM-MATCHED ANIMATION** (`PAGE_TURN_DURATION` ~0.18s) — but ONLY on a real turn (leaf / reveal / oral's utterance-replacement), never on a beat arriving to a retaining medium (that only marks unread; animating it would fight the yields rule). A clipped `_page_frame` (the authored `ScrollContainer` reparented in) holds the incoming page; a second `_outgoing` RichTextLabel snapshots the pre-swap page; a Tween drives one `progress` 0→1 into `_apply_turn_frame`, which position+alpha-blends the two per medium: **oral** = crossfade + a small DOWNWARD settle-drift (`PAGE_ORAL_DRIFT`), **slower + `EASE_IN_OUT`** (`PAGE_TURN_DURATION_ORAL` ~0.42s) — a bare alpha fade over 0.18s read as a "hard switch" flicker in playtest, so oral is lengthened and given spatial drift (downward, opposite painted's rise) so the eye can track it · **painted** = incoming rises from below with a fade · **written** = horizontal slide in the leaf direction (both snappy `EASE_OUT` `PAGE_TURN_DURATION` ~0.18s). Oral's turn fires on **beat ARRIVAL** (it pins to newest, keeping no held page — so its arrival IS its turn and does not fight the yields rule); painted/written animate only on the player's leaf/reveal. **Since pages differ in height, the frame holds `max(outgoing, incoming)` (capped) for the tween then settles to the incoming page's fitted height.** **Interruption-safe by construction:** every turn `_kill_tween()`s the running one and the primary label already shows the FINAL page, so a rapid second turn / medium change / collapse settles to the correct static state AND its correct fitted height (never a half-slid page); initial-population and reset never animate. **Backfill on connect** is free: a full snapshot carries the server's whole `commandEvents` ring and the signature de-dup makes re-ingesting it harmless — which is also why `Hud.reset_command_feed` resets the FEED only and deliberately **not** this panel. Collapsible (the page frame hides, the header + collapse control stay), persisted to the **same** `user://narrative.cfg` `[narrative]` section NarrativeForkPanel writes the voice register into (key `telling_collapsed`) — one narrative prefs file, not two |
| `ui/TellingPanel.gd` (the maturing voice) | Wire: `CampaignSection.voiceMedium[]` (`VoiceMediumState {faction, mediumId, mediumIndex}`), decoded in `native/src/lib.rs voice_medium_to_array` into **both** the snapshot and delta dicts under `voice_medium`, dispatched by `Main` → `Hud.update_voice_medium` (which scans for `PLAYER_FACTION_ID` following the `sedentarization` precedent exactly, **including the `snapshot.has(...)` guard — a delta omits an unchanged field, so absence means "unchanged", never "cleared"**) → `TellingPanel.set_voice_medium` + `NarrativeForkPanel.set_voice_medium`. **The medium sets BOTH the styling (`MEDIUM_STYLES`) and the book's CAPABILITIES (`MODE_TABLE`), both TABLES WITH AN `oral` FALLBACK, never a match** — `mediumId` is free-form by design (a new medium needs no schema change), so an unknown id degrades to the first rung. `MEDIUM_STYLES` → `{title, accent}`: `oral` "AT THE FIRE" (`HudStyle.WARN`, ember) → `painted` "ON THE WALL" (`VOICE_PIGMENT`) → `written` "THE RECORD" (`VOICE_INK`). `MODE_TABLE` → `{furniture, leaf_back, retain_pages}` drives the three rungs: **`oral`** {`FURNITURE_NONE`, no back, `retain_pages:false`} — current utterance only, `_page_index` **pinned to the newest** (`_reconcile_page_index`), no page chrome at all; **`painted`** {`FURNITURE_MARKS`, no back, retain} — the accumulating wall, walk FORWARD one page at a time via `›`, a marks + `k / n` position cue, no back control; **`written`** {`FURNITURE_BOOK`, `leaf_back:true`, retain} — the full book, a `Page k / n` number + **‹ ›** leaf controls, leaf freely both ways. The furniture row (built in `_build_chrome`) carries the `‹`/`›` line-art glyphs (text-presentation, judged at true size — the MagnifierButton hazard), the page-number/position label, and the accent **"a new telling waits"** unread cue. **Presentational ONLY: the medium never selects different copy** — per-medium copy is a deliberate non-goal, documented server-side, and the same wardrobe line renders under every rung. **RESTRAINT IS THE REQUIREMENT:** the HUD is dark and STAYS dark, so the maturation is carried by the **title, the accent, a hairline rule and the line-art book furniture** — never a light "parchment" background (which reads as a rendering bug, not a chronicle). The title tint goes through `PanelCard.set_title_color`. **`switch → _apply_medium` calls `render()` (not just a repaint)** because a medium change flips `retain_pages`, so the visible page must be reconciled (oral re-pins, a retaining medium re-clamps). ui_preview: `telling_panel_oral` (bare) / `telling_panel_painted` (marks + forward-only) / `telling_panel_written` (page number + ‹ › off the last page) / `telling_panel_unread` (held on an old page + the cue) / `telling_panel_oral_two_beats` (a single-tick TWO-beat page GROWS to fit both with no scrollbar — asserts `debug_page_scrolls()` false) / `telling_and_feed` (fitted page + legible feed) — these SETTLED end-states park via the non-animating `debug_jump_to`. The page-turn animation is captured mid-transition (drive a turn, `debug_freeze_turn_at(0.5)`, save): `telling_turn_written_mid` (two pages offset horizontally) / `telling_turn_painted_mid` (incoming risen + fading) / `telling_turn_oral_mid` (both at partial alpha, offset by the settle-drift) / `telling_turn_interrupted` (an ASSERTION: a rapid second turn settles to the right final page with no leftover overlay). **`telling_live_oral_arrival`** is the LIVE-PATH proof (no debug hook): it drives the real `update_voice_medium`→`ingest_command_events`→`_refit_right_dock` per-snapshot sequence with a new oral beat and ASSERTS `debug_turn_active()` — a running tween is created by beat arrival AND survives an in-cycle refit (the freeze states only prove the tween CAN render, not that the live path TRIGGERS one) |
| `tools/ui_preview.gd` / `.tscn` | Dev-only preview harness: instances the real `HudLayer` with canned selection/targeting data, renders each state, and saves PNGs to `ui_preview_out/` (gitignored). The one-card selection layout has its own states — `tile_panel_land` / `tile_panel_no_forage` / `tile_panel_herd` / **`tile_panel_crowded`** (3 bands + 2 herds: every row visible, drawer capped, dock not scrolling — the frame the cap is judged on) / **`tile_panel_land_sticky`** (the sticky-selection ASSERTION, driven through the REAL path: it instances MapView, wires Main's two signals, clicks the crowded hex, clicks the land row, and replays whatever `refresh_selection_payload` answers — the payload must not be `"unit"` and the land must still be lit. Proven to fail with MapView's `select_occupant` `"land"` branch removed) / `tile_panel_unseen` (remembered hex: chips + land row + the unknown-contents note, NO occupant rows) / `tile_panel_band` (the Band/City pointer line, not a blank gap, **plus the drawer's `Move`** — and its behavioural ASSERTION: the hex carries three player bands, the SECOND is selected through the real list path, and pressing the REAL button must put the HUD into move-band targeting for **that** band (302), not the faction default `_player_band` (301). Proven to fail with `_on_move_band_pressed` resolving to `_player_band`; `tile_panel_crowded` additionally asserts the no-panel fallback shows exactly ONE Move, proven to fail with a second one added) / `tile_panel_feed_shown` (`R` on: both growing cards fit the dock). Part 2 (the compose sheet) adds **`tile_panel_compose_forage`** / **`tile_panel_compose_herd`** (the expedition branch + raid forecast) / **`tile_panel_compose_gated`** (a locked rung greyed AND its gate reasons rendered beside it, inside the sheet) — all three must show the map UNDIMMED behind the sheet — and **`tile_panel_standing`** (the CLOSED read state on a worked source: `⇄ 4 foragers · +2.74 /turn ⚠ · only 2 of 4 working`, the ⚠ and the note being the same two INDEPENDENT flags a Band-panel Current-actions row carries). Plus four behavioural ASSERTIONS, each proven to fail before it was trusted: a snapshot (`reapply_selection`) leaves the sheet OPEN, the same refresh CLOSES it when the subject is swapped (the half that proves the first is not vacuous), starting move-band targeting closes it, and — with a sheet and targeting BOTH active, the only configuration that can tell the order apart — `Main.escape_claimant` answers `compose_sheet`. **A state that exists to judge the picker/stepper/forecast/gate reasons must call the harness's `_compose_forage` / `_compose_herd`** (which open the sheet); the drawer alone now shows only the summary + button. Iterate on HUD styling without a server: `godot --path . res://tools/ui_preview.tscn` |
| `tools/map_preview.gd` / `.tscn` | Dev-only **MapView** preview harness (HUD-only ui_preview's companion): instances the real `MapView`, feeds a canned `display_snapshot` + selects a band, and dumps PNGs (`map_*.png`) to `ui_preview_out/`. Verifies the selected-band labor highlights (work-range ring / worked forage tiles / hunted-herd ring+link; scouting draws no disc — it extends sight in the fog), the terrain/blend states, and the **rivers** state (`map_rivers*.png` — hex-edge Minor/Major rivers + the NavigableRiver terrain chain, incl. `map_rivers_join.png`: a zoomed, hex-anchored close-up of the trunk HEAD, where two tributaries hand over at corners — the frame the `river_inflow` spurs are judged on — `map_rivers_head_minor.png`: a second navigable head fed by a **Minor tributary only**, the frame the HEAD TAPER is judged on; **`map_rivers_midchain.png`**: a Minor tributary handing over at a vertex of a **MID-CHAIN** trunk hex (upstream *and* downstream channel exits) — the frame the head-taper's **exit-count gate** is judged on: the trunk must hold **constant full width through the junction** (any pinch-and-swell at the hex centre is the HOURGLASS the gate exists to prevent) while the spur still reaches its vertex. The case the drainage-network rewrite created and the fixtures never had; **`map_rivers_notch.png`**: a chain HEAD whose tributary hands over at its BOTTOM vertex (corner 1) and whose single channel exit is the ADJACENT SW side — both flanking the same corner, the geometry the old centre-hub routing drew a NOTCH / inverted-V on. The direct inflow-corner→exit-midpoint routing must draw ONE smooth tapered channel with no notch (zoomed via `NOTCH_ZOOM_IN`); **`map_rivers_lake_alongside.png`**: a one-hex `inland_sea` ringed by three navigable hexes whose `river_channel` exits all run along their own chain / out to the sea — NONE into the lake (the @21,61 case). The shore pass's per-edge MOUTH test must draw the lake's FULL beach/foam ring INCLUDING the navigable-adjacent edges (the old "any navigable adjacency" exclusion ate them); the true mouth into the eastern sea in the same frame STAYS open; and `map_rivers_web.png`: a solid CLUMP of adjacent navigable hexes with `river_channel` winding through it as ONE snake — the **regression guard** for the spider-web bug, since the other river fixtures build their chain by hand and are paths by construction, which is why the harness never caught it. Any cross-link/triangle there = the terrain-inferred arm rule is back) and the **starving-pen distress badge** (`map_herd_starving` — a starving pen beside a fed one, **plus a third starving pen (boar)**: every species now has bundled sprite art, so all three pens are `FaunaSprites` markers and the frame proves the ring/badge reads over a sprite — it no longer exercises the emoji fallback at all) and **`map_fauna_sprites`** (the SPRITE ROSTER: one herd per bundled-art species on its own hex, in one row because MapView is cover-fit and a second row is cropped away unseen — the only frame where the whole art set is judged at once for swapped/clipped/fringed sprites) and its food twin **`map_site_sprites`** (the same idea for `SiteSprites`: one food site per bundled art key in one row, including a `game_trail` site — which must draw the fauna DEER — and an unknown module, which must fall to the `default` sprig; the riverine fish↔reeds pair is judged separately on `map_riverine_split`, since one module drawing two icons needs two terrains, not two hexes) Also state **"pasture"** (`map_pasture.png`) — the **graze distribution** on an earthlike-shaped fixture map under the `pasture` overlay channel (see Overlay Channels): the frame Phase 2a exists to be judged on (is prairie really pasture? is the alluvial fallback dominant? are glacier/lava/water distinct from merely-poor ground?). It stages a **woodland block a live map does not have** (the palette thins forest out), sizes the window to the grid's aspect (MapView is **cover-fit**, so a mismatch CROPS exactly the distribution you came to see), and **prints the legend dict** (this harness has no HUD to draw it into). Also state **"forage"** (`map_forage.png`) — the **human-food distribution**, the SAME earthlike fixture painted from the human-food table under the `forage` channel, so it compares tile-for-tile with `map_pasture` and the two food webs' divergence reads directly (forest/river rich on forage / poor on pasture; the shelf column glows on forage where it is barren on pasture) without a server: `godot --path . res://tools/map_preview.tscn` |
| `tools/blend_probe.gd` / `.tscn` | Dev-only **edge-blend probe rendered at the GAME's on-screen hex radius** — the other harnesses *fit* their grid to the window (r ≈ 83–178) and the blend look is radius-relative, so every judgement made in a fitted frame was wrong. Pins a 1:1 1920×1080 canvas + a grid sized so `_fit_map_to_view` lands on the target radius (it prints the achieved radius and warns if it drifts). **Two states:** (1) a **band strip** of flat biomes at r≈45 (desert · prairie · scrub · alluvial · tundra · salt flat — every adjacent pair is a flat↔flat seam) → `blend_bands_*.png`; (2) **ISOLATED prairie hexes surrounded on all six sides by dark rocky soil** at **r≈75** (the user's on-screen size) → `blend_isolated_shipped.png` + one full frame & native-res close-up per tuning variant + a labelled contact sheet (`V6_*.png`). **State 2 is mandatory for any blend change**: a straight band seam looks fine even when the blend is tearing holes in hex interiors — only a surrounded hex exposes it (that is how the shredding regression shipped). **Two more states (V7, water↔water):** (3) an irregular **deep-ocean region embedded in continental shelf** (plus isolated deep hexes) at r≈77 → `V7_water_W1.png` (water on the shared LAND levers — still a soft-edged hexagon) vs `V7_water_W2.png` (the shipped `water_blend` block — the silhouette dissolves); (4) a ragged **coast** frame with a single water id → `V7_coast_unchanged.png`, the **bit-identical reference** any blend-eligibility change is pixel-diffed against (it must not move the shoreline). **Two more states:** (5, V8) the water patch rendered **FoW OFF vs FoW ON** (a mix of active + discovered hexes, nothing unexplored) → `V8_water_fow_off.png` / `V8_water_fow_on.png` — the FoW tint comes from a **per-hex, NEAREST-sampled vis-map**, which used to make every discovered↔active adjacency a **hard hex-shaped tint boundary that is not a terrain seam**. Any "hard straight edges are back" report must be checked against this pair BEFORE the blend is touched. This is also the frame the **FoW boundary softening** is judged on (see Fog-of-war softening: the steps must be gone, pure states unchanged); (6, V10) the shipped **shoreline profile** on the ragged coast at r≈75, rendered against TWO land biomes → `V10_shore.png` + `V10_shore_closeup.png` (prairie) and **`V10_shore_dark_land.png` + `V10_shore_dark_land_closeup.png`** (rocky_regolith). The close-ups are where the "is there a hard line anywhere on land→sand→foam→water?" call is made (the downscaled full frame hides a 1px line; see Shoreline), and **the DARK-land one is decisive** — prairie's tan hides sand-vs-land contrast and masked an invisible-beach bug through several passes, so never judge the beach on prairie alone. `_render_variant(overrides, name, crop…)` overrides any `terrain_config` lever (incl. the nested `water_blend` / `shore` blocks) live, which is how the shipped values were swept. **One more state (8, W): the FoW hex-step BEFORE vs AFTER the boundary softening** — one camera, one terrain, one visibility map, only `fow_softness` varying → `W_fow_off.png` (FoW off, the terrain-only reference: the deep-ocean blob's edges are already soft, which **exonerates the blend**), `W_fow_on.png` (softness `0` — reproduces the **unsmoothed per-hex tint**, i.e. the hard hexagonal brightness steps), `W_fow_fixed.png` (the shipped softness — steps gone, mist preserved). Each also dumps a `_closeup` and, decisively, a **`_same_terrain`** crop straddling hexes **(4,3) Active / (3,3) Discovered — BOTH continental shelf**, so the only thing that can draw an edge between them is the FoW tint. That crop answers any "hard straight edges in open water, even between hexes of the same terrain" report. **One more state (9, X): the DARK-WATER report on REAL game terrain** → `X_dark_water.png` + `X_dark_water_closeup.png`, rendered from a **verbatim 14×10 window of a LIVE snapshot's id-map** (`X_WATER_IDS`), FoW OFF, r≈75. The synthetic water states (3/5/8) never reproduced the "dark patches of open water with hard full-hexagon edges" report because their deep-ocean region is ONE clean ragged blob; the real ocean is **salt-and-pepper** shelf/deep, and a lone deep hex ringed by shelf can only read as a dark HEXAGON. **Any "dark water hexagons" report must be rendered on THIS state** — a synthetic blob will not show it. It is the frame the water **depth field** (see Edge Blending → water) was verified against. **One more state (10, L): the PER-WATER-TERRAIN shore profile on a SMALL INLAND SEA** → `L1_current.png` / `L2_no_wisp.png` / `L3_half.png` / `L4_tenth.png` (+ `*_full.png`), a 7-hex `inland_sea` lake in a field of **dark rocky_regolith** (prairie's tan camouflages both sand and foam) at r≈75, one camera/crop across all four. `_render_lake_variant` overrides the inland_sea entry's `shore_profile` in the live config and calls `TerrainTextureManager.rebuild_layer_shore_map()` — the sweep for choosing a lake's coast (now in the three-scale scheme; **L3 IS the shipped lake**, `sand 0.5 / foam 0.5 / wisp 0`, and L4 = the whole profile scaled so its OUTERMOST reach, `wisp_center + wisp_half` = 0.68·r, lands at ~0.10·r → 0.147). **The harness disables `MapView._unhandled_input`** — it renders in a REAL window, so the OS cursor otherwise drew a faint HOVER hex outline into the frames, a run-to-run difference of a few thousand pixels that silently defeats the pixel-diff the coast states exist for. With it off, consecutive runs are **byte-identical**, so `V7_coast_unchanged.png` / `V10_shore*.png` are usable as strict bit-identity references. **One more state (11, H): ROLLING HILLS "cut off at the hex edge"** → `H_*.png`, a `rolling_hills` (24) blob + **isolated** hills hexes + an **isolated alpine (26)** hex in a field that is dark `rocky_reg` west / tan `prairie` east, at r≈75 with the **hex grid overlay OFF** (a drawn hexagon would answer the very question under test). Frames: `H_before` (the artifact), **`H_base_only`** (peaks skipped by pushing `peak_min_radius` above the render radius — isolates the BASE floor, and is what proved the cut is the rugged base hexagon, **not** a weak mound overhang), `H_peaks_only` (the amplified `before − base_only` pixel diff = the peak pass's exact footprint: it shows the mounds DO overhang, and that the peak **cast shadow darkens the whole neighbour hex**, a second hard hexagon), and the candidate fixes `H_fix_overhang` / **`H_fix_base`** (`blend_rugged_land`) / `H_fix_both`. Each renders a full frame + a seam close-up + the **isolated-hex** and **alpine** close-ups (the mandatory shred checks). `H_gate_bands_full` / `H_gate_coast` re-render the flat↔flat strip and the coast with the rugged gate ON — they must byte-compare **identical** to `blend_bands_full` / `V7_coast_unchanged`. **One more state (12, R): the RUGGED-GATE SWEEP** — `blend_rugged_land` is GLOBAL, so shipping it lets EVERY rugged biome's base floor blend, and the failure mode is SHREDDING. R renders **each rugged biome as an ISOLATED hex** (even col + even row ⇒ never adjacent to another subject) in TWO fields, each **gate OFF vs gate ON** so every biome is a controlled A/B: `R_flatoff_*` / `R_flat_*` (dark `rocky_reg` west, tan `prairie` east) and `R_ruggedoff_*` / `R_rugged_*` (a field of `canyon_badlands` — the rugged↔rugged case), plus `R_*_field_full`. **The gate-OFF pair is not optional**: several biomes' own art (e.g. `karst_highland`'s semi-transparent overhanging spires) *looks* like neighbour texture leaking into the hex, and only the A/B tells art from tear. **One more state (13, S): the PEAK CAST-SHADOW HEXAGONS** — an alpine massif + an isolated `rolling_hills` hex in a light prairie field, grid OFF → `S_shadow.png` + `_closeup` + `_iso`, and decisively **`S_shadow_footprint*.png`**, the amplified diff against a `shadow_strength = 0` render (the cast shadow **in isolation** — the only frame on which "is it hex-shaped? is it still directional?" can actually be answered, since the semi-transparent mound fringe contaminates every other measurement). **Two harness bugs were fixed here and must not regress:** (a) `project.godot` opens the window **MAXIMIZED** (`window/size/mode=3`) and the WM applies that a few frames into the run — *after* `_ready` sized it — so the viewport became the whole monitor and every state after the second silently rendered at **r ≈ 154, not the game's 75** (and the taller states overflowed the canvas, clipping the close-ups). `_pin_canvas` re-asserts WINDOWED + 1920×1080 on every `_refit`. (b) Lever overrides now go through **`_override_config`/`_restore_config`**, which **ERASE** a key that was absent instead of writing `null` back: MapView reads levers as `bool(config.get(key, DEFAULT))`, the default only applies when the key is **missing**, and a present-but-null key reaches `bool(null)` — a **runtime error that aborts `_update_terrain_shader_quad` before it pushes a single uniform**, so every later frame renders with STALE uniforms and lies. **One more state (14, G): the REAL NEIGHBOURHOOD from the user's screenshot** — the "hills are STILL cut off, with the rugged gate ON" report → `G_*.png`. State H could not see why: its hills blob sits in FLAT fields only, so every peak edge in it is a peak↔non-peak one (which the overhang feathers). G rebuilds the screenshot — a `rolling_hills` blob against `canyon_badlands` (rugged, **no** peak asset), **`alpine_mountain` (which HAS one → the peak↔PEAK case)**, `high_plateau` (a peak at ~the SAME elevation as the hills → the near-zero-Δ case), `alluvial_plain`, `rocky_reg` and an `inland_sea` lake hex — at r ≈ 75, grid OFF. It is the **only** probe state that ships a real **elevation raster** (`G_ELEVATION_BY_ID` + `elevation_sea_level`): every other snapshot omits the channel, so MapView falls back to `PEAK_ELEV_FALLBACK` for EVERY hex and **no elevation asymmetry can be judged in them**. Frames: `G_before` (shipped), **`G_no_peaks`** (peak pass skipped — it renders the same seam as a soft ecotone, which **exonerated the base blend** and convicted the peak overlay), `G_no_shadow` (cast shadow off, peaks on — attributes a residual line to the shadow vs the art), `G_peaks_only` (the amplified diff = the peak pass's exact footprint), each with native-res crops `_peakpeak` (hills↔alpine, big Δelev), `_sameelev` (hills↔plateau, Δ≈0 → must stay a soft symmetric cross-fade), `_canyon` (peak↔non-peak — the control), `_lake` (the shoreline — hard BY DESIGN), `_iso` + `_iso_alpine` (the mandatory isolated-hex shred checks; both sit on the LEFT of the frame because MapView's minimap CanvasLayer is NOT hidden and a bottom-right crop captures IT). **A `--only=` state filter** (`godot --path . res://tools/blend_probe.tscn -- --only=G`, or `--only=1,4,G`; keys are `<number>/<letter>`, no filter = every state) renders one state instead of all 14 — a diagnosis loop re-renders one state many times. **A third harness bug was fixed here and must not regress:** `project.godot` opens the window **MAXIMIZED** and macOS applies — and **RE-applies** — that asynchronously, many frames in, so a fixed pair of `process_frame`s is a RACE that does not stay won. A filtered run puts a radius-critical state FIRST and it fitted at **r ≈ 154, not the game's 75**; a re-maximize BETWEEN two frames of one state rendered them at different resolutions (the pixel-diff then dies on a size mismatch); and one DURING a crop sequence made the captured image the monitor's while the viewport still reported the pinned size (`content_scale_size` pins the viewport, so **only `get_window().size` can see the maximize**) — the crop then landed off-frame as a 686×1 sliver. `_ensure_canvas` (called from `_settle`) re-pins and WAITS on the window; `_capture` re-draws until the captured geometry is the canvas's (or an integer HiDPI multiple) instead of silently saving a bad frame. **One more state (15, D): the THREE-SCALE shore profile — CLIFF vs BEACH vs LAKE, and the MIXED coast** → `D*.png`, the ragged coast against **dark `rocky_reg`** (prairie's tan camouflages both sand and foam) at r≈75, **grid overlay OFF**, one camera/crop per comparison set. `_snapshot_coast(shore_id, water_id)` now takes the SEA's id, which is what selects the `shore_profile` under test. Frames: **`D1_cliff`** (`deep_ocean` meeting land — NO sand anywhere, big surf, and the full-strength surf peak must still conceal the base's own step at the waterline, since there is no sand out there to hide it); **`D2_shelf_C1/C2/C3`** (the shelf's muting ladder, `foam_scale` 0.85/0.75/0.65 × `wisp_scale` 0.5 — the surf's measured footprint falls 18.0k → 15.8k → 13.9k → 12.2k px against the cliff's; **C2 ships**); **`D3_mixed_coast`** — THE DECISIVE FRAME: a `deep_ocean` hex and a `continental_shelf` hex **adjacent along ONE coastline**, both touching the same land (`_snapshot_mixed_coast` swaps the sea by row), where a nearest-water PICK would jump the profile at their bisector and make the sand appear along a **hard line**; the weighted-mean profile field must instead **fade the beach in** along the shore (measured: the land-pixel difference vs `D1_cliff` ramps from 0.00 over ~220px ≈ 3 hex radii — not a step); and **`D4_lake_unchanged`** (the lake, shipped config — the two-lever → three-scale migration must be a no-op). **One more state (16, SURF): THE BRIGHT WHITE SHORELINE OUTLINE** → `W_*.png`, the state the **waterline base cross-fade** + **`foam_opacity`** were built and chosen on (r≈75, grid OFF; the archipelago frames also render at **r≈30 — map scale**, which is the zoom the complaint was made at). The report was that the surf reads as "an obvious bright white outline on most land". Every frame uses the **MIXED coast** (`_snapshot_mixed_coast`: deep_ocean CLIFF in the north rows, continental_shelf BEACH in the south, both against **dark rocky_reg**) so each rung is cropped on **both coast types at once** (`_cliff` / `_beach`) — they fail differently. Frames: `W_base` (the shipped near-white ring — the complaint, and it is unmistakable); **`W_optA_1/2/3`** (option A, the **recolour-only** ladder: still an OPAQUE ring, just greyer — rendered so the "just make it grey" idea can be *seen* to be insufficient); **`W_optB_1/2/3`** (option B's `foam_opacity` ladder 0.35/0.55/0.75 on the cross-fade + muted colour; **0.55 ships**); and **THE MAKE-OR-BREAK PAIR — `W_step_control` vs `W_optB_step_check`**, the CLIFF coast with the **foam disabled entirely** (`foam_opacity 0` kills surf *and* wisp): the control (cross-fade also off) shows the **raw base step — a razor-straight hex-edge cut**, which is what the opaque foam was hiding all along, and the step check must show it GONE. **Any change to the surf must re-render that pair** — a translucent surf over a live base step is exactly the bug that broke this shoreline four times. `W_step_wl_1/2/3` is the `waterline_width` sweep it was chosen on (0.08 dissolves the step, **0.14** reads as a wet-rock rim, 0.20 ghosts land pebbles out to sea). **Judge the step check at 4× magnification** — at 1:1 the cross-fade and the razor step look nearly identical, and the first (too-narrow) cut was wrongly passed by eye before the magnified strip caught it. `W_base_wide` / `W_optB_wide` (+ `_farzoom`) are the **archipelago** (`_snapshot_archipelago` — islands on a lattice, alternating shelf-ringed BEACH coasts and deep-touching CLIFF coasts, so both types are in one frame; deterministic and grid-size independent, so the same map renders at r≈75 and at map scale): **`W_base_farzoom` vs `W_optB_farzoom` is the frame that actually answers the complaint.** **One more state (17, BANK): the NAVIGABLE-RIVER BANK CORRIDOR reading as a CHAIN OF HEXAGONS** → `BANK_*.png`, the state the per-terrain **`blend_profile`** (see Edge Blending) was diagnosed and chosen on. A navigable hex is a silty **bank** whose `blend_class` is `flat`, so the flat↔flat interlock IS eligible on its land edges — and a shader probe (tint the mix factor `t` on id 37) confirmed it **FIRES**: this was never a gate/eligibility bug, and no amount of re-checking `blend_class` or the water gates will find one. It is a LOOK failure — the global ecotone is ~`0.35·r` wide and near-straight, which is invisible between two tan grasslands and glaring between grey gravel and orange grass. The frame renders the corridor (a real `river_channel` chain, so the water draws) at the game's **r ≈ 75** crossing a field that is **floodplain (9, luma 58) in its west half and prairie (11, luma 112) in its east** — **both ends of the brightness range a river corridor actually touches, in ONE frame**, because the bank is *darker* than prairie but *brighter* than floodplain and a fix tuned against only one of them fails on the other. Plus an **ISOLATED bank hex in each field** (the mandatory shred crops — a corridor seam cannot show a torn interior; they sit in the TOP rows because a bottom-right crop captures MapView's minimap). `_render_bank_variant` sweeps the profile live via `_set_blend_profile` + `TerrainTextureManager.rebuild_layer_blend_map()`: **`BANK_off` is the NEUTRAL profile — i.e. the BEFORE**, the shipped global levers, in the same camera, and it reproduces the report exactly. `BANK_v1/v2/v3` are the ladder (**v2 = 2.6/2.2/2.6 SHIPS**; v1 still traces the hexagon, v3 dissolves the bank) and `BANK_shipped` is config's. `godot --path . res://tools/blend_probe.tscn` (or `-- --only=SURF` / `-- --only=BANK`) |
| `tools/band_panel_preview.gd` / `.tscn` | Dev-only preview harness for the **Band/City dockable panel**: instances the real `BandCityPanel` + `HudLayer`, injects the panel into the HUD, pushes a seeded player band through `update_band_alerts`, and dumps the panel docked left/right/top/bottom + collapsed (`band_panel_*.png`) so the chrome + the relocated band detail + the HUD reflow can be eyeballed without a server: `godot --path . res://tools/band_panel_preview.tscn`. **It isolates BOTH prefs files** — `NarrativeForkPanel.config_path_override` *and* `BandCityPanel.config_path_override` (the dock/collapse/TAB prefs). Without the second one the harness read whichever narrow-shell TAB the previous run left selected — so the band-zone frames silently rendered the work or parties zone instead — and then wrote its own tab walk back over the player's `user://band_city_dock.cfg`. Any state that judges a specific zone in the NARROW shell must still `set_active_tab` explicitly (`DEFAULT_TAB` is `work`). Disclosure states drive the REAL click path: `_click_disclosure` emits `meta_clicked` on the live vitals label with the very `[url]` meta its own text carries, never a poke at Hud state |
| `tools/marker_field_guard.gd` / `.tscn` | Headless **regression guard** for the "unit marker drops a panel-consumed field" bug class (twice hit: `hunt_mode`, then `working_age`/`idle_workers`). Feeds one realistic population entry through the real `MapView._rebuild_unit_markers` and asserts the produced marker is a superset of `PANEL_CONSUMED_KEYS` (the keys `Hud._unit_summary_lines` + `_build_allocation_panel` read off `_selected_unit`) and that the drop-prone fields round-trip (not defaulted). Exits non-zero on failure (CI-usable). No rendering, so headless: `godot --headless --path . res://tools/marker_field_guard.tscn`. When the panel starts reading a new marker field, add it to `PANEL_CONSUMED_KEYS`. **It guards a SECOND bug class — the NARROWED continuous field** (`FRACTIONAL_ROUND_TRIP_KEYS`): a field the decoder emits as a float (a fixed-point Scalar through `fixed64_to_f64`, or a `float` wire field) that the marker copies with `int(...)`. Presence-only checks structurally cannot see it — the key is there, the value is merely truncated — yet it is live-visible, because the marker IS the selection payload for a band clicked ON THE MAP. That is how `age_children`/`age_working`/`age_elders` shipped truncated: 9.29 + 16.54 + 4.64 became 9 + 16 + 4, and with every remainder zeroed `Hud._apportion_people` had nothing to redistribute, so the PEOPLE header read **29** beside a top bar reading **30** until the next snapshot re-resolved it from the raw floats (indefinitely, while paused). Each key in that dict is fed a deliberately NON-INTEGER value (the dict IS the fixture's value for that key, merged over `FIXTURE_ENTRY`, so the two cannot drift) and must come back within `FRACTIONAL_EPSILON`. **Membership rule: continuous end to end** — integer counts (`size`, `working_age`, `idle_workers`), entity ids and coordinates are deliberately EXCLUDED, since a fractional assertion on one would be a false claim. |
| `assets/terrain/TerrainTextureManager.gd` | Autoload singleton for terrain texture loading |
| `assets/terrain/TerrainDefinitions.gd` | Single source of truth for terrain definitions |

---

## Architecture

### Scene Structure
- `Main.tscn` - Root `Node2D` scene with a `Camera2D`, the `MapView` map layer, and `CanvasLayer`s for HUD/inspector/Band-City panel
- The client is **2D-only**; an experimental 3D relief view was permanently removed (see `docs/architecture.md` → "Removed: 3D Relief Rendering")
- Toggle: `I` hides/shows inspector, `L` shows/hides the Terrain Types legend, `V` shows/hides
  Victory. The legend + Victory cards ship **hidden** (both persisted to `user://narrative.cfg`
  `[hud_panels]`), so the right dock is the narrative surface's by default

### Data Flow
```
Server (FlatBuffers) -> SnapshotStream.gd -> parsed snapshot
                                          -> MapView (terrain/overlays)
                                          -> Inspector (panels)
                                          -> Hud (legend, selection)
```

### Native Extension
`native/` contains GDExtension bindings for FlatBuffers decoding (generated from `sim_schema/schemas/snapshot.fbs`).

#### Module map (`native/src/`)
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
    base/                        # 38 terrain textures (512x512 PNG); forest bases are grass FLOOR (no trees)
      00_deep_ocean.png
      ...
      37_navigable_river.png     # NavigableRiver's BANK ground (the channel water is rivers/02) — see Rivers
    canopy/                      # RGBA tree-crown overlays (transparency); one per canopy biome (3 today: 07/12/13)
    peaks/                       # RGBA mountain-relief overlays (transparency); one per relief biome (5 today: 24/25/26/27/29)
    rivers/                      # flowing water, NOT keyed by terrain id (see Rivers): 00_minor / 01_major
                                 # are the hex-EDGE classes (layer = class - 1); 02_navigable is the CHANNEL
                                 # water painted over a NavigableRiver hex's bank
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
land↔water and rugged stay hard — see Edge Blending below) — and may carry an optional
**`"blend_profile"`** block (`width_scale` / `noise_scale` / `noise_cell_scale`) scaling the flat↔flat seams
**it** is on, for a texture too far from its neighbours in tone+hue for the global ecotone (shipped on the
NavigableRiver bank only; neutral and bit-exact everywhere else — see Edge Blending → per-terrain
`blend_profile`). The top-level `blend_*` keys are the
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
  for the blend shader's peak overlay (see Edge Blending → Peak overlay), and `river_textures` (a FOURTH
  Texture2DArray of flowing water from `textures/rivers/`) for the blend shader's river pass (see Edge
  Blending → Rivers). The river array is the one array **not** keyed by terrain id — a river is not a
  biome, it rides an edge — so its layer is the file's numeric prefix = river **class - 1**, and there is
  no `river_layer_for(id)`

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
`EDGE_BLEND_DEFAULT_RUGGED_LAND` in `ui/TerrainRenderer.gd` → the shader's `blend_rugged_land` uniform).** Under
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
0.25/0.35/0.30; fallbacks are `WATER_BLEND_DEFAULT_*` in `ui/TerrainRenderer.gd`, pushed as the
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
- **Config levers (all fallbacks mirrored as `EDGE_BLEND_DEFAULT_*` consts in `ui/TerrainRenderer.gd`):**
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
  - **PER-TERRAIN `blend_profile` — because ONE ecotone does not fit every PAIR.** The five levers above are
    the GLOBAL ecotone, and they are tuned for the biome pairs that actually border each other: neighbours a
    few brightness points apart that share a hue. Their visible ramp is only ~`0.35·r` wide and the wobble
    displaces it by a fraction of that, so the boundary still essentially **traces the hex polyline** — which
    is invisible between two tan grasslands and *glaring* between two textures far apart in **both tone and
    hue**. The `NavigableRiver` **bank** (id 37) is exactly that: grey, low-contrast gravel (mean luma **89**)
    whose neighbours in a river corridor are prairie/scrub (**112–127**) on one side and floodplain/alluvial
    (**55–58**) on the other. Under the global levers alone the corridor renders as a **chain of grey
    hexagons** — the blend fires correctly, it is simply far too narrow and too straight to read as an
    ecotone at that contrast. **This is NOT fixable with the global levers** (widening them to suit the bank
    would move every biome seam main tuned), so a terrain entry may carry an optional block scaling the seams
    **it** is on, along three axes — the flat↔flat twin of the water side's `shore_profile`:
    `{ "id": 37, …, "blend_profile": { "width_scale": 2.6, "noise_scale": 2.2, "noise_cell_scale": 2.6 } }`
    * `width_scale` multiplies `blend_band` — the ecotone's **REACH**.
    * `noise_scale` multiplies `blend_noise_amount` — the boundary wobble's **AMPLITUDE**, so the boundary
      leaves the hexagon instead of tracing it.
    * `noise_cell_scale` multiplies `blend_noise_cell` — the wobble's **WAVELENGTH**. **Amplitude without
      wavelength is a fine fringe on a straight line**, not a meander: the lobes must scale with the (now
      wider) band. The two noise axes move together.
    * **CROSS-EDGE AGREEMENT — an edge takes the per-axis `max()` of its two terrains' profiles.** `max` is
      **commutative**, so both hexes flanking a seam derive the *identical* band, amplitude and cell; `p` is
      0.5 at the edge from both frames and the mix stays continuous across it, exactly as under the global
      levers. This is the same discipline that makes `shore_profile` key on the **water** side — if the two
      sides disagreed, the profile would itself draw the hard line it exists to remove.
    * **A terrain with no `blend_profile` is neutral (1, 1, 1) → a BIT-EXACT no-op**, and a seam between two
      unprofiled terrains is `max(1,1) = 1` on every axis. Verified: with only the bank profiled, **239 of
      247** harness frames are **byte-identical** to before it landed — the 8 that move are exactly the 8
      `map_rivers*` frames (i.e. every frame containing a navigable hex, and nothing else). Every
      `blend_bands_*` / `blend_isolated_*` / `V7_*` / `V10_*` / `H_*` / `R_*` / `S_*` / `G_*` / `D*` / `W_*`
      frame and `map_biome_blend` / `map_biome_shore_seam` / `map_swatch` are untouched.
    * Plumbing **mirrors `layer_shore_texture` exactly**: `TerrainTextureManager` packs the profiles into
      `layer_blend_texture` (1×N `FORMAT_RGBAF`, R = width_scale, G = noise_scale, B = noise_cell_scale),
      bound once by MapView as the `layer_blend_map` uniform and fetched in-shader by layer index
      (`blend_profile(layer)` → `vec3`; `edge_blend_profile()` is the `max` over the pair).
      `rebuild_layer_blend_map()` is public and updates the ImageTexture **in place** (so the binding
      survives) — that is how `blend_probe` state **17 (BANK)** sweeps it. Fallbacks are the
      `BLEND_PROFILE_DEFAULT_*` consts; `BLEND_PROFILE_MAX_SCALE` (4.0) guard-rails the reach, since the
      apothem is only 0.866·r and a wider band would collide with the opposite seam.
    * **Shipped:** only `navigable_river` (2.6 / 2.2 / 2.6) — chosen on `blend_probe` state 17, which renders
      the corridor against a **dark** field and a **bright** one in ONE frame. `1.8/1.6/2.0` still traced the
      hexagon; `3.4/2.8/3.2` started dissolving the bank's identity as a distinct silty corridor. Judge any
      new profile there, **including the isolated-hex shred crops** — a corridor seam cannot show a torn
      interior.
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
  larger fewer — `BASE_DEFAULT_TEXTURE_SCALE` in `ui/TerrainRenderer.gd`). **LOD:** below `EDGE_BLEND_MIN_RADIUS`
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
as before — both sides non-water → skipped). **The one exception is a `NavigableRiver` hex, whose edges are
excluded from the pass entirely — a river meeting the sea is not a coast; see Rivers → NavigableRiver for why
it cannot be expressed as a `shore_profile`.** Seaward read: **land → sand → surf → open water**, and the
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
- **THE WATERLINE BASE CROSS-FADE (`waterline_width`) — the last hard seam in the shader, and the reason the
  surf no longer has to be opaque.** Until this existed the **base texture itself stepped at `u = 0`**: on a
  beach coast the (sand-tinted) land met open water with nothing in between; on a **cliff** coast
  (`deep_ocean`, `sand_scale` 0) it was **raw land meeting raw water**. The full-strength foam peak was the
  ONLY thing papering over that flip — which is why **every previous attempt to "just soften the foam"
  re-exposed a hard land↔water line and had to be reverted** (four times). So the base now cross-fades across
  a short reach either side of the coastline: `mixed = mix(land_base, water_base, smoothstep(-w, w, u))`,
  held at full across `±w` and handing back to the true base over `SHORE_WATERLINE_FADE` beyond it.
  * **`land_base` / `water_base` are the SAME weighted-mean-over-{own + 6 neighbours} construction as the
    shore-profile field** (weight `smoothstep(−apothem, 0, d)`, unwobbled, own = 1) — so both are pure
    functions of the **id-map and the world UV**, never of `result` (which carries the own hex's
    interlock/depth-field history). The land hex and the water hex therefore compute the **same pair** at a
    given world point, `mixed` is frame-independent, and at `u = 0` both sides land on it **exactly**:
    continuous across the hex boundary by construction, like every other shore term. See
    `SHORE_PROFILE_REACH_APOTHEMS` for why that mean is exactly continuous.
  * **It is a WET EDGE, not an ecotone.** `waterline_width` **0.14** (·r) sits well under the sand's 0.25, so
    no land texture reads out to sea and no water texture reads up the beach. **Chosen on the foam-off step
    check** (`blend_probe` state **SURF**, cliff coast — the worst case, no sand out there either): 0.08
    already dissolves the step, **0.14** reads as a natural wet-rock rim, 0.20 starts **ghosting land pebbles
    into the water**. `0` disables it bit-exactly (and then `foam_opacity` must go back to 1).
  * **DO NOT envelope it with a ramp that also peaks at `u = 0`.** The first cut multiplied the cross-fade by
    `1 − smoothstep(0, w, |u|)` — two ramps peaking at the waterline — so the water content on the land side
    was already down to **8% at half the reach**: the visible gradient was a **quarter** of the configured one
    (~4px) and **the base step survived**. Hence `SHORE_WATERLINE_FADE`: full weight across the reach, fade
    back to the true base outside it.
- **Surf — peaks AT the waterline and washes BOTH ways.** Inland over the sand across `foam_inland_band`
  (the crossfade that kills the sand↔foam line) and seaward into open water across `foam_band`. **Its peak is
  the `foam_opacity` lever (shipped 0.55)** — and it is a lever *only because the waterline cross-fade above
  removed the base step it used to conceal*. With `waterline_width = 0` the peak is load-bearing again and
  `foam_opacity` must go back to ~1. It scales the **wisp** (`SHORE_WISP_STRENGTH`) too, so the whole surf
  mutes as one gesture rather than the peak fading while the offshore froth stays bright. `1.0` is a
  bit-exact no-op. This is what answers the **"obvious bright white outline on most land"** report: with the
  base step gone the surf can be a translucent highlight instead of an opaque cover-up.
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
  that dies at 0.41·r — and **`waterline_width`** (**0.14** — the base cross-fade's half-reach; see the
  waterline bullet above). **All six are fractions of the hex radius** → the `sand_band` / `foam_inland_band` /
  `foam_band` / `wisp_center_band` / `wisp_half_band` / `waterline_band` px uniforms (computed in
  `MapView._update_terrain_shader_quad` like `blend_width`), plus **`foam_opacity`** (**0.55** — the surf's +
  wisp's peak opacity, a unit scalar) and `foam_color` / `beach_color` (RGB 0–255, parsed by
  `MapView._shore_color` into normalized `vec3` uniforms). **`foam_color` ships MUTED — `[176, 194, 205]`, a
  grey-blue** (it was `[223, 242, 247]`, a near-white that read as a hard bright outline at map-scale zoom);
  the recolour alone was rendered as a candidate ("option A") and rejected — it only greys the ring, it does
  not stop it being an opaque ring, because the ring's opacity was structural. Fallbacks are the
  `SHORE_DEFAULT_*` consts in `ui/TerrainRenderer.gd`; the fixed feel-tuning (`SHORE_SAND_PLATEAU_MAX` /
  `SHORE_SAND_OPACITY` / `SHORE_WISP_STRENGTH` / `SHORE_WATERLINE_FADE`) is named consts in the shader. The `land_beach_width` / `sand_land_width` / `sand_sea_width` keys of the rejected passes are
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
    **and** `foam_band` = the surf's seaward reach). **REACH only — the surf's PEAK is the GLOBAL
    `foam_opacity` lever**, not a per-water one (see the Surf bullet above). `foam_scale 0` is not a legal
    profile.
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
  `EDGE_BLEND_MIN_RADIUS`). Fallbacks are the `CANOPY_DEFAULT_*` consts in `ui/TerrainRenderer.gd`.
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
  `PEAK_DEFAULT_*` consts in `ui/TerrainRenderer.gd`. Peaks are shader-only (same caveat as canopy).
- Verify via `tools/map_preview.gd` **State swatch** with `SWATCH_BIOME_ID = 26` (alpine) →
  `map_swatch.png` (+ `map_swatch_farzoom.png`): faceted peaks composite with light-left/dark-right
  self-shading, overhang the alpine↔prairie seam + cast a darkening shadow onto the prairie, and the
  far-zoom alpine band reads as a raised mountain mass. Restore `SWATCH_BIOME_ID = 2` after.

**Rivers — Minor/Major on hex EDGES, Navigable as a water TERRAIN:** rivers are two different kinds of
thing, and the split is the whole design (see `docs/plan_rivers.md`). A **Minor/Major** river lives on a
hex **edge** — that is where a future crossing cost can live ("the side the river is on is the side that
costs") — and is drawn by a **river pass in `terrain_blend.gdshader`** so the water is painted exactly on
the edge the penalty will apply to. A **Navigable** river is a body of water you are *in*, so **in the sim**
it stays an ordinary water terrain (`TerrainType::NavigableRiver`, **id 37** — blocking + boats fall out of
the existing water rules). **Its RENDER, though, is not a water hex** — see the navigable-channel pass
below: a water hex ran through the land↔water shore pass and came out a hex-shaped puddle with a sandy
beach and surf, i.e. **visually identical to an InlandSea lake** and nothing like a river. It is now drawn
as a silty **BANK with a wide channel through it**. The old `HydrologyOverlay` polyline (and
`MapView._draw_hydrology`) is **deleted** — the tiles now fully determine the render.
- **The wire primitive:** `TileState.riverEdges` (`ushort`), decoded in `native/src/lib.rs tile_to_dict`
  as `river_edges` (both the snapshot and delta tile paths share that one function). A **12-bit mask, 2
  bits per odd-r direction** — `class = (river_edges >> (2*dir)) & 0b11`, `0 = none / 1 = Minor / 2 =
  Major` (3 reserved). **Both hexes flanking an edge carry it** (hex `H` dir `d`; the neighbour dir
  `(d+3)%6`), so a hex answers "is there a river on my side `d`?" locally, with no cross-hex sampling.
  Ingested by `MapView.display_snapshot` into `tile_river_edges` (`Vector2i → int`, like
  `tile_habitability`).
- **The SECOND wire primitive — `TileState.riverInflow`** (`ushort`, decoded as `river_inflow` by the same
  `tile_to_dict`, ingested into `tile_river_inflow`): the same 12-bit / 2-bits-per-slot packing, but keyed
  by hex **CORNER** — `class = (river_inflow >> (2*corner)) & 0b11`. **Why it must exist:** an edge river
  runs *along* a side, so it does not end mid-edge, **it ends at a VERTEX** — and a trunk hex can flank two
  or three river edges (the tributary ran along several of its sides on the way in), which `river_edges`
  alone cannot disambiguate. So the sim names the hand-over vertex. It means **"a tributary hands over to the
  channel at this vertex"** — true of **ANY** navigable hex, **not** just a chain head (a real drainage network
  joins tributaries to trunks MID-CHAIN; the semantics widened with `docs/plan_rivers_drainage_network.md` §A —
  same field, same bits, same corner convention). A navigable hex with no tributary reports 0; more than one
  corner may be set (two tributaries can terminate on the same hex), so **loop all 6**. **Never read it as
  "this hex is a chain HEAD"** — that is what `river_channel`'s exit count says (below), and keying the head
  taper off inflow is exactly the HOURGLASS bug. Corner
  `i` is the vertex at angle `60*i + 30`, +y down — **exactly `MapView._hex_points` order** (0 lower-right,
  1 bottom, 2 lower-left, 3 upper-left, 4 top, 5 upper-right), so the shader derives it from the hex centre
  and radius with no table; side `dir` spans corners `{dir-1, dir}`. **Deliberately NOT surfaced in the
  Tile card / tooltip** — it is a rendering detail, not player-facing geography (`RiverEdges.gd` still
  reports the SIDES, which is what a crossing cost will key on).
- **The THIRD wire primitive — `TileState.riverChannel`** (`ubyte`, decoded as `river_channel` by the same
  `tile_to_dict`, ingested into `tile_river_channel`): **1 bit per odd-r direction** —
  `exits(dir) = (river_channel >> dir) & 1` — naming the sides a **navigable** hex's channel actually flows
  out through: its upstream and downstream neighbours in its own chain, plus (on the chain's LAST hex only)
  its exit into the sea / inland sea / `RiverDelta` mouth. **Why it must exist:** the trunk's connectivity
  is a **path**, and terrain cannot say which two of a hex's neighbours are on it. The renderer used to
  infer an arm for every navigable/water/`RiverDelta` neighbour, and wherever navigable hexes sat adjacent
  — parallel reaches, a chain bending back on itself, the blob a buggy worldgen once emitted — that rule
  **cross-linked them into a spider WEB with triangular holes**. Only the sim's tracer knows the path, so
  the sim states it and the shader arms **only the set bits**. Symmetric across a shared side **except at
  the mouth** (open water carries no channel, so that bit is not mirrored back) — so read the OWN hex's
  bits and never assume the neighbour agrees. It does **not** double-encode the head: the sim sets no exit
  toward the tributary, because the inflow SPUR (above) draws that. Do not "simplify" this back to a
  terrain test — `map_rivers_web.png` is the regression guard, and a web there is that bug returning.
- **The shader's `neighbor_offset` table IS a wire contract now.** It was reordered to the SIM's odd-r
  direction order (`core_sim` `grid_utils::HEX_NEIGHBOR_OFFSETS`, clockwise from E: 0=E, 1=SE, 2=SW, 3=W,
  4=NW, 5=NE) because the river pass indexes the mask **by direction**. The blend/shore/canopy/peak passes
  only ever loop over all 6 and are order-agnostic, so the reorder was free — but **do not reorder it
  again**.
- **RGBA8 river-map splatmap** (`_rebuild_terrain_shader_maps`): all four id-map channels are already
  taken (id / blend_class / canopy / peak), so the river masks get their **own** texture — and BOTH ride
  it: `R/G = river_edges` (low 8 / high 4), `B/A = river_inflow` (low 8 / high 4). Two 12-bit masks are 24
  bits, so they do not fit one RG8 texel; one RGBA8 texture is cheaper than a second sampler. NEAREST,
  rebuilt each snapshot — **after** the tile loop in `display_snapshot` (it reads `tile_river_edges` /
  `tile_river_inflow`, which the tiles populate). All 32 of ITS bits are now spoken for too, so
  `river_channel` (6 bits) rides a **second, R8 `river_channel_map`** built in the same pass, also NEAREST.
- **River pass (shader), after the shore pass, before canopy/peaks:** trees overhang a river and mountains
  sit above it; sitting before the FoW tint, a river in a Discovered tile **dims with the mist rather than
  disappearing**. Per fragment, for each of the own hex's carrying edges: distance to the **shared edge
  SEGMENT** — `mid ± perp * (hex_radius * 0.5)` (a regular hexagon's side == its circumradius), clamped to
  the segment, **not** the infinite bisector, which would smear the band across the whole hex — then keep
  the edge with the **max coverage** (`half_width - distance`). That min-distance-over-edges pick is what
  **rounds the corner joins for free**: a 120° turn softens with no spline math. The water samples in
  continuous map space (`v_map`, like the canopy) plus a **`TIME` scroll along the winning edge's tangent**
  so it flows.
- **THE HONEYCOMB, and what actually fixes it — read this before touching the river look.** An edge river
  drawn as a wide, constant-width, hard-edged stroke reads as *the hex borders, inked blue*. The instinct is
  to meander harder. **That is a dead end, and not because the meander is under-tuned:**
  - the amplitude ceiling is real — past ~`0.24` of the warp cell the warp's gradient exceeds the band
    half-width and the river **tears into disconnected pools**; and
  - more fundamentally the river is **edge-LOCKED by design**. The water must be painted on the edge the
    future crossing cost applies to ("the side the river is on is the side that costs"), so a warp can only
    displace the band about a band-width before it **detaches from its own edge and starts lying about the
    geometry**. Pushing meander trades a honeycomb for a lie.
  What actually kills the honeycomb, in order of impact: **(1) THINNESS** — halved to `minor_width 0.05` /
  `major_width 0.09`; a thin stroke reads as a river, a wide one as an outline. **(2) WIDTH VARIATION ALONG
  the river** (`width_variation`, low-frequency world noise on a `RIVER_WIDTH_NOISE_CELL = 2.6` hex-radii
  cell — deliberately several radii, so a swell is a property of the *reach*, not of the hex; a cell near 1
  would re-key the variation to the lattice and *reinforce* the honeycomb). **(3) RAGGED BANKS** — a
  higher-frequency wobble of the half-width (`bank_noise_width`, `RIVER_BANK_NOISE_CELL = 0.35`), the same
  idiom as the shore pass's noisy `reach`, plus a wider `softness_width`. Both noises are sampled in
  **world space** (`v_map`), so the two hexes flanking an edge get identical values at the shared boundary —
  the symmetric **no-seam** meeting of the two half-bands survives. A `RIVER_MIN_HALF_WIDTH` px floor keeps
  the noise from severing the band (and keeps it a legible hairline at far zoom).
- **MEANDER — a domain warp, not a distance bias.** Kept (it still bends the centerline rather than
  bulging/pinching a straight one) but **capped**, per the above: `RIVER_MEANDER_CELL = 0.9` hex radii,
  `meander_width = 0.22`. The warp cell is keyed to `hex_radius`, **not** the shared px-sized `noise_cell`
  (which would make the wander's character change with zoom and only fuzz the bank). It is warped ONCE per
  fragment in world space, so both flanking hexes warp the same point → no seam.
- **ONE river growing, not two spliced.** The two class textures are deliberately different art (`00_minor`
  light/shallow-over-gravel, `01_major` dark/deep), and untreated they meet as turquoise-next-to-near-black:
  a class change read as *two waterways joining*. Two shader fixes, no art edits: (a) the class **crossfades**
  — the pass tracks the best coverage per class and mixes the two layers by
  `smoothstep(-river_class_blend, river_class_blend, cov_major - cov_minor)`, so a hex carrying both classes
  dissolves one into the other over `class_blend_width` (a pure-class hex is unaffected: the loser stays at
  `-1e9`); and (b) `river_harmonize()` pulls both layers' luma toward `RIVER_DEPTH_PIVOT`
  (`depth_compress`) and their chroma toward `RIVER_SHARED_HUE` (`tint_strength`), preserving the luma
  ORDER — Minor stays lighter, Major deeper — which is the thing that should say *bigger*.
- **NAVIGABLE-CHANNEL pass (shader), right AFTER the Minor/Major pass** (so a Major feeding a navigable
  trunk composites into it), before canopy/peaks. **This is a RENDER-ONLY change — the sim is untouched.**
  Three parts:
  - **`blend_class` is `"flat"`, not `"water"`** (a *render* eligibility class with no sim meaning — the
    sim's `WATER | FRESHWATER` tags and water movement profile are unchanged). Treating it as land is
    correct — it takes the hex **out of the shore pass** (no beach, no foam) and lets it **blend softly
    into neighbouring flat land**, merging the corridor into the landscape.
  - **A navigable hex is a VALLEY with a river in it — its base renders the UNDERLYING biome, not a
    whole-hex bank** (rivers slice #3, `docs/plan_rivers.md` → "A navigable hex is a valley with a river
    in it"). The old whole-hex silty-bank base (`biome_array` layer 37) hid the land; now the hex body
    renders the **valley the river cut**, with only a **slim silty-bank skirt hugging the channel**. Two
    wire/shader pieces:
    - **The valley biome rides its OWN wire field + map.** `TileState.underlyingTerrain` (== the tile's own
      `terrain` on ordinary tiles, the preserved valley biome on a navigable hex) is decoded in
      `native/src/lib.rs` as `underlying_terrain`, ingested into `MapView.tile_underlying_terrain`, and
      packed into a NEW R8 `navigable_underlying_map` (built in `_rebuild_terrain_shader_maps` beside the
      river-channel map). The shader swaps the base sample from layer 37 to `navigable_underlying_map`'s id
      **only on a navigable hex** (`own_navigable`); everywhere else `base_layer == own_layer`, a no-op.
      **The `id_map` R channel STAYS terrain id 37** — that is the navigability signal the shader keys
      `own_navigable`/the channel pass on; only the *base texture* is swapped, never the id.
    - **The bank is a thin annulus riding the channel's distance field.** In the navigable channel pass, the
      silty bank (`biome_array` layer for id 37 — resolved via `river_navigable_terrain_id`, never hard-coded)
      is composited OVER the underlying base across an annulus just outside the water, out to
      `river_navigable_bank_half_width` beyond the channel edge (`bank_cov = best_cov +
      river_navigable_bank_half_width`, so it follows every arm/spur/taper/turn/mouth for free); the water
      channel then paints OVER the bank as before. Read across the hex: water (dist < navigable half-width) →
      thin bank gravel (out to + bank half-width) → underlying terrain. Config lever
      `rivers.navigable_bank_width` (**0.10**, hex-radius fraction → the `river_navigable_bank_half_width`
      uniform via `RIVER_DEFAULT_NAVIGABLE_BANK_WIDTH`).
    - The bank's base texture (`textures/base/37_navigable_river.png`) is still the **BANK ground**
      (placeholder: a copy of `09_floodplain`; real silty-bank art lands later) and its config `color` (the
      fallback solid + minimap pixel) is a bank tone. **The id-37 layer ALSO carries a per-terrain
      `blend_profile`** (`2.6 / 2.2 / 2.6` — see Edge Blending), retained for the bank's flat↔flat seams;
      judge the bank contrast on `blend_probe` state **17 (BANK)**. **The `blend_class` G-channel code stays
      "flat" (from terrain 37)** — since both the valley base and its flat neighbours are flat class, the
      flat↔flat blend fires and the navigable hex body merges seamlessly into the surrounding land with no
      hard hex seam (verified on `map_rivers_navigable.png`/`map_rivers_web.png`). Writing the underlying
      terrain's blend_class into the id-map for navigable hexes was NOT needed; a possible follow-up only if
      a valley biome of a *different* class (rugged) ever seams.
  - The shore pass additionally **skips a TRUE MOUTH edge — the one navigable edge whose `river_channel`
    exits through it INTO the water**: `blend_class` alone is not enough at the MOUTH, where the (now-land)
    bank would take a beach and the sea across from it would draw a **surf line across the river's mouth** —
    the river visibly walled off from the sea it drains into. A river meeting the sea is not a coast.
    **But the test must be per-EDGE, not "any navigable hex on either side"** — that over-broad exclusion
    (the original) also fired where a navigable river merely runs **ALONGSIDE** a lake without draining into
    it (a real @21,61 case: a one-hex `InlandSea` ringed by 3 navigable hexes, **none** of whose channels
    exit toward it), eating the lake's own shore ring on those edges and leaving a hard seam (glaring now
    that the bank renders the valley terrain, not neutral gravel). So the pass reads the sim-authored R8
    `river_channel_map` (the same mask the channel pass arms from; the shore loop's `dir` is already sim
    odd-r order, matching the channel bit index) and skips ONLY a true mouth — by the time the check runs
    exactly one side is navigable (flat/land) and the other genuine water, so it reads the channel from
    whichever side is navigable, toward the water: own navigable → its exit bit for `dir`; neighbour
    navigable → its exit bit toward us, `(dir+3)%6`. Everywhere else (alongside, no exit here) falls through
    to the normal coast, so the lake keeps its full ring and the valley/bank gets its beach. **This stays an
    EDGE-LEVEL exclusion, not a `shore_profile` entry** (see Shoreline → per-water-terrain shore profile):
    the profile is keyed on the **water** side — only a `CLASS_WATER` hex contributes one — and a navigable
    hex is land-class, so it can never supply one; the profile that would apply at the mouth is the
    **sea's/lake's**, which must keep its coast everywhere else. Dropping a mouth edge removes the whole
    chain at once (profile, waterline cross-fade, sand, surf, wisp all live under the pass's `best_d` guard)
    and does so symmetrically from **both** hexes' frames, so no half-drawn coast survives on one side of it.
    Judged on `map_rivers_lake_alongside.png` (the alongside lake keeps its ring) vs `map_rivers_mouth.png`
    (the true mouth stays open).
  - The **channel** (`river_tex` **layer 2**, `textures/rivers/02_navigable.png` — the deep teal water that
    used to be the terrain's base) is TWO kinds of stroke, unioned by the **max-coverage (min-distance)**
    pick — the same trick that rounds the Minor/Major corner joins, here fusing them into one connected
    body with rounded junctions for free:
    - **TRUNK ARMS**, at the channel's own (navigable) width: hex **CENTRE → the MIDPOINT** of each side
      **`river_channel` says the river flows out through** (`(mask >> dir) & 1`). **The connectivity is
      SIM-AUTHORED, not inferred from the neighbouring terrain** — see the third wire primitive above. The
      old rule (arm every navigable / water / `RiverDelta` neighbour) is exactly what drew the **WEB**:
      adjacent navigable hexes that are not consecutive on the chain got cross-linked, and the corridor
      filled with triangles. The mask also carries the mouth (the last hex's unmirrored exit into the sea /
      delta), so the river still does not dead-end a hex short of the sea. The arm needs **no neighbour
      fetch** — only the neighbour's CENTRE, which is pure math — so it also draws correctly at the map
      border and across the wrap seam.
    - **INFLOW SPURS**, at the arriving tributary's **own Minor/Major width**: hex **CENTRE → the CORNER**
      named by `river_inflow` (all 6 checked; a mask bit needs no neighbour fetch, so it spurs even at the
      map border / across the wrap seam). The spur wears the tributary's class art and **crossfades** into
      the channel over `class_blend_width` — the edge pass's Minor→Major crossfade, for the same reason:
      one river growing, not two waterways spliced. **This centre-hub form is used for a MID-CHAIN junction
      (`>= 2` exits with an inflow) — a hex the trunk passes THROUGH, whose centre is genuinely on the flow.**
    - **A CHAIN HEAD FED BY A TRIBUTARY routes STRAIGHT from the inflow corner to its single exit, NOT via
      the centre** (the notch fix). On a head (`exits == 1`) with an inflow, the centre-hub form draws the
      inflow as a centre→corner spur and the exit as a centre→edge-midpoint arm; when the inflow corner and
      the exit side flank the **same** vertex, that union is `inflow_corner → centre → exit_mid`, which
      **DOUBLES BACK into a NOTCH / inverted-V at the corner** (reads as "the tributary hooks into the wrong
      corner"). So a head-with-inflow instead draws ONE tapered segment per inflow corner — `inflow_corner →
      exit-midpoint` — narrow (the tributary's own width) at the corner, swelling to the full navigable width
      at the exit edge (the head taper, now laid along the true flow line), with the tributary art
      crossfading to the channel art along it (`head_class_mix`). `t_head` is the UNWARPED projection, same as
      the arm loop, so the exit edge still lands on exactly `navigable_half_width` and the downstream hex
      meets it with no step. It **rides the same `best_cov`** the bank annulus reads, so the slim bank follows
      the new flow line for free. Multiple inflow corners on one head (a Major+Minor confluence, the join
      frame) draw one segment each, unioned into a Y at the exit. Judged on `map_rivers_notch.png` (inflow at
      the bottom vertex, single exit on the adjacent SW side — the exact geometry that notched).
    - **HEAD TAPER — a trunk does not spring to full width at a hex centre.** On the **first hex of a
      chain** — **gated on the `river_channel` EXIT COUNT (`<= RIVER_CHANNEL_HEAD_MAX_EXITS`, i.e. 1), NOT on
      `river_inflow != 0`**: a head has only its downstream link; a mid-chain hex has its upstream one too (2),
      a confluence 3. Since the drainage network a tributary hands over at ANY navigable hex's vertex, so an
      inflow-gated taper would **pinch the full-width trunk to the tributary's width at a mid-chain junction's
      centre and swell it back out on both sides — a visible HOURGLASS in mid-channel.** The **SPUR stays
      unconditional**: it carries the tributary from the hex centre out to its vertex, and a mid-chain junction
      needs it MORE (without it the tributary dead-ends at the vertex, short of the arms, which only reach the
      edge midpoints). Judged on `map_rivers_midchain.png`. On a head, the arms **start at the half-width of the
      WIDEST class feeding in** (max over the 6 inflow corners — Major wins if any Major lands, and the
      sim already stores the widest class per corner) and **swell to the full navigable width by the hex
      EDGE**: `half_w = mix(inflow_half_width, navigable_half_width, pow(smoothstep(0,1,t), head_taper_curve))`,
      `t` = the arm's own centre→edge-midpoint projection. Without it a hairline Minor arrived at a vertex
      and was a great river a few px later — a jump-cut, not a river. Any hex that is **not** a chain head (or
      is a head with no tributary) is **unchanged**: `inflow_half_width` stays the navigable width and the mix
      is a no-op — no extra per-hex branching.
      **`t` is taken from the UNWARPED point** (unlike the distance-to-centerline `t`, which must use the
      meander-warped one), and that is load-bearing: every fragment on the shared edge projects to
      **exactly `1.0`** on the arm axis (the edge line's projection onto the arm direction is the apothem,
      whatever the lateral offset), so the taper lands on **exactly** `navigable_half_width` where the
      next, constant-width navigable hex takes over — no step, no notch at the head's downstream edge. The
      warped point's projection would wander by the meander amplitude and leave one. A hex with **>= 2 channel
      exits** is mid-chain and keeps the CONSTANT full navigable width, whatever its inflow. Width is a scalar
      field of world position here, the same as `river_width_mod` / `river_bank_wobble` (both also sampled
      unwarped), and the organic machinery rides **on top of** the tapered base width, so the continuity
      guarantees survive. The taper also makes the **spur→trunk join seamless**: the trunk now leaves the
      centre at the same width the spur arrives there with.
    - **An arm is NOT keyed off `river_edges`** — that was the fat-teal-blob bug. An edge river runs ALONG
      a side; it does not flow through the side's MIDPOINT, and a trunk head can flank two or three river
      edges, so the mask-armed rule drew three fat centre→midpoint arms **at the trunk's width** and the
      hex filled with water. Water enters a trunk hex at a **vertex**, which is what `river_inflow` names.
    A navigable hex with **zero arms** (the sim should never emit one; an inflow spur is not an arm) draws
    a centre **blob** rather than a hex of bare bank, and `MapView._warn_orphan_navigable_rivers`
    `push_warning`s it — now a pure MASK test (no `river_channel` exit **and** no `river_inflow` = water
    neither enters nor leaves), mirroring the shader's arm rule; keep the two in step.
  - It reuses the **same organic machinery** as the edge pass — the `river_meander_warp` domain warp, the
    low-frequency `river_width_mod` swell, the `river_bank_wobble` ragged bank (all three factored into
    shared shader functions rather than copied) — and `river_harmonize`, so the trunk reads as the same
    river grown bigger. All noise is sampled in **WORLD space**, which is exactly what makes the channel
    **continuous across adjacent navigable hexes**: both hexes warp the same point and read the same width
    at their shared boundary, so the half-channels line up with no seam, pinch or gap. The **spurs ride the
    same three**, which is why a tributary's band arrives at the vertex already warped exactly as the edge
    pass warped it on the far side — the two meet without a notch.
- **Config levers** (`terrain_config.json` → `rivers` block): `minor_width` / `major_width` /
  **`navigable_width`** (the channel HALF-width as a fraction of the hex radius — `0.14`: clearly the
  biggest water on the map, but **only somewhat** wider than Major's `0.09`. It shipped at `0.24` and read
  as a flood filling the hex, which is the puddle read this whole pass exists to kill; softness / meander / width-variation /
  bank-noise / flow-speed are **shared with the edge classes**, not duplicated per class) /
  `softness_width` / `meander_width` / `bank_noise_width` / `class_blend_width` (fractions of the hex radius
  → px uniforms, computed in `_update_terrain_shader_quad` exactly like `blend_width` / `canopy_overhang`),
  the unitless `width_variation` / `tint_strength` / `depth_compress` / **`head_taper_curve`** (the
  exponent on the head taper's smoothstep — `0.8` ships, i.e. swell slightly EARLY; `1.0` = plain
  smoothstep, `> 1` holds the tributary's width longer then flares. An exponent, never a width, so it
  cannot disturb the exact navigable-width match at the hex edge), plus `texture_scale`,
  `river_min_radius` (the LOD floor), and `flow_speed`. Fallbacks are the `RIVER_DEFAULT_*` consts in
  `ui/TerrainRenderer.gd`.
- **River LOD is DECOUPLED from the blend LOD** (own `rivers_lod_enabled`, `radius ≥ river_min_radius`,
  default 3.0 ≪ `EDGE_BLEND_MIN_RADIUS`) — a river is a landmark you navigate *by*, so it must survive
  zooming out; the mipmapped/trilinear river array keeps the thin band stable (no shimmer).
- **`set_highlight_rivers`** (the Map tab toggle) survives, repointed from the deleted polyline draw to the
  shader's `river_highlight` uniform.
- **TEXT surfacing — `ui/RiverEdges.gd`, ONE formatter, two surfaces.** Seeing the water isn't knowing
  which SIDES carry it — which is exactly what a crossing penalty will key on. `MapView._tile_info_at`
  copies the mask onto the tile dict as `river_edges` (from `tile_river_edges`; **deliberately NOT in
  `FOW_DISCOVERED_HIDDEN_KEYS`** — a river is permanent geography like the terrain label or a discovered
  Wondrous Site, so a remembered tile still reports it; never-seen tiles are already covered by the
  `unexplored` redaction), and both the **Tile card** (`Hud._tile_terrain_lines`, with the other
  terrain-intrinsic rows, before the FoW discovered early-return) and the **map hover tooltip**
  (`Hud.show_tooltip`, after `Terrain:`) render it from the same `RiverEdges.summary_lines(mask)` call.
  `RiverEdges` owns the vocabulary (classes + direction names + bit widths as named constants) and emits
  **one line PER CLASS, Major first** — `Major River: NE, NW` / `Minor River: SW` — plain `Key: Value`
  rows needing no `_format_detail_bbcode` tint case, and an **empty array on a riverless tile** so no
  empty label renders. It keeps **two direction orders**: the sim's `HEX_NEIGHBOR_OFFSETS` order
  (clockwise from E — the wire contract) DECODES the mask, and a **compass display order** (clockwise
  from NE) lists the directions within a line, because a compass reading is what a player parses.
  ui_preview: `river_tile_both` (two-class) / `river_tile_minor` (single-class) / `river_tile_none` (no row).
- **Caveat — rivers are shader-only** (same as canopy/peaks): the blend-OFF **per-hex CPU path** renders no
  rivers. That is the reference/fallback path only; the live client runs blend-on.
- Verify via `tools/map_preview.gd` State **rivers** → `map_rivers.png` (a Minor→Major edge river wandering
  west→east with corner turns, joining a NavigableRiver chain that turns corners of its own and drains to
  the eastern sea — **with a real InlandSea lake in the same frame as the control**: the lake keeps its
  beach + surf, the navigable hexes have neither, and the two must read as obviously different things) +
  `map_rivers_seam.png` (edge/corner close-up framing the class change: the band hugs the EDGE, joins are
  rounded, the two half-bands meet with no seam down the middle, Minor grows into Major) +
  `map_rivers_navigable.png` (the trunk: the Major edge river flowing INTO it, the corner turns, and the
  channel CONTINUOUS across adjacent navigable hexes) + `map_rivers_mouth.png` (the channel reaching open
  sea + its delta lobe — no dead-end, and no surf line across the mouth) +
  `map_rivers_head_minor.png` (the HEAD TAPER's own frame: a second, one-hex navigable branch fed by a
  **Minor tributary only** — its arm must start hairline at the centre and swell to the full channel width
  by the shared edge with the trunk, with **no step** there; the Major+Minor head in `map_rivers_join.png`
  is the other half of the test, starting at the **wider** — Major — width) +
  `map_rivers_farzoom.png` (decoupled LOD). The fixture generates the edge chain as the **boundary of a
  region** (hexes north of a bank row `f(x)`), which is contiguous by construction — no gaps — and turns a
  corner at every step; the navigable chain then WALKS `RIVER_NAV_STEPS` (E/SE/E/NE/E) out to the sea, so the
  trunk's arm/junction geometry is actually exercised. The river is kept in the map's **upper rows**
  deliberately: the map is cover-fit and that fit is the zoom FLOOR, so on a window wider than the grid's
  aspect the lower rows cannot be scrolled into view at all. **`RIVER_PATTERN` must stay a mostly-MONOTONE drift**: an up-down-up staircase makes
  the boundary wrap 4+ sides of the same hexagon, manufacturing a honeycomb that real hydrology (a downhill
  walk on the corner lattice) never produces — the original fixture did exactly that and made the render
  look far worse than it is.

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
- API: `card_title` / `set_card_title()`, `get_content()`, `hotkey_hint`
  (renders the toggle key in the header, e.g. `"Terrain Types (L)"`; leave empty
  for panels with no show/hide hotkey), and `set_title_color()` — for a card whose
  TITLE is itself a signal rather than just a name (today only the Telling panel,
  whose title and accent age with the narrator's medium). Most cards should leave
  the title on the shared `HudStyle.INK`.
- Replaces the bespoke `ui/AutoSizingPanel.gd` height math — the dock's own
  `ScrollContainer` owns overflow, so cards only size to content. A card whose
  content grows without bound (the command feed) additionally caps itself against
  the dock via the shared `ui/hud/DockScrollFit.gd`; the Telling panel grows to
  fit its own bounded page capped at `PAGE_MAX_HEIGHT` and needs neither.

### PanelDock (`ui/PanelDock.gd`)
Ordered controller for one dock region's `VBoxContainer`. Panels `add(panel,
priority)` to register; the dock reparents them in priority order. Visibility is
data-driven — `set_relevant(panel, false)` (or `panel.visible = false`) removes a
panel from layout flow and the stack reflows with no gap. Hud builds `left_dock`
and `right_dock` in `_ready()`.

**The current roster:** LEFT = Tile 10 · Stockpile 20 · Command feed 30 (the feed
`set_relevant(false)` by default too — six read-only receipts and no verbs, toggled
by `R` via `Hud.toggle_command_feed`, persisted to the same `[hud_panels]` section).
RIGHT = **Telling 10** · Victory 20 · Terrain Types 30, the last two
`set_relevant(false)` by default and toggled by `V` / `L` (`Hud.toggle_victory` /
`toggle_legend`, both persisting to `user://narrative.cfg` `[hud_panels]` — the
same file the voice register and the Telling panel's collapsed state use; do not
add a third prefs file). A card that ships hidden must go through `set_relevant`
rather than a bare `visible = false` so the dock reflows without leaving a gap.

**Scroll behaviour:** on construction the dock disables **horizontal** scrolling
on its enclosing `ScrollContainer` and zeroes the stack's horizontal minimum, so
the stack always fills the dock width and content wraps to fit rather than
spilling under a sideways scrollbar (which reads as unpolished for a game HUD).
**Vertical** scroll mode is *not* set by PanelDock — it is configured per dock in
the scene (`HudLayer.tscn`); both docks use `AUTO`, so a scrollbar appears only
when the stack actually overflows.

**Migration status:** `TilePanel` (the one selection card), `CommandFeedPanel`, `TellingPanel`, and
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
  **settlement stage**, which the sim resolves from `settlement_stage_config.json`: the **bundled
  sprite** for its `settlement_stage_id` where we have art (`StageSprites` — see its row above; the
  sprite is tried BEFORE the empty-glyph placeholder branch, which returns early), else the opaque
  `settlement_stage_icon` emoji (⛺ nomadic / 🛖 camp / 🏘️ village). Either way at
  `BAND_STAGE_GLYPH_SIZE_FACTOR` via the shared drop-shadow helpers (`_draw_marker_sprite` /
  `_draw_marker_glyph`), **no faction ring or disc**. Ownership is carried by a **faction-colored nameplate banner** (`_draw_band_banner`,
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

- **The selection card — ONE card, ONE list, ONE drawer** (`Hud.gd`,
  `docs/plan_tile_panel_layout.md`; this SUPERSEDES the earlier split Tile + Occupants
  cards). A populated hex used to ask the left dock for ~1450px of content — two inline,
  permanently-expanded compose blocks — so the action buttons fell below the fold. The hex is
  now **one left-dock `PanelCard`** (`TilePanel`, priority 10, title = the coordinates):
  - **`%TileChips`** — a pinned `HFlowContainer` of the tile's STANDING CONDITION, so the facts
    you reason with while composing never scroll away: Sight (`_sight_value_color`, SIGNAL when
    live) · Habitability (`TileHabitability.rating_for`/`color_for`) · Climate (neutral INK_DIM —
    informational, never the warning palette) · Tags (skipped when empty/`none`) · Site. **Each
    chip is skipped when its field is absent**, exactly as the equivalent row is, so a rehydrated
    tile never shows an invented rating; on an Unexplored hex ONLY the Sight chip renders. Chrome
    comes from `HudStyle.chip_stylebox(border)` — the palette owns it, never an open-coded box.
    **A chip FACE is a word, not a sentence** — `Remembered — not in sight now` was the widest
    element in the strip, so the Sight chip reads `In sight` / `Remembered` / `Unexplored`
    (`_tile_sight_chip_value`) and the full sentence moves to `_make_chip`'s optional `tooltip`
    (the only chip that takes one; the rest stay mouse-transparent). One value behind both forms.
    **THE CHIPS REPLACE THOSE ROWS, THEY DO NOT ACCOMPANY THEM** — see `_tile_terrain_lines`.
  - **`%SubjectList`** — the selectable list, with **the LAND as its first row**
    (`_build_land_row`, no group header) above the `Bands (N)` / `Wildlife (N)` sub-groups. The
    land is the same KIND of thing they are — a subject on this hex you can put workers on. Its
    label is the BIOME name, its glyph the tile's food-module icon (`FoodIcons.for_site`, the
    same one the map marker draws) or the neutral `◈`, its dot the patch's ecology tier, and its
    meta the shortest true form: `N 🌾` staffed · else the module label · else `No forage` (gated
    on the module KEY, never its `"None"` label). Selecting it emits
    **`roster_occupant_selected("land", -1)`** — an ADDITIVE third kind on the existing `(kind, id)`
    contract (Main forwards it blindly to `MapView.select_occupant`, so Main needed no change). It
    moves no ring — there is no occupant, and the hex outline already marks the tile — but MapView's
    `"land"` branch **clears `selected_unit_id` / `selected_herd_id`** (leaving `selected_tile` and
    `cycle_index` alone, so map re-click still cycles the band stack from where it was). **That clear
    is what makes the land selectable at all on an occupied hex:** `refresh_selection_payload`
    answers `kind: "unit"` for as long as `selected_unit_id >= 0`, so without it the per-snapshot
    refresh restored the band and the tile branch was never reached.
  - **`%SubjectScroll` / `%SubjectBody`** — the ONE drawer, filled by whichever row is lit and
    **height-capped** via `DockScrollFit.fit_height` against the room left in the dock
    (`SUBJECT_DRAWER_MIN_HEIGHT` floor), so a crowded hex scrolls INSIDE the drawer instead of
    dragging the dock. Only one drawer is ever open — rows are ~30px, a compose block is 300+, so
    making the drawer the scarce shared resource is what bounds the card. The fit **waits a whole
    frame** (not just `call_deferred`): the drawer's content height is a function of its width, so
    a measurement taken before the new subject lays out reports the previous one's wrapping. A 1px
    `HudStyle.hairline_stylebox()` rule (`%SubjectDivider`, the same LINE_SOFT weight
    `header_stylebox` draws under a title) marks where the list ends and the drawer begins —
    without it the drawer's first row runs straight on from the last wildlife row.
  - **The LAND drawer renders only what a CHIP CANNOT CARRY** (`_tile_terrain_lines`, whose ONE
    caller is `_render_land_drawer` — the map hover tooltip builds its own text): Height · the
    river lines · Pasture / Pasture ecology · Forage / Forage biomass / Ecology · Cultivation /
    Field · plus the two FoW sentences (`Last seen …` / `Not yet scouted …`), which are statements,
    not conditions, and have no chip. Sight / Habitability / Climate / Tags / Site are the CHIPS'
    and `Biome` is the land ROW's own label, so printing any of them here restated the strip
    verbatim (§8's "no restated identity"). The `TILE_SIGHT_KEY` / `Habitability` cases in
    `_format_detail_bbcode` stay — it is a shared key→tint registry every detail surface consults.
  - **THE DRAWER IS THE READ STATE; THE COMPOSE SHEET IS THE WRITE STATE** (Part 2 of
    `docs/plan_tile_panel_layout.md`, §10-§17). Capping the drawer bounded the card but did not make
    it SMALL — the two compose blocks were still ~270px of always-expanded picker sitting permanently
    in a column that also has to show the land, the roster and the detail rows. Composing is **modal
    by nature** (open, decide, commit, done), so `%ForageAssignControls` / `%HerdAssignControls` now
    end at a one-line **standing-assignment summary** + an **`Assign foragers ▸` / `Assign hunters ▸`
    / `Assign herders ▸`** button (`_build_forage_drawer_actions` / `_build_herd_drawer_actions`),
    and the block itself renders into the floating `ui/hud/ComposeSheet.gd`. `%AllocationPanel` stays
    INLINE (for an expedition it is two buttons and a callout).
    - **The builders were NOT reparented — they were PARAMETERISED.** `_build_forage_assign_controls(
      tile_info, target)` / `_build_herd_assign_controls(herd, target)` take an explicit target
      container, because reparenting a `%Name` node silently clears `unique_name_in_owner` and breaks
      every lookup in the owner script (`PanelCard`'s contract note). Every rebuild path (stepper
      tick, policy click, band-picker change) re-runs the same builder against the same target, so the
      compose state members (`_forage_assign_*` / `_hunt_assign_*` and the autofill one-shots) are
      untouched — the sheet is a different HOST, not a different state model. **Gate-reason lines
      travel WITH the picker**: they explain the greyed buttons, so they belong beside them.
    - **Extend-pen stays in the DRAWER.** It is a one-click standing action on a built pen, not a
      compose flow; hiding it behind a sheet you must open first would be worse. So
      `_build_herd_drawer_actions` renders it (or the "Fencing N%" badge) directly.
    - **The standing summary reuses `SourceForecast.source_yield_readout` — it never recomputes a rate.**
      `♻ 3 foragers · +2.74 /turn`, policy glyph from `FoodIcons.for_policy`, and the SAME two
      INDEPENDENT flags a Band-panel Current-actions row wears: the ⚠ overdraw (ecological, the
      sim-answered `overdraws`) and the `· only N of M working` overstaff note (labor). `has_yield` is
      the one key the readout needs that is not on the wire assignment, so it is set locally; every
      number comes off the assignment the sim sent. Unstaffed → no summary row, just the button.
    - **LIFECYCLE.** Opens on the drawer button; one sheet at a time. Closes on commit, the `✕`, a
      catcher click, `Esc`, a **selection change** (`show_*_selection` / `_select_roster_occupant` /
      `_on_land_row_selected` / `clear_selection`) or a **targeting flow starting** (`_on_move_band_
      pressed` / `_on_send_expedition_pressed` / `_on_pick_quarry_pressed` — a sheet floating
      over the map while the player is asked to click a hex is a trap). **A SNAPSHOT MUST NOT CLOSE
      IT** — `reapply_selection` runs every turn and closing would make the sheet unusable under
      autoplay; `_refresh_compose_sheet` (called from `_render_subject_drawer`, the snapshot
      chokepoint, and again from `update_band_alerts` so a staffing change lands too) re-renders it
      IN PLACE and closes only when the composed subject is actually gone. `close_compose_sheet` is
      idempotent, and `ComposeSheet.closed` is what clears `_compose_kind`/`_compose_subject`, so the
      two can never disagree about whether a sheet is open.
    - **ESC PRECEDENCE.** `Hud.is_compose_sheet_open()` is checked BEFORE `is_targeting_active()` —
      the sheet is the innermost surface. The chain is `Main.escape_claimant(pause_open, compose_open,
      targeting)`, a pure static extracted so the ORDER is assertable without standing up the app
      scene; `Main._unhandled_input` matches on its answer.
    - **Nothing is re-derived.** Every yield, forecast, ceiling and gate reason comes from the same
      call it came from when the block lived in the drawer, and the forage range gate / herd
      local-vs-expedition branch still read the **selected band's** position, explicitly threaded.
  - `_selected_subject` (`SUBJECT_LAND|UNIT|HERD`) says which KIND of row is lit;
    `_selected_unit`/`_selected_herd` stay authoritative for WHICH. **The auto-select rule is
    unchanged plus a land fallback**: first roster unit → else first herd → else the land — but it
    fires **only where the player has not already chosen on THIS hex**. `_subject_choice_tile`
    (set by `_select_roster_occupant` and the land-row click, `(-1,-1)` = never) is what tells a
    fresh hex from a decided one: choosing the LAND row clears both occupant dicts, so without it
    the per-snapshot `reapply_selection("tile", …)` re-ran the default and **stole the selection
    back to the first band**, making the land unselectable on any occupied hex. A new hex has
    different coords, so the default is preserved exactly. **This guard is only half the fix** — it
    covers the `reapply_selection("tile", …)` path, which on an occupied hex is only ever reached
    because the land row also clears MapView's own occupant selection (see `%SubjectList`). Guarded
    by ui_preview `tile_panel_land_sticky`, which drives the REAL path — it instances MapView, wires
    the two signals Main wires, clicks the hex, clicks the land row and feeds back whatever
    `refresh_selection_payload` answers (never a hardcoded `"tile"`, which would assert a path the
    bug cannot reach). Verified to FAIL with MapView's `"land"` branch removed.
  - Each occupant row is a `Button` hosting a mouse-transparent
  HBox — a selection accent, a **vitality dot**, name, size, and (bands) an
  activity glyph; a **wildlife** row reads **species + its STAFFING** — the hunters on the herd in
  the same `<count> <glyph>` form the land row uses (`🦌 Red Deer   1 🏹`, twin of `◈ Savanna   2 🌾`),
  with the unworked-but-huntable form `0 🏹` and *no* meta at all on a non-huntable herd. The
  **size class** moved into the herd drawer's first row (`Size: Big game`) because the row's one
  meta slot now belongs to the count. **A detail row never restates what its
  roster row already shows** (the same rule the Band/City panel header follows). The roster
  row IS the identity line — name + size/staffing — so every drawer dropped
  the rows that echoed it: band → `Unit` + `Size`; herd → `Herd` / `Species`
  (the name appeared three times); expedition → `Unit` + `Party` (`Party`
  printed the same `size` field the row's meta shows). **THE FAUNA ID IS A DATABASE KEY AND IS
  NEVER RENDERED** (`game_fowl_27` means nothing to a player and crowded out the two things that
  do). It briefly rode the row as a dim meta on the theory that the command feed named herds by
  it — the right fix was to stop the FEED leaking it (`Main._on_hud_send_hunt_expedition` now
  notes `fauna_label`, the species, while the command line keeps `fauna_id`), not to teach the
  player the key. It stays **data**: the row's `pressed` bind and every `assign_labor` / `tame` /
  `send_hunt_expedition` address the herd by it. Renders of it elsewhere are **fallbacks only**
  (`SourceForecast.herd_display_name` / `_herd_label_for_id` reach for `id` only when species AND label are
  both missing) — never the normal path. What's left in a drawer is only what the row can't show — herd: Size / Biomass / Ecology /
  Husbandry / Corral / Position; expedition: Mission / Target / Policy / Phase / Carried /
  Position. **Expedition `Policy` / `Phase` keep their WORDS** — the compact
  Active-expeditions row is where the glyph vocabulary belongs; the drawer IS the
  disclosure. In the drawer,
  `%OccupantDetail` is the selected occupant's
  **detail** for **herds/expeditions** (`_herd_summary_lines` +
  `%HerdAssignControls`; expedition → `_build_expedition_panel` into
  `%AllocationPanel`). **Player-band detail relocated into
  the dockable `BandCityPanel`** (see **Band/City dockable panel** below): the list
  still lists the band, but its summary + labor allocation render in the panel — the drawer
  renders the one-line `BAND_PANEL_POINTER_TEXT` pointer instead, since an empty drawer is now
  VISIBLE furniture and would read as a rendering fault. **One order stays here beside that pointer:
  a ghost `Move`** (`_build_band_move_actions`, `docs/plan_tile_panel_layout.md` §18) — repositioning
  is a MAP action and the player is already on the map with this hex open, so crossing to another
  panel to give it is the wrong shape. It is wired straight to `_on_move_band_pressed`, which
  resolves through `_resolve_assign_band()` and therefore targets **the band selected in THIS list**
  — the whole point on a hex carrying several. It shares the `%AllocationPanel` host with
  `_build_expedition_panel` / `_build_allocation_panel`, which are mutually exclusive branches, so
  the no-panel fallback path's own Orders `Move` is never doubled (asserted in ui_preview). Player
  resident bands only, and `Clear all` is deliberately NOT here — returning every worker to idle is
  a heavier action that belongs beside the labor allocation it clears. `BandCityPanel` and
  `_build_allocation_sections` are untouched. Selecting a row (`_on_roster_row_selected`) re-homes the
  selection and emits `roster_occupant_selected(kind, id)`; **Main forwards it to
  `MapView.select_occupant`, which moves the map selection ring** (sets
  `selected_unit_id`/`selected_herd_id`) with no hex click. A fresh tile click
  auto-selects the first occupant through the same path. The **vitality dot is
  unified** across map/roster/drawer: a band's dot uses `BandFoodStatus.color_for_turns`
  (`turns_of_food` → green/amber/red), a herd's uses `_ecology_tier_color`
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
    "current actions" report — a `Population <size> · Workers <working_age> (Idle <n>)` line (spells
    out that only the ~16 working-age labor, not the 30 people — children/elders are dependents;
    `WORKERS_HEADER_FORMAT`, idle from `_effective_idle` so it counts optimistically), a
    **Current actions** section with one `−/+` **worker-stepper** row per staffed Forage tile / Hunt
    herd (from the cohort's `labor_assignments`; an empty-state hint when none). **A Forage/Hunt row is
    TWO lines** (the `status_line` opt-in on `_build_worker_stepper` → a `VBoxContainer`; the
    Scout/Warrior role rows and the compose steppers stay the single-line `HBoxContainer`): **line 1** is
    the resource-glyph title + tile/species (`🌰 Forage (27, 26)`) beside the `−/+` stepper, keeping its
    click-to-jump link; **line 2** is an INDENTED, smaller (`ALLOC_SECTION_FONT_SIZE`), `HFlowContainer`
    that WRAPS carrying the yield + policy glyph + status glyph + any ⚠/overstaff/wasted notes
    (`+0.48 /turn  ♻  ●  · only 2 of 5 working`), so the row reads narrow and never forces the panel
    wider. `_build_two_line_stepper` / `_build_row_name_label` / `_build_status_part` /
    `_add_stepper_controls` factor the title/stepper/status parts so both forms share them. **A row
    states its policy and its status as GLYPHS, not words** — the old
    `[sustain]` / `· pending` word-tags were long and, for pending, redundant with the amber tint.
    Both come from the one glyph registry, `FoodIcons` (`for_policy` / `for_status`; see the
    **action-status vocabulary** header block in `Hud.gd`), and the WORDS move into the row tooltip
    (policy name + its existing `FORAGE_POLICY_HINTS`/`LOCAL_HUNT_POLICY_HINTS` behaviour hint — a
    worked source row is always a RESIDENT band's, so the hunt side reads the local set, never
    `SEND_HUNT_POLICY_HINTS`, whose payoffs differ; `_policy_hint` is the one lookup), plus the
    status in words), composed WITH the tooltip the row already carried (yield readout, overstaffing
    explanation, click-to-focus hint). Two orthogonal layers: **status** = what the action is doing
    (a confirmed local forage/hunt row has no sim phase — it is simply `working` `●`), and
    **`pending`** = a state of the ORDER (composed locally, not yet acknowledged; it rides on ANY row,
    is a modifier rather than a phase member, wins the glyph slot with `○`, and keeps the amber label
    tint). The policy glyph is read off the assignment's `policy` field (populated for forage too); an
    an assignment whose policy is unset falls back to no glyph. **Each source row headlines its per-turn food yield**
    (`… +0.31 /turn`, the assignment's `actual_yield`), with a WARN-tinted `⚠` **overdraw flag** driven by
    the **sim-answered `overdraws` bool** on the assignment (`LaborAssignment.overdraws`, policy-driven:
    `!managed && policy.overdraws()`, false for Sustain and managed/investment sources; decoded in
    `native/src/lib.rs` beside `wasted_yield`). This **replaced** the old client-derived `actual >
    sustainable + ε` test on the confirmed rows, which **false-positived on a hunt's kill turn** — cashing a
    banked whole animal spikes `actual` above the steady `sustainable` even under Sustain, so the row wrongly
    flashed ⚠. A Sustain source reads `… · renewable` (no flag); a Surplus/Market/Eradicate forage patch or
    an over-hunted herd trips the flag. A `tooltip_text` spells out actual-vs-sustainable. (The **compose
    previews** still derive it from the steady forecast via `_is_overdraw` — there is no assignment, hence no
    `overdraws` field, at compose time, and the forecast is not a lumpy `actual`.) **Each source row also flags overstaffing** — a
    WARN-tinted `· only N of M working` note (`OVERSTAFF_NOTE_FORMAT`) when `workers > workers_needed`
    (and `workers_needed > 0`), i.e. the source's take was capped at its ceiling so the surplus workers
    idled HERE and should be reassigned; the `tooltip_text` (`OVERSTAFF_TOOLTIP`) explains it. This is
    **orthogonal to the ⚠ overdraw flag** and deliberately NOT the same glyph: overdraw is *ecological*
    (taking past regrowth), overstaffing is *labor* (wasted workers) — a source can be overstaffed while
    perfectly sustainable (every policy has a ceiling), or overdrawn while fully used. `workers_needed
    == 0` (rehydrated, or a pending optimistic assign) means "unknown" → no note, never a
    wrong one.
    **ONE yield row per rung — each rung gets the row that informs ITS decision, never both.** On the
    **local hunt** the EXTRACTIVE four render `_local_hunt_preview_bbcode` (the crew's honest carry-aware
    delivered take, ANIMALS-first — `≈1 Red Deer/turn` — PLUS the sustainability verdict `· renewable` /
    `⚠ overdraws the herd`, and a WARN `· ⚠ N% wasted` suffix when a kill can't be carried; see the
    animals-first preview note below) and the INVESTMENT rung (Corral)
    renders `_forecast_yield_row` (`Preparing: +0.23 → then +1.05` — the dip→payoff deal, which a single
    rate structurally cannot express; Corral draws sustainably, so no overdraw verdict is lost).
    **Forage now mirrors the hunt split** — its EXTRACTIVE rungs render `_local_forage_preview_bbcode`
    (the plant twin, a bare rate + `· renewable` / `⚠ … — overdraws the patch`; no animal rhythm, so no
    waste suffix) and only its INVESTMENT rungs (Cultivate/Sow) keep `_forecast_yield_row`. Rendering
    both on a hunt was a merge artifact: the flat `per_worker_yield`/`ceiling_*` scalars and the
    `hunt_policy_ceilings` list are **two views of ONE sim hunt model** and agree numerically (verified:
    both give +0.54 on a Market take — the redundancy was measured before it was removed, not assumed), so
    the second row added no information and **argued with the first** — a HEALTHY-green "Expected yield"
    directly above a WARN-amber "⚠ overdraws the herd" for the same number. (The two overlapping wire
    representations should be collapsed to one server-side; tracked separately.) Both the ⚠ and the note are rendered by `_build_worker_stepper` (`warn` / `note` params)
    off one `SourceForecast.source_yield_readout`, so Forage and Hunt rows share the logic.
    **Each source row leads with its resource glyph** — `FoodIcons.for_site(module)` for a Forage
    row (resolved from `_food_module_by_tile`, the snapshot `food_modules` array pushed by `Main` →
    **`Hud.update_food_modules`**, keyed by tile) and `FoodIcons.for_herd(species)` for a Hunt row —
    the SAME icon the map marker draws, so a source reads identically in the panel and on the map. An
    unresolvable module renders the row bare (no fallback sprig).
    **Each source row's LABEL is clickable — it jumps the map to the source being worked.**
    `_build_worker_stepper`'s optional `on_focus_source` Callable turns the label into an inline link
    Button (`HudStyle.apply_link_button` — plain at rest, hover tint + `SIGNAL` text + pointing-hand
    cursor, a far tighter padding than the boxed ghost chrome); it is a *separate child* from the
    `−`/`+` stepper, which is untouched, and the count stays right-aligned. Both handlers route
    through `_focus_labor_source` — the SAME path the Active-expeditions rows and the turn-orb
    "Jump →" use: `alert_focus_requested` → `MapView.focus_and_select_tile`, plus (herd only)
    `roster_occupant_selected` → `MapView.select_occupant` so the herd's own drawer opens rather than
    whatever occupant the hex auto-selects; `_panel_band` is restored afterwards, so focusing a hex
    that hosts another band can't hijack the panel. **Forage** jumps to the assignment's
    `target_x/target_y` (a patch is a fixed tile). **Hunt** deliberately does NOT — herds MIGRATE, so
    `_focus_hunt_source` resolves the herd's **live** tile from `_world_herds` via `_find_world_herd`
    (the Hud mirror of `MapView._herd_by_id`, which the hunted-herd ring already resolves through),
    falling back to the assignment target only when the herd is unknown. `_world_herds` is the
    snapshot `herds` array, pushed each snapshot by `Main` → **`Hud.update_herds`**; it also backs
    `_herd_label_for_id`'s new fallback, so an off-hex hunted herd reads "Red Deer" instead of the raw
    `game_deer_07` id. **Scout/Warrior are band-wide roles with no tile → plain, non-clickable
    labels.** Verified by `band_panel_preview` state `band_panel_source_row_hover` (the harness
    force-hovers the Hunt link, so the affordance shows in a static frame).
    `actual_yield`/`sustainable_yield`/`workers_needed` are decoded per assignment in
    `native/src/lib.rs` (inside
    `labor_assignments`); the band-level food flow (net rate + Gathered/Hunted/Eaten breakdown) lives
    on the **Food summary line**, not here — see "Band food status". Then a **Band roles**
    section with the always-shown **Scout** + **Warrior** rows (even at 0), each with a one-line hint so
    the `−/+` steppers read as "this is how you staff this standing role" (Scout's hint reads "Extends
    the band's sight — more scouts see further"; more staffed scouts extend the band's actual sight
    range, so the effect shows directly in the fog, not as a map-action or a reveal disc). Then
    **Move** / **Clear all**.
    Each stepper re-sends `assign_labor_requested` with the new count (0 removes). **The Forage/Hunt
    Current-actions rows are PER-SOURCE max-useful capped** (mirroring the compose controls' cap): each
    row's `+` is gated on `idle > 0 AND workers < max_useful` via `_source_worker_cap_state` +
    `SourceForecast.max_useful_workers`, so a single source can't absorb workers past the point they help. The Hunt
    row reads its herd's forecast from `_find_world_herd(herd_id)` (bare `BARE_FORECAST_PREFIX`); the
    Forage row reads its patch from the new `_forage_patch_lookup` (Main pushes the snapshot
    `forage_patches` → `Hud.update_forage_patches`, mirroring `update_herds`) with the SAME
    `BARE_FORECAST_PREFIX` (the raw wire patch dict carries the forecast fields un-prefixed, unlike
    the `patch_`-prefixed tile_info cross-ref the compose control reads) — the two rows are told apart
    by their `SOURCE_KIND_*`, never by the prefix they share. An unknown forecast
    (`MAX_USEFUL_UNBOUNDED`) falls back to the plain `idle > 0` gate; a source capped at max-useful with
    idle still available spells the reason in the row tooltip (`MAX_USEFUL_CAPPED_TOOLTIP`). **Scout /
    Warrior are band-wide roles with no ceiling — they keep the plain `idle > 0` gate.** Verified by
    `band_panel_preview` state `band_panel_source_cap`.
  - **Optimistic pending feedback** (slice 3b UX): assigning workers or moving the band shows
    immediately, before the next snapshot. `_emit_assign_labor` / `_try_dispatch_pending_move_band`
    record a HUD-local **pending** entry per band entity (`_pending_labor[entity] = {turn, assign:{key→…},
    move:{x,y}}`) and re-render. In the panel, a pending source row reads **amber with the `○` pending
    glyph** (the words live in its tooltip — "Pending — starts when you advance the turn"; the amber
    stays the primary signal, tying the row to the amber pending hex on the map) and the header
    **Idle** counts optimistically (`_effective_idle` = working-age − effective
    assigned, overlaying pending). **Reconciliation is turn-based:** each pending entry is tagged with the
    snapshot `turn` (header tick, set in `update_overlay`); `_reconcile_pending` (called from
    `update_band_alerts` each snapshot) drops entries issued on an OLDER turn — a newer-turn snapshot is
    authoritative confirmation and cleanly absorbs server-side clamping (the snapshot shows the real
    count). Pending is emitted to MapView via `labor_pending_changed` → `set_labor_pending`.
  - **Selected-band map highlights** (`MapView._draw_band_work_highlights`, drawn when a player band
    is selected, cleared on deselect): the **worked forage tiles** (strong green fill on each
    `forage` assignment's `target_x/y`), the **three range borders** (`_draw_range_border`: a clean
    PERIMETER outline of each reach's hex disk — traced edge-by-edge, drawing an edge only where the
    neighbour across it leaves the disk, NOT a filled tile-by-tile mesh — using the sim's true **odd-r
    hex distance** `hex_distance_wrapped` via `MapView._hex_distance`, so the boundary ==
    actually-in-range; forage **green** at `work_range` (ties to the worked-forage fills), hunt **red**
    at `hunt_reach` when it extends past `work_range` (ties to the hunted-herd rings), scout **azure**
    at `scout_reveal_radius` when scouts are staffed — nested and color-distinct, all at every zoom),
    and the **hunted herds** (red ring on the herd tile + a band→herd link, drawn wherever the herd is
    since hunt reach = `work_range` + leash). **Per-source yield annotations** (`_draw_yield_label`): each staffed forage
    tile / hunted herd is labeled with its per-turn rate (food/turn, from the assignment inside
    `labor_assignments`) as a small drop-shadow number above the tile center (reusing `_draw_marker_glyph`),
    food-income **green**. **A HUNT label headlines `sustainable_yield`** (the steady per-turn rate),
    **a FORAGE label `actual_yield`** — the exact split `SourceForecast.source_yield_readout` uses for the Band
    panel (a hunt's `actual_yield` is the kill-credit PULSE — 0 on a wait turn, a spike on a kill turn —
    so its honest rate is `sustainable_yield`; forage has no pulse, `actual == sustainable`), so the map
    label and the Band panel's hunt headline can never disagree. A source that overdraws (the
    **sim-answered `overdraws` bool** on the assignment — the SAME wire flag the Band panel's
    `SourceForecast.source_yield_readout` reads, NOT the client-derived `actual > sustainable`, which false-positives on a
    hunt's kill turn) reads
    **WARN amber + a `⚠`** — an over-hunted herd, or a non-Sustain forage patch now that the forage
    policy axis can decline one (a Sustain forage gathers at regrowth, so it stays green). The label sits on a **dark rounded banner/pill plate** (`_draw_pill_plate`, the shared
    pill chrome extracted out of `_draw_count_pill` — the `×N`/`+N` badges draw the same primitive):
    bare drop-shadowed text washed out on the light tan biomes (prairie/desert), so the plate is sized to
    the MEASURED text+glyph run plus symmetric padding (`YIELD_LABEL_PLATE_PAD_FACTOR`, a fraction of the
    font size) and centered on the label's existing anchor, near-black + slightly translucent
    (`YIELD_LABEL_PLATE_BG`) so the terrain still reads through. The
    label font scales with the hex radius (clamped) and the whole annotation (plate included) is
    **LOD-suppressed below
    `ICON_MIN_DETAIL_RADIUS`** (like the secondary markers) so far zoom stays clean. Scout/Warrior
    produce no food → no label. **The labels are DEFERRED to the very end of `_draw`** — they are an
    annotation OVER the map, and drawn inline in the highlight pass they were painted over by every
    later layer (the dashed-amber pending overlays, the band→herd links, the hunted-herd rings, and the
    secondary herd/food glyphs — a deer glyph landing squarely on the number). The highlight pass now
    `_queue_yield_label`s each request into `_deferred_yield_labels` (cleared at the top of
    `_draw_band_work_highlights`, before its early-outs) and `_flush_yield_labels()` renders the batch
    as the LAST draw call, after the markers/rings/links/pending/targeting. The LOD gate stays at the
    QUEUE site (`show_yields`), so a far-zoom label is never queued and deferral can't bypass the
    suppression. Guarded by `map_preview` state `map_band_label_overlap` (a herd parked ON a worked
    forage tile + a pending hunt dashing across the hunted herd's label) and `map_band_yield_farzoom`. **Scouting draws its azure range border** (the scout vantage reach `scout_reveal_radius`, when
    scouts are staffed) — a perimeter outline like the forage/hunt borders, NOT a filled reveal disc:
    the old faint-blue scouted DISC was removed because `scout_reveal_radius` is a scout-vantage /
    sight-range value, not a revealed-area radius, and the client can't reconstruct the true LOS-revealed
    area; the border just marks how far the vantage reach extends. The band's actual sight is still
    visible directly in the fog (a wider Active radius). Snapshot fields `work_range` / `hunt_reach` /
    `scout_reveal_radius` are decoded in `native/src/lib.rs population_to_dict` and flowed onto the
    MapView unit marker in `_rebuild_unit_markers` (alongside `labor_assignments`). **Optimistic pending**
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
    count (`_hunt_assign_count`) + a **policy picker** (`_build_policy_picker`, `_hunt_assign_policy`,
    default `sustain`). **The two policy axes are separated BY BRANCH, and the sim enforces it:** a
    **local** hunt offers `HUNT_POLICY_OPTIONS` (the four extractive rungs **+ the `Corral` investment
    rung**, gated by `_hunt_policy_gates`), while a hunting **EXPEDITION** offers only the extractive
    `LABOR_HUNT_POLICIES` — a detached party follows the herd and builds no pen, `send_hunt_expedition`
    REJECTS Corral server-side, and the sim exports no `hunt_trip_estimates` row for it, so a Corral
    ETA could only ever be a lie. The
    **local** branch renders `LOCAL_HUNT_POLICY_HINTS` under the picker (the band's real payoffs:
    Sustain → the herd stays healthy AND, on a thriving herd, **builds husbandry toward livestock**;
    Surplus → more food now, pushes settling; Market → sells the take as trade goods, "trade has little
    effect yet" — deliberately not oversold; Eradicate → denial, no food/husbandry/trade). **These are
    NOT the expedition hints** (`SEND_HUNT_POLICY_HINTS`): an expedition's Hunting arm credits **food
    only** — no husbandry accrual, no trade goods (a known v1 gap, tracked server-side) — so the
    expedition set promises neither, and the two sets must stay separate. `LOCAL_HUNT_POLICY_HINTS`
    also owns the **`corral`** hint (Corral is a local-hunt-only rung) — which must carry all three
    halves of that bargain: the ~25-turn half-yield build, the ladder's best payoff, and the fact that
    **penned animals can't graze, so you feed them from your larder every turn and an underfed herd
    shrinks**, and it is the set `_policy_hint`
    spells out on a worked Hunt row's tooltip. **The hint is rendered per BRANCH, never once above
    both** — one shared line under the picker would promise an expedition player the band's payoffs. The
    button + command switch on the **wrap-aware hex distance** from the **SELECTED band's** own tile
    to the herd vs that band's **`hunt_reach`** (= `work_range` + hunt leash, decoded as `hunt_reach`
    and flowed onto the marker): **within reach** → a `Hunters` stepper + **"Assign Local Hunt"** →
    `assign_labor hunt <herd_id> <policy> <workers>`; **beyond reach** → a `Party` stepper (cap
    `min(idle_workers, max_expedition_party_size)`) + a distance hint + **"Send Hunting Expedition"** →
    `send_hunt_expedition <faction> <band> <party_workers> <fauna_id> <policy>` (emitted directly, no
    herd-targeting step — the herd is already selected). Every part of the decision (distance, reach,
    band-entity target) keys off the band the picker selects, explicitly threaded — never the faction's
    default band. **Both branches show a LIVE forecast above the button** (everything — band, count,
    policy, herd — is known at compose time, and the block re-renders on every stepper tick / policy
    click, so it's live, not a confirmation; missing levers/ceilings → no line, panel otherwise
    unchanged): the **expedition** branch renders the SAME raid line as the targeting banner
    (`SourceForecast.hunt_trip_forecast` → `SourceForecast.hunt_forecast_line_bbcode`, shared — the two entry points can't quote
    different numbers) and gives the **button itself** the verdict (`SourceForecast.style_send_hunt_button`).
    **A hunting expedition is a GREEDY RAID** (server `5a130e0`): it grabs the herd's standing surplus
    above the policy floor in a burst and comes home. A party too small to carry a whole animal now
    **kills one and hauls only the fraction its pack holds, wasting the rest** (mirroring the local hunt's
    `quantise_animal_take`), so the headline is the delivered **PAYLOAD** — the animal count over the turns,
    the FOOD the party actually LANDS, and the WASTE below it: **`delivers ≈1 Thunder Mammoth over ≈20
    turns · ~4 food · ⚠ 75% wasted`** (`HUNT_FORECAST_DELIVERS_FORMAT` + `HUNT_FORECAST_TRAVEL_BREAKDOWN` +
    `HUNT_FORECAST_FOOD_FORMAT` + a WARN-amber `HUNT_WASTE_SUFFIX_FORMAT`; `animals` =
    `HuntTripEstimate.animalsTaken` (now a KILL count ≥ 1 whenever there's surplus), **food =
    `HuntTripEstimate.deliveredFood`** — the sim's forward-simulated landed food, NOT `animals ×
    foodPerAnimal`, which counts the whole kill and overstates a partial — and waste % =
    `wastedFood / (deliveredFood + wastedFood)`). A high waste % is **informative, not a block** — the
    button stays enabled. **`turnsToFill` is HUNTING turns only** (server `3bb9731` — travel is NOT in it;
    the per-herd estimate table is band-agnostic). The client adds the **round-trip TRAVEL** itself
    (`SourceForecast.round_trip_travel_turns`, matching the server launch feed EXACTLY: `ceil(2 × wrap-aware
    hex_distance(band, herd) / band_move_tiles_per_turn)`) and headlines the **total** trip length, spelling
    the split out via `HUNT_FORECAST_TRAVEL_BREAKDOWN` when travel > 0. `band_move_tiles_per_turn` (a
    LaborConfig scalar echoed per-cohort) is **now decoded in `native/src/lib.rs` and flowed onto the band
    marker** (`_rebuild_unit_markers`, guarded by `marker_field_guard`), so travel lights up on the live
    wire (it degrades to hunting turns only if a snapshot omits it).
    **WARNED vs BLOCKED — the line that matters:** a **slow** raid (the **TOTAL** trip —
    `hunt_turns + round-trip travel`, NOT hunting-only `turnsToFill` — past `viability_warn_turns`;
    see `SourceForecast.hunt_forecast_line_bbcode`'s `total > warn_turns`, so a distant herd is "slow" on travel
    alone) or a **long** raid (`turnsToFill == 0` — ran the whole horizon still
    delivering) is a real tradeoff, so it is WARN-amber `"armed"` + `Send Anyway (≈54 turns)` /
    `Send Anyway (long raid)` and stays **enabled**. A **denial** mission (Eradicate, `delivers_food ==
    false`) likewise stays enabled (`Send (delivers no food)`). The ONE blocked case is **no surplus**
    (`SourceForecast.hunt_trip_no_surplus`: **`deliveredFood == 0`**) — the herd is at/below the policy's floor, so the raid
    would return empty at every party size: a mistake with no upside, so the button is **DISABLED**
    (`Herd too lean to raid`). This is `deliveredFood == 0`, **NOT `animalsTaken == 0`** — a small party on
    big game now delivers a partial (`animalsTaken 1`, high waste), which is NOT too lean; only a genuinely
    at-floor herd blocks. Party size cannot fix it — **surplus is a property of the HERD, not the party** —
    so the reason (`SourceForecast.hunt_no_surplus_reason` → `SEND_HUNT_NO_SURPLUS_REASON`) names **no alternative size**
    (the old row-scan / `_recommended_party` / step-up-impossible machinery is retired). `SourceForecast.hunt_estimate_key`
    is the one definition of the `"<policy>:<workers>"` estimate key, shared by the single-cell lookup and
    the max-useful scan.
    **The party stepper caps at MAX-USEFUL on both branches** (`SourceForecast.expedition_useful_cap`): **`deliveredFood`**
    PLATEAUS with party size once the herd's surplus (not the pack) binds, so extra hunters past the plateau
    raid no more food — a table SCAN for the smallest size at which delivered food stops rising, capped there
    with the SAME "max N useful here — more would be idle" note the local hunt uses (`MAX_USEFUL_NOTE_FORMAT`).
    It scans **`deliveredFood`, not `animalsTaken`** — the whole-animal count sits at a leading 1 across every
    small-party size on big game (the leading-zeros bug that fooled the old scan into capping at 1); with
    partials, delivered food rises smoothly, so the cap tracks the true bind. That closes the silent-idle-
    hunter gap the whole pass exists for.
    **Picking a policy AUTO-FILLS the crew/party to that policy's max-useful cap** (`_hunt_assign_autofill`,
    a one-shot set only by a policy CLICK, consumed on the next rebuild before the clamp — the "give me
    everything this herd sustains" default that guarantees zero waste + the full rate). Both branches;
    the manual `−/+` stepper is untouched (it never sets the flag).
    The **band-panel launch flow gates identically, in its own form**: its compose sheet picks the
    quarry first and then styles its Send with the SAME `SourceForecast.style_send_hunt_button` + `SourceForecast.hunt_no_surplus_reason`,
    so a no-surplus herd disables there too. The quarry PICKER itself (`_try_pick_quarry`) deliberately
    does NOT test surplus — no policy is chosen at that point, so the verdict is unknowable; it only
    nudges "No huntable herd there — click on a herd." so a click is never silently swallowed. The **local** branch has no carry cap, so a raid readout is meaningless and
    it instead previews the crew's honest **carry-aware delivered take, ANIMALS-first**
    (`_local_hunt_preview_bbcode` / `_hunt_delivered_and_waste`). A hunt takes WHOLE animals via a
    kill-credit bank, so the crew's raw food throughput
    (`workers × hunt_per_worker_provisions × output_multiplier`, capped by the band's flow ceiling)
    is quantized to the whole bodies it can HAUL: `delivered = min(ceiling, floor(collection ÷
    food_per_animal) × food_per_animal)`. The line reads `≈<delivered ÷ food_per_animal> <animal>/turn`
    (e.g. `≈1 Red Deer/turn`, 2-dp trailing-zero-stripped via `_format_animal_rate`), income-green
    `· renewable` or WARN-amber `⚠ … — overdraws the herd` when the delivered take exceeds the herd's
    Sustain ceiling (the shared `_is_overdraw` test). When the crew can't carry even one whole animal the
    surplus meat rots → a **separate** WARN-amber `· ⚠ N% wasted` suffix (`waste_pct`, its own flag,
    rendered amber even on a green line; overdraw + waste can co-occur). Because the animal rate is a
    long-run average of lumpy whole-animal delivery, EVERY extractive rung shows a **STABLE, always-on
    averaging-WINDOW disclaimer** under the policy picker — `HUNT_AVG_WINDOW_FORMAT`: `This estimate is a
    long-run average over ~<X> turns — you take whole animals, so per-turn delivery varies.` X =
    `_hunt_avg_window_turns(herd, policy)`, derived from the SELECTED policy's raw flow ceiling (NOT the
    crew's current delivered rate), so it is **worker-independent and never blinks out** as the Hunters
    count steps up: `g = ceiling ÷ food_per_animal`; slow/big game (`g < 1`) → `ceil(1/g)` (deer Sustain →
    ~2, mammoth Sustain → ~7), fast game → `ceil(1/frac)`, clamped to `HUNT_WINDOW_MAX_TURNS` (12). Keyed on
    the composed policy (a faster policy averages over a different span), extractive rungs only (an
    investment rung shows a dip→payoff, not a cadence), skipped when the window is unknown (missing
    food_per_animal / ceiling → returns 0). The resident band applies its
    morale/discontent productivity modifier at payout, an expedition does not; when `food_per_animal` is
    unknown the line degrades to the old smoothed `≈ +X /turn · renewable` food line (unchanged). **The
    two branches read DIFFERENT herd fields**
    (see "Hunting expedition" below): the expedition line is a pure LOOKUP into the sim's
    forward-simulated `hunt_trip_estimates` (`HERD_TRIP_ESTIMATES_KEY`, zero client arithmetic — a
    `carryCap / rate` division is WRONG for Surplus/Market), while the local line is carry arithmetic over
    the band's flow ceiling `hunt_policy_ceilings` (`HERD_BAND_CEILINGS_KEY`, via `_hunt_delivered_and_waste`
    / `SourceForecast.hunt_policy_ceiling`; `_hunt_take_rate` still backs the food-line fallback). The ecology/MSY model
    is NEVER re-derived client-side.
    Distance uses Hud-local mirrors of MapView's odd-r `_hex_distance` /
    `_wrapped_col_delta`, fed grid width + wrap via `Hud.set_grid_dimensions` (Main forwards the
    snapshot `grid` key). Compose state re-seeds from current staffing when the selected herd changes.
    Covered by ui_preview states `herd_verbs` (local) / `herd_hunt_expedition` (single far band) /
    `herd_hunt_band_near` + `herd_hunt_band_far` (two bands, one herd — picker flips local↔expedition),
    plus the live-forecast states `herd_hunt_forecast_viable` (Mammoth Sustain: cyan "delivers ≈8 …
    over ≈6 turns" + primary button) / `herd_hunt_forecast_slow` (Red Deer Sustain, 54 turns past the
    warn line → amber "⚠ … — a slow raid" + "Send Anyway (≈54 turns)") / `herd_hunt_forecast_surplus`
    (the SAME Red Deer on Surplus: a deeper floor → more animals, brisk turns) /
    `herd_hunt_forecast_no_surplus` (collapsing Wild Fowl at its floor → animalsTaken 0 → red "too lean
    to raid" + disabled button) / `herd_hunt_forecast_eradicate` (denial → amber "Send (delivers no
    food)", enabled), the RAID + max-useful set `herd_hunt_boar_raid` (the server's measured Wild Boar,
    1 hunter → "delivers ≈5 Wild Boar over ≈7 turns · ~20 food", ascending per-policy compact `≈N` picker
    buttons — glyph + metric, name-in-tooltip) / `herd_hunt_max_useful` (2 hunters → "delivers ≈8 … over ≈8 turns"; a 3rd raids no more, so
    the stepper caps at 2 with "max 2 workers useful here — more would be idle") /
    `herd_hunt_raid_travel` (the SAME boar 8 tiles from a band carrying a move rate → the client adds the
    round trip: "delivers ≈8 Wild Boar over ≈16 turns (8 hunting + 8 travel) · ~32 food", cap still 2) /
    `herd_hunt_no_surplus` (a herd stripped to its floor → 0 animals at every size → disabled "Herd too
    lean to raid") / `herd_hunt_eradicate` (the boar on Eradicate → denial, still enabled), and
    `herd_hunt_local_sustain` /
    `herd_hunt_local_overdraw` (local branch, animals-first: green `≈0.14 Red Deer/turn · renewable` vs
    amber `⚠ ≈0.27 Red Deer/turn — overdraws the herd`), and the carry-aware set
    `herd_hunt_delivered_clean` / `herd_hunt_delivered_waste` / `herd_hunt_automax` /
    `herd_hunt_big_game_window` (see the animals-first preview + "up to X/turn" cap notes above).
  - **`%ForageAssignControls`** (Tile card, food-module tiles, `_build_forage_assign_controls`): the
    band-picker, then a sustain/surplus/market/eradicate **policy picker** (`_build_policy_picker`,
    `_forage_assign_policy`, `LABOR_HUNT_POLICIES`, default `sustain`) — carrying the SAME ascending
    per-policy **COMPACT** button metric the local-hunt picker does. **Each button is ONE line:
    `<glyph> <compact-metric>`, NO name** (`[♻ +0.96] [⬆ +1.92] [⇄ +2.88] [💀 +4.80] [🌱 →+1.20] [▦ Sow]`).
    **The picker is a `GridContainer` `POLICY_PICKER_COLUMNS` (3) wide, each button `SIZE_EXPAND_FILL`**, so
    the six-rung forage/local-hunt pickers wrap to **two rows of three** (equal-width, filling the panel
    content width) instead of one over-wide row; the six wide two-line `♻ Sustain / up to +0.90/turn`
    buttons used to overflow, and even the compacted six-in-a-row read too wide docked. A picker with
    `≤ POLICY_PICKER_MAX_SINGLE_ROW` (4) rungs — the 4-rung expedition launch/compose picker — stays a
    **single row** (`grid.columns = options.size()`): a 3+1 grid would strand a lone one-third-width button
    on a second row, and 4 narrow rungs already fit one row. Each `*_policy_takes` helper emits a **`{compact, full}` pair** per policy: the
    bare compact string rides the face, the verbose full string moves to the tooltip. Extractive rungs →
    compact `+0.96` (just `SourceForecast.format_signed(ceiling)`, fed by `_forage_policy_takes` off `SourceForecast.forecast_inputs`),
    full `up to +0.96/turn` (`POLICY_CAP_FORMAT`). INVESTMENT rungs on BOTH pickers → compact `→+1.20`
    (`POLICY_PAYOFF_COMPACT`), full `builds toward +1.20/turn` (`POLICY_PAYOFF_FULL_FORMAT`) — the
    `tended_yield`/`field_yield` (forage) or `pastoral_yield`/`corral_yield` (hunt) they build toward, NOT
    the prep dip, which reads below Sustain and was identical for both hunt rungs (quoting it made
    taming/penning look worse than hunting); a locked rung may still show its payoff, the gate-reason line
    (under the picker) explains the lock. **The name lives ONLY in the tooltip now** — every button's
    `tooltip_text` leads with `<Name> — <full metric>` (`POLICY_TOOLTIP_NAME_FORMAT`, e.g. `Sustain — up
    to +0.96/turn`, `Tame — builds toward +1.20/turn`), and a gated button appends its gate reasons below
    that (so a hover names the rung AND explains any lock; enabled buttons carry the name+metric tooltip
    too). A rung with **no** metric (older snapshot / metric-less gated rung, or the send-expedition launch
    picker that passes no `takes`) falls back to the **name** on the face, so a button is never a lone
    glyph. The selected policy's name still shows in the behaviour-hint line below the picker and in each
    locked rung's gate-reason line — the name is never lost, just off the button face. The three pickers —
    forage / local hunt / expedition — now wear an **identical** face + metric: `+X` (extractive, `up to
    X/turn` via `POLICY_CAP_FORMAT` / `SourceForecast.extractive_take`) and `→+X` (investment, Cultivate/Sow AND
    Tame/Corral). **The expedition picker no longer shows raid animals** (`≈N` / `EXPEDITION_TAKE_COMPACT`
    is retired) — `SourceForecast.expedition_policy_takes` now emits each policy's **MAX obtainable food/turn**, the max
    over party sizes of `deliveredFood / trip_turns` (`trip_turns = turnsToFill + round-trip travel`), so it
    is **worker-independent** (never blinks as the Party stepper steps) and the four read ASCENDING Sustain <
    Surplus < Market. **Eradicate is denial** (`deliversFood == false`, never qualifies) → no rate, falls back
    to its name + skull glyph. **Picking a policy AUTO-FILLS the
    foragers to that policy's max-useful cap** (`_forage_assign_autofill`, the forage twin of
    `_hunt_assign_autofill` — a one-shot set only by a policy CLICK, consumed on the next rebuild before the
    clamp; the manual `−/+` stepper never sets it). It carries a
    **forage-appropriate**
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
    Reuses the same `_hex_distance_wrapped` / `SourceForecast.band_tile` / grid-dim plumbing and explicit
    selected-band threading as the herd hunt. Covered by ui_preview states `food_tile` (in range) /
    `food_forage_out_of_range` (single far band) / `food_forage_band_near` + `food_forage_band_far`
    (two bands, one tile — picker flips enabled↔disabled).

  - **Cultivate / Sow / Tame / Corral — the FOUR INVESTMENT rungs** (on BOTH assign controls; the
    sim's `FollowPolicy::Cultivate` / `Sow` / `Tame` / `Corral`, and `INVESTMENT_POLICIES` names the
    set). The extractive four take from a wild source; these pay an **up-front cost** — while the
    source is being prepared it yields only its dip ceiling, then steps up a rung. Each ladder runs a
    verb **twice**, one per rung-transition (`docs/plan_intensification_ladder.md` §2):
    *plants:* wild --`cultivate`--> **Tended Patch** --`sow`--> **Field**;
    *animals:* wild --`tame`--> **Pastoral herd** --`corral`--> **Pen**.
    **Kind-specific and the sim rejects the cross pairing**: Cultivate + Sow are forage-only
    (`FORAGE_POLICY_OPTIONS`), Tame + Corral hunt-only (`HUNT_POLICY_OPTIONS`) — and both hunt rungs
    are offered on a **local hunt only** (a detached party follows the herd and builds nothing, so the
    expedition keeps the extractive `LABOR_HUNT_POLICIES`, as does the send-expedition launch picker).
    - **These are POLICIES, not standalone commands.** They ride the existing
      `assign_labor … <policy> <workers>` path, exactly as Cultivate/Corral always have — so `Tame`
      and `Sow` needed **zero** new command wiring. The server *also* exposes convenience verbs
      (`tame <faction> <herd_id>` / `sow <faction> <x> <y>`, which switch the policy on bands already
      working the source), but the client does not use them: the picker composes band + workers +
      policy in one act, and routing through a second verb would fork the emit path.
    - **The husbandry CEILING hides a rung outright; knowledge only greys it.** Both hunt rungs are
      filtered against `HerdTelemetryState.husbandryCeiling` (Grazing 2d-δ): Corral needs `"pen"`,
      Tame needs anything above `"wild"` (and retires once `domestication >= 1` — its meter is full
      and Corral is what's next). Hidden, never greyed, because no amount of knowledge or work will
      ever let you pen a `"pastoral"`-ceiling species — greying it would imply a reachable
      prerequisite. Knowledge = "I know how"; ceiling = "this animal allows it" (§4.2, decoupled).
    - **Disabled-with-reason-AND-remedy, never hidden.** `_build_policy_picker(on_pick, selected,
      options, gates)` renders a gated option **greyed, with every reason in the tooltip (one per
      line) AND spelled out under the row**, so the player discovers the rung and its prerequisites
      *before* acting. `gates` maps **policy → `Array[String]` of reasons** (read only through
      `_gate_reasons`); **1 reason** renders the compact one-liner `🌱 Cultivate — <reason>`, **2+**
      render a `🐄 Corral needs:` header + one indented `· <reason>` bullet each (a reason now carries
      its remedy, so two on one line would not fit).
      **Each reason states what's missing + live progress + the action that fixes it** — naming the
      prerequisite alone told the player a door was locked without saying where the key is. **A reason
      is one of exactly two kinds, and the split is the point** (see the two-meter split above): a
      KNOWLEDGE reason is fixed by **practice** and names the ♻ Sustain glyph (pulled from
      `FoodIcons.POLICY_ICONS`, i.e. literally the button beside it) — `Your people know Penning 45%
      — ♻ Sustain-hunt a tamed herd to learn it`; a SOURCE reason is fixed by that rung's **verb** —
      `This herd is 40% tamed — ◎ Tame it to finish`.
      **THE GATE RESHUFFLE (§4.3) — one knowledge per transition, and the client encodes it in
      `_hunt_policy_gates` / `_forage_policy_gates`** (mirroring the sim's `assign_labor` validation):
      * `Cultivate` ← `cultivation >= 1` **and** a Thriving patch **and NOT already `is_cultivated`** —
        a finished patch retires Cultivate outright (`GATE_REASON_ALREADY_TENDED_FORMAT`, "Already a
        Tended Patch — ♻ Sustain-forage it to harvest"), because re-running the verb only pays the low
        prep dip forever. The completed reason SUPERSEDES the prep prerequisites (a done patch's
        Thriving/knowledge gates are moot). Since a gated rung can never be the composed policy, this is
        also what STOPS the panel lying on a done patch: a standing Cultivate falls back to Sustain, so
        the "Preparing → then" prep line disappears and the forecast reads the Sustain harvest.
      * `Sow` ← `seed_selection >= 1` **and** the ground will take seed (see the Sow site gate below)
        **and NOT already `patch_is_field`** — a finished Field retires Sow the same way
        (`GATE_REASON_ALREADY_FIELD_FORMAT`). Deliberately **no** Thriving gate: sown ground starts at
        the reseed floor (i.e. Collapsing), so a health gate would forbid the very case the rung exists for.
      * `Tame` ← `herding >= 1`. **Herding gates Tame ALONE now** — it no longer gates Corral.
      * `Corral` ← **`penning >= 1`** (the new rung-3 knowledge) **and** `domestication >= 1`.
      Two more remedies are the *opposite* of "work harder", because their conditions are stocks, not
      policies: the **patch-ecology** gate (a fully staffed Sustain takes the whole regrowth and holds
      a Stressed patch Stressed forever) reads `Patch is Stressed — ease workers off and let it regrow
      to Thriving`; and `_tame_stalled_hint` (below) says the same of a stalled tame. A gated rung can
      never be the composed policy (re-validated every render, since a source can leave Thriving under
      a standing selection). **Known gap (pre-existing):** `_hunt_policy_gates` does NOT check herd
      **ownership** — the tracks are per-faction, so a herd tamed by ANOTHER faction reads as
      available client-side while the sim rejects the assign.
    - **`_tame_stalled_hint` — the one silent rule, said out loud.** Taming accrues only while the
      herd is **Thriving**, but that is deliberately NOT a gate: a herd's phase swings as it is
      hunted, so refusing the verb would be un-actionable churn. The sim just **pauses** the meter
      (progress is neither lost nor switched). Silence would recreate exactly the hidden-rule problem
      this arc exists to kill, so whenever `Tame` is composed on a non-Thriving herd the drawer states
      the pause, its live phase, that progress is safe, and the ease-off remedy (WARN amber).
      ui_preview `herd_tame_stalled`.
    - **The Sow SITE gate — the refusal is an ANSWER, not a bool.** Only ~**46 of 4160** tiles (1.1%)
      will take seed, so "why can't I sow here?" is *the* question rung 3 provokes — and the client
      **cannot re-derive** it (no per-biome capacity table, no hydrology). The sim ships the verdict
      as a stable key on `ForagePatchState.sowSiteRefusal` (`""` / `too_poor` / `too_dry` /
      `too_poor_and_too_dry`), resolved through the same `RungSiteRequirement::refusal` seam the `sow`
      command gates on, and `_sow_site_refusal_reason` maps it to `SOW_REFUSAL_REASONS` — each naming
      the fault AND pointing at rung 4 (Worked Land — irrigation/the plough), in the manual's voice.
      An **unknown key still refuses** (fail closed: the sim gates the command regardless, so a button
      offered here would only fail unreadably). This is the only gate reason on either ladder a player
      answers by **moving** rather than by working. ui_preview `forage_sow_too_dry` /
      `forage_sow_too_poor`.
    - **The forecast states the deal.** `SourceForecast.forecast_inputs` maps an investment policy's ceiling to the
      DIP yield and additionally returns its `payoff`; `_forecast_yield_row` (now INVESTMENT-only) then
      reads **`Preparing: +0.24 /turn → then +1.20 /turn`** — the deal, not a single rate — both halves
      scaled by the band's `output_multiplier` like every other forecast. The managed source reports
      per-worker == ceiling, so the stepper caps at **1 worker**, as it should.
      **Corral's payoff is GROSS** (`corralYield` does NOT deduct the pen's feed), so its row never
      shows the payoff bare (`FORECAST_FEED_KEYS`, the rungs with a running cost — Corral only; a
      tended patch has none): `Preparing: +0.75 /turn → then +5.40 /turn − 1.74 feed`. `penUpkeep` is
      **one field with one meaning on both sides of the decision** — the feed this pen demands, *or
      would demand once built*, at the herd's current biomass, on the SAME basis `corralYield` uses —
      so the subtraction is a pure difference of two numbers the sim exported for THIS herd and the
      client models no ecology. (It is **demanded**, not paid: the *paid* figure is the cohort's
      `penFeedUpkeep`, and `penFedFraction` is their ratio. Don't cross the wires.)
      **A ZERO PAYOFF IS DATA — it must never be suppressed.** The pen harvests by constant
      escapement, so a herd at/below `K/2` honestly pays **+0.00** until it rebuilds: penning it would
      eat feed forever and pay nothing. The row renders both zeros in full and **emphasizes** them —
      WARN-amber plus `⚠ Too depleted to pen — it would eat feed and pay nothing until the herd
      rebuilds` (`INVESTMENT_FORECAST_DEPLETED_NOTE`) — rather than blanking the 0 as "no data". A
      player who pens a depleted herd because the UI declined to show them a zero has been actively
      misled. ui_preview `herd_corral_depleted`.
    - **TAME's dip — like EVERY herd ceiling — rides the list; its PAYOFF is a scalar.** A herd's only
      wire representation of a per-policy ceiling is the `huntPolicyCeilings` LIST, so no herd rung has a
      `FORECAST_CEILING_KEYS` entry (that dict is now the FORAGE PATCH's ceiling map and only that);
      `SourceForecast.forecast_inputs(src, kind, prefix, policy)` branches on the **caller-stated `kind`**
      (`SOURCE_KIND_HERD` / `SOURCE_KIND_FORAGE`) and resolves every herd policy —
      Sustain/Surplus/Market/Eradicate, Tame, Corral — through `SourceForecast.hunt_policy_ceiling`, falling back to the
      list's Sustain row for an unrecognized policy. **It must NEVER branch on the prefix**: a herd dict
      and a raw wire forage-patch dict both carry the forecast fields bare, so they share the ONE
      `BARE_FORECAST_PREFIX` and a prefix test cannot separate them. That is not merely a convention —
      the bare case was deliberately collapsed from two same-valued consts (`HERD_FORECAST_PREFIX` /
      `WIRE_FORAGE_PATCH_PREFIX`, both `""`) into one **because** a herd-sounding name for the empty
      string invited exactly that test: `prefix == HERD_…` read as discriminating, was not, and sent the
      Current-actions Forage row down the herd branch, where no `hunt_policy_ceilings` key exists —
      collapsing its ceiling to 0 and leaving the row's `+` button permanently dead. Nor may it infer the
      kind from the dict's shape (`has("hunt_policy_ceilings")` would misread a herd whose snapshot
      omitted the list). The `prefix` parameter survives for the FORAGE scalar key lookup only. The PAYOFF, by contrast, IS a real scalar: `HerdTelemetryState.pastoralYield` (the
      pastoral MSY once tamed, the twin of `corralYield`), decoded as `pastoral_yield` and mapped in
      `FORECAST_PAYOFF_KEYS` → so Tame is a full investment rung (`forecast["investment"] == true`) and
      renders the SAME dip→payoff row as Cultivate/Sow/Corral: `Preparing: +<dip> → then +<pastoralYield>`
      (no feed term — Tame has no running cost). `INVESTMENT_POLICIES` still names the set (an investment
      rung must never fall through to the extractive `renewable / ⚠ overdraws` preview), and both hunt
      investment rungs' picker buttons wear the `→ +Y/turn` PAYOFF (Tame `→ pastoralYield`, Corral
      `→ corralYield`) via `_hunt_policy_takes` — NOT the during-building dip, which reads below Sustain
      and was identical for both, making taming/penning look worse than hunting. The payoff shows even on
      a gated/greyed rung (the gate-reason line explains the lock). ui_preview `herd_tame` /
      `two_meter_split` (gated Corral still quotes its payoff).
    - **Progress meters — one row per rung, never merged.** Tile card: `Cultivation N%` → `🌾 Tended
      Patch`, joined by its own **`Field`** row — `Sowing N%` → the SIGNAL-tinted **`▦ Field`**
      (`patch_field_progress` / `patch_is_field`, `_field_label` / `_field_value_hex`). Herd drawer:
      `Husbandry: Domesticating N%` → `🐄 Domesticated`, joined by `Corral: Building N%` → `🐄
      Corralled`. **A patch carries BOTH plant meters at once** (a Field may stand on ground that was
      never tended — seed travels, so `Sow` needs no prior patch), so they are two independent rows.
      A completed **Field** deliberately reads as a *different thing* from a Tended Patch — different
      word, different glyph — not as a bigger percentage; that IS rung 3's readout test.
      `Sowing`/`Building`/`Fencing` share one build-verb convention.
    - **Knowledge-unlock nudge.** `_ingest_intensification` keeps the per-faction tracks (all four,
      driven off `KNOWLEDGE_TRACK_LABELS` — adding a rung's knowledge is a label entry + a decoder
      field, never an edit there) and fires a ONE-SHOT `KNOWLEDGE_UNLOCK_NOTES` command-feed note the
      turn a track crosses to complete. Only a real `<1 → >=1` transition fires it (a track already
      complete on first snapshot / a rehydrated save is silent), player faction only, keyed per
      faction+track.
    - **Wire fields decoded in `native/src/lib.rs`** (snapshot + delta share `herds_to_array` /
      `forage_patches_to_array`). **This decoder has now FOUR times silently dropped appended fields
      — check it FIRST when a new field "arrives as zero".** `ForagePatchState`: `ceilingCultivate` /
      `tendedYield` → `patch_ceiling_cultivate` / `patch_tended_yield`, and the five slice-6a fields
      `fieldProgress` / `isField` / `ceilingSow` / `fieldYield` / `sowSiteRefusal` →
      `patch_field_progress` / `patch_is_field` / `patch_ceiling_sow` / `patch_field_yield` /
      `patch_sow_site_refusal` (MapView cross-refs all onto `tile_info` with the `patch_` prefix; ALL
      are in `FOW_DISCOVERED_HIDDEN_KEYS`, mirroring their rung-2 twins). `HerdTelemetryState`:
      `corralYield` / `corralProgress` / `domestication` / `huntPolicyCeilings`
      (the herd's SOLE ceiling representation — the sim exports one row per
      `FollowPolicy::HUNT_POLICIES`, i.e. the four extractive rungs **plus `tame` and `corral`**, so
      the investment DIPS ride it too; the old per-policy scalars `ceilingSustain` / `ceilingSurplus` /
      `ceilingMarket` / `ceilingEradicate` / `ceilingCorral` are retired `(deprecated)` schema slots and
      are no longer decoded) +
      **`bodyMass` → `body_mass`** (a real appended field, the 4th drop; BIOMASS, surfaced for
      completeness — it **cannot** drive the rhythm, see below) and **`foodPerAnimal` →
      `food_per_animal`** (slot 72, the food-unit quantity the rhythm actually divides by) and
      **`pastoralYield` → `pastoral_yield`** (the newest slot — Tame's payoff, the pastoral twin of
      `corralYield`, which lets Tame render `→ +Y`; verified present on the herd dict) → bare keys
      on the herd dict. `LaborAssignment`: `actualYield` / `sustainableYield` / `workersNeeded` +
      **`wastedYield` → `wasted_yield`** (the understaffing signal, also dropped) +
      **`overdraws` → `overdraws`** (the sim-answered overhunting ⚠ for the confirmed rows/map labels,
      policy-driven `!managed && policy.overdraws()`) → per-assignment keys
      inside `labor_assignments`. `IntensificationKnowledgeState`: `cultivation` / `herding` +
      slice-4's `seedSelection` / `penning` → `seed_selection` / `penning` (present — the "Penning 0%"
      playtest report was NOT a decoder drop; see the kill-rhythm/knowledge notes below).
    - **The hunt row headlines the honest RATE, never the kill-credit PULSE** (`SourceForecast.source_yield_readout`,
      slice 8b UX + the local-hunt UX cleanup): a Current-actions Hunt SUMMARY row + the local-hunt preview
      show `sustainable_yield` (the smoothed per-turn take), not `actual_yield` (0 on a wait turn, a spike on
      a kill turn — the "+0.00 /turn" lie). **The summary row is now JUST the food rate + glyphs** — it reads
      `Hunt <species> +X /turn ♻ ●` (food rate, policy glyph, status glyph). The **animals-per-turn cadence
      (`≈<rate> <animal>/turn`) belongs to the COMPOSE-PREVIEW line only** (`_local_hunt_preview_bbcode` /
      `_format_animal_rate` — `sustainable_yield ÷ food_per_animal`, up to 2 dp, trailing zeros stripped;
      fast game `≈1.3 Marsh Fowl/turn`, big game `≈0.15 Woolly Mammoth/turn`): on a summary row the food rate
      is enough, so the cadence suffix was dropped there (the old `_hunt_row_animal_rate` / `HUNT_RHYTHM_SEPARATOR`
      helpers are gone). The **old fast/slow flip** (`_hunt_kill_rhythm`'s `≈1 Mammoth / N turns` slow form)
      had already been retired — its jarring format switch confused the reading. **The preview cadence divides
      FOOD by FOOD** — the rate (`sustainable_yield`, provisions) by **`food_per_animal`**
      (`HerdTelemetryState.foodPerAnimal`, slot 72 = `body_mass × provisions_per_biomass` = the sim's
      `SourceYieldForecast::body_mass_yield`, one animal's worth of yield in provisions). It must **NOT**
      divide by `body_mass` (BIOMASS): with `provisions_per_biomass 0.02` that reads ~50× too long. A herd
      whose `foodPerAnimal` is 0/unknown → no cadence drawn (the honest rate still shows). The **hunt policy
      picker** (`_build_policy_picker(…, takes)`, fed
      `_hunt_policy_takes` off `huntPolicyCeilings`) shows each rung's **CAP** as the bare compact `+X` on
      the button face (full `up to X/turn` — `POLICY_CAP_FORMAT` — in the tooltip; the shared const also
      used by the forage picker — the source's worker-independent ceiling, FOOD units, distinct from the
      crew's carry-aware per-turn preview line below the picker) so Sustain < Surplus < Market < Eradicate
      reads as ASCENDING. `wasted_yield > 0` renders a muted "· N.N wasted" understaffing note (the low-key
      mirror of the WARN overstaff note). A MANAGED
      (corralled/pastoral, or composing-Corral) herd's local crew are **Herders**, not Hunters
      (`SourceForecast.is_managed_hunt_source` → the stepper + "Assign …" title noun), since `workersNeeded` there is
      the herding crew (max herders, haulers), not a hunt party. The in-progress Cultivation tile-card
      row leads with the **"Preparing N%"** build-verb, matching the herd's "Domesticating N%".
    - ui_preview (slice-8b UX + the local-hunt cleanup): `hunt_actions_rhythm` (two Current-actions Hunt
      SUMMARY rows — each `Hunt <species> +X /turn ♻ ●` with NO `≈… /turn` animals-per-turn cadence; the
      big-game row also keeps the muted `· 1.90 wasted` under-crewed note) / `hunt_picker_ascending` (the local picker + the preview's per-crew line,
      "Hunters" stepper on a wild herd) / **`herd_hunt_delivered_clean`** (2 hunters → `≈1 Red Deer/turn ·
      renewable` + the four ascending `up to +2.33/+3.50/+5.00/+7.00 /turn` cap buttons) /
      **`herd_hunt_delivered_waste`** (1 hunter can't carry one whole deer → green `≈0.65 Red Deer/turn ·
      renewable` + amber `· ⚠ 35% wasted`) / **`herd_hunt_automax`** (a policy click auto-fills the crew to
      the max-useful cap — count sits at 4) / **`herd_hunt_big_game_window`** (mammoth: auto-max staffs the
      20 carriers, `≈0.15 Woolly Mammoth/turn` + the averaging-window disclaimer `This estimate is a
      long-run average over ~7 turns — you take whole animals, so per-turn delivery varies.`; the deer
      `delivered_*` states carry the same disclaimer reading ~2 turns at every worker count) /
      `herd_hunt_local_sustain` +
      `herd_hunt_local_overdraw` (green vs amber `⚠ … — overdraws the herd`) / `hunt_crew_herders`
      (a corralled herd → "Herders" stepper + "Assign herders") / `knowledge_penning_climbing`
      (Penning 34% climbing in the top strip) / `food_tile` (the "Cultivation Preparing 60%" row).
    - ui_preview: `forage_cultivate` (enabled + the Preparing→then forecast + the feed nudge) /
      `forage_cultivate_locked` (1 reason — knowledge + its Sustain-forage remedy) /
      `forage_cultivate_stressed` (1 reason — the ease-off-and-regrow ecology remedy) / `herd_corral`
      (enabled + `Corral: Building 40%`) / `herd_corral_locked` (1 reason — the herd 40% tamed +
      **`◎ Tame it to finish`**, the copy fix: it used to say "♻ Sustain-hunt this Thriving herd",
      the hidden rule the arc exists to kill) / `herd_corral_locked_both` (**2 reasons** — the `🐄
      Corral needs:` header + bullets, gated on **Penning** with Herding fully known, so the frame
      guards the §4.3 reshuffle). Slice 6b adds: **`two_meter_split`** (THE headline frame — the
      top-bar knowledge strip + this herd's own meter + the bridging gate reason, all at once) /
      `herd_tame` / `herd_tame_stalled` / `forage_sow` (enabled, `Preparing: +0.02 → then +2.40` —
      near-zero dip, 2× tended payoff) / `forage_sow_locked` (2 reasons, one fixed by practice and one
      only by moving) / `forage_sow_too_dry` / `forage_sow_too_poor` (the two refusals must read
      differently) / `forage_field_building` (`Sowing 45%` beside `🌾 Tended Patch`) / `forage_field`
      (`▦ Field`) / `forage_cultivate_done` (a COMPLETED Tended Patch with a standing Cultivate: 🌱
      Cultivate greys "Already a Tended Patch — ♻ Sustain-forage it to harvest", the "Preparing → then"
      line is GONE, and the policy falls back to Sustain's extractive preview `+0.32 /turn · renewable`) /
      `forage_sow_done` (a completed Field: ▦ Sow greys "Already a Field …" the same way, one rung up).
  - **Pre-commit yield forecast** (on BOTH assign controls): setting up a forage/hunt assignment used
    to give no feedback — you staffed 6 workers, committed, advanced a turn, and only then learned 5
    were wasted. The sim now streams, on `ForagePatchState` and `HerdTelemetryState` alike, a
    `perWorkerYield` plus one take ceiling per policy — all food/turn at the source's **current
    biomass**, exported at `output_multiplier = 1.0` — enough to compute the take *while composing*.
    **The two source kinds carry the ceilings differently, and that asymmetry is load-bearing:** the
    PATCH has flat scalars (`ceilingSustain` / `ceilingSurplus` / `ceilingMarket` / `ceilingEradicate`,
    plus `ceilingCultivate` / `ceilingSow`) because it has no policy list; the HERD has ONLY the
    `huntPolicyCeilings` list (its identically-named scalars are deprecated slots — a new policy costs
    the herd no schema change).
    `expected(workers, policy) = min(workers × per_worker_yield, ceiling[policy])` (the ceilings are
    already biomass-clamped, so that `min` IS the take) and `max_useful_workers(policy) =
    ceil(ceiling[policy] / per_worker_yield)`. Decoded in `native/src/lib.rs`
    (`herds_to_array` bare / `forage_patches_to_array`, both the snapshot + delta paths), carried to
    the controls via the herd dict and — for the patch — via `forage_patch_lookup` → `_tile_info_at`
    as `patch_`-prefixed keys (in `FOW_DISCOVERED_HIDDEN_KEYS`, so a remembered tile redacts them).
    Two affordances, both recomputed on **every** stepper *and* policy change (both already re-render
    the controls): a live forecast line (scaled by the **selected band's `output_multiplier`** — the sim
    exports at 1.0), and a **worker-stepper cap** of
    `min(idle-worker cap, max_useful_workers(policy))` — the `+` goes dead at the cap and, when
    max-useful is the binding one, a `"max N worker(s) useful here — more would be idle"` note
    explains why (a Market/Eradicate ceiling exceeds Sustain's, so switching policy moves the cap).
    **The LOCAL-hunt cap's usefulness ceiling is `max(take/prepare max-useful, herders_needed)`** —
    a managed (corralling/pastoral) herd needs `herders_needed` hands EVERY turn to hold its tameness,
    but the take/prepare side alone ignores that (a Corral rung's prep forecast reports "1 worker
    suffices to prepare"), which pinned the player at 1 even when a growing herd needed 2 herders — an
    unwinnable trap where the corral slips and is lost. `_forecast_worker_cap(forecast, assignable,
    useful_floor)` folds `herd.herders_needed` in as a floor on the usefulness ceiling (a RAISE, never a
    new cap; an UNBOUNDED forecast stays unbounded), so the maintenance crew is always staffable and the
    "max N useful here" note reads the corrected N. Auto-max on policy-select fills to it. A wild herd
    reports `herders_needed 0`, so `max(useful, 0)` is a no-op there. **Local hunt ONLY** — the
    expedition party has no herding crew, so `SourceForecast.expedition_useful_cap` is left alone. The Herders drawer
    row (`N / N — under-herded`) and the tameness-slipping consequence line read the SAME
    `herders_needed`, so the cap, the row and the consequence never contradict.
    **When the *labor* cap binds instead** (idle workers run out *below* the usefulness ceiling), the
    silent-disable case is filled by a companion note — `LABOR_BOUND_NOTE_FORMAT` = `"N of M useful —
    free up idle workers to send more"` (M = the usefulness ceiling, so it tracks the selected policy;
    the expedition's party-size sub-case, `idle >= max_party_size`, reads `PARTY_SIZE_BOUND_NOTE_FORMAT`
    = "N of M useful — at the max party size"). The cap value is unchanged (still `min(labor,
    usefulness)`); only the note now names *which* ceiling bound and the M you're working toward, so a
    disabled `+` is never mute. (`SourceForecast.expedition_useful_cap` scans the full estimate table for M even past
    the fieldable party, so "of M" can exceed the party you can currently staff.)
    **ONE forecast row per rung, and forage now mirrors the local hunt exactly** (`Hud.gd`): an
    **INVESTMENT** rung (Cultivate/Sow — the `_forage_assign_policy in INVESTMENT_POLICIES` branch)
    keeps `_forecast_yield_row`'s dip→payoff deal (`Preparing: +X /turn → then +Y /turn`); an
    **EXTRACTIVE** rung renders `_local_forage_preview_bbcode` — the plant twin of
    `_local_hunt_preview_bbcode` — a bare rate + verdict (`+2.74 /turn · renewable`, or WARN-amber
    `⚠ … — overdraws the patch` on Market/Eradicate via `_is_overdraw` against the Sustain-ceiling
    yield), through the SAME `_forecast_label` RichTextLabel at `ALLOC_SECTION_FONT_SIZE` the hunt line
    uses. This retires the old `"Expected yield:"` prefix for extractive forage (`FORECAST_LABEL_FORMAT`
    is gone and `_forecast_yield_row`'s non-investment `else` branch was unreachable and removed — its
    only two callers, hunt via `forecast_active` and forage via the `INVESTMENT_POLICIES` guard, both
    gate on an investment rung) and fixes the gap where an Eradicate/Market forage rendered no overdraw
    warning. Shared helpers `SourceForecast.forecast_inputs` / `SourceForecast.max_useful_workers` / `SourceForecast.expected_yield` /
    `_forecast_worker_cap` / `_forecast_yield_row` (investment-only now) serve both controls. **Guards:**
    `per_worker_yield == 0` (a dead-season tile) → no row,
    no cap, never a divide-by-zero; a **tended patch / corralled herd** reports every ceiling ==
    `per_worker_yield` ⇒ max-useful 1, policy irrelevant. Applied to the **local hunt only** — an
    expedition accumulates toward a carry cap over several turns of travel, so the herd's per-turn
    ceiling is not the bound on its party size. The **post-hoc** `"· only N of M working"` overstaffing
    note on the allocation rows stays: it still covers a source whose biomass FELL after you staffed
    it. ui_preview: `food_tile` / `forage_forecast_cap` / `tended_tile` / `herd_hunt_band_near`.

  All emit `assign_labor_requested(payload)` (payload: `faction/band/kind/workers/x/y/herd_id/policy`);
  `Main._on_hud_assign_labor` formats the `assign_labor …` text command. **Clear all** emits
  `cancel_order_requested` (the repurposed `cancel_order` = clear-all → fully idle). The roster
  glyph keeps reading the still-populated `activity` (now the largest-worker
  kind: `idle|forage|hunt|scout|warrior`) and `hunt_mode`. `harvestTask`/`scoutTask` are always
  null server-side and no longer decoded. **Convenience shortcut:** double-clicking a herd on the
  map (`MapView.herd_quick_hunt_requested` → `Main._on_map_herd_quick_hunt` → `Hud.quick_assign_hunters`)
  assigns the player band's idle workers to hunt that herd at Sustain — a no-op with a command-feed
  note when there are no idle workers (never silently nothing).
- **Fog gate on live tile contents — "nothing here" ≠ "I can't see what's here"** (`MapView.gd` +
  `Hud.gd`). Herd MARKERS were always Active-gated (`_draw_herd`), but the herd **lookup** wasn't:
  `_herds_on_tile` matched by coordinate with no visibility test, so a fogged hex listed its herds in
  the Occupants roster, let you target them for a hunt, and fed them into the trip forecast.
  - **MapView (source of truth):** `_herds_on_tile` now early-returns on `not _is_tile_visible(col,row)`
    — the SAME gate the renderer uses. It's the single chokepoint (roster / herd-selection click /
    hunt-target click / forecast all read herds through `_tile_info_at` → `tile_info.herds`), so
    "you can only hunt and forecast what you can see" is true by construction. Three sibling leaks
    closed with it: `_herd_at_point` (double-click quick-hunt could hit an undrawn marker), the
    `need == "herd"` targeting glow in `_draw_targeting` (it haloed every huntable herd, fogged ones
    included — the halo WAS the leak), and the `selection_payload` re-resolve of `selected_herd_id`
    (a selected herd that WALKS into fog kept streaming live biomass/ecology + a live forecast; it now
    drops with its marker and the hex falls back to the tile card). **The server still exports every
    herd unfiltered — a wire-level leak tracked separately — so this client gate is LOAD-BEARING, not
    cosmetic. Never read `herds` by coordinate without it.**
  - **Units — same rule, plus the ownership exception** (`_unit_hidden_by_fog`, the ONE definition):
    `hidden == tile not currently visible AND the unit is not ours`. **Your own units are ALWAYS shown,
    including on an Unexplored hex** — that exception is load-bearing, not a courtesy: the sim excludes
    expeditions from fog reveal (`calculate_visibility` runs `Without<Expedition>` — discovery is
    comm-range gated), so a scouting party ROUTINELY stands on an Unexplored tile, and a plain
    visibility gate would erase your own expedition from the map exactly while you're using it. Applied
    at all five leaks: **`_draw_primary_bands`** (had NO gate — foreign bands rendered straight through
    the fog; the worst of them), `_units_on_tile` (roster/click/stack-cycling chokepoint),
    `_unit_at_point` (marker hit-test), `_nearest_unit_sample` (leaked a hidden band's label *and* a
    bearing on it into `tile_info`), and `refresh_selection_payload`'s selected-unit re-resolve (a
    foreign band walking into fog kept streaming live state — now drops its selection, mirroring the
    herd rule). Already-correct (left alone): everything player-scoped — `_draw_supply_links`,
    `_selected_player_band`, the `need == "band"` targeting glow, band alerts, own work highlights.
    Hud mirrors the exception in `_assemble_roster` (an unseen hex lists your own units, never foreign
    ones, and no herds) and appends `OCCUPANTS_UNSEEN_OTHERS_HINT` ("Out of sight — you can't see
    anything here but your own.") so a lone own-party row never implies the hex is otherwise empty.
    ui_preview: `tile_sight_own_expedition` (the regression guard — own expedition on Unexplored still
    listed + selectable + Move/Recall) / `tile_sight_foreign_hidden` / `tile_sight_foreign_visible`.
  - **Hud (says the truth):** the Tile card leads with a **`Sight:` row** — `In sight` (SIGNAL cyan) /
    `Remembered — not in sight now` / `Unexplored` (both INK_DIM; it states what you KNOW, so it never
    borrows the WARN/DANGER palette) — via `_tile_sight_line` + `_sight_value_hex`. On an unseen hex,
    `_tile_contents_unseen` (which re-reads MapView's `visibility_state` flag — NOT a second visibility
    test) makes `_assemble_roster` list nothing, `_build_forage_assign_controls` offer nothing, and
    `_render_unknown_contents_note` state it in the drawer instead of an empty list
    ("You remember the ground here, but not what's on it now — bands and herds move. Scout it to see."
    / "Nobody has been here…"). An EMPTY roster is a claim of emptiness the client can't back up, so it
    is never rendered on a hex you can't see. Terrain rows stay (geography is remembered knowledge;
    occupants are live state). ui_preview states `tile_sight_active` / `tile_sight_remembered` (fixture
    deliberately carries a herd → proves it is NOT listed) / `tile_sight_unexplored`.
- **Herd ecology readout** (`Hud.gd` `_herd_summary_lines`): the selection panel shows
  the group's `ecology_phase` (snapshot `HerdTelemetryState.ecologyPhase`) as an
  **Ecology** row — a neutral "Thriving", or a warned "⚠ Stressed" / "⚠ Collapsing"
  that `_format_detail_bbcode` tints amber / red (`_ecology_value_hex`, `HudStyle.WARN_HEX`
  / `DANGER_HEX`). A `Collapsing` herd has been overhunted past the point of no return and
  is crashing to local extinction (see `core_sim` Fauna & Wild Game — depensation collapse).
- **Herd grazing range + carrying capacity** (Grazing Phase 2b-iii; `docs/plan_grazing_2b.md` §8,
  `core_sim` Phase 2b-ii — K becomes ecological): make the ecological carrying-capacity model
  *visible*, so the player sees WHY a herd is the size it is. Two wire fields on `HerdTelemetryState`
  (appended after `penFedFraction`), decoded in `native/src/lib.rs herds_to_array` (both snapshot +
  delta share it): **`carryingCapacity`** → `carrying_capacity` (the herd's CURRENT derived K, what it
  caps at on its range) and **`grazeRangeRadius`** → `graze_range_radius` (the hex radius of its
  grazing range: small game 0, big game 1, migratory = its loiter_radius). Surfaced two ways:
  - **Herd drawer rows** (`Hud._herd_summary_lines`): the **`Biomass`** row carries the herd's CURRENT
    head vs the K its range supports as a **`current / max` pair** — **`Biomass: 1480 / 2150`** — the
    same convention the forage patch (`Forage biomass: 84 / 120`) and the tile card (`Pasture: 236 /
    240`) use, so a herd reads like the other food stocks. The old standalone `Carrying cap: ~K` row was
    merged INTO it and removed; the `~` is dropped because a `current / max` pair already implies the max
    is the derived ceiling. A separate **`Range: N tiles`** row stays (the ground the herd grazes — the
    hex-disk count `1 + 3r(r+1)` via `_graze_range_label`: radius 0 → "Range: 1 tile" singular, 1 → 7, 2
    → 19; the SAME count the map ring draws; key ≤ 16 chars so `_split_detail_kv` aligns it as a table
    row beside Biomass). **Overgrazing is a FEATURE of the pair:** an overgrazed herd has `biomass > K`,
    so the row honestly reads `current > max` (e.g. **`Biomass: 2100 / 1352`**), and when `biomass >
    carrying_capacity × (1 + OVERGRAZE_EPSILON)` a WARN-amber full-width **`⚠ Overgrazing — range can't
    sustain this herd`** row appears beneath (a `_format_detail_bbcode` branch tinting the sentence with
    the shared `HudStyle.WARN_HEX` — NOT a parallel styling path). The ⚠ row carries the overgrazing
    signal; the merged value is deliberately left un-tinted (tinting it too was rendered and rejected as
    a noisy double-up). This is a **trivial honest comparison of two sim-provided numbers**, never a
    re-derivation of the ecology model (K and graze flow are the sim's). **Guards:** `carrying_capacity
    <= 0` (a herd momentarily on barren range derives K = 0) falls back to the bare `Biomass: X` (never
    `X / 0`) and suppresses the overgrazing test; a **corralled** herd (doesn't roam-graze a range)
    suppresses the Range row + overgrazing test entirely (its K is a frozen pen-time value), but keeps
    the merged `Biomass: X / Y` pair.
  - **Map range ring** (`MapView._draw_herd_range_highlights`, drawn from `_draw` when a herd is
    selected, under the herd markers): the tiles within `graze_range_radius` of the herd — the EXACT
    ring the sim grazes / derives K over — as a warm graze-amber FILLED region + gold tile outlines
    (`HERD_RANGE_FILL` / `HERD_RANGE_OUTLINE`), deliberately DISTINCT from the band work-range ring's
    faint cyan (a herd's range is a different thing, and both can be on at once) and readable over the
    Pasture overlay (so the ring sits on the actual graze). Reuses the band ring's odd-r `_hex_distance`
    / `_band_effective_col` (seam-wrapped) / `_fill_hex` / `_outline_hex` primitives. `graze_range_radius
    == 0` (small game) → the herd's own single tile. A **corralled** herd draws nothing. Fog-gated via
    `_is_tile_visible` like the herd marker.
  - Verify: ui_preview `herd_grazing_healthy` (`Biomass: 1480 / 2150`, current < max, no warning) /
    `herd_overgrazing` (`Biomass: 2100 / 1352`, current > max → the ⚠ row) / `herd_grazing_small_game`
    (radius 0 → "Range: 1 tile") / `herd_domesticated` (the penned case: `Biomass: X / Y` with NO Range
    row and no ⚠); map_preview `map_pasture_herd_range` (the gold ring over the Pasture overlay).
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
  (SIGNAL tint via `_husbandry_value_hex`) once fully domesticated. This is the **per-source** half
  of the two-meter split — THIS herd's own meter (see "The Intensification Ladder" below). Progress
  builds while a band works the herd under the **`Tame`** policy (and pauses, without loss, whenever
  the herd is not Thriving — surfaced by `_tame_stalled_hint`). **NOT under Sustain**, and there is
  no `domesticate` command: both were retired by the ladder arc (`docs/plan_intensification_ladder.md`
  §4.1) — taming as a hidden Sustain side effect, with a visible-but-disabled `Corral` beside it, is
  the exact UX problem that arc exists to fix. See `core_sim` Fauna & Wild Game — Domestication /
  husbandry.
- **Herd staffing / "Herders" row — the under-herded deficit made VISIBLE** (`Hud.gd`
  `_herd_summary_lines`; snapshot `HerdTelemetryState.herdersNeeded` / `herdedFraction` → decoded in
  `native/src/lib.rs herds_to_array` as `herders_needed` (int) / `herded_fraction` (float)). A managed
  herd needs `herders_needed` herders every turn to HOLD its tameness; understaffed (`herded_fraction <
  1`) its domestication decays, it slips back to WILD, and stops earning Penning — the silent stall a
  playtest hit ("🐄 Domesticated" with no signal Penning had stopped). Immediately after the Husbandry
  row, ONLY when `herders_needed > 0` (0 = wild/unmanaged, `herded_fraction` defaults to `FULLY_HERDED`
  = 1.0 = "no problem"), a **Herders** row shows a calm `N / N` when fully staffed (neutral ink) or an
  amber `A / N — under-herded` (`assigned = round(herded_fraction · needed)`) when slipping, tinted via
  `_herders_value_hex` (WARN, the shared overgrazing/pen-debit path). When under-herded AND
  `domestication > 0`, a muted consequence line — `Tameness slipping — teaching Herding, not Penning.
  Staff all N herders to hold it.` — states WHY Penning stalled and the one lever that fixes it. The
  honest-label choice: the Husbandry label is LEFT as-is (0.98 still reads "Domesticating 100%") — the
  new Herders + consequence lines carry the warning. ui_preview `herd_fully_herded` (calm `4 / 4`) /
  `herd_under_herded` (amber `2 / 4 — under-herded` + the slipping line, Husbandry 98%). **Server half
  (`herdersNeeded`/`herdedFraction` on `HerdTelemetryState`) already landed** — this is the client
  consumer.
- **Per-species husbandry ceiling — gate the ladder by species** (Grazing 2d-δ,
  `docs/plan_grazing_2d.md` §4a; snapshot `HerdTelemetryState.husbandryCeiling` → `husbandry_ceiling`,
  decoded in `native/src/lib.rs herds_to_array` beside `ecology_phase`). Not every animal climbs the
  whole ladder — the string says how far this species can go: **`"wild"`** hunt-only, **`"pastoral"`**
  tameable + roams but never pennable, **`"pen"`** (or **empty/absent** ⇒ treated as pen) the full
  ladder. `SourceForecast.husbandry_ceiling(herd)` normalizes it (unknown → `"pen"`, so an un-tagged herd behaves
  exactly as before the field shipped). Two gates, both keyed off it:
  - **Herd drawer** (`_herd_summary_lines`): `"wild"` shows **no** husbandry track at all (no
    domestication / corral / pen rows), just the dim `Wild game — hunt only` hint; `"pastoral"` keeps
    the domestication (Husbandry) row but replaces the whole corral/pen readout with the dim `Herdable,
    not pennable` hint; `"pen"` renders the full ladder. The hints are colon-free, so
    `_format_detail_bbcode` renders them as dim informational sentences.
  - **Assign controls** (`_build_herd_assign_controls`): the **Corral** rung is filtered OUT of the
    local-hunt policy picker for any non-`"pen"` species (`.filter`, so `HUNT_POLICY_OPTIONS` is
    untouched) — an OUTRIGHT hide, not a greyed "learn Herding" gate, because penning is *impossible*
    for the species, not merely unlearned. The Extend-pen action is implicitly gated (it only shows on a
    `corralled` herd, which is pen-ceiling by construction).
  ui_preview: `herd_ceiling_wild` (hunt-only, no husbandry track + hint, no Corral policy) /
  `herd_ceiling_pastoral` (domestication kept, "Herdable, not pennable", no Corral policy) —
  the existing `herd_*` states carry no ceiling → the unchanged pen path.
- **Herd corral readout** (`Hud.gd` `_herd_summary_lines`): when a herd's `corralled`
  (snapshot `HerdTelemetryState.corralled`, decoded beside `domestication` in
  `native/src/lib.rs herds_to_array`) is true, a **Corral** row shows "🐄 Corralled"
  (SIGNAL tint). The herd end of the intensification ladder — a penned, domesticated herd.
  While the pen is still being built under the Corral policy (`corralProgress`, decoded as
  `corral_progress`; `0 < p < 1`) the SAME row reports the meter — "Corral: Building 40%" —
  the animal twin of the tile card's "Cultivation N%". See the Cultivate/Corral investment-rung
  bullet under **Labor allocation UI**.
- **The pen is a managed POPULATION** (`docs/plan_corral_managed_population.md`; snapshot
  `HerdTelemetryState.penUpkeep` / `penFedFraction` → `pen_upkeep` / `pen_fed_fraction`): a penned
  herd cannot graze, so its keeper hauls it food every turn, and **an underfed herd shrinks**. Two
  rows carry that, both in `_herd_summary_lines`:
  - the **Corral** row flips from the "🐄 Corralled" badge to a DANGER-tinted **"⚠ Starving — 40%
    fed"** whenever `PenStatus.is_starving(pen_fed_fraction)` (`_corral_label` / `_corral_value_hex`,
    one tint path, no parallel styling);
  - a **Pen feed** row (only on a penned herd) states the demand — `−1.74 /turn`, WARN amber as a
    standing debit — and, when the keeper came up short, what was actually paid: `−1.74 /turn — only
    40% paid`, DANGER (`_pen_feed_label` / `_pen_feed_value_hex`).
  `pen_upkeep` is this HERD's demand; the band's ledger row is the sim-summed `pen_feed_upkeep`
  across all its pens — the two are never added together, and the client sums neither.
  ui_preview: `herd_domesticated` (fed) / `herd_corral_starving` (40% fed).
  **The map flags it too** (`MapView._draw_herd` → `_draw_distress_badge`): a starving pen's marker
  gets a DANGER **ring** (under the glyph) plus a filled DANGER **badge with a hand-drawn "!"** (over
  it). Both are **drawn geometry, never a tint or a font glyph** — a herd marker is a full-color
  **emoji**, so `modulate` merely yields a slightly-darker brown animal (tried, rendered, reverted),
  and a font ⚠ carries emoji presentation and blobs at marker size (the hazard that forced
  `MagnifierButton` + the line-art policy icons to hand-draw). map_preview: `map_herd_starving` — a
  starving pen beside a **fed** one, which is the A/B the tint failed and the badge passes.
  **And the turn orb** surfaces it as the `starving_pen` attention producer — see the orb bullet.
- **The pen is fenced LAND — the pen-economy surface** (Grazing 2d-γ, `docs/plan_grazing_2d.md` §7;
  snapshot `HerdTelemetryState.penRadius` / `penFootprintTiles` / `penPastureFraction` /
  `penExtendProgress` → `pen_radius` / `pen_footprint_tiles` / `pen_pasture_fraction` /
  `pen_extend_progress`, decoded in `native/src/lib.rs herds_to_array`). A penned herd grazes its own
  fenced footprint and the grass it eats **offsets** the larder bill (`pen_upkeep` is now that offset).
  Three surfaces:
  - **Herd drawer** (`_herd_summary_lines`, corralled branch): a **`Pen: radius R · N tiles`** footprint
    row — `pen_footprint_tiles` is the SERVER's in-bounds count, shown **verbatim** (the closed-form
    hex-disk count is wrong at map edges) — and a **`Fed by pasture NN% · larder N.N food/turn`** feed
    split (`pen_pasture_fraction` × 100 + `pen_upkeep`): a self-feeding pen on lush land reads "100% ·
    larder 0.0" (and the amber Pen-feed debit row disappears), a scrub pen "0% · larder 1.7". The Corral
    / Pen-feed / starving rows above are unchanged.
  - **Extend affordance** (`_build_extend_pen_control`, in the herd `%HerdAssignControls`): on a built
    pen with no ring in flight (`pen_extend_progress == 0`) an **"Extend pen"** button emits
    `extend_pen_requested{faction,x,y}` → `Main._on_hud_extend_pen` → `extend_pen <faction> <x> <y>` at
    the pen anchor (a penned herd sits AT `corralled_at`, so its own tile). While a ring is being fenced
    (`pen_extend_progress > 0`) the button is replaced by a WARN-amber **"Fencing N%"** badge — the pen
    twin of the corral-build "Building N%" meter. The server rejects an extend at max radius / unowned /
    Herding-unknown with a feed message; the client does not pre-gate (max radius is not on the wire).
  - **Map footprint highlight** (`MapView._draw_pen_footprint_highlight`, drawn under the herd markers
    when a corralled herd is selected): the fenced hex disk of radius `pen_radius` around the pen anchor,
    in a distinct **enclosure-green** tint (`PEN_FOOTPRINT_FILL`/`_OUTLINE`) — deliberately NOT the gold
    of the roam-range ring, so a fenced footprint reads as a different thing. Reuses the range ring's
    wrapped-column / `_hex_distance` / `_fill_hex` / `_outline_hex` primitives (bounds-clamped by the
    loop). A corralled herd draws no roam-range, so exactly one of the two ever renders.
  ui_preview: `herd_pen_self_feeding` (radius 2 · 19 tiles, 100% · larder 0.0, Extend-pen button) /
  `herd_pen_extending` (mid-extension → "Fencing 60%" badge) / `herd_domesticated` (radius 1 · 7 tiles,
  0% · larder 1.7); map_preview: `map_pasture_pen_footprint` (the green footprint disc, the A/B against
  `map_pasture_herd_range`'s gold roam-range).
- **Forage-patch cultivation readout** (`Hud.gd` `_tile_terrain_lines`): a forage tile's
  intensification state, mirroring the herd Husbandry row. `native/src/lib.rs
  forage_patches_to_array` decodes `foragePatches[]` (`ForagePatchState`) into both the
  snapshot and delta dicts under `forage_patches`; `MapView.display_snapshot` ingests it into
  the tile-keyed `forage_patch_lookup`, and `_tile_info_at` cross-refs it onto `tile_info`
  (`cultivation_progress` / `is_cultivated` / `patch_ecology_phase` / `patch_has_owner` /
  `patch_owner` / `patch_biomass` / `patch_carrying_capacity`, all in `FOW_DISCOVERED_HIDDEN_KEYS`
  so a remembered tile redacts them). The
  card shows a **Cultivation** row: "N%" while the patch is being tended, "🌾 Tended Patch"
  (SIGNAL tint via `_cultivation_value_hex`) once `is_cultivated` — and, beside it, its own
  **Field** row for plant rung 3: "Sowing N%" → "▦ Field" (`patch_field_progress` / `patch_is_field`,
  `_field_label` / `_field_value_hex`). The two are **independent meters on one source** and never
  merge: `Sow` needs no prior patch (seed travels), so a Field may stand on ground that was never
  tended. See `core_sim` intensification ladder — cultivation, and the two-meter split above.
  It also shows an **Ecology** row (`patch_ecology_phase`) for **every** tile carrying a patch —
  cultivated or not, directly under **Forage biomass**. The phase gates whether cultivation can
  accrue at all, so it is the tile's headline condition; it is deliberately **not** gated on
  `is_cultivated` (it was, which hid it on exactly the ordinary forage tiles that needed it).
  Named and rendered **identically to the herd's Ecology row** — same `_ecology_phase_label`
  (neutral `Thriving`, warned `⚠ Stressed` / `⚠ Collapsing`) and the same `_ecology_value_hex`
  amber/red tint applied by `_format_detail_bbcode`, which now keys one shared `"Ecology"` case
  for both surfaces. The module's internal `seasonal_weight` is **not** printed on the `Forage:`
  row (it is a yield coefficient, meaningless to the player); it still drives the sim's yield.
  ui_preview: `food_tile` (Thriving) / `food_tile_stressed` (⚠ Stressed) / `tended_tile`.
  It also shows a **Forage biomass** row — `Forage biomass: 84 / 120` (`biomass` /
  `carryingCapacity`, decoded in `forage_patches_to_array`) — the patch counterpart to a herd's
  **Biomass** row, so a foraged patch reads like wild game does ("how much there is"). Foraging draws
  the biomass down and it regrows logistically toward the capacity (sim default 120). Rendered only
  when `patch_carrying_capacity > 0`, so a plain food-module tile with no patch stays bare.
- **Tile-card "What grows here" — the plant COMPOSITION** (Flora Roster F1,
  `docs/plan_flora_roster.md` §2; snapshot `ForagePatchState.composition:[FloraShareInfo]` →
  decoded in `native/src/lib.rs forage_patches_to_array` as a `composition` array of
  `{species, display_name, share}`, cross-refed by `MapView._tile_info_at` as
  `patch_composition`). One compact row directly under `Forage:` — `What grows here: Wild Grain
  45% · Ground Nut 30% · Berry Scrub 25%` (`Hud._flora_composition_text`) — naming the plants the
  tile's forage capacity is MADE OF. **Naming decomposes, it does not add**: the shares sum to 1,
  so this says what the Forage number already on the card consists of; nothing about the economy
  changed. Three rules: the wire list is **already sorted** (share DESC, then species key ASC) and
  is rendered **verbatim, never re-sorted**; the **displayed percentages always sum to 100** —
  independent rounding can total 99/101, so the remainder is folded into the LARGEST share (the
  first entry), which is what stops a decomposition visibly failing to decompose; and an empty /
  absent list renders **no row** (a biome that carries no forage). **Deliberately NOT in
  `FOW_DISCOVERED_HIDDEN_KEYS`** — it is a pure function of the BIOME, like the terrain label or
  the river edges, so a remembered tile still knows what grows there (never-seen tiles are already
  covered by the `unexplored` redaction, and nothing on the patch can change it). ui_preview:
  `food_tile` / `tile_panel_land` (the fixture's shares naively round to 101%, so those frames ARE
  the rounding test) and `tile_panel_no_forage` (no list → no row).
  **ONE ROW, TWO STATES — the COMMITTED crop** (Flora Roster S1, `docs/plan_flora_roster.md` §4.3;
  `ForagePatchState.committedSpecies` / `committedDisplayName` → decoded in the same
  `forage_patches_to_array` as `committed_species` / `committed_display_name`, cross-refed by
  `_tile_info_at` as `patch_committed_species` / `patch_committed_display_name`). Once a band works
  the patch under `Cultivate`/`Sow` it **commits to a single crop and the rest of the basket is
  displaced** — so the same row slot renders `Crop: Wild Emmer` (`FLORA_CROP_ROW`) **instead of** the
  basket, never beside it: the tile is one plant now, and listing the wild mix would name plants that
  no longer grow there. `committedSpecies == ""` means **the wild mixed basket**, not "unknown", so
  the row switches on it rather than treating it as missing data. `Crop` is well under
  `_split_detail_kv`'s 16-char key limit, so it aligns as a table row exactly like the key it
  replaces. The composition list stays on the wire either way — the card CHOOSES, it does not fall
  back. Committed-ness is patch STATE (unlike the biome-derived basket), but it needs no
  `FOW_DISCOVERED_HIDDEN_KEYS` entry: the row sits under `Forage:`, past the discovered early-return,
  so a remembered tile never reaches it. ui_preview: `food_tile_crop` (the committed twin of
  `food_tile` — the two frames differ in exactly that row).
- **The CROP PICKER — committing is a DECISION, not a server default** (Flora Roster S1,
  `Hud._build_crop_picker` inside `_build_forage_assign_controls`; `FloraShareInfo.canCultivate` /
  `canSow` → decoded beside `share` as `can_cultivate` / `can_sow`). It renders **only under the two
  COMMITTING rungs** (`FLORA_COMMITTING_POLICIES` = Cultivate + Sow) — the extractive rungs gather the
  whole basket and choose nothing, so a crop control there would be noise. One row per basket entry in
  **wire order** (`Wild Emmer 34%`), sharing the F1 percentages through the extracted
  `SourceForecast.flora_basket_entries` (the ONE decomposition of the composition list, rounding already resolved),
  so the picker and the "What grows here" row can never quote different numbers for one plant.
  **THE TWO FLAGS ARE SPECIES-GLOBAL** — "can this plant *ever* climb this rung", not "is this a good
  idea here" — and the gate reads **the composed rung's own flag** (`_flora_entry_allows`), which is
  why Hazel is pressable under Cultivate and greyed under Sow. An illegal entry stays **visible and
  disabled with its reason in the tooltip, never hidden**: that a tile carries Oak Mast you cannot farm
  is information about the LAND, and hiding it would make the tile read poorer than it is. **A
  legal-but-marginal crop is NEVER disabled** — a 20%-share plant is a bad choice, not an illegal one,
  and being free to make it is the decision §4.3 exists to create; only the two flags disable anything.
  The selection (`_forage_assign_species`) is re-resolved **every render** by `_resolve_crop_selection`
  — the player's pick while it is still legal on this tile+rung, else the **highest-share legal** entry,
  which is the sim's own `default_species_for_rung`, so picking nothing and accepting the default behave
  identically. `""` is always a valid thing to send (non-committing rung, nothing legal, or an
  **already-committed** patch, which gets a locked read-only readout instead of an editable picker, since
  the commitment is one-way until it lapses). It rides the existing emit path: `_emit_assign_labor` gained
  a trailing `species` (defaulted `""`, so no other caller changed) → the payload → `Main` →
  `assign_labor <f> <b> forage <x> <y> [policy] [species] <workers>` — the **second** optional token,
  worker count always last, omitted entirely when empty.
  **THE PAYOFF, BESIDE THE SHARE** (`cultivateYieldRatio` / `sowYieldRatio` → `cultivate_yield_ratio` /
  `sow_yield_ratio`, read per rung by `_flora_entry_ratio`): a row reads `Wild Emmer 34% · 2.7×` —
  what committing this tile to this plant yields **relative to gathering it wild**. The sim folds the
  share AND the species' conversion rate into it through the same seams the real payout uses, so the
  client only **formats** it (`FLORA_CROP_ROW_FORMAT`, one decimal — the question is "better or worse
  than wild", not a second significant figure); **never do arithmetic on it here**, and note the raw
  per-species rate is deliberately unpublished (meaningless alone, and it would put the payoff formula
  in two places). Below `FLORA_CROP_BREAK_EVEN_RATIO` the row is **WARN-inked and fully pressable** —
  the ratio exists to stop a bad idea being invisible, never to forbid it, so nothing is hidden,
  clamped, sorted by or disabled on it. **`0` is the "cannot climb this rung" SENTINEL, not a number**
  (a real ratio is never 0), so a row greyed by the climbability flags prints no ratio at all.
  **ABOVE 1.0 IS THE NORM, so the verdict wording is keyed to 1.0 and never to an impression of the
  numbers.** The sim's ratio once omitted `tended_regrowth_gain` and understated every Cultivate figure
  by exactly 2× — a genuinely strong crop rendered `0.9×` over a tooltip calling it poor. Fixed sim-side;
  best-country ratios now run 2.3–2.7×. The tooltip therefore has **three tiers**, all relative to
  `FLORA_CROP_BREAK_EVEN_RATIO`: below it *"it loses to simply gathering here"*, at/above
  `FLORA_CROP_STRONG_RATIO` *"strong ground for it"*, and the honest middle *"worth committing to"*.
  Amber is now the exception rather than the rule on good ground, which is the intended read.
  **AND THE `→ then` TERM FOLLOWS THE SELECTED CROP** (`cultivatePayoff` / `sowPayoff` →
  `cultivate_payoff` / `sow_payoff`, carried through `SourceForecast.flora_basket_entries` and substituted by
  `_forecast_for_selected_crop`). Without it the forecast quoted a species-BLIND patch, so committing to
  Ground Nut displayed Wild Emmer's payoff and **the picker appeared to change nothing above it**. Same
  units and output-multiplier convention as the forecast `payoff` it replaces, so this is a
  **SUBSTITUTION, not a calculation** — do no arithmetic on it here. Only `payoff` is substituted; the
  ceiling and per-worker rate still describe the PATCH, which is what caps the stepper. The picker's own
  handler rebuilds the whole controls, so changing the crop moves the line on the same frame — pinned by
  the `forage_crop_then_emmer` / `forage_crop_then_groundnut` pair, whose assertion is that the two
  frames' forecast lines **differ** (`+1.35` vs `+0.45`); asserting the line merely *exists* would pass
  against a hardcoded one. **Carrying the payoff through `SourceForecast.flora_basket_entries` is the load-bearing
  half** — the substitution silently no-ops if the basket entry drops the field, which is exactly how it
  first failed.
  **SIZING — the picker's LIST scrolls within itself, and the cap is MEASURED**
  (`FLORA_CROP_LIST_MAX_HEIGHT`, derived as `FLORA_CROP_LIST_VISIBLE_ROWS × row + separations`, with the
  rows on the work board's compact idiom via `_compact_control` — default button chrome pads 9px top AND
  bottom and makes the whole picker unaffordable). The ComposeSheet's `CARD_MAX_HEIGHT` is deliberately
  NOT raised (that cap belongs to every compose card), so the picker lives in the room the sheet has
  left. **Five rows, so NO SHIPPED BASKET EVER HIDES A CROP** — the longest a tile can carry today is 5
  (the navigable-hex valley+fishery blend), and a picker that hides the best crop behind a scroll is the
  guess the payoff ratio exists to remove. Measured: the worst realistic compose (5 plants under
  Cultivate, Sow locked) lands the sheet at **528 of its 560 cap**. The cap is still a live guard, not
  dead code — F5 refines this coarse roster into a fine-grained one and baskets lengthen — and
  `forage_crop_picker_overlong` (a **synthetic 8-plant** tile, longer than any real one, labelled as such
  in the fixture) keeps the scroll path RENDERED so it cannot rot unseen. The marginal-crop warning rides
  each row's TOOLTIP rather than a standing hint line for the same budget reason (a line under the list
  costs ~40px, and the commit button is what pays). **What bought the rows was collapsing the OTHER
  rung's gate reasons — and that collapse is OPT-IN, deliberately narrow** (`_build_policy_picker`'s
  trailing `collapse_other_gates`, default **false**): three wrapped paragraphs explaining why *Sow* is
  refused while the player composes a *Cultivate* answer a question they did not ask and cost about a
  third of the card, so the forage compose asks for the collapse **only while a COMMITTING rung is
  selected** — i.e. exactly when the crop picker is on the card competing for height. **Every other
  picker (hunt, expedition, work board) and every non-committing forage compose is unchanged**, because
  spelled-out reasons are also how the ladder TEACHES: `forage_cultivate_locked`, `forage_sow_locked`,
  `herd_corral_locked*` and `two_meter_split` all exist precisely to show a NON-composed rung's full
  prerequisites, and a blanket collapse would put each of those frames' whole subject in a tooltip. When
  it does fire, the other rung reads `▦ Sow — locked (2 requirements unmet)` with the full list in the
  line's own tooltip (via `_set_label_tooltip` — a `Label` ignores the mouse by default, so a bare
  `tooltip_text` there is a silent no-op). **Four ui_preview ASSERTIONS pin all of this** and must stay
  green: `forage_crop_picker` asserts the sheet has nothing left to scroll (i.e. `Forage` is on screen —
  it caught a 124px regression eyeballing would have shipped) **and** that the collapse fired where it
  was bought; `forage_crop_picker_overlong` asserts the same at 8 plants; and `forage_sow_locked` +
  `two_meter_split` assert the collapse did **not** leak — the blast-radius guard for the shared picker.
  Change the row count and let the assertions answer, never assume. (Byte-diffing frames is **not** a
  valid instrument here — the harness has time-based pulses, so most frames differ run to run.) ui_preview: `forage_crop_picker` (Cultivate, the 5-plant navigable-hex basket —
  default lands on the highest-share legal row `1.4×`, a WARN `0.7×` still pressable, greyed rows with no
  ratio, the list scrolling internally + the assertion) / `forage_crop_marginal` (the all-marginal
  RollingHills tile — every legal crop below `1.0×`, all warn-inked, all pressable, the default still the
  highest-share one) / `forage_crop_picker_overlong` (the SYNTHETIC 8-plant tile — the internal scroll's
  only frame, plus its own on-screen-button assertion) / `forage_crop_picker_sow` (the SAME basket one rung up — only Wild Emmer survives
  and reads `4.2×`, which is what proves both the gate and the ratio are per-rung) /
  `forage_crop_committed` (the locked readout) / `forage_cultivate` (the 3-entry reference tile) /
  `forage_crop_then_emmer` + `forage_crop_then_groundnut` (the selection-tracking PAIR).
- **The forage compose's TWO ZERO-WORKER SUBMITS** (`_build_forage_assign_controls`; playtest defect,
  pre-existing). `workers == 0` is the **sim's unassign** (`server.rs`: *"Unassigning (`workers == 0`) is
  always allowed"*) and the Work zone's unassign paths depend on it, so the submit is gated on **"would
  this change anything"**, never on a raw count — a client-side floor of 1 would fix the no-op and break
  the unassign. `current` (pending-aware standing staffing on this tile for THIS band) splits the two,
  and **the button and the forecast line must agree in each**:
  - **0 and NOT currently assigned** → the command would do nothing: button **disabled**, still reading
    `Forage`, and the forecast drops its `→ then` promise. `Preparing` is staffing-scaled while the
    payoff is not, so an unstaffed row used to read `Preparing: +0.00 /turn → then +1.20 /turn` — a
    sequence the player is explicitly NOT on track for, since an unstaffed build meter never advances.
    It now states the payoff as a CONDITION (`INVESTMENT_FORECAST_UNSTAFFED_FORMAT` — *"Assign foragers
    to begin — prepared, this pays +1.20 /turn"*), keeping the number, which is how you decide the tile
    is worth staffing at all. The copy is deliberately SHORT — `Assign foragers — +1.20 /turn` — because
    the moment one worker is on it the full `Preparing: … → then …` line renders anyway. It doubles as
    the dead button's explanation, per the `_forecast_worker_cap` "a dead button is always explained"
    precedent.
  - **0 and currently assigned** → a real unassign: button **enabled**, renamed `Unassign`, and the
    forecast row is **suppressed entirely** — "assign foragers to begin" above an `Unassign` button
    tells the player two opposite things. No new warning was invented for it: what abandoning costs is
    already on the card in the rung's own policy hint (*"It must stay staffed or it goes feral"*).
  The unstaffed copy lives in the SHARED `_forecast_yield_row`, so the hunt/herd investment rungs get the
  same fix; it takes a `crew_label` so the sentence names hunters/herders/foragers correctly. ui_preview:
  `forage_unstaffed` / `forage_unassign`, each with assertions on the button state AND on the copy, so
  the pair cannot drift back into contradicting itself.
- **Tile-card Pasture rows — the ANIMAL-edible twin of Forage biomass** (`Hud._tile_terrain_lines`;
  Grazing Phase 2a, `docs/plan_grazing_foundation.md`). `TileState.grazeBiomass` / `grazeCapacity` /
  `grazeEcologyPhase` are decoded in `native/src/lib.rs tile_to_dict` (plain floats, not fixed-point;
  the ubyte phase code is resolved THERE into the same phase *strings* the herd/patch payloads carry,
  so the client keeps ONE ecology vocabulary), cached in `MapView.tile_graze` — **only for tiles that
  actually carry pasture**, mirroring the sim's `GrazeRegistry`, so "no pasture" is an *absent*
  reading — and cross-referenced onto `tile_info` by `_tile_info_at`. Two rows:
  `Pasture: 236 / 240` and `Pasture ecology: ⚠ Stressed`. The pair with `Forage biomass` **is** the
  point: what HUMANS can eat here (seeds/nuts/tubers, food-module tiles only) vs what ANIMALS can eat
  here (grass/browse, nearly every land tile) — *your best farm is usually not your best pasture*.
  - **Rendered only when `graze_capacity > 0`** — on a glacier the card prints **nothing**, never
    `0 / 0` (which would read as a starved pasture rather than an absent one). ui_preview
    `tile_pasture_none`.
  - **The ecology row reuses the shared path** — `_ecology_phase_label` + `_ecology_value_hex`, the
    same neutral/amber/red tint a stressed herd or a stressed forage patch gets. It carries its own
    row KEY (`PASTURE_ECOLOGY_KEY`) purely so a forage tile does not print two rows both named
    "Ecology"; `_format_detail_bbcode` keys both to the one helper — the styling path is not forked.
  - **Pasture is REMEMBERED knowledge, not live state** — it is emitted BEFORE the Discovered
    early-return and is deliberately **not** in `FOW_DISCOVERED_HIDDEN_KEYS`. Grass is a property of
    the GROUND (you can read a steppe from a ridge) and the biome above it is already remembered; what
    a remembered tile redacts is live *contents* (the bands and herds standing on it).
  - ui_preview: `food_tile` (the healthy pair — `Forage biomass 84 / 120` beside
    `Pasture 240 / 240 · Thriving`) / `tile_pasture_stressed` / `tile_pasture_none`.
- **Sedentarization meter** (`Hud.gd` `update_sedentarization`, dispatched from `Main.gd`):
  the player faction's `SedentarizationState.score` (snapshot `sedentarization[]`) shows as a
  compact top-bar block-glyph meter (`▰▰▰▰▰▱▱ 62/100 · soft`, `SedentarizationLabel` in
  `TurnBlock`), tinted amber (soft) / cyan (hard) by stage and hidden until the score is
  meaningful. The soft/hard threshold prompts themselves arrive in the command feed
  (`CommandEventKind::SedentarizationPrompt`). See `core_sim` Campaign Loop — Sedentarization.
- **The Intensification Ladder — THE TWO-METER SPLIT** (`docs/plan_intensification_ladder.md` §4.1;
  the arc's root fix). Two meters advance from one action and they are **different kinds of thing**;
  the client's whole job here is to never let them read as two numbers in a list:
  - **FACTION KNOWLEDGE — the top-bar strip, and the ONLY place a knowledge meter appears.**
    `Hud.update_intensification` (dispatched from `Main.gd`) renders all **four** tracks of
    `IntensificationKnowledgeState` (`intensification_knowledge[]`, decoded in `native/src/lib.rs
    intensification_knowledge_to_array`) — `cultivation` / `seed_selection` / `herding` / `penning`,
    in `KNOWLEDGE_TRACK_LABELS` order (each web's ladder, bottom rung first, so the strip reads as
    two ladders climbing). Prefixed **`⚒ Your people know:`** (`KNOWLEDGE_STRIP_PREFIX`) — that
    prefix is load-bearing: it is what stops the strip reading as a stat of whatever is selected.
    A track is hidden until the faction begins it (the row is sparse), reads a bare `✔`
    (`KNOWLEDGE_KNOWN_BADGE` — the prefix already supplies "know") once complete, else a
    **5-cell** bar + the live percent. **The narrow bar + the bare ✔ are not cosmetic**: at the
    shared 10-cell `_meter_bar` width plus the word "learning", four tracks overflowed the top bar
    and clipped the last one off-screen (caught in `two_meter_split.png`). `_meter_bar(score, cells)`
    takes the width as a defaulted param, so Sedentarization is untouched. **AND the strip WRAPS** —
    even narrowed, four tracks on one line ran off the right edge (the "Penning clipped" playtest
    report), so `update_intensification` groups the tracks into rows of `KNOWLEDGE_STRIP_TRACKS_PER_LINE`
    (2) joined by explicit `\n` (the prefix rides the first row). The label lives in the content-sized
    right-docked `TurnBlock`, so Godot autowrap can't engage without a bounded width — the explicit rows
    are what guarantee no track is ever lost off the edge, at any window width or ladder length.
  - **PER-SOURCE PROGRESS — the source's own drawer row, never the strip.** A herd's `Husbandry`
    (`domestication`) + `Corral` (`corral_progress`); a patch's `Cultivation` (`cultivation_progress`)
    + `Field` (`patch_field_progress`). Local to ONE source, decays if abandoned.
  - **THE BRIDGE — a gated verb's reason line** (`_hunt_policy_gates` / `_forage_policy_gates`,
    rendered under the policy picker by `_build_policy_picker`). This is the one place the two meet,
    and the one line that teaches the ladder: a KNOWLEDGE reason names the track, its live percent
    and the **practice** that fills it (`Your people know Penning 45% — ♻ Sustain-hunt a tamed herd
    to learn it`); a SOURCE reason names the meter and the **verb** that fills it (`This herd is 40%
    tamed — ◎ Tame it to finish`). Judge on `two_meter_split.png`.
  - **The `KNOWLEDGE_UNLOCK_NOTES` one-shot feed nudge** fires per track on a real `<1 → >=1`
    transition (player faction only). Note `herding`'s note now names **Tame**, not Corral — see the
    gate reshuffle below.
  See `core_sim` intensification ladder — knowledge.
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
  `◈ Discoveries N` line followed by a **strip of one mark per distinct site KIND**
  (`DiscoveriesRow` in `TurnBlock` — a `Label` for the text + a sibling `DiscoveriesStrip` HBox for the
  marks; the row hides/shows as one unit, cyan), hidden when 0. **THE TWO NUMBERS MEAN DIFFERENT THINGS
  AND ARE BOTH RIGHT:** `N` is `sites.size()`, the count of INSTANCES found (a site's identity is its
  tile `(x, y)`); the strip shows KINDS, so three peaks read `N = 3` behind one peak mark. Never
  "reconcile" them to a unique count.
  **KEYED ON `site_id`, LIKE THE MAP RENDERER** — this was the last consumer in the client still keying
  site presentation on the `glyph` string, and it had both failure modes that choice implies: two
  distinct site types sharing one glyph (the fixture's `sky_arch` reuses `great_peak`'s ⛰) **collapsed
  into a single strip entry** while the count beside it stayed right — the strip silently disagreeing
  with its own number — and a catalog row with an empty glyph **vanished from the strip entirely**.
  Dedupe is now on `site_id` (the sim's stable catalog key from `sites_config.json`), falling back to
  the `(x,y)` pair ONLY when `site_id` is empty, so a catalog-pruned site still appears once rather
  than disappearing. Presentation resolves in strict precedence: **`WonderSprites.for_site_id`** →
  the same bundled sprite the map marker draws (reused, never a second art table) · non-empty `glyph`
  → the server's emoji as text · neither → the named `DISCOVERIES_UNKNOWN_GLYPH` (`◇`) fallback, because
  **a site the player has FOUND must never render as blank space**. Sprite `TextureRect`s are boxed to
  the label's **derived** `get_theme_font_size` (never a hardcoded pixel size) and drawn
  aspect-preserved, so the strip keeps the text baseline it had when every entry was an emoji. Strip
  children are rebuilt and freed each snapshot (this runs per update — do not leak nodes). Verify on
  ui_preview `discoveries.png`, whose fixture is built to prove exactly the two cases the glyph could
  not distinguish: the `great_peak`/`sky_arch` glyph collision must render TWO marks (sprite + emoji),
  and a repeated `great_peak` instance must lift the count without adding a mark.
  (2) **Map markers** (`MapView.gd`): ingested into `discovered_sites` + a `discovered_site_lookup`
  (`Vector2i → site`) mirroring `food_modules`; `SecondaryMarkerRenderer.draw_discovered_site` draws the
  site's **bundled sprite where we have art for its `site_id`** (`WonderSprites` — see its row above; the
  sprite is resolved BEFORE the `glyph == ""` guard) and the server's `glyph` (drop-shadow, no backing disc)
  otherwise — and unlike the fauna/food tables that emoji path is **live**, since the site catalog is
  data-driven and can outgrow the art. Either way in a fixed **edge slot** via the shared secondary-marker system (see Map markers below),
  gated on `_visibility_state_at != "unexplored"` (persists on any known/remembered tile — Discovered OR
  Active — since a site is permanent geographic knowledge, unlike the Active-only food-site/herd markers).
  (3) **Tile card** (`Hud._tile_terrain_lines`): a `Site: <display_name>` row (from `_tile_info_at`'s
  `discovered_site_lookup` cross-ref → `site_name`), shown before the FoW discovered early-return since
  it's known knowledge. The server also pushes a `SiteDiscovered` command-feed entry, which renders
  generically via the server-provided `kind`/`label` (no client kind→label map needed). See
  `core_sim` — Wondrous Sites.
- **Band food status** (snapshot `PopulationCohortState.turnsOfFood` / `activity` / `supplyNetworkId` /
  `stores[]`, decoded in `native/src/lib.rs` `population_to_dict` as `turns_of_food` / `activity` /
  `supply_network_id` / `stores{item:qty}`).
  > **THE FIELD IS A COUNT OF TURNS, AND ITS NAME NOW SAYS SO** (it was `daysOfFood`; renamed
  > end-to-end — wire, decoder, config key `food_turns.{warn,critical}`, and every client helper).
  > It is the
  > **larder runway — turns until the larder empties, WITH income counted** — resolved sim-side by
  > walking the per-source `arrivalSchedule`s (falling back to `larder / net_drain`, and to the
  > `999` not-food-limited sentinel when the band is net-positive). It used to be
  > `larder / consumption`, i.e. *"how long if you stop hunting"*, which read badly pessimistic for
  > any band with real income and visibly contradicted the FOOD OUTLOOK chart beneath it. Because
  > it walks the same schedules the chart does, the header and the chart now agree. **This game
  > counts turns, never days** — `Hud._food_turns_text` is the single place the unit is spelled, and
  > it spells it from the shared `Hud.FOOD_RUNWAY_UNIT` const. **The Food-row threshold tint in
  > `_format_detail_bbcode` keys on that SAME const**, because it recognizes the row by finding the
  > unit word in the rendered value — the guard tested a bare `"day"` literal after the renderer had
  > moved to turns, so a starving band's Food row rendered in neutral ink and only the `∞` case
  > tinted. Never spell the unit at either site with a literal. One consequence to keep in mind: the runway assumes income *holds*, so a
  > band whose income nearly covers its drain reports a very long runway and reads green until that
  > income lapses. The old figure warned earlier by assuming the worst.
  The green/amber/red warn·critical thresholds and the
  runway→color mapping live in one place, `ui/BandFoodStatus.gd` (config `src/config/band_status_config.json`,
  key `food_turns.{warn,critical}`; `999` = not food-limited → ∞). Surfaced three ways:
  (1) `MapView._draw_band_status` draws a food-runway dot on each **player** band
  (`_is_player_unit`); (2) `Hud._band_food_line` adds a `Food  <N>  (<D> turns)`
  row to the band selection panel, tinted by the thresholds via `_format_detail_bbcode`
  — **player bands only** (`_is_player_unit`, the same gate Morale uses, and for the same
  reason: **a rival's larder is not ours to see**). A foreign cohort carries no
  `turns_of_food`/`stores` on the wire, so rendering the row for one **fabricated knowledge**
  — a healthy-green `Food 0 (∞)`, the UI claiming we'd counted a larder we cannot observe.
  A foreign band's drawer now shows only what is honestly observable from outside: its
  **Position**, plus the name/size on its roster row. The reset of the disclosure context
  (`_food_flow_present` / `_selected_band_food_turns` / `_disclosure_state`) lives at the top
  of `_unit_summary_lines`, NOT inside `_band_food_line` — the skipped call must not leave the
  previous render's caret or food-runway tint behind;
  (3) `MapView._draw_supply_links` faint-chains player bands sharing a `supply_network_id` (`0` = solo).
  **Band food flow on the Food line** (snapshot `PopulationCohortState.foodIncome`/`foodConsumption`/
  **`penFeedUpkeep`**, decoded as `food_income`/`food_consumption`/`pen_feed_upkeep`, flowed onto the
  MapView unit marker + guarded by `marker_field_guard`): for a **player** band with real flow,
  `_band_food_line` appends the **steady net per-turn rate** — `Food 15 (19 turns) · +0.76 /turn` —
  where **net = `_band_net_food` = income − food_consumption − pen_feed_upkeep**, tinted green (≥0) /
  red (<0). **The income term is the fix:** `_band_food_income = Gathered + Hunted = Σ per-source
  `realized_yield`** (the honest long-run average of the lumpy take, client-summed from the same values
  as the breakdown rows), so the net **no longer swings turn-to-turn** the way the old lumpy
  `food_income`-based net did (0 on a hunt's wait turn, a spike on its kill turn). It is summed from the
  breakdown rows rather than off any band-level wire total, so the net's income half can never disagree
  with the Gathered/Hunted rows beneath it. (A cohort-level `foodIncomeAverage` was added for exactly
  this and then **retired as redundant** — a separately-computed total is a second source of truth that
  can drift from the rows. Don't reintroduce it; the sum IS the contract.) **The ledger has THREE terms, not two:**
  a band keeping a corral pays its penned herd's feed straight off the larder every turn (a confined
  herd cannot graze), and that debit is in *neither* of the other two. Omitting it made the row **lie** —
  a Red Deer pen overstated the surplus by ~1.74/turn against a band that eats ~1.2, and the larder then
  drained with no explanation.
  `penFeedUpkeep` is the food the sim **actually paid** this turn summed across every pen the band
  keeps; the client **must not** re-derive it by summing the herds' `penUpkeep` (the sim owns every
  yield number — see `core_sim/CLAUDE.md` → Pre-commit Yield Forecast; the identity
  `larder_delta == income − consumption − pen_feed` is pinned by `integration_tests/tests/pen_food_ledger.rs`).
  The turns-to-empty stays only in the `(N turns)` figure; it is not
  repeated. The `Food` label is a **click-to-open disclosure** (a `▸/▾` caret) opening a
  **category breakdown** in a **POPOVER** — indented `▲ +X  Gathered` / `▲ +Y  Hunted` / `▼ −Z  Eaten
  (people)` / `▼ −W  🐄 Pen feed (animals)` rows (Gathered/Hunted = Σ per-source `actual_yield`
  by kind, Eaten = `food_consumption`, Pen feed = `pen_feed_upkeep`, shown only when a pen is kept —
  **people and animals eat from the same larder but are DIFFERENT decisions**, so they are different
  rows), rendered through the **shared morale-breakdown path** in `_format_detail_bbcode` (income ▲
  green, debits ▼ amber). ui_preview: `band_pen_feed` (fed pen: net +2.99 = 5.88 − 1.15 − 1.74) /
  `band_pen_starving` (part-paid feed, net −0.53 red). No flow → the bare `Food N (N turns)` line,
  no net/disclosure.
  **THE BREAKDOWN OPENS IN A POPOVER, NEVER INLINE — and that is a correctness rule, not a style
  one.** Expanding it in place grew the vitals `RichTextLabel` (`fit_content = true`) by several
  lines AFTER `Hud._build_band_zone_content` had already picked its height tier from
  `_zone_box().y`; the Band panel's zone box is FIXED by design and its hosts `clip_contents`, so
  the extra lines silently sliced the WORKFORCE key row mid-glyph and ate BOTH role cards. A Window
  cannot change a zone's height — the same reason the section `⋯` menus are `MenuButton`s and the
  destructive confirms are `ConfirmationDialog`s. (The work board's budgeted inline inspector strip
  is the other idiom and does NOT apply: in the SHORT tier the chart is already dropped and the role
  cards already hint-less, so there is nothing left to spend but PEOPLE/WORKFORCE — the content.)
  The popover is a `PopupPanel` styled through `HudStyle.card_stylebox()`, anchored under the vitals
  label, dismissed by clicking away / Esc / clicking the caret again; the caret flips `▸`→`▾` only
  while THAT row's popover is up.
  **The auto-show-when-concerning rule is GONE and is now a TINT** — a popover that popped itself
  open on a snapshot would be worse than the clipping it replaced, so `_food_is_concerning`
  (net-negative OR runway below warn, mirroring `_morale_is_concerning`) instead renders the row's
  caret in **WARN** rather than SIGNAL: the invitation to read the breakdown stays visible without
  anything opening itself. `band_panel_food_concerning_*` is that frame.
  **The Food + Morale rows share ONE disclosure mechanism, and it is now the SAME in both hosts** —
  `_register_disclosure` (stashes the rows into `_breakdown_payloads`, keyed `"<kind>:<entity>"`,
  and records the caret) / `_on_detail_meta_clicked` / `_open_breakdown_popover`. The `[url]` meta IS
  that key, so the handler needs no band lookup and **the old `is_panel` fork is gone**: it existed
  only to route the inline re-render, and one click behaviour needs no routing. The label + click are
  wired on BOTH the Occupants-card drawer's `%OccupantDetail` and the dockable Band/City panel's
  per-render vitals label, each binding ITSELF as the popover's anchor.
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
    click-to-open disclosure identical to Food, opening in the SAME popover** (the `▸/▾` caret +
    `meta_clicked` share `_register_disclosure` / `_on_detail_meta_clicked` / `_open_breakdown_popover`,
    keyed `"morale:<entity>"`). Like Food it no longer auto-expands: `_morale_is_concerning` (below
    warn **or** falling past `MORALE_TREND_EPSILON`) tints the caret WARN instead. The contributions
    always compute so the good state can be opened too; the disclosure is offered only when there's
    actually something to show (a contribution above epsilon, or the concerning recovery line) —
    `_register_disclosure` declines an empty payload and no caret renders.
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
  gracefully). **The band CUT POINTS are the SIM's, not the client's** (Climate Authority arc,
  `docs/plan_climate_authority.md`): the sim derives a tile's BIOME from a temperature-based
  `ClimateBand` and PUBLISHES the cut points per-map in the snapshot (`MapSection.climateBands` →
  the native surfaces `overlays.climate_{polar,boreal,temperate}_max_temp`, °C — mirroring the
  `elevation_sea_level` precedent). `MapView._ingest_overlay_channels` adopts them via
  `TileClimate.set_cut_points(...)` (presence-based like the sea level; a per-map constant that
  persists across deltas). `ui/TileClimate.gd` is the single source of truth for the LABELS and the
  classification, which mirrors `climate::climate_band_for_temperature` (`core_sim/src/climate.rs`)
  EXACTLY — inclusive upper bounds, a tile AT a cut point sits in the colder band: `temp <=
  polar_max → Polar`, `<= boreal_max → Boreal`, `<= temperate_max → Temperate`, else `Tropical`.
  The **client's own `cool_min` (3.0) threshold is RETIRED** — it could show a biome and a climate
  that disagree, the exact defect this arc removes; `tile_climate_config.json` is emptied and no
  longer read (its whole 5-band `tropical_min/warm_min/temperate_min/cool_min` scheme is gone). The
  four band names mirror the sim's own vocabulary (Polar/Boreal/Temperate/Tropical) so the label can
  never drift from the band the sim decided. **Fallback:** until the sim publishes cut points (older
  sim / table absent — a bug, not a supported case), `TileClimate.has_bands()` is false and
  `Hud._tile_terrain_lines` SKIPS the Climate row rather than inventing a threshold (`band_for`
  returns `BAND_UNKNOWN "—"`). The row is **informational** — neutral ink, no HEALTHY/WARN/DANGER
  tint, so it doesn't overload the Habitability row's warning semantics.
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
  `starving`→🍖, `losing_population`→📉, `idle_workers`→🛠, `awaiting_orders`→▮▮ (read from
  `FoodIcons.STATUS_ICONS` — the same glyph the Band panel's awaiting row wears), unknown→●.
  Row labels **clip** and `POPOVER_WIDTH` is sized to the widest producer row: a row's inner HBox is
  anchored to its Button (not a container child), so an over-wide label used to spill its `Jump →`
  outside the card instead of widening it. Wiring stays stable via Hud
  relays: a row's jump → `focus_requested` → `alert_focus_requested` → `MapView.focus_on_tile`
  (the same centering the retired Alerts panel used); the footer → `advance_requested` →
  `next_turn_requested(1)`; `update_overlay` pushes the turn number via `set_turn`. The **four live
  producers** (all in `Hud.update_band_alerts`, each pushed with the tile `current_x`/`current_y` so
  Jump locates it) — the folded-in Alerts panel, plus the expedition one. The first three run in one
  loop over the player faction's BANDS:
  - **`starving`** (critical) — `BandFoodStatus.is_critical(turns)`; label `"<band> starving"`, detail = `_food_turns_text(turns)`.
  - **`losing_population`** (warn) — shrank vs the previous snapshot (`_prev_band_sizes`); label `"<band> losing population"`, detail = `_decline_reason(days, morale, morale_cause, last_emigrated)` (`— starving` / `— people leaving` / `— harsh terrain|climate|unrest` / `— low morale`).
  - **`idle_workers`** (warn) — `idle_workers > 0`; label `"N idle workers"`, detail = band name. Supersedes the old `activity == idle` alert (a worker count is more actionable).

  - **`starving_pen`** (warn, `_starving_pen_attention`) — a pen this band keeps whose feed it could
    not pay: the herd is **shrinking every turn** and a 25-turn investment is draining away (it
    recovers if fed, so the player must hear about it *while it is reversible*). Label `"<Species> pen
    starving"`, detail `"40% fed — the herd is shrinking"`, icon = the corral 🐄 (`FoodIcons.POLICY_ICONS`).
    **Found via the band's own Corral labor assignments, never a scan of `herds`** — a herd carries no
    owner field client-side, so scanning would alarm on a RIVAL's pen. Its **Jump routes to the HERD**
    (`_starving_pen_at` → `_focus_labor_source`, the Band panel's Hunt-row path), so the drawer that
    explains the alert actually opens. **On the double-report question:** a pen only goes unfed when
    the keeper's larder came up short, so the same empty larder usually also trips `starving`
    (critical) on that band. They are **not one alert twice** — one cause, two different losses (the
    people are dying / the herd is dying), two subjects, two jumps, two remedies — but only **one gets
    to shout**: the band's row stays critical, this one rides below at WARN. ui_preview
    `turn_orb_starving_pen` renders exactly that pair.
  - The detail line is deliberately terse: orb rows **clip at `POPOVER_WIDTH`**, and appending the
    keeper's name ("· Band 1") pushed this row past it (rendered, seen cut, shortened).

  The fourth (`_awaiting_orders_attention`) runs over the **EXPEDITIONS** split out of that loop:
  - **`awaiting_orders`** (warn) — an expedition in `ExpeditionPhase::Awaiting`: parked at its
    objective, burning provisions, doing nothing until the player acts. Structurally the same class
    as idle workers (a demand on the player, an efficiency loss, not a crisis) — hence WARN, and
    hence it belongs on the orb rather than only on a band panel you happen to have open. **One row
    per party, not one aggregate** (each is a separate decision with its own destination; idle
    workers genuinely IS one aggregate): label = the phase words from `EXPEDITION_PHASE_LABELS`
    ("Awaiting orders"), detail = `"<mission> · <objective>"` (mission from
    `EXPEDITION_MISSION_LABELS`; objective = the followed herd for a hunt party, the party's tile for
    a scout). Capped at `ATTENTION_AWAITING_MAX_ROWS` — the popover is positioned ABOVE the orb, so an
    unbounded list would climb off-screen and take the `Advance ▸` footer with it — with the remainder
    folded into one `"+N more awaiting orders"` row that jumps to the first party past the cap (so
    even the aggregate row is actionable, not a dead `Open ▸` stub). **Its Jump reuses the Band
    panel's expedition-row path**: `Hud._on_turn_orb_focus` resolves an awaiting expedition standing
    on the jumped-to tile (`_awaiting_expedition_at`) and routes through
    `_on_panel_expedition_selected` (recenter + pin that exact expedition so its drawer opens),
    falling back to the plain `alert_focus_requested` recenter for the band-located producers.

  A sixth producer is snapshot-driven rather than band-derived:
  - **`decision`** (critical, **`blocking`**, NON-locating) — a pending narrative fork (The Telling).
    One row per fork, label `"A question awaits an answer"`, detail = the fork's narration truncated
    to the row width (the orb clips; the full telling is the panel's job). It is the **client-side
    end-turn gate**: the server never blocks turn resolution and auto-expires an unanswered fork to
    its defer branch. Because `set_attention` is a **full replace**, it folds into
    `update_band_alerts` via `_push_attention()` (which concatenates the cached `_band_attention`
    with `_pending_fork_attention()`) — a second `set_attention` call would wipe every band row, and
    re-invoking `update_band_alerts` would consume `_prev_band_sizes` and eat the losing-population
    alert. **Gating covers the ORB ONLY**: `Inspector._send_turn` (the dev toolbar + autoplay) is
    deliberately NOT gated — autoplay disables itself on a failed advance, so a hard gate there would
    deadlock the dev loop — but it is not silent either: `Inspector.set_turn_advance_observer` →
    `Main._on_inspector_turn_advanced` → `Hud.note_unanswered_fork()` posts a command-feed receipt.

  The orb severity-sorts (critical floats up), so a starving band tops the popover. Future producers
  (`war` / `decision`) are stubs the model already fits — one producer each, **no orb changes** (the
  awaiting one needed only a kind→icon entry). ui_preview: `turn_orb_fork_blocks` (**the gate's own frame + behavioural assertion**: with a blocking fork seeded a face click must NOT emit `advance_requested`, and the popover's Advance must be `disabled` — the inverse of `turn_orb_clear_click_advances`) / `narrative_fork_panel` + `narrative_fork_panel_warm` (the panel on the REAL authored `soft_drift.long_chase` copy, both registers) / `telling_panel_oral` + `telling_panel_written` (the Telling panel on six REAL authored beats from `beat_definitions.json`, incl. the catalog's longest line `cold_open.bone_ground` so wrapping is exercised — the pair is the medium maturation, same copy, only title + accent age) / **`telling_and_feed`** (**the frame that proves the split**: the Telling panel holding six beats while the command feed still shows four fully-legible receipts — before PR-C two beats pushed every receipt off; the old `narrative_feed` state, which tested prose-vs-receipt styling *inside* the feed, was retired with the behaviour it tested) / `turn_orb_attention` (the three band
  producers) / `turn_orb_awaiting_orders` (awaiting rows + idle workers coexisting, incl. the cap's
  overflow row).
- **Targeting: move-band + send-expedition + send-hunt-expedition** (`Hud.gd`): the single-task
  forage/scout/hunt/follow `_pending_*` flows were retired with labor allocation. Three targeting
  flows remain, all built on the same `_pending_*` → `_current_targeting_info()` →
  `_refresh_targeting()` machinery: `_pending_move_band` (`command: "move"`, `need: "tile"`),
  `_pending_send_expedition` (`command: "expedition"`, `need: "tile"`, carries the outfitted band +
  party size), and `_pending_pick_quarry` (`command: "quarry"`, `need: "herd"`, plus **`min_distance`**
  = the band's `hunt_reach` — the party compose sheet's quarry PICKER: it carries only the band,
  dispatches nothing, and returns the clicked herd to the sheet).
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
  (`_expedition_summary_lines`) shows Mission / humanized Phase / Party / Provisions (`turnsOfFood`);
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
  cap, and drops it at the band — the second verb on the same expedition machinery.
  **The in-flight next-delivery forecast** (`PopulationCohortState.expeditionEtaTurns` /
  `expeditionProjectedDelivery` / `expeditionRecurring`, decoded in `native/src/lib.rs` as
  `expedition_eta_turns` / `expedition_projected_delivery` / `expedition_recurring`) is the client's
  "Next delivery: ~N food in M turns" readout — see the parties inspector strip under Band/City. **All
  three MUST be copied onto the unit marker in `MapView._rebuild_unit_markers`** (beside
  `expedition_target_herd` / `expedition_carry_cap`), because the Occupants **detail panel** reads
  `_selected_unit` — which is the marker, NOT the raw population dict — so a field the marker drops
  renders the panel blank even while the Parties ROW (which reads the raw dict) shows it. This is the
  drop-prone-marker-field bug class: `expedition_projected_delivery` is in `marker_field_guard`'s
  `FRACTIONAL_ROUND_TRIP_KEYS` (a continuous float, must not `int()`-narrow), all three in
  `PANEL_CONSUMED_KEYS`. Surfaced:
  (1) **Distinct map marker** (`MapView._draw_expedition_body`): a hollow 🏹 **bow disc** (vs the
  scout's ⚑ flag), keyed on `expedition_mission == "hunt"`. Phase read: `hunting` (gathering) draws a
  small red "working" cue ring; `delivering`/`returning` (hauling home) draw a green food pip.
  (2) **Hunt drawer panel** (`Hud._expedition_summary_lines` branches on mission): Mission "Hunting
  expedition", **Target** herd (`expedition_target_herd`, species via `_herd_label_for_id` → raw id
  fallback), **Policy** (`expedition_hunt_policy`, capitalized), humanized **Phase**
  (Hunting/Delivering/Returning), Party, and **Carried X / cap** (`stores` total vs
  `expedition_carry_cap`, turns from `turnsOfFood`) with a **· FULL** badge at the ceiling. Reuses
  `_build_expedition_panel` (Recall + Move, "Returning"-when-returning treatment — mission-agnostic,
  so hunt parties get it too).
  (3) **Outfit UI** (`Hud._build_send_expedition_controls`): under the shared "Send expedition"
  section (party stepper + "Send scouting expedition"), a **hunt policy radio**
  (`_build_policy_picker(…, _send_hunt_policy)`, Sustain/Surplus/Market/Eradicate, default Sustain)
  with a one-line behaviour hint (`SEND_HUNT_POLICY_HINTS`), then "Send hunting expedition". It enters
  a HERD-targeting pending mode (`_pending_pick_quarry`, `command: "quarry"`, `need: "herd"`) carrying
  the band; the pick resolves to a huntable herd on the clicked hex (`_huntable_herd_on_tile` reads
  `tile_info.herds`), fills the sheet's Quarry row, and the sheet's own Send then emits
  `send_hunt_expedition_requested` → `Main._on_hud_send_hunt_expedition` →
  `send_hunt_expedition <faction> <band> <party_workers> <fauna_id> [policy]` (trailing policy;
  server defaults Sustain). No huntable herd on the hex → a command-feed nudge, stays in targeting.
  For `need == "herd"` `MapView._draw_targeting` reticles the hovered hex and glows the herds that are
  **valid quarries — those strictly BEYOND the outfitting band's `hunt_reach`**, never every huntable
  herd. A nearer herd is a LOCAL hunt (the same split `_build_herd_assign_controls` makes between
  "Assign Local Hunt" and the expedition branch), so haloing it would promise a mission the pick then
  refuses. The reach rides the targeting info dict as **`min_distance`** — "a valid target must lie
  strictly farther than this from `origin_x/origin_y`"; every other targeting mode omits it and MapView
  defaults it to **0**, which admits everything and changes nothing for move/scout-tile targeting. The
  MapView test is commented as the RENDER-SIDE MIRROR of `Hud._is_expedition_quarry` — change the two
  together, in both directions.
  (4) `marker_field_guard` covers `expedition_target_herd` / `expedition_hunt_policy` /
  `expedition_carry_cap`. Recall is the unchanged `recall_expedition` (works for hunt parties too).
  (5) **Pre-launch RAID forecast — the delivered payload + waste** (server `5a130e0`): a hunting expedition
  is a **greedy raid** — it grabs the herd's standing surplus above the policy floor in a burst and comes
  home. A party too small to carry a whole animal now **kills one and hauls the fraction its pack holds,
  wasting the rest**, so the readout headlines the delivered PAYLOAD: **the animal count over the turns, the
  FOOD landed, and the WASTE**, `delivers ≈1 Thunder Mammoth over ≈20 turns · ~4 food · ⚠ 75% wasted`. The
  player must know **before** committing workers — and the band-panel launch flow now guarantees they can,
  because it asks for the **QUARRY FIRST, inside the compose sheet**. The old premise ("the herd isn't
  chosen until the targeting step, so the forecast has to hang off the targeting banner") is **inverted and
  gone**, and the hover-forecast + `_hovered_tile_info` with it: the herd is what determines the useful
  party size, the per-policy take, the trip length and whether the raid is worth making, so it cannot be
  the LAST question. The targeting mode is now a quarry **PICKER** (`_pending_pick_quarry` /
  `_on_pick_quarry_pressed` / `_try_pick_quarry`, `command: "quarry"`, `need: "herd"` — still what makes
  MapView glow the huntable herds): it carries only the band, dispatches nothing, and on a hit stores the
  herd id in the sheet and re-renders. **The forecast, the max-useful cap, the ascending per-policy metrics
  and the no-surplus block therefore all live in the FORM**, from the SAME helpers the herd drawer's
  beyond-reach branch uses (`SourceForecast.expedition_policy_takes` · `SourceForecast.expedition_useful_cap` · `SourceForecast.hunt_trip_forecast` →
  `SourceForecast.hunt_forecast_line_bbcode` · `SourceForecast.style_send_hunt_button` · `SourceForecast.hunt_no_surplus_reason`), so the two entry
  points structurally cannot quote different numbers. The line reads cyan
  `delivers ≈N <Herd> over ≈M turns · ~F food` (+ amber `· ⚠ P% wasted`) for a brisk raid, WARN-amber `⚠ … — a slow raid` past `expeditionViabilityWarnTurns` (or `delivers ≈N <Herd>
  over many turns … — a slow raid` for a **long** raid, `turnsToFill == 0`, that ran the whole horizon still
  delivering), amber denial `<Herd> — denial mission … delivers no food` (Eradicate), and DANGER-red
  `⚠ <Herd> is too lean to raid — its surplus is spent` when **`deliveredFood == 0`** (the herd at/below the
  policy floor — a small party on big game delivers a partial with waste and is NOT too lean). The click
  still commits (information, not a gate — except the no-surplus case, which the herd panel's button
  DISABLES; see `%HerdAssignControls`).
  **The food total** is `HuntTripEstimate.deliveredFood` — the sim's forward-simulated landed food (NOT
  `animals × foodPerAnimal`, which counts the whole kill and overstates a partial), set on the returned dict
  as `food` (always present on a delivering forecast); the waste % is `wastedFood / (deliveredFood +
  wastedFood)`. All rendered by the shared `SourceForecast.hunt_forecast_line_bbcode` at **both** entry points (the party
  compose sheet + the herd drawer), so the two can never quote different numbers.
  **The client does ZERO arithmetic for an expedition's raid — it is a pure TABLE LOOKUP.** A band and
  an expedition are different actors and read **different herd fields**; never one for the other:
  - **Expedition → `HerdTelemetryState.huntTripEstimates`** (one entry per policy × party size),
    decoded in `native/src/lib.rs` into `hunt_trip_estimates` on the herd dict, keyed
    `"<policy>:<party_workers>"` → `{turns_to_fill, delivers_food, animals_taken, delivered_food,
    wasted_food}` (so it flows through `tile_info.herds` untouched — **`delivered_food`/`wasted_food` are
    the newest appended fields, added to this decoder dict in this pass; the decoder has silently dropped
    appended fields 6× now, always audit it first**). `SourceForecast.hunt_trip_forecast` just looks it up:
    `delivers_food == false` → **denial** (Eradicate — "delivers no food", the SIM decides this, the client
    never infers it from the policy string); **`delivered_food == 0`** → **no surplus** (the one blocked
    case — the raid returns empty at every party size; NOT `animals_taken == 0`, which is now ≥ 1 whenever
    there's any surplus since a small party still kills one animal and wastes the uncarried meat); else the
    raid delivers `delivered_food` food (`animals_taken` kills, `wasted_food` rotted), with `turns_to_fill
    == 0` meaning a **long raid** (ran the whole horizon) and `> expeditionViabilityWarnTurns` flagged
    **slow**. `deliveredFood` PLATEAUS with party size once the surplus binds — that plateau is the
    **max-useful** party the stepper caps at (`SourceForecast.expedition_useful_cap`), and the per-policy picker cap is the
    max over party sizes of `deliveredFood / (turnsToFill + travel)`. **Do not re-derive any of this** — the
    sim forward-simulates the raid (the herd's state moves under the party, a horizon bounds the answer) and
    exports the numbers.
  - **Resident band → `huntPolicyCeilings`** (`provisionsPerTurn`, the herd's renewable **flow**),
    decoded as `hunt_policy_ceilings`. This one IS pure client arithmetic, and the schema blesses it:
    `min(workers × huntPerWorkerProvisions, ceiling) × outputMultiplier` (`_hunt_take_rate` →
    `_local_hunt_preview_bbcode`) — but it must still never re-derive the ecology/MSY model.
  Plus the global levers echoed on every cohort (same idiom as `maxExpeditionPartySize`, decoded +
  flowed onto the MapView unit marker + covered by `marker_field_guard`). **Neither of them is an
  input to an expedition's raid** — that is the lookup above. Their real jobs: `expeditionViabilityWarnTurns`
  = the **slow-raid threshold** applied to the **TOTAL** trip (`turnsToFill` HUNTING turns **+** the
  client's round-trip travel — a distant herd trips it on travel alone), and
  `huntPerWorkerProvisions` = the **resident-band local-hunt take rate** (the one legitimate piece of
  client arithmetic, pinned by `exported_snapshot_fields_reproduce_band_hunt_take`). The one-liner
  that keeps this straight: **band = flow arithmetic; expedition = lookup.** Missing estimate /
  levers absent → no forecast line, banner unchanged. (The old `haul` key — `party ×
  expeditionPerWorkerCarry` — is retired: a raid's payload is the sim's `animalsTaken`, not a
  party×lever product. `expeditionPerWorkerCarry` is still decoded onto the marker for completeness but
  no longer feeds the forecast.)
  ui_preview banner states `hunt_forecast_viable` / `hunt_forecast_slow` / `hunt_forecast_no_surplus`
  + `expedition_launch_policy_sustain`; herd-panel expedition states `herd_hunt_forecast_viable` (the
  partial-with-waste Thunder Mammoth: `~4 food · ⚠ 75% wasted`, button ENABLED) / `_slow` / `_surplus` /
  `_no_surplus` (`deliveredFood 0` everywhere → disabled "too lean") / `_eradicate` (denial, enabled),
  the raid set `herd_hunt_boar_raid` (clean, no waste) / `herd_hunt_max_useful` / `herd_hunt_raid_travel`
  (travel-inclusive `over ≈16 turns (8 hunting + 8 travel)`, and the picker caps correctly lower) /
  `herd_hunt_expedition_automax` (a policy click fills the Party to max-useful).
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
- **Header rows — no restated identity.** The panel's own chrome already states the band's **name +
  settlement stage**, so its summary grid does NOT repeat them: `_unit_summary_lines(unit, in_panel =
  true)` **drops the `Unit: <name>` row** (it was a third copy of the name) and **replaces `Size: <n>`**
  — population under another name — with a **`Population  29 · Workers 14 (Idle 12)`** row
  (`WORKERS_VALUE_FORMAT`, idle from the SAME `_effective_idle` the `+` steppers gate on). That labor
  line used to render as the allocation stack's first block, which meant it appeared wherever CURRENT
  ACTIONS did — **stranded between Active expeditions and Current actions**; the panel now passes
  `with_population_header = false` to `_build_allocation_sections`, so it exists once, in the identity
  grid. The header reads: name / stage / Population / Food / Morale / Position.
  `Unit` and `Size` are gone from **both** hosts — the Occupants drawer's roster row names the band
  and shows its size, so they restated it there too. `in_panel` survives as the gate on the
  **Population** row alone: the dock is the only host with a labor readout, and a foreign band has no
  `working_age`/`idle_workers`, so rendering it in the drawer would print a fabricated
  `Workers 0 (Idle 0)`. `_unit_summary_lines` is still shared with the Occupants-card drawer (foreign
  bands + the no-panel `ui_preview` fallback), and the legacy in-card allocation host keeps the
  population header block.
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
- **Arrival schedule — WHEN the steady food actually lands** (the discrete twin of the steady
  `realized_yield` headline). The sim streams `LaborAssignment.arrivalSchedule:[float]` (index i = food
  delivered i+1 turns from now, length = `arrivals_horizon_turns` = 20, `0.0` = nothing that turn, EMPTY =
  "not projected" — never a famine), decoded in `native/src/lib.rs` beside `realized_yield` as
  `arrival_schedule` (a `PackedFloat32Array`; `Hud._as_schedule` coerces a fixture Array/absent to
  packed). Two client surfaces, both presentation over sim numbers — no yield/ecology is re-derived:
  - **Per-source TICK STRIP** (`ui/hud/ArrivalStrip.gd`, a `_draw` Control): under each Current-actions
    Forage/Hunt row's secondary line (`_build_two_line_stepper` appends it as an indented line 3, inside
    the row container so the section-block/wide-tall packing is untouched), a compact 20-cell strip — one
    cell per upcoming turn, `HEALTHY` = a delivering turn, `LINE_SOFT` = an empty one, ~2px apart. **It
    renders ONLY when the schedule has a GAP** (`ArrivalStrip.has_gap` — at least one `0.0` in the
    horizon): a continuous forage source has no lumpiness to explain, so it gets no strip; the gap test
    is the whole rule (deliberately NOT a hunt/forage kind check). Per-cell tooltip `"Turn N — +X.XX
    food"` / `"Turn N — nothing lands"` (N = `_current_turn + i + 1`; relative "in N turns" before the
    first overlay).
  - **Merged FOOD OUTLOOK chart** (`ui/hud/FoodOutlookChart.gd`, a `_draw` Control) — its own **section
    block** (`_build_food_outlook_block`, appended right after the summary block, headed `FOOD OUTLOOK`;
    BBCode can't host a drawn chart, so it is NOT a summary line). Composed CLIENT-SIDE: start from the
    band's larder (`stores.provisions`), walk `food += Σ arrival_schedule[i] over the band's assignments
    − (food_consumption + pen_feed_upkeep)`, clamped at 0, over the 20-turn horizon (drain held flat).
    Draws a `SIGNAL` filled area + line, a `HEALTHY` dot on each haul turn, a faint `LINE_SOFT` baseline,
    and a dashed `DANGER` vertical labelled `empty ~turn N` where the walk first hits 0. Same player +
    real-food-flow gate as the Food breakdown, plus at least one non-empty schedule; a
    `custom_minimum_size` (≤ `SECTION_COLUMN_WIDTH`) lets the wide-column packer measure it.
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
  (correct for N bands; omitted when none). Row summary — mission glyph + subject + the sim
  `ExpeditionPhase` as a **glyph** (`FoodIcons.for_status`), the phase WORD having moved into the row
  tooltip: hunt `🏹 <herd> · <Policy>  ●`, scout `⚑ → (x,y)  ➤`. The tooltip spells out the mission,
  the hunt policy's behaviour hint, the phase + what it means, and the click affordance.
  **`awaiting` is the one exception — it keeps its words, WARN-amber** (`▮▮ Awaiting orders`): it is
  not a status but a demand on the player (the party is parked at its objective burning provisions
  until you act), and a call to action must never require a hover to find. (A follow-up will make
  `awaiting` a turn-orb attention producer; the orb model already fits it.)
  A row click reuses the cycler's routing —
  `alert_focus_requested`→`focus_and_select_tile` + `roster_occupant_selected`→
  `MapView.select_occupant` — so the map ring moves to the expedition and the
  **Occupants card** (not the band panel) renders its `_build_expedition_panel`
  drawer; `_panel_band` stays put. `home_band_entity` is decoded in
  `native/src/lib.rs population_to_dict` from the snapshot's `homeBandEntity`,
  flowed onto the MapView unit marker, and covered by `marker_field_guard`.
- **Responsive body — THREE NAMED ZONES, two shells (`set_zones`).** The block-packing body
  (`set_band_sections` + `_pack_wide_columns`) is **gone**: column membership used to be a function
  of *measured block heights*, so a section hopped columns when the player pressed a `+`, and the
  panel fitted its cross-axis size to content, so every content change re-emitted
  `reservation_changed` and flickered the map. The body is now three named zones — `band` / `work` /
  `parties` — at a **fixed** cross-axis size, hosted by the wide (3 columns) or narrow (tabbed) shell
  per the panel's own WIDTH. Nothing is balanced, so nothing migrates; nothing is content-fitted, so
  the reservation is constant per dock edge. See the `ui/BandCityPanel.gd` roster row for the full
  contract (`work_zone_size()`, `zones_resized`, `set_tab_badge`, the no-ScrollContainer rule, and
  the plain-`Control` zone hosts).
- **Zone `band` — vitals · PEOPLE · food outlook · WORKFORCE + role cards** (`_build_band_zone_content`).
  The Food/Morale/Output rows are the disclosures — and their breakdowns open in a POPOVER, never
  inline (see Band food status: inline growth is what clipped this very zone); the `Population … Workers … (Idle …)` LINE is
  **gone** — the two bars below state the same facts as charts, and a text restatement above them was
  the third telling of one fact. **PEOPLE** is the new one: a stacked children/working/elders bar
  (`age_children`/`age_working`/`age_elders`, falling back to `working_age` for the middle) plus its
  key and the **dependent count** — `14 dependents`, WARN-tinted once the ratio
  `(children+elders)/working × 100` passes `PEOPLE_DEPENDENCY_HEAVY`. **THE RATIO IS NOT SHOWN
  ANYWHERE** — it only decides that tint. `dep 88/100` read as a score out of 100, the game's own
  designer could not tell what it meant, and a tooltip quoting it did not make it any more useful; the
  bar beside it already shows the split, so the chip states the COUNT, which is the fact the player
  acts on. `_dependency_tooltip` is deliberately SHORT: what a dependent is (children and elders, who
  eat but cannot be put to work), how many adults carry them, and — only when heavy — "More mouths
  than hands."
  **The top-bar strip no longer carries a dependency figure at all** (`Pop 30 👶9 🛠16 🧓5`): it is
  the FACTION total across every band, and dependents are fed per BAND — a band in trouble is in
  trouble whatever the faction average says, and a healthy average hides it. `_dependency_color` went
  with it.
  **`Label` DEFAULTS TO `MOUSE_FILTER_IGNORE`**, so `tooltip_text` on one is a SILENT no-op — six
  labels across this HUD (the dependency chip, the discoveries strip, both detail-row builders, the
  zone-head readout, the work total) shipped tooltips that had never once been seen. Every Label
  tooltip now goes through **`_set_label_tooltip`**, which sets the filter with the text; use it.
  **The brackets arrive FRACTIONAL** (`Scalar` — see the decoder note) and are apportioned to whole
  people by LARGEST REMAINDER (`_apportion_people`), never rounded one at a time: 9.29 + 16.54 + 4.64
  rounds independently to 9 + 17 + 5 = **31** for a band of 30, and a panel that disagrees with the
  top bar about how many people are in the band reads as a bug in both.
  **Absent age data OMITS the whole block** — never a fabricated split.
  Its palette is deliberately MUTED (`VOICE_PIGMENT` / `INK_DIM` / `VOICE_INK`) against
  **WORKFORCE**'s saturated one (`HEALTHY` / `SIGNAL` / `VOICE_INK` / `WARN` / `INK_FAINT`): two bars,
  same shape, different question — *who they are* vs *what they do* — and they must not read as the
  same chart twice. Scout + Warrior are **CARDS** now (bordered, name · hint · the same `−/+` stepper
  and `assign_labor` emit), not rows in a list — the fix for a standing role being indistinguishable
  from a worked source. **The zone yields by HEIGHT TIER** (`_band_zone_tier`, measured against the
  zone box — never the dock edge): full chart + hinted cards at/above `BAND_ZONE_TALL_MIN_HEIGHT`, a
  compact chart above `BAND_ZONE_CHART_MIN_HEIGHT`, and below it **no chart and hint-less cards** (a
  360px T/B dock). A tier change re-renders the zones; anything else just re-pages the board — that
  is what `_on_zones_resized` distinguishes, and skipping it lands a tall-shell band zone in a short
  box where its host silently clips it.
- **Zone `work` — THE PAGED BOARD** (`_build_work_zone_content` / `_fill_work_zone`). Header (`WORK` ·
  n sources · total /turn · a `⋯` `MenuButton`) · filter CHIPS · the board · pager · inspector strip.
  **The chips ARE the summary and the filter** (All / 🌿 Foraging n · rate / 🦌 Hunting n · rate / ⚠ k,
  the last hidden at k = 0), replacing collapsible group headers. Rows are ONE line at a fixed
  `WORK_ROW_HEIGHT`: severity stripe (WARN overdrawing/overstaffed, SIGNAL pending) · glyph · clipped
  label · rate · policy/⚠ marks · the existing `−/+`. **Capacity is derived ENTIRELY from
  `work_zone_size()`** (`_work_board_capacity`): `cols = clamp(w / WORK_COLUMN_MIN_WIDTH, 1,
  WORK_MAX_COLUMNS)`, `rows = (h − head − chips − inspector − pager) / WORK_ROW_HEIGHT`, filled
  **column-major** with a hairline between columns; the pager is resolved in **two passes** because it
  only exists when one page cannot hold everything yet costs a row. **EVERY reserved height must be
  what the element actually draws at** — the default `HudStyle` button chrome pads 9px top and bottom,
  which alone makes a stepper ~40px and pushes the page off the bottom of the zone, so the board's
  buttons take `_compact_control`'s squeeze. Clicking a row opens the **inspector strip**: the row's
  old second/third lines in one place (yield/policy/status in words, warning lines, the `ArrivalStrip`)
  plus three inline links — `Jump to source` · `Change policy` (an inline picker, the four EXTRACTIVE
  rungs only — the investment rungs are ladder commitments made at the source's own compose control,
  where their gates and payoff forecasts live) · **`Unassign`**. That is the per-source removal: a
  hover `✕` beside the `−` stepper would be a mis-click hazard, this is the labelled version. One row
  open at a time, and it COSTS board rows, which is why the capacity maths subtracts it.
  **A source STANDING ON AN INVESTMENT RUNG is the case that picker cannot express, and it is handled
  HERE rather than by widening the picker.** `model["policy"]` can legitimately be `corral`/`cultivate`/
  `tame`/`sow`, none of which is in the offered four — so the radio highlighted **nothing** (reading as
  an unset control on a very-much-set assignment) and a press silently re-sent `assign_labor` at an
  extractive policy, **discarding a ~25-turn ladder build with no confirmation and no cue**. Two fixes,
  both keyed on `policy in INVESTMENT_POLICIES`: (1) the strip renders a WARN line above the picker
  naming the standing rung and what a pick costs (`WORK_INSPECT_STANDING_INVESTMENT_FORMAT`, built from
  the shared `_policy_face` glyph+name vocabulary — the picker's own gate-reason lines use it too, so a
  rung cannot read one way beside the buttons and another in the dialog), and it reserves
  `WORK_INSPECTOR_STANDING_LINE_HEIGHT` on top of `WORK_INSPECTOR_POLICY_HEIGHT` via the shared
  **`_work_inspector_height(model)`** that BOTH `_work_board_capacity` and the strip itself measure from
  (the work-board rule: every reserved height is what the element actually draws at); and (2) the pick
  routes through the same **`_confirm_destructive`** behind "Unassign all work" / "Recall all parties"
  (`_on_work_policy_picked` → `_commit_work_policy`), naming the rung being ended, the source, and the
  rung replacing it. **The EXTRACTIVE path is byte-for-byte unchanged** — no confirm, immediate emit —
  and `band_panel_preview`'s two CONTROL assertions exist to keep it that way. The strip does NOT show
  the build's progress: the work model carries no `corral_progress`/`cultivation_progress`, and this is
  not worth new plumbing (the source's own compose control has the meter).
- **Zone `parties`** (`_build_parties_zone_content`): head + a `⋯` menu (`Recall all parties (n)`,
  behind the same confirm), one row per party (mission glyph · subject · phase · a **DANGER-red**
  recall `✕` — steady, full-opacity, reading as a destructive control like the Work inspector's
  Unassign), an **inspector strip** the row body opens, and the footer.
  **The row `✕` CONFIRMS before recalling** — `_confirm_recall_expedition(exp)` names the party
  (`_herd_label_for_id` for a hunt, "scouting" for a scout) through the shared `_confirm_destructive`,
  and every SINGLE-recall entry point (the row `✕`, the strip's Recall link, the Occupants drawer's
  Recall button) routes through it; `_on_recall_expedition_pressed` stays the RAW emit, so "Recall all"
  loops it under its OWN one confirm and never pops N prompts.
  **The row BODY opens an inspector strip** (`_toggle_parties_inspector(str(entity))` → `_party_open_key`
  → `_rerender_panel_allocation`, the exact `_work_open_key`/`_build_work_inspector` pattern): a bottom
  `PanelContainer` (reusing `_work_inspector_stylebox`) with a titled header + close `✕`, the full
  `_expedition_summary_lines` detail as dim status parts (Mission / Target / Policy / Phase / Carried /
  **Next delivery** / Position — so the strip IS the detail panel), and `Jump to party` (INK) / `Recall`
  (DANGER) inline links. The **"Next delivery" line** (`_expedition_next_delivery_line`, shared by the
  strip, the Occupants drawer, and the row tooltip) is ALWAYS shown for a hunt party once the field is on
  the wire (`has("expedition_projected_delivery")`): `Next delivery: ~N food in M turns` when projecting
  (`↻` appended for a recurring/Market party), `~N food (raid underway)` when the ETA is unknown, and —
  when the projection is `0` — a line that **disambiguates on the party's own TARGET, not the tile's
  herd**. A hunt party is bound to ONE specific herd (`expedition_target_herd`) chosen at launch, and a
  projected `0` over a **healthy** herd is structurally impossible (the sim proves it — the in-flight
  forecast byte-equals the pre-launch estimate), so a `0` means the target is *elsewhere*: `none — its
  target herd has no surplus to raid` when `_find_world_herd(expedition_target_herd)` **is** in telemetry
  (a different, at-floor herd — NOT the boar the player is inspecting), or `target herd lost — the party
  is returning home` when it is **absent** (lost/replaced). This is the fix for the live "reads no-surplus
  next to a thriving boar" report — the target was a different herd. To make that visible, the drawer's
  **`Target:` row appends the target herd's live `(x, y)`** (read from `_world_herds`, keyed `x`/`y` — a
  migrating target is usually NOT the herd on the current tile). Never a silently blank line.
  `_build_parties_zone_content` orders
  `head → rows → inspector(if open) → EXPAND_FILL spacer → footer`, so the Scout/Hunt footer stays
  bottom-pinned with the strip under the clicked row; the strip's detail-line separation is tightened to
  `PARTIES_INSPECTOR_LINE_SEPARATION` to keep row + strip + pinned footer inside the height-capped T/B
  zone. The footer offers the two missions **DIRECTLY** — `⚑ Scout` and `🏹 Hunt`, side by side —
  and **both stay VISIBLE and DISABLED with their reason when idle == 0** (the section vanishing is
  what made expeditions look removed from the game). Pressing one swaps in the **compose sheet already
  on that mission**, titled `Setup a scouting/hunting party…`, with the `✕` as the only way back. The
  mission is therefore still chosen FIRST and the policy picker is still unreachable except under Hunt
  (it used to sit above the scouting button and read as if it modified it) — what is gone is the
  intermediate `Send a party…` page that only existed to ask which mission.
  **The HUNT form asks QUARRY → POLICY → PARTY**, in the order the decision is actually made: the herd
  sets the per-policy take, the useful party size and the trip length, so every field under it is
  unanswerable without it. The `Quarry` row mirrors the `Party` row's shape with a button instead of a
  stepper (`Choose…` primary when empty, `🐗 Wild Boar` ghost once picked, either way opening the map
  quarry picker); with no quarry the sheet renders the hint plus a **visible, disabled** Send and nothing
  else. **A quarry must lie strictly BEYOND the band's `hunt_reach`** — a hunting party exists for game
  the band cannot work from home, so a nearer herd is a local hunt. `_is_expedition_quarry` is the ONE
  definition (`SourceForecast.band_tile` + `_hex_distance_wrapped`, the herd drawer's own split) and all three sites
  route through it: MapView's glow rings only eligible herds (via `min_distance` — see Command
  Targeting), `_try_pick_quarry` REFUSES an in-reach herd and stays in targeting with a
  `QUARRY_WITHIN_REACH_FORMAT` nudge naming the herd, the distance, the reach and the local alternative
  (the split is invisible on the map, so the refusal is where it gets taught), and the sheet
  re-validates every render, so a herd that MIGRATES into reach falls back to `Choose…` rather than
  forecasting a raid the player should not make. With one, the policy rungs finally carry their ascending metric, the party stepper caps at the
  raid's max-useful plateau (a policy click auto-fills to it via the sheet's own `_send_party_autofill` —
  **not** the herd drawer's `_hunt_assign_autofill`), the trip forecast renders, and the Send button takes
  its viable/slow/denial/no-surplus treatment and emits `send_hunt_expedition_requested` directly.
  `_send_party_quarry_id` is re-resolved through `_find_world_herd` every render (a vanished herd clears
  it rather than forecasting a stale id) and cleared on open, cancel, send, and a panel-band change.
  **SCOUT is unchanged** — its only input is party size and nothing about it depends on the destination,
  so it has no ordering problem to fix and still picks its target tile on the map after the send.
- **Destructive bulk actions ASK, and name what is SPARED** (`_confirm_destructive`, a
  `ConfirmationDialog` — a Window, like the `⋯` `MenuButton`'s popup, so opening either cannot move a
  zone's height). `Unassign all work` sends **`cancel_order <faction> <band> work`** — the signal
  `cancel_order_requested(band, scope)` gained the scope this pass; `work` clears Forage + Hunt only
  and leaves standing roles, parties and an in-progress move alone. `Recall all` is one
  `recall_expedition` per party (no bulk verb, and parties are few).
- **Move and Clear all are GONE from the panel.** Move belongs to the Tile panel in a later change;
  `_on_move_band_pressed` / `_pending_move_band` / the whole targeting machinery are intact and still
  reachable (the expedition drawer's Move), just not surfaced here.
- **A zone must FIT its zone.** The hosts clip, so overflow is invisible in a frame — and a zone
  content whose *minimum* size exceeds the zone (four policy rungs abreast in a 380px dock) does worse:
  it drags the whole zone column out past its host, taking the section menu beside it off the edge.
  Hence `ZONE_POLICY_PICKER_COLUMNS` and `band_panel_preview`'s **recursive zone-bounds assertion**,
  which is the only thing that catches either.
  **CONTAINMENT IS NOT COMPLETENESS, and that distinction is a second assertion.** Content the box
  cannot hold gets CLIPPED, and clipped content still reports a rect *inside* its host — so the
  bounds assertion passes on a frame that is visibly sliced (the Food/Morale inline breakdown cut the
  WORKFORCE row mid-glyph and erased both role cards, with every assertion green). `band_panel_preview`
  therefore also runs **`_assert_zone_content_fits`**: for every visible descendant of a zone host,
  `top + get_combined_minimum_size().y` must fit the zone box. It recurses past the zero-minimum
  plain-`Control` wrappers `Hud._wrap_zone` produces (they report no minimum, so measuring them alone
  proves nothing) and stops at the first control that DOES report one — its minimum already accounts
  for its children. Run it beside the other two at every state.
- **The no-dock fallback renders the SAME three builders**, stacked into `%AllocationPanel`
  (`_build_allocation_panel`) — there is no second layout to maintain. It passes `with_vitals = false`,
  since the Occupants card's own drawer already prints those rows above it.
- Verify chrome + reflow via `tools/band_panel_preview.gd`
  (`godot --path . res://tools/band_panel_preview.tscn` → `ui_preview_out/
  band_panel_{left,right,top,bottom,collapsed}.png`). **The ZONE states are the Part-2 frames:**
  `band_panel_people` (both bars, the dependency ratio, the two role cards) ·
  **`band_panel_people_map_path`** (the SAME block reached the OTHER way — by clicking the band ON THE
  MAP, through the real `MapView._rebuild_unit_markers` → `refresh_selection_payload` →
  `show_unit_selection` → `_render_band_into_panel`. `band_panel_people` drives the SNAPSHOT path,
  which re-resolves the brackets from the raw `populations` floats and therefore SELF-HEALS a
  truncating marker copy — so it structurally could not catch the `int()`-narrowed age brackets. This
  state ASSERTS the three PEOPLE brackets sum to the band's own `size`, and was verified to FAIL —
  `sum to 29 but the band holds 30 (raw [9.0, 16.0, 4.0])` — with the narrowing put back) ·
  `band_panel_work_page` (34 sources, narrow shell) · `band_panel_work_wide` (the same 34 in the
  bottom dock — 4 columns, column-major, `Page 1 / 2`, `1–28 of 34`) · `band_panel_inspector` (a row
  open, the board shrunk to 31 rows and a pager appearing to pay for it) · `band_panel_compose_hunt`
  (quarry → policy → party → forecast, with the real per-policy metrics and max-useful cap) ·
  `band_panel_compose_hunt_no_quarry` (the empty state: `Choose…`, the hint, a disabled Send, nothing
  below) · `band_panel_compose_scout` (the same sheet under Scout — no quarry row, no policy picker). A
  BEHAVIOURAL assertion rides beside them: `_assert_quarry_eligibility` drives the real
  `_try_pick_quarry` with a herd INSIDE the fixture band's `hunt_reach` (must leave
  `_send_party_quarry_id` empty and stay armed) and one beyond it (must set it) — verified to FAIL
  with the `_is_expedition_quarry` test removed. The GLOW is MapView's, so its frame is
  `map_preview`'s `map_quarry_targeting` (two huntable herds straddling the reach; only the far one
  may wear the ring) · `band_panel_no_idle` (both mission buttons disabled and their shared reason) ·
  `band_panel_clear_confirm` · the **work-inspector policy-picker** PAIR, which is the only coverage
  that control has ever had (`_work_policy_open` was never set true in either harness):
  **`band_panel_work_policy_investment`** (a Hunt row standing on `corral` — no rung lit, the WARN
  standing line above the picker) and **`band_panel_work_policy_extractive`** (the same picker on the
  `sustain` row beside it). Four assertions, and which ones may move is the whole point: the two RED
  ones ride the investment frame — the standing line is RENDERED, and pressing a real rung button
  raises a `ConfirmationDialog` while `assign_labor_requested` does NOT fire (both verified to FAIL
  before the fix, the second emitting immediately) — while the two CONTROLS ride the extractive frame
  and must pass BEFORE and AFTER: a pick emits immediately with no dialog, and **exactly one** rung
  wears the `primary` variant, read back off the button's `normal` stylebox against
  `HudStyle.BUTTON_PRIMARY_BG` (there is no other marker of "this rung is lit", which is why that
  colour is now a named const rather than a literal inside `apply_button`). The harness also ASSERTS, per state, that **no
  `ScrollContainer` exists anywhere in the panel** and that **nothing a zone renders falls outside its
  zone rect** — checked RECURSIVELY, since the top-level content is anchored full-rect and so always
  "fits" while the thing that actually overflows is a board row off the bottom of a column. Both
  assertions have already caught real regressions (a stepper's default button chrome busting
  `WORK_ROW_HEIGHT`; the band zone standing 5px past a 360px T/B dock); **keep them green.** A THIRD
  per-state assertion guards the shell threshold: **whenever the wide shell is active,
  `work_zone_size().x` must be at least `ZONE_WORK_MIN_WIDTH`** — the invariant the old hand-picked
  `WIDE_SHELL_MIN_WIDTH` violated, and one the zone-bounds assertion structurally cannot catch (a
  CLIPPED label still sits inside its rect, so "everything is within bounds" is true and useless).
  States `band_panel_shell_below_threshold` / `band_panel_shell_at_threshold` bracket the flip — one
  pixel below (must be the NARROW tabbed shell) and exactly at it (the narrowest legitimate wide
  shell, work zone exactly 380px, rows still legible) — each additionally asserting WHICH shell it
  got. They pin the **canvas** via `_pin_canvas`, not just the window: `project.godot` stretches
  `canvas_items` with an `expand` aspect, so the canvas never goes below the 1920 base width and a
  plain `_pin_window` renders a 1920-wide panel that proves nothing about a sub-1920 threshold. State `band_panel_status_glyphs` is the
  **row-vocabulary** frame: a confirmed working forage row (`●` + `♻` + the overstaffing note) and a
  working hunt row (`●` + `⚠`) beside a pending row (`○`, amber), plus one Active-expeditions row per
  phase (`➤` outbound / `●` hunting / `◄` delivering / `◄` returning / `▮▮ Awaiting orders` in amber)
  — read it at true size whenever a glyph changes. States `band_panel_arrivals_left` /
  `band_panel_arrivals_top` / **`band_panel_arrivals_bottom`** are the **arrival-schedule** frame (a
  lumpy hunt row with a gappy tick strip beside a continuous forage row that draws NONE, + the rising
  `FOOD OUTLOOK` sawtooth chart, tall and wide), and `band_panel_arrivals_empty` is the emptying-larder
  case (the descending chart's dashed `empty ~turn N` marker). **The T/B (`_top`/`_bottom`) frames are
  the band-zone HEIGHT guard** — they render the chart-bearing `_arrivals_band_fixture` (NOT the
  chartless `_band_fixture`) through `_assert_zone_content_fits`, so the SHORT-tier chart drop
  (`_build_band_zone_content` gates `_build_food_outlook_block` behind `_band_zone_tier !=
  BAND_ZONE_TIER_SHORT`) is actually asserted: forcing the chart ungated needs 415px in the ~300px T/B
  box (115px over), and the tier gate is what makes it fit at 0 overflow while the tall L shell keeps the
  full chart. Without a chart-bearing fixture in a T/B dock, `content-fits` was vacuously green (the
  chartless fixtures never overflow).

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
| `pasture` | Straw→grass ramp, **+ two off-ramp barren tones** | The GRAZE layer's per-tile **capacity** (`TileState.grazeCapacity`) |
| `forage` | Wheat→green ramp, **+ one off-ramp barren tone** | The FORAGE (human food) layer's per-tile **capacity** (`TileState.forageCapacity`) |
| `hunt_danger` | Danger orange (generic lerp) | **NOT a wire raster** — projected client-side, `attack × ferocity` per herd (see below) |
| `threat` | Threat red (generic lerp) | **NOT a wire raster** — projected client-side, `attack × aggression` per herd (see below) |

Legend rendering: min/avg/max values + channel description.

**`hunt_danger` / `threat` — the two derived-danger overlays (Predators Phase 0).** STRENGTH ≠ DANGER:
the wire carries four RAW components on `HerdTelemetryState` — `attack` / `defense` (open-ended, against
the human-strength anchor 1.0) and `ferocity` / `aggression` (native 0..1, fights-back-vs-flees /
initiates-unprovoked) — and **danger is DERIVED, never a stored field**. There are TWO: **HUNT danger =
`attack × ferocity`** (cost to hunt it — a mammoth reads high) and **THREAT = `attack × aggression`**
(menace unprovoked — a grazer reads ~0 however deadly it is to hunt; predators read high in Phase 1).
Both are per-ENTITY, NOT per-tile wire fields, so the **native decoder projects each onto tiles**
(`snapshot_dict`, beside the pasture/forage blocks): a grid-sized zero-init array, `max(existing, value)`
stamped at each herd's tile index, normalized against **that channel's own** map-max. Each is **guarded
on its own max > 0** — in Phase 0 nothing is aggressive yet, so `threat` is typically absent, and that
is correct. Neither is a two-tone ramp: MapView's `_color_for_tile` rides the generic
`GRID_COLOR.lerp(overlay_color, value)` path off `OVERLAY_COLORS` (`HUNT_DANGER_OVERLAY_COLOR` orange /
`THREAT_OVERLAY_COLOR` red, so the two read apart) — empty ground stays grid-colored — and the generic
scalar legend handles both. The overlay selector + legend are data-driven off `channel_order`, so the
channels appear with no OverlayPanel edit. **The herd drawer shows the four RAW components, NOT a verdict
word** (`Hud._herd_summary_lines` → `_append_danger_component_lines`, after Ecology, on EVERY herd): a
word can't survive the roster (a mammoth and later mech-infantry can't both be "Deadly"), so each is a
relative bar + raw value, Elevation-style — **Attack** / **Defense** bar against the max across
`_world_herds` (falling back to the bare value with no reference), **Fights back** / **Aggressive** as a
0..1 bar + %, plus a compact derived `Danger: Hunt X · Threat Y` summary. No `_format_detail_bbcode` tint
case — the component rows carry no verdict word. Verify via `map_preview` states **"hunt_danger"**
(`map_hunt_danger.png` — mammoth + wolf glow orange) / **"threat"** (`map_threat.png` — only the
aggressive wolf glows red) + the printed legends, and `ui_preview` `herd_verbs` (harmless deer, all-empty
bars) / `herd_danger` (mammoth: high Attack/Fights-back, empty Aggressive), whose behavioural assertions
prove the component rows render and NO Harmless/Deadly word appears.

**`pasture` — the graze (pasture) layer, Grazing Phase 2a** (`docs/plan_grazing_foundation.md`;
`core_sim/CLAUDE.md` → The Graze (Pasture) Layer). Graze is the **animal-edible** vegetal stock
(grass and browse — cellulose humans cannot digest), the twin of the **human-edible** `ForagePatch`
biomass, and it sits on nearly every land tile with its own per-biome distribution. Four things about
this channel are load-bearing:
- **It is NOT a wire raster.** Graze rides `TileState` (per-entity diffed → zero delta bytes on an
  ungrazed turn), so the channel is **assembled in the native decoder from the tiles**
  (`snapshot_dict`'s `OverlaySlices.pasture_capacity`), exactly as the logistics fallback already is.
  Everything downstream — MapView's channel ingest, the OverlayPanel selector, the legend — then works
  with no special-casing. (Do **not** synthesize it client-side in MapView the way `province` is: a
  MapView-only channel never reaches OverlayPanel's selector, so it can't be picked.)
- **It paints CAPACITY, not fill.** "How good a pasture is this ground?" is the question the layer
  exists to answer (is prairie really pasture; is forest really poor?) and it is a property of the
  biome. The *fill* (`biomass / capacity` — "how eaten-down is it?") is a different question: it rides
  the legend as a map-wide standing-stock %, and per-tile on the tile card. It earns its own ramp only
  once herds actually eat graze (Phase 2b).
- **Zero pasture is NOT low pasture, and the ramp must never say it is.** A desert at 8/8 (full, but
  marginal) and a glacier that carries no pasture at all are completely different facts — and a naive
  `biomass/capacity` ratio renders BOTH as 100%. So capacity 0 leaves the ramp entirely:
  `MapView._pasture_color` paints **water** (Water terrain tag — server truth, not the render-side
  `blend_class`) a drowned slate and **dead land** a bare rock-violet, while any positive capacity
  starts at `PASTURE_POOR_COLOR` straw. The normalization is against the map's **richest** pasture, not
  min-max (min-max would rebase the ramp onto the worst *land* value and make a marginal desert read
  like a dead glacier).
- **Its legend is its own** (`_build_pasture_legend`, not `_build_scalar_overlay_legend`): the generic
  builder reports min/avg/max over EVERY tile, and here the map-wide min is 0 (the sea), which would
  report the world's poorest pasture as "0". Rows: Poorest / Average / Richest **over the tiles that
  actually carry pasture**, then `Barren ground` + `Water` counts. Keep row labels short — the legend
  panel clips.

Verify with `map_preview` state **"pasture"** (`map_pasture.png` — an earthlike-shaped map; it also
prints the legend dict, since that harness has no HUD) and `ui_preview` `pasture_legend` /
`tile_pasture_stressed` / `tile_pasture_none` (+ `food_tile`, which carries both stocks). **The live
earthlike map generates zero forest** (the biome palette thins `MixedWoodland`/`BorealTaiga` out
entirely — tracked in `core_sim/CLAUDE.md`), so the forest-is-poor-pasture inversion the two-stock
split exists to create is **unobservable in a live frame**; `map_preview`'s fixture stages a woodland
block deliberately so it can be seen at all.

**`forage` — the human-food layer, the twin of `pasture`** (`docs/plan_grazing_foundation.md` §1.1;
`core_sim/CLAUDE.md` → The two food webs). Forage is the **human-edible** potential of a tile — seeds,
nuts, tubers, fruit and inshore fish — from `forage.capacity_by_biome` (`labor_config.json`), the
mirror table of graze's. It is a **per-tile POTENTIAL on every tile**, exactly like pasture (NOT the
sparse per-`ForagePatch` stock), sourced from a new per-tile `TileState.forageCapacity`. Built the SAME
way as pasture — assembled in the native decoder (`OverlaySlices.forage_capacity`, from
`tile.forageCapacity()` in the tiles loop), normalized against the map's **richest** forage tile, and
cached client-side in `MapView.tile_forage` (from `tile_to_dict`'s `forage_capacity`, only tiles > 0)
for the legend's Poorest/Average/Richest figures. **THE ONE THING THAT DIFFERS FROM PASTURE:** "no
forage" and "no pasture" mean **opposite** things, and the render must not lie about it —
- **Water is NOT uniformly barren.** ContinentalShelf (130) / CoralShelf (180) / InlandSea (110) carry
  real fishing potential and sit **ON the ramp**, so coastal shelves **glow** on the forage map where
  they are dead water on the pasture map — the signature divergence of the two food webs. Only
  genuinely-zero biomes (DeepOcean, Glacier, lava, salt flat) leave the ramp.
- **There is NO "land but no site" middle category and NO Water off-row.** `MapView._forage_color` is a
  straight twin of `_pasture_color` minus the water/dead split: `normalized > 0` → the wheat→green ramp
  (`FORAGE_POOR_COLOR`→`FORAGE_RICH_COLOR`, a distinct green from pasture's so the two layers read
  apart); `normalized <= 0` → the single `FORAGE_BARREN_COLOR` slate. (A dark forage tile can be
  perfectly good FARMLAND — the barren fill is only the genuinely-zero biomes.)
- **Its legend is its own** (`_build_forage_legend`): Poorest/Average/Richest over the tiles that carry
  forage, then **one** honest `No forage` barren row (no Water row — shelves are on the ramp). The
  description carries a **`Gathering sites: N tiles`** sub-count (from `MapView.food_sites`, the tiles
  you can actually forage today — a subset of the potential), so the ramp reads as POTENTIAL without
  calling the rest of the land dead.

Verify with `map_preview` state **"forage"** (`map_forage.png`, same earthlike fixture as `map_pasture`
so the two compare tile-for-tile — forest/river valleys read RICH on forage where prairie/steppe reads
richest on pasture, and the shelf column glows on forage where it is barren on pasture; it prints the
legend dict) and `ui_preview` `forage_legend` (the honest twin — `No forage` barren row, no Water row,
the gathering-sites sub-count). The forage `capacity_by_biome` table ships in the sim, so the live
inversion is real; the fixture stages it deterministically for the harness.

---

## Typography & Theming

> **This section described a system that does not exist.** There is **no
> `INSPECTOR_FONT_SIZE` constant** anywhere in the client, no shared `Theme`
> resource applied to the root `CanvasLayer`, and no `body`/`heading`/`caption`/
> `legend`/`control` typography map. `Typography.gd` is a **37-line no-op shim** —
> `apply()`, `apply_theme()`, `theme()` and `size_for()` all return null or do
> nothing. Only `DEFAULT_FONT_SIZE := 18` and `base_font_size()` carry real values,
> consumed at a handful of `Inspector.gd` call sites.

**What actually works today:** set sizes directly with
`add_theme_font_size_override`, as `TurnOrb.gd` does (`GLYPH_FONT_SIZE`,
`BADGE_FONT_SIZE`) and `NarrativeForkPanel.gd` does for its prose. The live base
size is `Inspector.get_resolved_font_size()`.

**The palette authority is `HudStyle.gd`**, and it is real: `SIGNAL`,
`SIGNAL_WASH`, `DANGER`, `WARN`, `HEALTHY`, `INK`, `INK_DIM`, `INK_FAINT`,
`GROUND`, `PANEL_SOLID`, `LINE_SOFT`, plus `card_stylebox()`, `banner_stylebox()`,
`empty_stylebox()`, `apply_button(btn, "primary"|"ghost")`. **No hardcoded hexes**
— the one surviving exception is documented at its call site.

Building a panel that expects `Typography` to style it is the trap this note
exists to prevent; it fails silently, since every method returns without error.

---

## Build overlay

The bottom-centre `build  cli <x> · srv <y>` overlay (`Hud._refresh_build_overlay`) confirms the
running client+server builds at a glance. **The `cli` value is a git STAMP, not a hand-bumped
constant** — mirroring the server's `CORE_SIM_BUILD_ID` (`core_sim/build.rs`). GDScript has no
compile step, so `scripts/run_stack.sh` writes the stamp (`<commit-date>-<short-hash>`, plus
`-dirty` when the tree has uncommitted edits — e.g. `2026-07-20-6dd31f9-dirty`) to
`res://build_stamp.txt` in its client-build block; on any git failure it removes the file.
`src/scripts/ClientBuild.gd` (a static-func helper, no `class_name`, preloaded by `Hud` — same
pattern as `ServerPortsFile`) reads it and **fails silently to the fallback** when absent: a bare
`godot` / ui_preview launch writes no stamp, so it reads `Hud.CLIENT_BUILD` = **`"dev-unknown"`**
(matching the server's fallback). `build_stamp.txt` is gitignored (a per-launch artifact, like the
ports file). **Do NOT hand-bump `CLIENT_BUILD`** — the git stamp is the source of truth, so the
shown build can never go stale.

---

## Scripting Capability Model

QuickJS sandbox for user scripts. Implemented in the **Godot native extension**
(`native/src/runtime.rs`, `rquickjs`) — *not* in `core_sim`, which has no script
code. Each script runs on its own OS thread with its own `Runtime`/`Context`,
talking to the host over mpsc channels, ticked from Godot's `_process`.

**Much of the model below is designed but unbuilt.** Status is marked per item;
see TASKS.md § Script Sandbox Hardening for the open work. Treat anything marked
_planned_ as a design note, not a description of current behaviour.

### Capability Families
| Capability | Status |
|---|---|
| `telemetry.subscribe` | **live** — snapshot/delta topics, subscription-filtered |
| `storage.session` | **live** — persisted with saves via `SimScriptState` |
| `alerts.emit` | **live** |
| `commands.issue` | **live, but ungated** — see the warning below |
| `ui.compose` | _declared only_ — in the capability registry (`sim_runtime/src/scripting.rs`) but **no handler arm** exists; a call logs "Unhandled host request" |

The JS surface is 8 globals assembled onto `globalThis.host` by a prelude
(`register`, `log`, `request`, `capabilities`, `sessionGet`, `sessionSet`,
`sessionClear`, `emit`). Capability families other than those are **string `op`
values passed to `host.request`**, routed in `handle_host_request`.

> **`commands.issue` is not sandboxed.** The "vetted command endpoints with
> throttle windows" phrasing is aspirational. In practice a script declaring
> this capability may submit **free-form command lines** (`payload.line`) at the
> same privilege as the player's own console — either via GDScript or, if a
> command endpoint is configured, over a raw `TcpStream` straight from the
> script thread, bypassing Godot entirely. There is no allowlist and no throttle.

### Determinism
Scripts are **not deterministic and not replay-safe**: they receive the raw
QuickJS globals (`Context::full`, so unseeded `Math.random()` and `Date`), and
tick off Godot's frame loop rather than sim turns. Do not host
simulation-authoritative or replay-sensitive logic here — see
`docs/plan_the_telling.md` §1a, where this ruled the sandbox out as a host for
the narrative beat engine.

### Script Distribution
- Discovery: recursive scan of `res://addons/shared_scripts` and `user://scripts`
  for `manifest.json`. **live**
- `.sscmod` bundles (zip), Ed25519 signatures, workshop feeds — _planned_,
  none implemented.

### Lifecycle
- Manifest validation on load (unknown capabilities rejected, subscriptions must
  be covered by a declared capability). **live**, in Rust
  (`sim_runtime/src/scripting.rs`).
- Explicit user-driven enable/disable/reload via `ScriptManagerPanel.gd`. **live**
- Hot reload via esbuild-lite bundling — _planned_.
- Suspension on sandbox violations — _planned_. There is a soft 8 ms tick budget
  (`SCRIPT_TICK_BUDGET_MS`) that is measured **after the fact** and only logs a
  warning; there is no memory limit, stack limit, interrupt handler, or
  preemption, so an infinite loop in `onTick` hangs that script's thread
  permanently.

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
| `L` | Show/hide the Terrain Types legend (**hidden by default**, persisted) |
| `V` | Show/hide the Victory panel (**hidden by default**, persisted) |
| `R` | Show/hide the Command Feed (**hidden by default**, persisted) |
| Double-click herd | Quick-assign the player band's idle workers to hunt it (Sustain) |
| `Esc` | Close the compose sheet, else cancel targeting, else open/close the pause menu (`Main.escape_claimant`) |

---

## See Also

- `README.md` - Setup and running instructions
- `docs/godot_inspector_plan.md` - Inspector migration progress
- `core_sim/CLAUDE.md` - Simulation engine (snapshot contracts, commands)
- `docs/architecture.md` - Cross-system data flow
