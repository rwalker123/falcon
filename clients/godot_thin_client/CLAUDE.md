# Godot Thin Client

<!-- HUB BANNER — source of truth: scripts/hub_banner_client.md, emitted into
     clients/godot_thin_client/CLAUDE.md right after the H1 by scripts/split_claude_md.sh.
     Edit the source file; an edit made only in the hub is reverted by the next
     re-run. Verify the two agree with: scripts/split_claude_md.sh --check -->

> ## ⛔ THIS IS A HUB FILE — rationale does NOT go here
>
> Before adding a paragraph, section, callout, or per-script row **anywhere in this file**, ask:
> **is this true of *all* Godot-client work?**
>
> - **No** — it explains one panel, one overlay, one shader, one HUD module, one script's job →
>   it belongs in the **rule file that owns the arc** (`.claude/rules/client/*.md`, routing table
>   below), and a new script's row goes in that rule's `## Key scripts` table. That is also what
>   keeps two concurrent worktrees off the same file.
> - **Yes** — a build/verify command, a socket/endpoint contract, a genuinely client-wide
>   invariant, or a **new row in the routing table** → here.
>
> This file loads into **every session in this repo**; a rule file loads only when you touch the
> code it describes, so a hub paragraph is paid for by every session forever. **If the owning
> rule's `paths:` already cover the code you changed, a hub copy is pure duplication** — the reader
> who could break the invariant loads the rule anyway. Root `CLAUDE.md` → "The hub files are not
> where rationale goes" has the long form.

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

**Sockets** (defaults — the client resolves each as env var → ports file → this constant):
- Snapshot stream: `127.0.0.1:41002` (FlatBuffers via `SimulationConfig::snapshot_flat_bind`)
- Command socket: `127.0.0.1:41001` (Protobuf `CommandEnvelope`)
- Log stream: `127.0.0.1:41003` (length-prefixed JSON tracing frames)

**THE STREAM PORT IS `snapshot_flat`, NOT `snapshot`.** `snapshot` is the legacy JSON snapshot socket;
the client consumes the **FlatBuffers** one. Reading the wrong key yields a client that connects to a
live socket and then **silently never renders** — no error, no frames — which is the easiest thing to
get wrong here and the hardest to diagnose.

The discovery precedence, the handshake-file path derivation and key contract, and
`ServerPortsFile.gd`'s degrade-silently design are one spec shared with the server —
`.claude/rules/core_sim/ports.md`, which loads on `ServerPortsFile.gd`, `Main.gd`/`LogsPanel.gd`
and the server's `port_alloc.rs`.

---

## HUD Module Architecture — keeping `Hud.gd` decomposed

`Hud.gd` (`class_name HudLayer`) was a **~9,850-line god-file**; it is now a **~1,400-line
coordinator** across 21 modules (`docs/plan_hud_decomposition.md`). It stays small only if new code
lands in the module that matches its KIND. **Before adding anything to `HudLayer`, ask which of these
it is — `HudLayer` itself is none of them:**

- **State models** (`RefCounted`, pure DATA, no nodes) — `HudSelectionState`, `HudBandLaborState`,
  `ComposeState`. A field read by two+ clusters goes on a model, **not** as a `HudLayer` member.
- **All-`static`, stateless shared layers** — `SourceForecast`, `HudWidgets`, `HudFormat`,
  `DetailFormat`. New shared math/format/widget code goes here, with state threaded in as
  PARAMETERS (never a `_hud` back-ref).
- **Vocab modules** (`class_name`d, ALL-`const`, zero funcs/vars) — `HudConst` +
  `Hud{Work,Compose,Flora,Expedition,Attention,Selection,Disclosure}Vocab`. **A new label / glyph /
  threshold goes in the matching vocab module — NEVER as a fresh `const` on `HudLayer`.** That block
  WAS the merge-conflict surface the whole arc removed; regrowing it re-creates the problem.
- **Controllers** (`RefCounted`) — one interactive cluster each, owning its nodes + per-cluster state
  + render/build/dispatch (13 of them, listed in the rule). A new interactive feature EXTENDS the
  owning controller or is a NEW controller — it does **not** land inline on `HudLayer`.
- **`HudLayer` = the coordinator ONLY** — `_ready` wiring, the reflective entry points `Main` calls,
  thin delegators, signal relays. **A cluster of feature methods accreting on `HudLayer` IS the smell
  — extract it.**

