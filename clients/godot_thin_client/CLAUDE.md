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

# Gate the FlatBuffers -> Dictionary decode path (run after ANY snapshot-field change)
cargo xtask decode-guard
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

## HUD Module Architecture — keeping `Hud.gd` decomposed

`Hud.gd` (`class_name HudLayer`) was a **~9,850-line god-file**; it is now a **~1,400-line
coordinator** across 21 modules (`docs/plan_hud_decomposition.md`). It stays small only if new code
lands in the module that matches its KIND. **Before adding anything to `HudLayer`, ask which of these
it is — `HudLayer` itself is none of them.** (The Inspector has the parallel "Tab-panel extraction
pattern" below; this is the HUD's.)

**The module taxonomy** (all under `src/scripts/ui/hud/`):
- **State models** (`RefCounted`, pure DATA, no nodes) — cross-cluster snapshot/selection state:
  `HudSelectionState` (what's selected), `HudBandLaborState` (the digested player world + optimistic
  overlay + the thin band-labor readers), `ComposeState` (what's being dialed but not committed). A
  field read by two+ clusters goes on a model, **not** as a `HudLayer` member.
- **All-`static`, stateless shared layers** — pure logic shared by 2+ clusters, with state passed as
  PARAMETERS (never a `_hud` back-ref): `SourceForecast` (yield/forecast math), `HudWidgets` (the
  widget factory), `HudFormat` (string/vocab format), `DetailFormat` (BBCode detail render + its pure
  producers). New shared math/format/widget code goes here; if a helper needs HUD state, thread it in.
- **Vocab modules** (`class_name`d, ALL-`const`, zero funcs/vars) — the topic word/glyph/format/
  threshold tables: `HudConst` (the universal leaf, reads nothing) + `Hud{Work,Compose,Flora,
  Expedition,Attention,Selection,Disclosure}Vocab`. **A new label / glyph / threshold goes in the
  matching vocab module — NEVER as a fresh `const` on `HudLayer`.** That block WAS the merge-conflict
  surface the whole arc removed; regrowing it re-creates the problem.
- **Controllers** (`RefCounted`) — one interactive cluster each, owning its nodes + per-cluster state +
  render/build/dispatch: `SelectionCardController`, `DrawerComposeController`, `SubjectDrawerController`,
  `BandPanelController`, `TargetingController`, `AttentionController`, `TurnOrbController`,
  `TopBarReadouts`, `DisclosureController`, `BandDetailLines`, `LegendController`,
  `CommandFeedController`, `DockRowController`. A new interactive feature EXTENDS the owning
  controller or is a NEW controller — it does **not** land inline on `HudLayer`.
- **`HudLayer` = the coordinator ONLY** — the `_ready` wiring, the reflective snapshot/selection ENTRY
  points `Main` calls, thin reflective delegators, and signal relays. It HOLDS the controllers + models
  as members; it does not hold feature logic. **A cluster of feature methods accreting on `HudLayer`
  IS the smell — extract it.**

**The rules that keep it safe** (each learned the hard way — see `docs/plan_hud_decomposition.md`):
- **Reflective / harness-reached methods stay as thin `HudLayer` delegators.** `Main.gd` reaches
  `HudLayer` by `has_method` / `has_signal`, and a failed probe **fails SILENTLY** (no error — the
  wiring simply never happens). So a controller emits its OWN signals and `HudLayer` RELAYS them, and a
  `has_method`-probed name is never moved off `HudLayer` (it keeps a thin delegator). The same applies
  to the `ui_preview` / `band_panel_preview` harnesses, which poke some `HudLayer` privates by DIRECT
  field access — those hard-error on a move (budget for the repoints).
- **Watch the hidden member straddle.** A bare `bool` / `int` / `Dictionary` written by one cluster and
  read by another is invisible to a call-graph scan and silently welds the two. **Before splitting a
  cluster, grep for shared MEMBERS, not just shared functions** — this bit the arc four times
  (`_grid_wrap_horizontal`, the three tint scalars, `_food_flow_present`, `_band_zone_tier`).
- **"An injection you still have to hold is relocated, not eliminated."** Moving a helper out while
  keeping the Callable to reach it has not reduced coupling. The real win is a controller holding a
  typed collaborator ref (`TargetingController` collapsed BandPanelController 6→3), or calling an
  all-`static` layer directly (TopBarReadouts is now injection-free).
- **A `const` moves iff EVERY reader moved — but dependency DIRECTION outranks that rule.** A leaf
  (`HudStyle`, a vocab module) must NEVER be made to depend on `HudLayer`; a stray reader becomes a
  **downward alias** instead. And `const` initializers evaluate at class load, so a cross-class
  const-ref **cycle fails to load the WHOLE client** — keep vocab leaves acyclic (`HudConst` reads
  nothing) and honor the co-location constraints noted on the vocab-module row.
- **Extract shared layers BEFORE controllers.** A controller pulled out over a still-inline shared
  layer needs a dozen Callable injections to reach it; extracting the all-`static` layer first drops
  that surface dramatically (this took `DrawerComposeController` from 36 injections to 3).
- **A `RefCounted` can't `add_child` or `get_tree()`** — pass the HUD `CanvasLayer` as a host `Node`
  and parent / `await` through it (the `TurnOrbController` fork-panel pattern). **Reparenting a `%Name`
  node clears `unique_name_in_owner`** — pass scene nodes by reference, never reparent them.

**The process when a cluster genuinely needs extracting:** MEASURE first (grep the surface — functions
AND members AND every reflective/harness seam), verify each "X-only" claim's REACHABILITY (not just its
presence — a dead branch is not a dependency), then extract behaviour-neutral with `ui_preview` /
`band_panel_preview` / `marker_field_guard` as the safety net. The same discipline applies to the other
big client files that were decomposed the same way (the native `lib.rs` module map below; `Inspector.gd`
→ per-tab panels).

---

## Key Scripts Reference

| Script | Purpose |
|--------|---------|
| `Main.gd` | Scene orchestration, streaming toggle. On boot sends the `new_game <preset> <w> <h> <seed> <profile>` command (built from the `GameLaunch` autoload handoff, or a dev default) since the server now boots idle and only generates a world on `new_game`. Owns the `$PauseLayer` ESC overlay: ESC opens/closes the pause `MenuShell`, but yields to MapView's targeting-cancel when `hud.is_targeting_active()` |
| `ui/MenuShell.gd` (`ui/MenuShell.tscn`) | The ONE shared menu surface (DRY) for BOTH the landing screen and the ESC pause menu; `mode` ("landing"\|"pause") re-filters a single registry-driven nav and re-lays-out (full-bleed vs centered card over a scrim). Built in code, styled through `HudStyle`. New Game pane = preset picker (earthlike / polar_contrast) + map-size picker (from `MapSizes.OPTIONS`, Standard default) + seed field. Functional items emit `new_game_requested`/`resume_requested`/`abandon_requested`/`exit_requested`; Map Selection/Load/Save render inert placeholder panes. **The Options pane is live**: "Map pan speed" + "Zoom speed" sliders (ranges/step from the `ClientSettings` consts) that apply-live + persist immediately via `ClientSettings`, plus an enabled "Restore defaults" that resets both. It opens in BOTH landing and pause modes, so it works pre-run and in-run |
| `ui/LandingScreen.gd` (`ui/LandingScreen.tscn`) | The boot main-scene (`project.godot` run/main_scene): a MenuShell in landing mode over a dark ground. `new_game_requested` stashes params in `GameLaunch.pending_new_game` and swaps to `Main.tscn`; `exit_requested` quits |
| `MapSizes.gd` | Canonical 5-entry map-size list (`OPTIONS` + `DEFAULT_KEY`), shared by `MapPanel` and `MenuShell` (DRY) |
| `GameLaunch.gd` (autoload) | Cross-scene handoff: `pending_new_game` dict set by LandingScreen, consumed + cleared by `Main._build_new_game_command` |
| `ClientSettings.gd` (autoload) | The first general client-settings store — a `ConfigFile` wrapper over `user://client_settings.cfg` (`[map]` section) modelled on `BandCityPanel`'s `_load_prefs`/`_save_prefs`. Holds `pan_speed_multiplier` / `zoom_speed_multiplier` (defaults 1.0, each clamped to [0.25, 3.0]); the BASE unit speeds stay as consts in `MapView`, these SCALE them. Setters clamp → `_save` → emit `changed`; `restore_defaults` resets both; `config_path_override` (static) isolates the file for tests. **No `class_name`** (it would clash with the autoload name). Read LIVE by `MapView` (keyboard + trackpad pan, all zoom paths — wheel/pinch/Q·E/zoom-rail); written by the Options pane |

## Where the rest of this document lives

This file is the **hub**: build/verify commands, the `Hud.gd` decomposition
invariant, the boot/menu/settings scripts, scene structure and data flow,
theming, the build overlay and hotkeys — the things true of *all* client work.
Everything else lives in `.claude/rules/client/`, scoped with `paths:`
frontmatter so a file loads only when you touch the code it describes.

**The Key Scripts Reference table went with them.** It was 169KB — 31% of this
file — and a per-script index is exactly the thing that should arrive with the
script. Each rule file below carries a `## Key scripts` table holding the rows
for the scripts it covers. The boot/menu/settings rows stay above.

| Rule file | Covers | Loads when you touch |
|---|---|---|
| `hud-modules.md` | `Hud.gd` + every `ui/hud/` module and vocabulary leaf | `Hud.gd`, `ui/hud/**` |
| `labor-ui.md` | The compose sheet, labor allocation, source forecasts, arrivals | `ComposeSheet.gd`, `ComposeState.gd`, `SourceForecast.gd` |
| `selection-card.md` | ONE card, ONE list, ONE drawer; the land as a subject | `SelectionCardController.gd`, `SubjectDrawerController.gd` |
| `band-readouts.md` | Demographics, food, morale, wellbeing, habitability, climate | `BandDetailLines.gd`, `TopBarReadouts.gd`, `BandFoodStatus.gd` |
| `herd-readouts.md` | Fog gate, herd ecology, husbandry, corral, the pen | `PenStatus.gd`, `FaunaPanel.gd` |
| `land-readouts.md` | Forage, "what grows here", the crop picker, pasture, the meters | `hud_flora_vocab.gd`, `FoodIcons.gd` |
| `turn-orb.md` | Band alerts and the attention model | `AttentionController.gd`, `TurnOrbController.gd` |
| `targeting.md` | Move-band and the scouting/hunting expeditions | `TargetingController.gd` |
| `band-city-panel.md` | The 4-edge dockable command centre | `BandCityPanel.gd`, `BandPanelController.gd` |
| `panel-framework.md` | Docked `PanelCard`s, `DockScrollFit`, `AutoSizingPanel` | `PanelCard.gd`, `PanelDock.gd` |
| `terrain-blend-shader.md` | The per-pixel biome-blend shader: blend, shore, canopy, peaks, rivers | `*.gdshader`, `TerrainRenderer.gd` |
| `terrain-textures.md` | Atlas assets, `terrain_config.json`, loading, the 2D pipeline | `TerrainTextureManager.gd` |
| `map-renderers.md` | `MapView`'s renderer decomposition and the 2D minimap | `MapView.gd`, `Minimap*.gd`, `*Renderer.gd` |
| `map-markers.md` | The layered hex-icon stack UX | `BandMarkerRenderer.gd`, `SecondaryMarkerRenderer.gd` |
| `overlay-channels.md` | Selected-band/herd overlays, annotations, trade links | `BandOverlayRenderer.gd`, `AnnotationRenderer.gd` |
| `inspector-panels.md` | Every `ui/inspector/` panel | `Inspector.gd`, `ui/inspector/**` |
| `telling-panel.md` | The Telling book UX and the narrative fork | `TellingPanel.gd`, `NarrativeForkPanel.gd` |
| `sprites-widgets.md` | Sprites, icons, `HudStyle`, small widgets | `*Sprites.gd`, `HudStyle.gd`, `IconSprites.gd` |
| `test-harnesses.md` | `ui_preview`, `map_preview`, `blend_probe`, `decode_guard`, `marker_field_guard` | `tools/**` |
| `native-extension.md` | The GDExtension module map | `native/src/**` |
| `scripting-capability.md` | The scripting capability model | `src/scripts/scripting/**` |

**Cross-reference convention.** A quoted phrase like `see "Map markers"` names a
*section heading*, not a file. Resolve it with
`grep -rn '^#* Map markers' .claude/rules/client/`. Directional words
("below"/"above") are only reliable *within* one file.

**Adding to these docs.** Put per-arc rationale in the rule file that owns the
arc — that is what keeps two concurrent worktrees off the same file. A new
script's row goes in that rule's `## Key scripts` table, not here. Only add here
if it is true of all client work.


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

**Speed scaling:** WASD pan / Q·E zoom, the trackpad pan + pinch gestures, and mouse-wheel zoom are
all scaled by the Options menu's **Map pan speed** / **Zoom speed** sliders (`ClientSettings`,
read live in `MapView`). **Mouse-drag pan (right/middle drag) stays 1:1** — deliberately unscaled.

---

## See Also

- `README.md` - Setup and running instructions
- `docs/godot_inspector_plan.md` - Inspector migration progress
- `core_sim/CLAUDE.md` - Simulation engine (snapshot contracts, commands)
- `docs/architecture.md` - Cross-system data flow
