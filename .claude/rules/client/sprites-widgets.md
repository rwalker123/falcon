---
paths:
  - "clients/godot_thin_client/src/scripts/ui/{HudStyle,HudPalette,IconSprites,FoodIcons,FaunaSprites}.gd"
  - "clients/godot_thin_client/src/scripts/ui/{SiteSprites,WonderSprites,StageSprites}.gd"
  - "clients/godot_thin_client/src/scripts/ui/{CropRoleSprites,FloraSprites}.gd"
  - "clients/godot_thin_client/src/ui/MagnifierButton.gd"
  - "clients/godot_thin_client/src/scripts/ui/{TileHabitability,TileClimate,RiverEdges,MinimapPanel}.gd"
  - "clients/godot_thin_client/src/scripts/{SnapshotStream,CommandClient,Typography}.gd"
  # The Theme row and the restart it now performs — the palette's only player-facing controls.
  - "clients/godot_thin_client/src/scripts/ui/MenuShell.gd"
  - "clients/godot_thin_client/src/scripts/GameLaunch.gd"
---

<!-- Extracted verbatim from lines 195-200;202-202;204-210 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

<!-- HAND-EDITED SINCE EXTRACTION: the Typography autopsy below was moved down out of the
     client hub (the hub keeps the 3-line actionable rule). A re-run of split_claude_md.sh
     without re-pinning the blob would drop it. -->

# Sprites, icons, styling and small widgets

## There is no typography system — `Typography.gd` is a no-op shim

The hub's rule is short (set sizes with `add_theme_font_size_override`; `HudStyle` owns the
palette). This is why. An earlier version of the docs described a system that **does not
exist**: there is **no `INSPECTOR_FONT_SIZE` constant** anywhere in the client, no shared
`Theme` resource applied to the root `CanvasLayer`, and no `body`/`heading`/`caption`/`legend`/
`control` typography map.

`src/scripts/Typography.gd` is a **37-line shim** — `apply()`, `apply_theme()`, `theme()` and
`size_for()` all return null or do nothing. **That is the trap: ~14 files across
`ui/inspector/` preload it and call `Typography.apply`, and every one of those calls is a
no-op.** Only `DEFAULT_FONT_SIZE := 18` and `base_font_size()` carry real values, and
`base_font_size()`'s sole caller is `Inspector.gd` (the live base size is
`Inspector.get_resolved_font_size()`).

**Building a panel that expects `Typography` to style it is the trap this note exists to
prevent** — every method returns without error, so it fails silently and looks like a layout
bug rather than a missing system.

## `[hint=…]` is unusable for prose — an apostrophe spills the rest of the label as raw markup

BBCode's per-run tooltip tag looks like the per-row hover seam a detail surface wants: the tile card
and the herd drawer are each ONE `RichTextLabel` holding every row, so there is no per-row `Control`
to hang a `tooltip_text` on, and `RichTextLabel.get_tooltip` answers a `[hint]` under the cursor.

**Godot terminates a tag with `_find_unquoted`, which treats `'` as a quote character.** A payload
containing *"this band's Agriculture"* therefore has no closing bracket as far as the parser is
concerned, and **the tag AND EVERY TAG AFTER IT render as literal text** — the card spills
`[hint=…][/color][table=2][cell]…` down the column from that row to the end of the label. Measured on
4.7: balanced `"` are fine, one `'` is fatal. That is the worst possible shape, most of this client's
prose carrying an apostrophe, and the failure is silent, total, and nowhere near the row that caused
it — the same species as the `Typography.gd` no-op above, one layer down.

**Put the tooltip on the LABEL instead** (`tooltip_text`), which is plain text and cannot be broken by
its own content. It answers over the whole detail block rather than over one row, so it is only honest
where the thing it explains is a fact about the SOURCE the card is showing; nothing else may be folded
into it, or the hover lies about which row the pointer is on.

## The checkbox indicator (`HudStyle.apply_checkbox`)