The six invariants that make an extraction safe (silent `has_method` probes, the hidden member
straddle, relocated-vs-eliminated injections, `const` direction and load cycles, shared-layers-first,
`RefCounted` node limits) and the measure-then-extract process are in
`.claude/rules/client/hud-modules.md`, which loads on `Hud.gd` and `ui/hud/**` — i.e. exactly when
you are in a position to break them. The Inspector has the parallel "Tab-panel extraction pattern"
below; this is the HUD's.

---

## Key Scripts Reference

| Script | Purpose |
|--------|---------|
| `Main.gd` | Scene orchestration, streaming toggle. On boot sends the `new_game <preset> <w> <h> <seed> <profile>` command (built from the `GameLaunch` autoload handoff, or a dev default) since the server now boots idle and only generates a world on `new_game`. Owns the `$PauseLayer` ESC overlay: ESC opens/closes the pause `MenuShell`, but yields to MapView's targeting-cancel when `hud.is_targeting_active()` |
| `ui/MenuShell.gd` (`ui/MenuShell.tscn`) | The ONE shared menu surface (DRY) for BOTH the landing screen and the ESC pause menu; `mode` ("landing"\|"pause") re-filters a single registry-driven nav and re-lays-out (full-bleed vs centered card over a scrim). Built in code, styled through `HudStyle`. New Game pane = preset picker (earthlike / polar_contrast) + map-size picker (from `MapSizes.OPTIONS`, Standard default) + seed field. Functional items emit `new_game_requested`/`resume_requested`/`abandon_requested`/`exit_requested`; Map Selection/Load/Save render inert placeholder panes. **The Options pane is live**: a "Fog of war" toggle row (`_make_toggle_row`) plus "Map pan speed" + "Zoom speed" sliders, all applying live + persisting immediately via `ClientSettings`, plus an enabled "Restore defaults". It opens in BOTH landing and pause modes, so it works pre-run and in-run. For the Options rows' boundary rules see `.claude/rules/client/fog-of-war.md` |
| `ui/LandingScreen.gd` (`ui/LandingScreen.tscn`) | The boot main-scene (`project.godot` run/main_scene): a MenuShell in landing mode over a dark ground. `new_game_requested` stashes params in `GameLaunch.pending_new_game` and swaps to `Main.tscn`; `exit_requested` quits |
| `MapSizes.gd` | Canonical 5-entry map-size list (`OPTIONS` + `DEFAULT_KEY`), shared by `MapPanel` and `MenuShell` (DRY) |
| `GameLaunch.gd` (autoload) | Cross-scene handoff: `pending_new_game` dict set by LandingScreen, consumed + cleared by `Main._build_new_game_command` |
| `ClientSettings.gd` (autoload) | The first general client-settings store — a `ConfigFile` wrapper over `user://client_settings.cfg` (`[map]` section) modelled on `BandCityPanel`'s `_load_prefs`/`_save_prefs`. Holds `pan_speed_multiplier` / `zoom_speed_multiplier` (defaults 1.0, each clamped to [0.25, 3.0]) — the BASE unit speeds stay as consts in `MapView`, these SCALE them — and `fog_of_war_enabled` (default `true`; the rules governing that key are in `.claude/rules/client/fog-of-war.md`). Setters clamp → `_save` → emit `changed`; `restore_defaults` resets all three; `config_path_override` (static) isolates the file for tests. **No `class_name`** (it would clash with the autoload name). Read LIVE by `MapView` (keyboard + trackpad pan, and the CONTINUOUS zoom paths — wheel/pinch/Q·E); written by the Options pane |

