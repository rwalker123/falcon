
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