**The client applies NO `Theme` resource.** `minimal_theme.tres` sits in the project folder but is an
`@tool` script reading `EditorInterface.get_editor_settings()` — an EDITOR theme, referenced by
nothing in `project.godot` or any scene. Every control therefore wears Godot's stock theme, which is
drawn for a LIGHT surface. On a `CheckBox` that is fatal: the `unchecked` icon is a FILLED near-black
rounded square (`#191919` at 50% alpha), so against `HudStyle.PANEL_SOLID` it reserves its width and
paints nothing. The improvement control's offered row shipped for a while as a line of prose with no
control on it (issue #445) — the leading `◎`/`🌱` a player saw was `FoodIcons.for_policy`'s RUNG
glyph inside the label, not an indicator.

**`icon_normal_color` and friends cannot fix a CheckBox, and that is the trap worth naming.** Two
separate reasons, either one sufficient: the stock colour already resolves to opaque white and icon
colours MULTIPLY (no tint lightens a black square); and more fundamentally **`CheckBox` draws its
indicator itself, from the `checked`/`unchecked` theme ICONS, unmodulated** — the `icon_*_color`
items reach a `Button`'s `icon` PROPERTY and nothing else, so an `add_theme_color_override` on that
family is a silent no-op. A first cut of the fix set them and rendered a stark white box.

So `apply_checkbox(box)` **replaces the art**, rasterised in its final palette colour: an empty
rounded-box OUTLINE in `INK_DIM` for `unchecked` (an outline, because an empty box is what "you may
tick this" looks like; a fill would read as already ticked), and the stock tick art RECOLOURED to
`SIGNAL` for `checked` — recoloured rather than redrawn, since the tick's shape is the part worth
keeping, and `SIGNAL` because this HUD spends that colour on nothing but live state. The `_disabled`
twins mirror them (`INK_FAINT` outline, `SIGNAL`-recoloured disabled tick); neither arises today, a
gated rung being a Label and a running build never gated. The four textures are cached statically and
sized to the stock icon's 16px, so swapping them moves no metrics. FONT overrides DO land (the face is
the Button's own `text`) and are set here too.

It is applied **per control, not through a project theme**: `HudWidgets.build_improvement_control` is
the client's only `CheckBox` construction site, the Options pane's toggles being `CheckButton`s (a
different widget with its own art), and there is no theme resource to hang it on anyway.

**`ui_preview` guards it by CONTRAST and HUE, never by "an override is set"** — an override-shaped
assertion passes on the `icon_normal_color` version that renders nothing. On `herd_corral_ungated`:
the `unchecked` art composited over `PANEL_SOLID` must clear `CHECKBOX_INDICATOR_MIN_CONTRAST` (stock
scores ~0.001), and the `checked` art's colour, brightness divided out, must sit within
`CHECKBOX_TICK_COLOUR_TOLERANCE` of `SIGNAL` (stock grey scores ~0.65). The second measure is
deliberately not contrast: the stock tick chip is light and would clear a contrast bar unchanged.

## The modal dialog (`HudStyle.apply_dialog`)

Same root cause as the checkbox above, one widget over: **the client applies no `Theme` resource**, so
an `AcceptDialog` wears Godot's stock chrome — a LIGHT-grey surface, stock-blue focus rings and a
`Confirm` title bar with an ✕ — over a near-black cyan-accented console. Reported from playtest as
looking like it came from another application, which it did. `apply_dialog(dialog)` dresses one:
`dialog_stylebox()` on the `panel`, `INK` at `DIALOG_BODY_FONT_SIZE` on `get_label()`, and
`apply_button` on the two buttons — **`primary` on OK, `ghost` on Cancel**, so the committing,
irreversible half is the lit one and backing out is the quiet one.

**The surface is OPAQUE (`PANEL_SOLID`), not the card's 92% `PANEL`.** A card floats over map; a
prompt lands over a dense work board, and it is the one place that translucency costs legibility
rather than depth.

**THE TITLE BAR IS REMOVED, NOT RESTYLED (`borderless`), and the trap is why.** An embedded
subwindow's title bar is drawn by the **VIEWPORT**, not by the dialog: `Viewport._sub_window_update`
reads `title_font` / `title_color` / `title_height` / `embedded_border` off the **`Window`** theme
type, while an `add_theme_*_override` on an `AcceptDialog` is resolved against `AcceptDialog`. So
those overrides are accepted, reported as set, and never read — the `icon_normal_color`-on-a-CheckBox
trap exactly, one class boundary over rather than one widget over. **Judge every item by what
renders.** The `panel` stylebox is safe because `panel` is bound on `AcceptDialog` itself; the four
Window items are not.

That is only half of why the bar went. The other half is editorial: a bar reading `Confirm` above a
one-line question says nothing the question does not, and its ✕ is a third way to spell Cancel.
`borderless` deletes the whole decoration in one flag — which is what makes this a suppression rather
than a restyle, and why no `title_*` override is set at all. The dialog's `title` is still assigned
(it is the Window's NAME, which an unembedded dialog would show) and simply does not draw.

**It is applied per control, like `apply_checkbox` and for the same reason**: there is exactly ONE
`ConfirmationDialog.new()` in the client (`BandPanelController._confirm_destructive`, behind all four
of the Band panel's prompts), and no theme resource to hang it on. A second dialog site calls this.

**It is judged in a FRAME, not by an assertion** — `band_panel_preview`'s
`band_panel_settle_confirm` and `band_panel_clear_confirm`, both embedded subwindows that land in the
capture. There is no contrast/hue measure here as there is for the checkbox: the failure mode was a
whole surface reading as the wrong application rather than one indicator drawing nothing, and that is
a thing to look at.

## The palette is a THEME, chosen in Options and installed at boot

`HudStyle`'s colour block is swappable. `ui/HudPalette.gd` holds four themes — `ember` (the
default), `loam`, `kiln` and `console` (the original cyan-on-slate palette, preserved verbatim) —
and `HudPalette.apply(id)` installs one. `ClientSettings` persists the chosen id under `[ui] theme`
in `user://client_settings.cfg` and calls `apply()` from its `_ready`.

**RESTART TO APPLY, and the autoload ordering is what makes that free.** `ClientSettings` is an
autoload, so its `_ready` runs before the main scene is instantiated and therefore before the first
Control exists; every panel then reads the installed palette on its first and only build, and there
is no rebuild pass anywhere in the system. The Options row's setter (`ClientSettings.set_theme`)
deliberately does NOT re-apply — a live swap would leave every already-built Control wearing the old
palette — and the row carries an always-visible caption saying so, in `WARN` while the selection and
`HudPalette.applied_id` disagree.

**The row also ACTS on that requirement, with a "Restart now" button beside the picker.** Godot has
no live restart, so `GameLaunch.restart_client()` relaunches the process — `OS.create_process` on
`OS.get_executable_path()`, passing `--path <project>` under `OS.has_feature("editor")` because there
the executable is the Godot binary and a bare launch opens the Project Manager instead of the game.
The button exists only while a restart is owed: `MenuShell._refresh_theme_row` derives its visibility
from the same `selected_id != HudPalette.applied_id` comparison that words the caption, in that one
place, so the two can never disagree — and a permanently-present restart control in an options pane
is a run lost by accident.

**A pause-mode restart ENDS the run, and that is stated on the button rather than in a modal.** The
client sends `new_game` on every connect (`.claude/rules/core_sim/world-handoff.md`), so a relaunched
client builds a new world rather than rejoining the running one, and Save Game is still an inert
placeholder pane. In `pause` the button therefore reads "Restart now — ends this run" in the `armed`
variant — the same mark "Abandon and return to menu" wears — and the caption gains "The current run
will be lost."; in `landing` there is no run, so it is a plain `primary` "Restart now".

**THE RESTART CARRIES THE WINDOW MODE, ON ARGV.** `project.godot` boots fullscreen and nothing in the
project pins the mode afterwards, so a restart out of a maximized or windowed session came back in
whatever mode the window manager chose — reported from play as the state simply inverting.
`restart_client` appends `-- --window-mode=<n>` and the new process's `GameLaunch._ready` applies it,
so the restart is continuous. **Argv rather than a `ClientSettings` key, and that is the load-bearing
half**: the render harnesses read the player's real `client_settings.cfg` (the same contamination the
theme pin exists for), so a key there could be consumed by a preview run rather than by the restart
it was written for, and applying a window mode inside a harness would fight the `override.cfg`
`scripts/preview.sh` uses to keep its window quiet. An argument reaches only the process we spawned,
which makes a harness immune by construction instead of by remembering to opt out. `MINIMIZED` is not
carried — a game that restarts into the dock with no window is indistinguishable from one that failed
to start — and a malformed or out-of-range value is warned about and ignored rather than passed to
`window_set_mode`.

> **ONE `window_set_mode` IS NOT ENOUGH, AND THE FIRST CUT OF THIS SHIPPED THE INVERSION IT WAS
> WRITTEN TO FIX.** `project.godot` boots the game FULLSCREEN, macOS ANIMATES every fullscreen
> transition, and a mode set while one is in flight is accepted and then silently discarded when the
> animation lands. Measured from a fullscreen boot: asking for `MAXIMIZED` settled at `WINDOWED`, and
> asking for `WINDOWED` settled at `FULLSCREEN` — exactly the "restart flips my window state" report.
> The same transitions are all correct given enough settling time, so it is a RACE, not a wrong
> argument, and reading the mode back straight after setting it confirms nothing: the read also
> catches the animation mid-flight and returns the mode the window is LEAVING.
>
> `_apply_window_mode` therefore ASKS, WAITS, CHECKS and ASKS AGAIN, up to `WINDOW_MODE_ATTEMPTS`.
> Retrying rather than sleeping a fixed span keeps the wrong mode on screen for as short a time as
> macOS allows — `WINDOWED` lands on the first attempt, `MAXIMIZED` takes three or four, and the
> ceiling is headroom. **Any future code that moves this window inherits the same rule.**

**The spawn decides whether anything quits.** `restart_client()` returns `false` when
`OS.create_process` hands back no pid, and the OWNERS (`LandingScreen._on_restart_requested`,
`Main._on_pause_restart`) quit only on `true` — quitting after a failed spawn would close the game
with nothing to replace it, the one outcome this path must never produce. On `false` the menu stays
open and the owner calls `MenuShell.show_restart_failed()`, which puts the row's caption into
`DANGER` reading "Could not restart — quit and start the game again."

**`static var`, not `const`, and the call sites did not change.** A `static var` reads identically at
the call site (`HudStyle.DANGER` is the same expression either way), which is why converting 28
colours and 6 hex strings left ~710 references across 58 files untouched. What it forbids is
`const X := HudStyle.DANGER` in another script: a static variable is not a constant expression, so
such a declaration is a **parse error** — loud, at load, never a silently wrong colour. The same
applies to `const X := MapView.CRISIS_COLOR` and to any `const` **dictionary** whose values are
themed colours.

**THE DERIVE-INSIDE-APPLY RULE.** Anything computed from a themed colour is assigned inside an
`apply_palette()`, **never as a static-var initializer**. An initializer runs when its script is
loaded, which for anything in an autoload's preload graph is before `apply()` has run; it would
capture the default palette and then never update, and the symptom is a correctly-themed HUD with a
few stubbornly cyan accents in it. What is derived rather than authored:

| Derived | From |
|---|---|
| `HudStyle.PANEL` | `PANEL_SOLID` at `PANEL_OPACITY` — they were two hand-written literals that could drift |
| `HudStyle.SIGNAL_WASH` | `SIGNAL` at `SIGNAL_WASH_OPACITY` |
| `HudStyle.{SIGNAL,WARN,DANGER,HEALTHY,INK,INK_DIM}_HEX` | the matching colour's `to_html(false)` |
| `MapView.{THREAT,HUNT_DANGER}_OVERLAY_COLOR` | `HudStyle.{THREAT,HUNT_DANGER}_ACCENT` — one danger language on both surfaces |
| `MapView.HERD_DISTRESS_COLOR` | `HudStyle.DANGER` |
| `MapView.SUPPLY_LINK_COLOR` | `HudStyle.SIGNAL` at `SUPPLY_LINK_OPACITY` |
| `MapView.OVERLAY_COLORS` | the ramp colours above it — a whole table rebuilt, not a value |

A theme authors **26 HUD colours** and **16 map ramp colours**; the three earth themes share one
`EARTH_MAP` ramp set, since a data ramp answers "how much of X is here?" and does not vary with the
chrome's warmth. `console` keeps its own. Everything else in either script stays `const`: paddings,
radii, alphas, font sizes, the two pure-black washes (`CHIP_BG`, `READOUT_BG`) that work on any
ground, and the map's independent legend/marker palettes (band tokens, glyphs, grid, fog, terrain
bg, `TERRAIN_TAG_COLORS` / `FOOD_MODULE_COLORS` / `FOOD_SITE_STYLES`).

**Four vocabulary modules hold style tables built out of `HudStyle` colours**, so each carries its
own argument-less `apply_palette()` hook that `HudPalette.apply()` calls after `HudStyle` and
`MapView`: `HudEventVocab` (`RUNG_STYLE`, `KIND_STYLE`, `DETAIL_STATUS_STYLE`, `TURN_STAMP_COLOR`),
`HudCraftingVocab` (`REASON_COLORS` and the chip/grade tints), `HudWidgets`
(`VERDICT_SEVERITY_COLORS`) and `TellingPanel` (`MEDIUM_STYLES`). A module that grows a themed table
needs a hook and a line in `HudPalette.apply()`, not an initializer.

**A THEMED COLOUR LIVES IN THE PALETTE. IT NEVER LIVES AS A LITERAL INSIDE A STYLING HELPER** — and
that is not a style preference, it is the failure mode that survived the first pass. `apply_button`
built its variants from palette entries for *some* colours and from inline `Color(...)` literals for
six others, so a `primary` button read correctly AT REST (it took `BUTTON_PRIMARY_BG`) and snapped
back to console teal the instant it was hovered, while a `ghost` button — every secondary control in
the client — was console teal always. The six are palette entries now (`GHOST_BG`, `GHOST_BG_HOVER`,
`PRIMARY_BG_HOVER`, `ARMED_BG`, `ARMED_BG_HOVER`, `ARMED_BORDER`), authored per theme rather than
derived: how quiet a secondary fill is, how far a hover lifts it and how warm the armed fill runs are
real decisions a palette makes. **The exception that is legitimate is a PURE BLACK OR WHITE WASH at
low alpha** — `CHIP_BG`, `READOUT_BG`, `SHADOW_COLOR`, the nav backing — because those darken
whatever ground they sit on rather than stating a colour, which is true under every theme. A TINTED
literal never qualifies; `banner_stylebox`'s fill was a hand-written near-copy of `GROUND` and is
`Color(GROUND, BANNER_OPACITY)` now.

**THREE KINDS OF COLOUR LIVE OUTSIDE GDSCRIPT, AND NONE OF THEM CAN FOLLOW A THEME.** A `.tscn`
`color = Color(...)` is baked into the scene file (`LandingScreen.tscn`'s full-bleed `Ground` was
console `GROUND`, so the landing backdrop stayed slate-blue while the shell on top of it turned
warm); `project.godot`'s `rendering/environment/defaults/default_clear_color` is read once at
startup and paints every pixel no Control covers; and `boot_splash/bg_color` is drawn before any
script runs at all. The first two are now assigned from the palette in code —
`LandingScreen._ready` sets `_ground.color`, and `HudPalette.apply` calls
`RenderingServer.set_default_clear_color` — so **a scene may hold the NODE, but the palette holds
its colour**. The boot splash is genuinely unreachable and stays as it is. Sweep with
`grep -rn 'color = Color(' src --include '*.tscn'` when adding a scene.

**A GENERATED ICON RASTER IS ALREADY PIXELS, so `apply_palette` DROPS IT.** The `CheckBox`
indicators and the slider grabber bake `INK`/`SIGNAL` into an `ImageTexture` cached in a `static
var`; re-assigning a palette colour cannot reach a raster built before it, so `HudStyle.apply_palette`
nulls all five caches and each rebuilds on the next styled control. Any new generated art joins that
list — it is the one thing in the file the derive-inside-apply rule does not cover on its own.

**A DROPDOWN IS TWO SURFACES, AND A SLIDER IS THREE PIECES ONE OF WHICH IS ART.** Both are the
`apply_checkbox` trap in a new widget: the client applies no `Theme` resource, so anything not
overridden per-control wears Godot's stock light-grey. An `OptionButton`'s face is a `Button`, but
its LIST is a `PopupMenu` on a separate embedded `Window` reached through `get_popup()`, and nothing
set on the OptionButton reaches it — hence `apply_option_button(picker)`, which installs the ghost
chrome on the face (from `button_styleboxes`, the same five boxes `apply_button` uses, so the two
cannot drift) and the console's own panel/hover/ink on the popup. Call it at every
`OptionButton.new()`. A `Slider`'s groove and filled part are styleboxes, but its HANDLE is a theme
ICON and so unmodulated stock art — `apply_slider(slider)` styles the two and swaps in the generated
`INK` grabber. **`menu_options_theme_popup.png` exists because no closed-face frame can show a
popup**; judge the list surface there.

**The offline harnesses pin the theme**, the same contamination `interface-scale.md` records for
`ui_scale`: `ClientSettings` has already installed the developer's own theme by the time a harness
`_ready` runs, so `ui_preview`, `band_panel_preview`, `menu_preview`, `workbench_preview` and
`map_preview` each call `HudPalette.apply(HudPalette.DEFAULT_THEME)` in their prologue, before any UI
is built. Re-applying is safe at that point precisely because every derived value is re-derived by
`apply` and nothing on screen has read a colour yet.

## Key scripts

**A PNG added to or changed under `assets/` is invisible to a render harness until the project is
re-imported, and the harness passes clean against the old art either way** — `test-harnesses.md` →
"A harness renders the IMPORT CACHE, not the art on disk".

| Script | Purpose |
|--------|---------|
| `ui/TileHabitability.gd` | Single source of truth for the Tile-card Habitability rating: buckets `TileState.habitability` (band-independent per-turn morale drain) into Hospitable/Fair/Harsh/Hostile via `tile_habitability_config.json` thresholds, with the HEALTHY/INK/WARN/DANGER color / `hex_for_rating` mapping. Consumed by `Hud._tile_terrain_lines` + `DetailFormat.detail_bbcode` |
| `ui/TileClimate.gd` | Single source of truth for the Tile-card Climate LABELS + classification: maps `TileState.temperature` (°) into **Polar/Boreal/Temperate/Tropical** using the SIM's PUBLISHED cut points (`MapSection.climateBands`, adopted via `set_cut_points` from MapView's overlay ingest — the client no longer keeps its own `cool_min`, retired with the Climate Authority arc so the shown climate can't disagree with the sim's biome). Mirrors `climate::climate_band_for_temperature` exactly (inclusive upper bounds). `has_bands()` gates the row — until the sim publishes, the Climate row is skipped (no invented threshold). INFORMATIONAL only — neutral ink, no HEALTHY/WARN/DANGER tint. Consumed by `Hud._tile_terrain_lines` |
| `ui/RiverEdges.gd` | Single source of truth for the TEXT reading of hex-EDGE rivers: owns the class vocabulary (Minor/Major), the 6 direction names, and the mask bit-widths as named constants, and formats `TileState.riverEdges` into `Major River: NE, NW` / `Minor River: SW` rows (`summary_lines`, Major first, directions in compass order from NE). Consumed by BOTH `Hud._tile_terrain_lines` (Tile card) and `Hud.show_tooltip` (map hover) — one formatter, two surfaces. See Edge Blending → Rivers |
| `SnapshotStream.gd` | Consumes length-prefixed FlatBuffers snapshots |
| `CommandClient.gd` | Issues Protobuf commands to server |
| `ui/MinimapPanel.gd` | Minimap component for the 2D map view (click-to-pan, aspect ratio sizing) |
| `ui/MagnifierButton.gd` | Zoom-rail in/out button that `_draw`s a crisp magnifier icon (lens + handle + inner `+`/`−`, `zoom_sign` picks which) — font magnifier glyphs render as tofu/blobs. Monochrome `HudStyle` ink → `SIGNAL` on hover |
| `ui/HudStyle.gd` | Single source of truth for the dark HUD console look: palette (cyan `SIGNAL`, amber `WARN`, ink/line neutrals), `card_stylebox()`, `header_stylebox()`, `banner_stylebox()`, `apply_button(btn, "primary"/"ghost"/"armed", selected_when_disabled)` + **`button_font_color(variant, disabled, selected)`** — the text colour behind it, split out because a button whose face is built from CHILD LABELS cannot use the theme override at all (`font_color` reaches a Button's own `text` and nothing else), which is the policy picker's two-line rung; `apply_button` now feeds from it, so a hand-built face and a themed one read ONE table and any new state colour arrives on both. **`ui_preview` now FAILS when a hand-built face stops tracking it** (`_assert_two_line_face_states`, issue #383) — the caller half asserts a shipped pill's two lines are one answer of this function, the state half renders a DISABLED face offscreen and reads back each line's peak luminance, so a state coloured through the theme override alone leaves both lines bright and is caught in pixels rather than by eye — with a separate `modulate`-identity claim beside it, since a luminance reading cannot tell a properly tinted face from the double-dim `modulate` shape (`.claude/rules/client/harness-ui-preview.md`). **The trailing flag on each is the SELECTED-yet-DISABLED state** (read only when disabled): the plain disabled treatment fades the border to `LINE_SOFT` and the text to `INK_FAINT`, which erases the only mark of which control is the current choice — right for a never-chosen locked control, wrong for the one the player is standing on. Set it, and the variant's own border survives and the text keeps its hue at `BUTTON_SELECTED_DISABLED_TEXT_ALPHA`. Its one caller today is the **crop picker's COMMITTED row** (a marked-and-locked row: you can see which crop the patch is committed to, and cannot pick another). It is NOT the policy picker's standing-but-gated rung any more — that caller was #420's, and #442 deleted it along with the picker's whole gates path. **Do not delete the flag on that news**: the crop picker still needs it, and the plain disabled treatment would erase the only mark of which crop is committed (`.claude/rules/client/labor-ui.md`). Plus `chip_stylebox(border)` (the selection card's pinned condition pills), **`readout_stylebox()`** (the compose sheet's recessed readout well) and **`apply_pill_button(btn, selected)`** (the crew targets' pill chrome — the chip's geometry on a control that is pressed rather than read) and **`apply_pill_toggle(btn, selected)`** beside it (the forage sheet's species chips: the SAME selected chrome, so *which one am I on?* keeps one answer HUD-wide, over an UNSELECTED state that draws no fill and no border at all — plain text in the row, with the box returning on HOVER alone so a bare label still says it is pressable. The quiet box is drawn TRANSPARENT rather than not drawn, `PILL_QUIET_ALPHA`: a `StyleBoxEmpty` carries no content margins, so a deselected chip would lose its padding and the row would jump on every toggle), plus the `DASHED_RULE_*` geometry `HudWidgets.build_dashed_rule` draws in, `hairline_stylebox()` (a standalone 1px LINE_SOFT rule inside a card — the list ↔ drawer boundary; the caller owns the thickness), the Band/City panel's three zone styleboxes + their geometry (`role_card_stylebox()` — the bordered standing-role card; `work_row_stylebox(open)` — the work board's row backing, SIGNAL-washed while its inspector is open; `work_inspector_stylebox()` — the inspector strip, written as the role card's chrome REUSED rather than a second identical copy), and **`apply_checkbox(box)`** — the CheckBox treatment, which REPLACES the stock indicator art rather than tinting it (see "The checkbox indicator" above; a CheckBox draws its indicator unmodulated, so the `icon_*_color` family is a silent no-op on it), **`dialog_stylebox()` + `apply_dialog(dialog)`** — the modal-confirm treatment, which SUPPRESSES the stock title bar rather than restyling it (see "The modal dialog" above; the title items belong to `Window` and an override set on an `AcceptDialog` is accepted and never read), and `apply_link_button(btn, base_color)` — the **inline link** treatment for a clickable label inside a row (no box at rest; hover tint + cyan text + pointing hand), used by the band panel's clickable Current-actions rows. Every HUD surface styles through here |
| `ui/HudPalette.gd` | The THEME ROSTER (`class_name HudPalette`): `THEMES` (4 entries, each `{name, hud, map}`), `DEFAULT_THEME`, `ids()`, `display_name(id)`, `applied_id` (what THIS session installed — the Options caption compares against it, not against the saved setting) and `apply(id)`, which installs a palette into `HudStyle`, `MapView` and the four vocabulary modules in that order. An unknown id degrades to the default with a `push_warning` rather than crashing, the same posture as `ServerPortsFile`. See "The palette is a THEME" above for the `static var` and derive-inside-apply rules |
| `ui/FoodIcons.gd` | Shared glyph vocabulary — food modules (`for_site`, which takes an optional tile `terrain_id`: **`riverine_delta` splits fish 🐟 ↔ reeds 🎋** — dry floodplain LAND (`alluvial_plain`/`floodplain`) reads as reeds via `RIVERINE_REED_ICON`, open `navigable_river` keeps 🐟; MapView stamps each food site's `terrain_id` so the map marker + HUD Forage row resolve the same glyph — the resolution itself is factored into the public **`site_key_for(module_key, is_hunt, terrain_id)`**, which returns a stable ART KEY (`"hunt"` / `"reeds"` / a module key verbatim / `"default"`, the three non-module keys deliberately disjoint from `ICONS`) so `SiteSprites` resolves the same site without a second copy of the fish↔reeds branch; `for_site` is written in terms of it, so there is exactly ONE implementation — the twin of `species_key_for` on the herd side), fauna herds (`for_herd`, species keyword matched in the herd label, longest-first — the matching itself is factored into the public **`species_key_for(label)`**, which returns the matched HERD_SPECIES key (`""` when none) so `FaunaSprites` can resolve the same species without a second copy of the matcher; `for_herd` is written in terms of it, so there is exactly ONE implementation), and two glyph families that used to be one. **`FLOOR_ZONE_ICONS` / `for_floor_zone`** — where a crew's ESCAPEMENT FLOOR sits relative to the food peak (strip 💀 / drawdown ⇊ / peak ♻ / learning ⬆ / untouched ⊘), the replacement for the four retired harvest stances: a floor is a continuous number, so one mark can only say which side of the peak it falls on, and three of the five glyphs are inherited verbatim from the stance that already meant their zone's thing (see `labor-ui.md` → "ONE HINT RULE"). And **`POLICY_ICONS` / `for_policy`**, now the **four investment** rungs of the Intensification Ladder alone — cultivate 🌱 / sow ▦ / tame ◎ / corral 🐄. Each verb wears the glyph of **the rung it builds** (🌱 the crop, ▦ the plotted Field, ◎ the pastoral herd that now keeps near your camp — the rung's defining effect is proximity — 🐄 the penned livestock; 🐄 is also the herd drawer's Domesticated/Corralled badge, and ▦ the tile card's `▦ Field` badge). Verified legible at picker size in `forage_cultivate.png` / `forage_sow.png` / `two_meter_split.png` / `herd_corral.png`; `""` for unknown). Used by the map's food-site / herd markers (`MapView._draw_food_site` / `_draw_herd`), the Harvest/Hunt button + the **band panel's Current-actions rows** (each row leads with its resource glyph), and — for policies — BOTH the Hud policy-picker buttons (`HudWidgets.build_policy_picker`, where the glyph now leads the rung's NAME on line 1 of a two-line face — `HudFormat.policy_face`) and the map's yield labels (`BandOverlayRenderer._draw_yield_label` appends the icon: `+0.38 ♻`), so a resource/policy always reads the same on the panel and on the map. **Policy glyphs are deliberately TEXT-PRESENTATION symbols** (♻ ⬆ ⇊ ▦ ◎) plus the high-contrast 💀: pictographic emoji (🪙 coin, 💰 money bag) render as a featureless grey blob at the ~12–13px these are drawn at, and ⚖ renders tiny/faint — same glyph-legibility hazard that forced `MagnifierButton` to hand-draw. Verified in `band_panel_left.png` / `map_band_work.png`. **The mechanism is sharper than "prefer line art", and it decides the choice:** a text-presentation glyph **inherits the label's font colour**, so it renders at the button's full contrast and greys out *with* the button when a rung is disabled; an **emoji carries its own colours and cannot be tinted**, so it renders at whatever contrast its art happens to have and stays stubbornly coloured while disabled. 🐾 was tried for `tame` and rejected on exactly that — at picker size it came out a faint washed-out tan against the dark console, the weakest glyph in a row next to a crisp white 💀 (see the first cut of `two_meter_split.png`) — and ◎ replaced it. Prefer a text-presentation symbol for any NEW policy glyph; the surviving emoji (💀 🌱 🐄) are grandfathered and legible. **Deplete wears ⇊ because the extractive four are ONE AXIS — harvest PRESSURE — and its glyph must name that, not a product**: it was ⇄ (exchange) while the rung was called `Market`, and the `Market` → `Deplete` rename (`docs/plan_hunt_yield_model.md` §2) made an exchange arrow wrong, since every rung then sold the species' trade goods (an account arc #527 has since retired — the rename's argument stands on the pressure axis alone). ⇊ is the doubled twin of Surplus's single ⬆, so the two read as neighbouring rungs of one ladder. It is the LIGHTEST glyph in the set — verified rendering (not tofu) at both picker size (`hunt_picker_ascending.png`, `food_tile.png`) and map-yield-label size (`map_band_work.png`), but thinner than ♻/⬆/💀, so re-judge it first on any sizing change. Also the **CROP-ROLE** marks (`CROP_ROLE_ICONS` / `for_crop_role`) — one per `FloraShareInfo.role`, worn by each row of the tile card's basket so the composition of a stand reads at a glance: how much of this ground is food for people, feed for animals, or goods to trade. **These are BUNDLED ART now (`CropRoleSprites`, issue #463), with the three emoji as a LIVE fallback**, and `for_crop_role` takes an `icon_px` deciding which: `0` (every non-drawer caller) yields the emoji, a real box yields an `[img]` tag. **The defect that replaced them was COLLISION, not legibility** — all three were borrowed, and two still meant something else elsewhere in this HUD: ⇄ was `TRADE_GOODS_GLYPH` on every yield readout in the game (both the glyph and the account it named are retired by arc #527, and the fallback is `🧵` now), and 🐄 is `POLICY_CORRAL`'s penned-livestock mark on the work board and the map, while fodder here means *"feed for penned animals, not food for people"* (`FLORA_CROP_FODDER_TOOLTIP_FORMAT`) — near enough to pass, but not the same claim, and a cash crop was being marked with **the account it paid into** rather than with what it is — and that account no longer exists, which is the retirement making the same argument twice. The art names the PRODUCT instead (grain ear / bale of cut forage / bolt of dyed cloth). **`""` (or an unknown tag) means UNSTATED and returns `""`, never "staple"** — the wire says so explicitly, and a client that defaulted a missing tag into a real category would invent a fact about the plant; the caller then renders a blank slot that holds its width, `crop_role_spacer` (a transparent image boxed like a real mark) falling back to `HudFloraVocab.FLORA_ROLE_ICON_UNSTATED`. **That spacer is deliberately SPLIT OUT of `for_crop_role`**, whose contract is that an unstated role yields nothing; holding the column anyway is the row's decision. Judged at true size on `tile_food_layers.png`, the one frame carrying all three; `tile_food_layers_unstated.png` is the blank-slot frame. Also the **action-status** glyphs (`for_status`, `STATUS_ICONS`) the Band panel's Current-actions + Active-expeditions rows use instead of words — `pending ○` (the ORDER isn't acknowledged yet; a modifier that rides on any row, amber) / `working ●` (a confirmed local forage/hunt row, and expedition phase `hunting`) / `outbound ➤` / `awaiting ▮▮` / `delivering ◄` = `returning ◄` (both are "coming home"; the tooltip distinguishes them). Same line-art rule and the same hazard: `◌` (dotted circle) was tried for `pending` and rejected — it renders thin and faint at row size — and `⏸` for `awaiting` carries emoji presentation (tofu/blob), so `▮▮` is used. Verified at true size in `band_panel_status_glyphs.png` |
| `ui/FaunaSprites.gd` | Bundled PNG art for map HERD markers — the sprite half of `FoodIcons`' herd vocabulary, and the reason a rabbit no longer renders white on macOS and pink on Windows: the emoji path draws through `ThemeDB.fallback_font`, so the OS emoji font owned the look. Static-only (same reasoning as `ServerPortsFile.gd`): `SPRITE_PATHS` maps a species KEY (a `FoodIcons.HERD_SPECIES` key, resolved via `FoodIcons.species_key_for` — **never a second matcher**) to a file in `assets/icons/fauna/`, aliasing shared art exactly as HERD_SPECIES aliases emoji (bison/buffalo → `aurochs.png`, caribou → `reindeer.png`). `for_herd(label) -> Texture2D` returns the cached texture or **`null` when this species has no art yet**, which is the fallback contract: `SecondaryMarkerRenderer.draw_herd` resolves the sprite first and calls `MapView._draw_marker_sprite`, else falls through to the unchanged emoji `_draw_marker_glyph`. **Coverage is COMPLETE, and `cargo xtask fauna-icon-guard` is what makes that a CHECKED FACT rather than an assertion** — it reads the sim's `fauna_config.json`, replicates `species_key_for`'s longest-first match, and fails unless every species' display name reaches a `SPRITE_PATHS` key whose PNG **and whose `<name>.png.import` sidecar** exist on disk — the sidecar is checked because Godot never loads the PNG itself, it reads the sidecar's `path=` and loads the imported `.ctex`, so a PNG committed without its (hand-staged, never gitignored) sidecar draws the OS emoji in every checkout but the author's, and a missing sidecar is reported as its own failure with its own fix (it reports every failure, not the first, and reads `SPRITE_DIR` out of the script rather than hardcoding it). **THE GUARD EXISTS BECAUSE THIS SENTENCE WAS FALSE HERE, IN `FaunaSprites.gd` AND IN `icon_prompts.txt` SIMULTANEOUSLY** (issue #439): it said "all N keys map to bundled art", which is TRUE and is the WRONG QUESTION — **`Steppe Runners` and `Marsh Grazers` had no key in either table at all**, so `species_key_for` answered `""` and both drew the 🦬 `HERD_DEFAULT` OS emoji on a live map. A check over the client's own keys is structurally blind to a species the client has never heard of, and `map_preview`'s `FAUNA_SPRITE_ROSTER` is a hand-written client-side list too, so the coverage FRAME could not catch it either — the identical blind spot that let four cervids share one marker. **A coverage claim has to be checked against the OTHER side's roster.** The player-visible tell was that the odd marker FACED LEFT, every bundled sprite obeying `icon_prompts.txt`'s "side profile facing right" clause while an OS emoji does not — a cheap thing to spot in a screenshot, and how it was actually found. Today: all 25 HERD_SPECIES keys map to one of 18 PNGs (aliases share art: bison/buffalo → `aurochs.png`, oxen → `cattle.png`, ibex → `goat.png`, caribou → `reindeer.png`, grouse → `fowl.png`, **hare → `rabbit.png`**; `seal.png` closed the last gap, `catfish.png` is the wet-biome roster pass's own art — deliberately unlike `sites/fish.png`, which can share a delta hex with it — and `wolf.png` is the first PREDATOR sprite (Predators Phase 1a, matched by the `"wolf"` keyword in "Grey Wolf Pack")), so no herd species in the game draws an OS emoji. Adding a species is still: drop the PNG in, add the key here — **and run the guard, which is the step that turns "I added the key" into "the roster is covered"**. **AN ALIAS IS ONLY LEGITIMATE WHEN NO ROSTER SPECIES STANDS BEHIND IT, and that is the rule issue #439 cost** — `deer.png` served `deer`/`elk`/`reindeer`/`caribou`/`gazelle`, and `fauna_config.json` ships a distinct species under four of those five keys (Red Deer, Wild Elk, Wild Reindeer, Desert Gazelle), so four species drew one marker and a player could not tell an elk herd from a deer herd. `elk.png` / `reindeer.png` / `gazelle.png` split them; `caribou` stays an alias precisely because it is a second English word for the animal `reindeer` already names, with no roster entry of its own. **The check before aliasing a new key is therefore `fauna_config.json`, not the emoji table** — HERD_SPECIES aliasing two keys to one glyph is not evidence they are one animal, only that Unicode lacks a second one, and copying its aliases wholesale is exactly how this shipped. The four cervids also lead `map_preview`'s roster frame ADJACENTLY, because the thing that hid the bug was that no frame ever put them side by side — each read fine alone. **The `null` fallback stays load-bearing even at full coverage** — it catches a herd label naming a species the client does not know (`species_key_for` → `""`) and the `HERD_DEFAULT` case, both of which still render emoji. Because every shipped species has art, **the emoji path is no longer exercised by map_preview fixtures at all**; only a fixture herd labelled with an unknown/unmapped species **would** guard it, and **no such fixture exists today** — the path is deliberately unguarded, not guarded elsewhere. Loaded with `load()` (not `preload()`) so a missing file degrades to the emoji rather than breaking scene load, with one warning per missing path. **The sprite is drawn UNTINTED**, like the emoji — a starving pen still reads as the distress ring + badge GEOMETRY drawn under/over the marker, never a modulate. **Import options are load-bearing**: the sources are 256px but `MapView.texture_filter` is pinned `TEXTURE_FILTER_NEAREST` (to keep the terrain-cache blit seam-free), so the `.import` files set `process/size_limit=64` to cut a 7:1 nearest minification down to ~1.8:1; `mipmaps/generate=true` is set too but is INERT under NEAREST — it only starts paying if that filter is ever raised to linear-with-mipmaps. Judge any art change at TRUE marker size (10–41px), not in a fitted preview frame, which renders them ~2.5× too big |
| `ui/SiteSprites.gd` | Bundled PNG art for map FOOD-SITE markers — the sprite half of `FoodIcons`' site vocabulary, and the food-module twin of `FaunaSprites` (same reasoning: the emoji path draws through `ThemeDB.fallback_font`, so the OS emoji font owned what a shellfish bed or a nut grove looked like). `SPRITE_PATHS` maps a site ART KEY — resolved via **`FoodIcons.site_key_for`, never a second copy of the fish↔reeds branch** — to a file in `assets/icons/sites/`; `for_site(module_key, is_hunt, terrain_id) -> Texture2D` takes the SAME arguments as `FoodIcons.for_site`, so the sprite and the emoji can never disagree about which site this is. **Coverage is COMPLETE** — all 10 `ICONS` modules plus the three non-module keys map to bundled art (12 PNGs, with **`hunt` reusing the fauna `deer.png`**: a hunted site IS game, and a second copy under `sites/` would be one more thing to keep in sync), so no food site in the game draws an OS emoji and — exactly as on the fauna side — **no map_preview fixture exercises the emoji path any more**. The `null` fallback stays load-bearing: it catches an art key with no art (a new food module added to `ICONS` without a PNG), which still renders the emoji. `SecondaryMarkerRenderer.draw_food_site` resolves the sprite first and calls `MapView._draw_marker_sprite`, else falls through to the unchanged `_draw_marker_glyph`. **Same import options as fauna** (`process/size_limit=64`, `mipmaps/generate=true` — inert under the pinned `TEXTURE_FILTER_NEAREST`, see the FaunaSprites row) and the same judging rule: at true marker size. The **reeds are the busiest icon in the set** — at ~36px the individual blades merge into a mass, though the vertical tuft + brown cattail heads stay unmistakable and unique; it is the first one to re-check on any sizing change. Verify the whole set on `map_preview`'s **`map_site_sprites`** (the SPRITE ROSTER: one site per art key in one row, incl. the hunted-site deer and an unknown module's `default` sprig) + **`map_riverine_split`** (the decisive frame: ONE module, `riverine_delta`, drawing the FISH on open navigable river and the REEDS on dry alluvial plain — the branch `site_key_for` exists for) |
| `ui/WonderSprites.gd` | Bundled PNG art for map **DISCOVERED-SITE (Wondrous Site)** markers — the third art family behind `IconSprites`, after `FaunaSprites` and `SiteSprites` (same reasoning: the emoji path draws through `ThemeDB.fallback_font`, so the OS emoji font owned what a Great Peak looked like, and ⛰/⛲ blob at marker size). **Keyed on `site_id`** — the sim's stable catalog key from `core_sim/src/data/sites_config.json`, **already on the wire** (decoded in `native/src/lib.rs`, already read by `SecondaryMarkerRenderer._wonder_key`), so this needed **no schema or server change**. Deliberately NOT keyed on the `glyph` string: that is presentation the server also happens to send, and two sites may share one glyph (the fixture's `sky_arch` reuses ⛰), so keying on it would collapse distinct sites onto one sprite. `for_site_id(site_id) -> Texture2D` returns the cached texture or `null`. **THE `null` FALLBACK IS GENUINELY LIVE HERE — the one way this table differs from `FaunaSprites`/`SiteSprites`**, whose coverage is complete and whose fallbacks only guard an unknown key. `great_peak` + `verdant_basin` are the whole catalog *today*, but that catalog is **data-driven** and expected to grow: a designer adds a site entry with a glyph and it ships with no art, so falling through to the server-provided emoji is a real, **exercised** path (`map_sites.png`'s `sky_arch` renders it). Adding art stays: drop the PNG in `assets/icons/wonders/`, add the id here. **TWO consumers now, and both key on `site_id`:** the map marker (`SecondaryMarkerRenderer.draw_discovered_site`) and the **HUD top-bar discoveries strip** (`Hud.update_discoveries` — see the Wondrous Sites bullet), which was the last place in the client keying site presentation on the glyph and has been migrated onto this table. Neither builds a second art map; `for_site_id` is the one lookup. A site with art must draw **even if the server sent no glyph**, and that takes BOTH halves of `SecondaryMarkerRenderer`, which is why they share one predicate, `_wonder_renders(site)` = *has a sprite OR a non-empty glyph*: (1) `compute_slots` must admit sprite-only sites to **slot eligibility** — it originally tested the glyph alone, so such a site got no slot and `draw_discovered_site` bailed at its `slot < 0` return long before any sprite check, making the guarantee unreachable; and (2) `draw_discovered_site`'s own early-return must likewise account for the sprite, not just the glyph. Past that guard it calls `MapView._draw_marker_sprite`, else falls through to the unchanged emoji `_draw_marker_glyph`. Latent while every shipped site carries a glyph — keep the two tests on the shared helper so they cannot drift back apart. **Same import options as fauna/sites** (`process/size_limit=64`, `mipmaps/generate=true` — inert under the pinned `TEXTURE_FILTER_NEAREST`) and the same judging rule: at true marker size. At ~36px `great_peak`'s snow-capped silhouette is unmistakable; `verdant_basin`'s leaf fronds merge into the green mass (the `reeds` caveat again) but its green-ring-around-blue-water read stays distinct — re-check it first on any sizing change. Verify on `map_preview`'s **`map_sites`** (both sprites + the unmapped `sky_arch` falling to emoji) and **`map_sites_fogged`** (the case unique to this marker: a site persists on a *remembered* tile under the mist tint — both sprites must still read there) |
| `ui/StageSprites.gd` | Bundled PNG art for **SETTLEMENT-STAGE** tokens — the fourth art family behind `IconSprites`, covering BOTH the map band token (`BandMarkerRenderer._draw_band_token`) and the **Band/City panel header** (`BandCityPanel.set_header`, which swaps a `TextureRect` in for each of its two glyph `Label`s). Same reasoning as the rest: the emoji path draws through `ThemeDB.fallback_font`, so the OS emoji font decided what a camp/village looked like, and ⛺/🛖/🏘️ blob at token size. **THE ONE WAY THIS FAMILY DIFFERS FROM ALL THE OTHERS: its key comes STRAIGHT FROM THE SERVER, with no client-side resolver.** `FaunaSprites` derives a species key from a free-text herd label (`FoodIcons.species_key_for`) and `SiteSprites` from a module+terrain branch (`site_key_for`); here the sim's `settlement_stage_id` (from `settlement_stage_config.json`, already on the wire and decoded in `native/src/lib.rs` — this needed **no schema or server change**, only a GDScript reader) IS the key, so `for_stage(stage_id) -> Texture2D` is a direct table hit. Deliberately NOT keyed on `settlement_stage_icon`: that is presentation the server also happens to send, and keying art on a glyph string is the brittle reverse-mapping this table exists to avoid. **The `null` fallback is LIVE, like `WonderSprites`' and unlike fauna/sites'** — `settlement_stage_config.json` is user-editable, so a game may define stages beyond the three bundled (`nomadic`/`camp`/`village`), and those must keep rendering their configured emoji. **Precedence in `_draw_band_token` is load-bearing:** expedition → **sprite** → glyph → placeholder square. The sprite attempt MUST precede the empty-glyph placeholder branch, which returns early — otherwise a sprite-mapped stage whose glyph happened to be empty would wrongly draw a square. `dim` (a behind card in the band stack) modulates the sprite by `BAND_STACK_BEHIND_TINT`, mirroring what the glyph path does to its colour — the ONE case `MapView._draw_marker_sprite`'s `modulate` param is for; it is structural recession, never a state encoding (see that helper's comment). Same import options as the other families (`process/size_limit=64`) and the same judging rule: at true marker size. Verify on `map_preview`'s **`map_stage_glyphs`** (the ⛺→🛖→🏘️ progression as sprites, plus the empty-stage band still drawing the neutral placeholder square) and `band_panel_preview`'s header (the fixture carries `settlement_stage_id: "camp"`) |
| `ui/CropRoleSprites.gd` | Bundled PNG art for the tile card's **CROP-ROLE** marks — the fifth art family behind `IconSprites`, keyed on the sim's `FloraShareInfo.role` tag verbatim (the `StageSprites` degenerate case: the server sends the key, so there is no client-side resolver). **TWO WAYS IT DIFFERS FROM THE OTHER FOUR, and both follow from it not being a map marker.** (1) **It returns a PATH, not a `Texture2D`.** The other four feed `MapView._draw_marker_sprite`, a canvas draw; these lead a row of a `RichTextLabel` (`DetailFormat.flora_composition_lines`) and are consumed as `[img]` BBCode, which addresses art by `res://` path — **the client's only `[img]` today**. The load still goes through `IconSprites.texture_for`, so "is there art for this role" is answered by the same load-and-cache-and-warn-once the others use, and `path_for` hands the path back **only once that load has succeeded**: a path returned for a texture that failed to load puts a broken-image box in the middle of a text row. (2) **It renders at ~13px, not 24–41**, so `assets/icons/icon_prompts.txt` → CROP ROLES gives it its own sub-style that **inverts the house style's thick-dark-outline clause** — `HudStyle.PANEL_SOLID` is luma ~24, darker than any terrain a marker sits on, so a charcoal outline is camouflage that eats the fill from both sides at a size with no budget for it; the fill carries the silhouette, and the three are separated on BOTH silhouette and hue because either axis alone can fail at that size. It also carries **`SPACER_PATH`** (`unstated.png`), a GENERATED fully-transparent PNG — not art, regenerate rather than edit; the regeneration command is on the const. A transparent image rather than spaces because every mark is boxed square regardless of its subject's aspect, so a spacer boxed the same way is the only thing guaranteed to occupy the identical width. **Coverage is COMPLETE** — all three roles the wire ships map to a PNG, so no basket row draws an OS emoji. Measured at 13px over the panel (the sizes are in `icon_prompts.txt` → CROP ROLES): the three separate as a **3×8 vertical**, an **11×5 horizontal** and a **10×11 block**, i.e. by SHAPE before colour, which is what survives a size at which interior detail does not — fodder's binding band and cash's cord are both gone below ~26px, deliberately. **The `null` fallback stays LIVE anyway, like `WonderSprites`'/`StageSprites`' and unlike fauna/sites'**: it is what let this land as a table plus a lookup rather than a flag day, and it still catches a PNG that fails to load. **`ui_preview` asserts the art is what rendered** (`3 of 3`), and that assertion is not optional — every other claim in that group survives all three PNGs failing to load, because `for_crop_role` then answers the emoji and any needle built from it falls back in step. **Mipmaps are LIVE here too** — `mipmaps/generate=true` is inert in all four marker families because `MapView` pins `TEXTURE_FILTER_NEAREST` on itself and a RichTextLabel does not inherit that; `project.godot` sets no `default_texture_filter`, so this family draws through Godot's Linear default and genuinely uses the mip chain. Do not copy a fauna sidecar and assume the flag is decorative |
| `ui/FloraSprites.gd` | Bundled PNG art for individual FLORA SPECIES — the **sixth** art family behind `IconSprites`, and the per-plant tier ABOVE `CropRoleSprites`' three role marks (issue #339). It exists because the emoji palette COLLAPSES the roster (every grain 🌾, every nut 🌰, every berry 🫐, every mushroom 🍄), so a basket row could not tell Wild Emmer from Wild Barley by its mark — while the ROLE marks carry a real distinction the palette CAN supply and therefore keep it. **COVERAGE IS 32 OF 33, and the ONE gap is deliberate and permanent**: `hay_grass` is the roster's only `fodder` species, so the fodder ROLE mark — a bound bale — already names it exactly and uniquely, and a standing-grass silhouette would collide with the four other grass spikes it co-hosts with (`icon_prompts.txt` → "32 prompts, 33 species"). That is the row every fallback claim in `ui_preview` is now aimed at, because it is the only one that cannot be closed by drawing another PNG. The wiring shipped BEFORE any art existed and every call answered `""` / `null` for a while — which is why a species' art was a file drop rather than a code change when it arrived, and why the fallback is exercised rather than assumed. **THE FILENAME IS THE KEY, and there is NO `SPRITE_PATHS` TABLE.** `FloraShareInfo.species` is the sim's own stable key (`wild_emmer`, `kelp`, `rock_tripe`), so `wild_emmer` resolves to `SPRITE_DIR + "wild_emmer.png"` by composition — the `StageSprites` / `CropRoleSprites` case (*the server sends the key, so there is no client-side resolver*) taken one step further, since those two still keep a table mapping key → file and this one does not. **AND THAT IS WHY THERE IS NO `cargo xtask flora-icon-guard` TWIN OF THE FAUNA GUARD**: that guard exists because `FaunaSprites` resolves a key out of a free-text DISPLAY NAME and then looks it up in a hand-written table, so the table can fall out of step with `fauna_config.json` and a roster species with no key draws an OS emoji unseen. Flora resolves nothing and holds no table, so the only way a species can miss is that its file is absent — a fact about the art, not a drift between two lists. **THE KEY IS WIRE DATA THAT BECOMES A `res://` PATH, so it is CHARSET-GUARDED** (`_is_valid_key`, a `[a-z0-9_]` alphabet spelled out rather than a `RegEx`): the empty key and anything outside that set answer `""` and compose no path at all, and the traversal cases (`..`, `/`) fail on the alphabet so there is no second path check to keep in step. Flora keys are snake_case by construction — the guard is about not TRUSTING the wire, not about a key we expect to be malformed — and nothing is normalized on the way in, lower-casing an unexpected key being how you INVENT one rather than reject it. **TWO ACCESSORS, AND THAT IS WHAT MAKES THIS FAMILY DIFFERENT FROM ALL FIVE SIBLINGS: it has two HOST KINDS.** `path_for(species) -> String` for the tile card's basket rows, which are a `RichTextLabel` addressing art by `res://` path inside `[img]` BBCode (`DetailFormat.flora_composition_lines`); `texture_for(species) -> Texture2D` for the compose sheet's crop-picker rows, which are `Button`s carrying art on their own `icon` property (`DrawerComposeController`, the `BandPanelController._build_quarry_row` precedent). `CropRoleSprites`' own doc already states the rule that THE HOST WIDGET DECIDES THE MECHANISM; this family is simply the first to have both hosts at once, so do not "unify" the pair. Both go through `IconSprites.texture_for`, sharing the one texture cache and the `load()`-not-`preload()` degradation — **but NOT the shared warning, and that split is the one thing to keep straight about this family.** It calls the cache with `warn = false` and warns for itself, through `_note_absent_once`, on the ONE miss that is a defect rather than a state: **a source PNG sitting in the directory with no imported resource behind it** (`FileAccess.file_exists` true, the load still null) — the missing-`.png.import`-sidecar failure `fauna-icon-guard` catches on the fauna side, art that renders for whoever generated it and silently falls back in every other checkout. A path with **no file at all** says nothing, that being every species' expected state until its art is drawn. Reported once per composed path via `_reported` (keyed by path, so the harness override and the shipped directory cannot mask each other), and it is a DEV-RUN check by construction: an exported build ships the imported `.ctex` and not the PNG, which is the right scope, the failure being one a developer commits. Verified by planting an unimported PNG under a `sprite_dir_override` — `file_exists=true`, `ResourceLoader.exists=false`, one warning across three `path_for` calls. And `path_for` hands a path back **only once that load has SUCCEEDED** (`CropRoleSprites._path_if_loadable`'s rule), so a bad path can never put a broken-image box mid-row. **`sprite_dir_override` is a HARNESS SEAM** modelled on `ClientSettings.config_path_override` — static, isolating the files a test sees from the ones the player gets. It was added because coverage was zero and the species tier would otherwise have shipped unexercised; it still earns its keep as the only way to drive the precedence chain against a directory whose contents the harness controls. `ui_preview`'s `chapters/land_readouts.gd` points it at `CropRoleSprites.SPRITE_DIR`, drives the real producer through it and clears it again. **THE KEYING PIPELINE NEEDS A PER-IMAGE RAMP, not the one the crops set used**: `icon_key.py` keys on euclidean RGB distance from a corner-sampled background, and these 32 renders came back on TWO different backgrounds (hot magenta and dark navy) with subject-to-background distances spanning 57–191, so the pale-subject `--lo 85 --hi 240` **erased `pine_nut` entirely (0.0% coverage) and all but 0.8% of `cattail`** — a dark subject on a dark background is close to it in RGB. Split each image's own distance histogram (Otsu) and straddle it. **The hazard that ramp cannot fix is a subject sharing the KEY's hue**: `alpine_herbs`' pink flower head sits 48.7 from its magenta background — against a background whose own noise is under 15 — so it keyed out entirely and the icon rendered as a headless stem. It is recoverable only because that background is uniform (p90 = 4.2), which left room for a `--lo 18 --hi 40`; **the durable fix is not to generate a pink-flowered plant on magenta**. **No directory is created for `SPRITE_DIR` by the code**: `IconSprites.texture_for` guards on `ResourceLoader.exists` and git cannot track an empty dir, so the folder arrived with the first PNG |
| `ui/IconSprites.gd` | The shared texture cache behind ALL SIX bundled-art tables (`FaunaSprites`, `SiteSprites`, `WonderSprites`, `StageSprites`, `CropRoleSprites`, `FloraSprites`): `texture_for(path, warn := true) -> Texture2D` owns the lazily-populated path→`Texture2D` dictionary, the `load()`-not-`preload()` (so a missing file degrades to the emoji rather than breaking scene load) and the **one warning per bad path** (a failed path caches `null`, so the load is attempted once, not once per marker per frame). **`warn` IS FOR A FAMILY WHOSE COVERAGE IS DELIBERATELY INCOMPLETE, AND `FloraSprites` IS THE ONLY ONE THAT PASSES `false`** — the other five call this with one argument and are untouched. For those five an absent path is a DEFECT (coverage is complete or guarded by `fauna-icon-guard`), so the warning is how missing art surfaces; flora art is drawn species by species and a row without it falls back to its crop-role mark BY DESIGN, so warning there fired **16 times in one `ui_preview` run** — each with a ~15-line GDScript backtrace, ~250 lines saying the feature was working — and noise for the expected state is what buries a real miss. **Caching is unaffected by the flag**: a quiet miss still caches `null`, so the load is still attempted exactly once. It does NOT mean "fail quietly" — `FloraSprites` warns itself on the one case that IS a defect, with a message naming the fallback that family actually takes (the wording here, *"falling back to the emoji marker"*, is the other five's and was wrong for flora). Extracted because the tables would otherwise carry that cache verbatim four times; a new art family is now just a `SPRITE_PATHS` table plus a key resolver (`WonderSprites` was exactly that — a table keyed on `site_id`, no cache code — and `StageSprites` is the degenerate case: the server sends the key, so there is no resolver at all; `FloraSprites` is one step past even that — the filename IS the key, so it has no table either and a new species costs no client edit at all). Static-only, same reasoning as `FoodIcons` |


---

## The HUD's TEXT surfaces render bundled art too, through one builder

Splitting `deer.png` fixed the MAP marker and left every text surface collided, because `HERD_SPECIES`
still maps `deer`/`elk`/`reindeer`/`caribou`/`gazelle` to one 🦌 and Unicode has no second cervid to
offer. Four surfaces rendered that emoji — the Band panel's work row and its inspector strip, the
compose sheet's quarry picker, and the selection card's land + herd roster rows — so a Wild Elk and a
Red Deer read identically wherever they were LISTED rather than drawn on the map.

They now render the same `FaunaSprites` / `SiteSprites` art the map does. **THREE of the four go
through `HudWidgets.build_marker_icon`; the compose sheet's quarry picker does NOT** — its host is a
`Button`, so `BandPanelController._build_quarry_row` sets the art on the Button's own `icon` property
(with `expand_icon` and the `icon_max_width` theme constant) and never calls the builder. It also
resolves `FaunaSprites` only, never `SiteSprites`, a quarry always being a herd. That rule and its
rationale live in `hud-modules.md`. Two facts worth carrying back here, because they are decisions
about ART:

- **The host widget decides the mechanism, and it decided differently three times.** `CropRoleSprites`
  returns a PATH for `[img]` BBCode because its host really is a `RichTextLabel`; the work row, the
  inspector strip and the roster rows are `Label`s in `HBoxContainer`s, so they take a `TextureRect`
  — the `StageSprites` + `BandCityPanel.set_header` precedent; the quarry picker is a `Button`, which
  carries its own `icon`. None is the "right" way, and **"one builder for all of them" is exactly the
  claim to resist** — pick by host, and expect the count of hosts sharing a mechanism to be smaller
  than the count of surfaces.
- **The UNTINTED rule reaches the HUD.** A row's sprite is drawn untinted exactly as a marker's is, and
  the row's state rides geometry beside it (severity stripe, ecology dot, the marks column) rather than
  a `modulate` — the treatment the map tried, rendered as a slightly darker brown animal, and reverted.

**No `*Sprites` table changed for any of this**, and that is the point: `for_herd` / `for_site` already
answered these questions, so a fifth consumer was a call, not a new resolver.

---

## RETIRED — the trade-goods glyph (`FoodIcons.TRADE_GOODS_GLYPH`, issue #337, retired by arc #527)

`⇄` marked every non-food component of a yield **on the TIGHT surfaces** — board rows, filter chips,
zone-head totals, map yield labels — so a rate could never be mistaken for food. **ONE glyph for the
whole product**, because the sim modelled trade goods as a SCALAR and the client said so.

**The sim retired the scalar, and the glyph had nothing left to stand for.** What replaces it is a
VECTOR of named materials, and **a material needs no glyph: it has a NAME.** A crop-picker clause
reads `0.29 fibre` exactly as its neighbour reads `1.80 hay`, which is a better mark than an abstract
arrow saying only "not food" — and there is no longer a generic account for a generic mark to mean.
**Do not add one.** `FoodIcons.TRADE_GOODS_GLYPH` is deleted; the surviving non-food account, fodder,
never had a glyph and wears its own word for the same reason.

**Three things it left behind that are still load-bearing:**

- **The tinting rule that chose it.** `⇄` is a **text-presentation** symbol in bold line art, so it
  inherited the label's colour — it tinted WARN-amber on an overdrawn row and greyed out with a
  disabled button, which an emoji cannot. 🪙 / 💰 / ⚖ were measured and rejected at these sizes. Any
  new glyph on these surfaces takes the same test.
- **The word-versus-glyph verdict on the POLICY PICKER.** Playtest found two glyph families adjacent
  in one line at one weight doing incompatible jobs — the rung glyph (`♻ ⬆ ⇊ 💀`) says *which rung*,
  `⇄` said *which product* — and the eye cannot tell which axis it is reading. The rung buttons went
  **two-line**, rung NAME on line 1 and the products in WORDS on line 2 (`labor-ui.md` → the policy
  picker), and that face is unchanged: fodder is a word there for the same reason trade was.
- **`CROP_ROLE_CASH` survives the retirement**, and only its MARK changed. Its emoji fallback was `⇄`,
  which was the COLLISION that bought `CropRoleSprites`' art in the first place; it is now `🧵`, and
  the role means *this plant pays a material, not calories* rather than *this plant pays trade goods*.

Where each component is rendered — and the rule that a component appears ONLY when it is non-zero —
lives with the forecast layer that owns it: see the hunt-pays-two-products section in `labor-ui.md`.