<!-- HUB ROUTING BLURB — source of truth: scripts/hub_blurb_client.md, appended into
     clients/godot_thin_client/CLAUDE.md by scripts/split_claude_md.sh. Edit the source file; an
     edit made only in the hub is reverted by the next re-run.
     Verify the two agree with: scripts/split_claude_md.sh --check -->

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
| `event-dock.md` | The notification bar: importance rungs, channels, the pinned alert, `seq` de-dup | `EventDockPanel.gd`, `hud_event_vocab.gd` |
| `panel-framework.md` | Docked `PanelCard`s, `DockScrollFit`, `AutoSizingPanel` | `PanelCard.gd`, `PanelDock.gd` |
| `terrain-blend-shader.md` | The per-pixel biome-blend shader: blend, shore, canopy, peaks, rivers | `*.gdshader`, `TerrainRenderer.gd` |
| `terrain-textures.md` | Atlas assets, `terrain_config.json`, loading, the 2D pipeline | `TerrainTextureManager.gd` |
| `map-renderers.md` | `MapView`'s renderer decomposition and the 2D minimap | `MapView.gd`, `Minimap*.gd`, `*Renderer.gd` |
| `fog-of-war.md` | Fog of war is server-owned: preference → command → snapshot → render | `MapView.gd`, `Main.gd`, `MenuShell.gd`, `ClientSettings.gd` |
| `map-markers.md` | The layered hex-icon stack UX | `BandMarkerRenderer.gd`, `SecondaryMarkerRenderer.gd` |
| `overlay-channels.md` | Selected-band/herd overlays, annotations, trade links | `BandOverlayRenderer.gd`, `AnnotationRenderer.gd` |
| `inspector-panels.md` | Every `ui/inspector/` panel | `Inspector.gd`, `ui/inspector/**` |
| `workbench.md` | The designer surface replacing the Inspector: shell, page registry, config tuning | `ui/workbench/**`, `tools/workbench_*` |
| `telling-panel.md` | The Telling book UX and the narrative fork | `TellingPanel.gd`, `NarrativeForkPanel.gd` |
| `sprites-widgets.md` | Sprites, icons, `HudStyle`, small widgets | `*Sprites.gd`, `HudStyle.gd`, `IconSprites.gd` |
| `test-harnesses.md` | `ui_preview`, `map_preview`, `blend_probe`, `decode_guard`, `marker_field_guard`, `inspector_hidden_guard` | `tools/**` |
| `turn-profiling.md` | Where an applied snapshot's time goes (the client costs ~10× the sim), the `TurnProfile` contract and its flag | `TurnProfile.gd`, `Main.gd`, `SnapshotLoader.gd`, `MapView.gd`, `bridge/decoder.rs` |
| `native-extension.md` | The GDExtension module map | `native/src/**` |
| `scripting-capability.md` | The scripting capability model | `src/scripts/scripting/**` |
| `../core_sim/ports.md` | Endpoint discovery, the ports handshake file (the server owns this contract) | `ServerPortsFile.gd`, `Main.gd`, `LogsPanel.gd` |
| `../core_sim/world-handoff.md` | Which world a frame belongs to: the reveal gate, retry-until-answered, resetting per-world caches (spans both halves) | `Main.gd`, `Hud.gd`, `MapView.gd`, `TopBarReadouts.gd`, `TellingPanel.gd` |

**Cross-reference convention.** A quoted phrase like `see "Map markers"` names a
*section heading*, not a file. Resolve it with
`grep -rn '^#* Map markers' .claude/rules/client/`. Directional words
("below"/"above") are only reliable *within* one file.

**Adding to these docs.** Per-arc rationale goes in the rule file that owns the
arc, and a new script's row in that rule's `## Key scripts` table; a new arc gets
a **row above**, not a section here. See the hub banner at the top of this file
for the test.

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

**There is no typography system.** Set font sizes directly with
`add_theme_font_size_override`, as `TurnOrb.gd` does (`HINT_FONT_SIZE`, `BADGE_FONT_SIZE`) and
`NarrativeForkPanel.gd` does for its prose. `Typography.gd` is a **no-op shim** — styling
through it fails *silently* — and the autopsy is in `.claude/rules/client/sprites-widgets.md`.

**The palette authority is `HudStyle.gd`**, and it is real: `SIGNAL`,
`SIGNAL_WASH`, `DANGER`, `WARN`, `HEALTHY`, `INK`, `INK_DIM`, `INK_FAINT`,
`GROUND`, `PANEL_SOLID`, `LINE_SOFT`, plus `card_stylebox()`, `banner_stylebox()`,
`empty_stylebox()`, `apply_button(btn, "primary"|"ghost")`. **No hardcoded hexes**
— the one surviving exception is documented at its call site.

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
| `F` | Toggle fog of war (server-owned — see `.claude/rules/client/fog-of-war.md`) |
| `T` | Toggle terrain textures |
| `I` | Hide/show inspector |
| `` ` `` | Hide/show the Workbench, the designer surface (**hidden by default**) — see `.claude/rules/client/workbench.md` |
| `L` | Show/hide the Terrain Types legend (**hidden by default**, persisted) |
| `V` | Show/hide the Victory panel (**hidden by default**, persisted) |
| `R` | Show/hide the **event dock** (the notification bar; **shown by default**, persisted) |
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
