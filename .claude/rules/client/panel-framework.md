---
paths:
  - "clients/godot_thin_client/src/scripts/ui/{PanelCard,PanelDock,AutoSizingPanel}.gd"
  - "clients/godot_thin_client/src/scripts/ui/hud/DockScrollFit.gd"
---

<!-- Extracted verbatim from lines 203-203;1447-1552 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# HUD panel framework — docked PanelCards

**Never reimplement bespoke height/scroll logic.** There are two shared helpers and
picking the wrong one *silently misbehaves* rather than failing, so choose by what the
panel IS:

- **Free-floating panel** (anchored against the viewport — the Inspector,
  `NarrativeForkPanel`) → `ui/AutoSizingPanel.gd`: attach it and call `fit_to_content`.
  It sizes against the *viewport* via `global_position` + anchors + `offset_bottom`.
- **Dock card** (a child of a `PanelDock` `VBoxContainer` — the subject drawer)
  → `PanelCard` + `ui/hud/DockScrollFit.gd`. The container overwrites a dock child's size
  every layout pass and the ceiling that matters is the *dock's* remaining height, not the
  window's — `AutoSizingPanel` there just fights the container.

Writing height math by hand means you picked the wrong helper, or found a third case worth
extracting — extract it rather than open-coding it.

### `AutoSizingPanel` IS A PLAIN `Control`, so BOTH axes need an explicit fit

Only a `Container` aggregates its children's minimum sizes. This node is a bare `Control`, so
**nothing a child demands ever reaches it**: it is whatever size the caller sets, and the
children lay out around that number whether or not they fit. That is exactly why the height
is fitted — and it makes `target_width` a **nominal width, not a cap and not a measurement**.

A card whose content can outgrow the nominal must say so with **`fit_width(content_width,
extra_width)`**, the width twin of `fit_to_content`, bounded by a **`max_width` declared per
fit from the live viewport** (a fixed pixel cap can only ever bite before the real bound —
the same argument the height ceiling already rests on). It is deliberately **opt-in**: a
caller that has never measured its content width keeps its fixed-width behaviour rather than
collapsing onto its widest child. `NarrativeForkPanel` is that caller — 660px of card around
a 169px content minimum, because its prose wraps — while `ComposeSheet` is the one that
needed it (see `labor-ui.md` → "THE CARD IS AS WIDE AS ITS WIDEST ROW").

`_fitted_width` is why the two fits cannot disagree: `fit_to_content` re-asserts the card's
width every pass, and re-asserting `target_width` there would silently undo `fit_width` on
the next height fit.

**A card pinned narrower than its content does not fail; it lies.** The inner
`PanelContainer` — a real Container — grows out of the card and draws the background at the
content's width, so the card *looks* right while its own rect, and every placement decision
made from it, is the nominal number.

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/AutoSizingPanel.gd` | Shared helper for panels that expand to fit content — `fit_to_content` (height, ceiling `max_height`) and `fit_width` (width, ceiling `max_width`), both against the viewport |
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
  to both surfaces. Three reservers today: the **event dock** (`&"event_dock"`,
  `SIDE_TOP`/`SIDE_BOTTOM` — see `event-dock.md`), the **Inspector** (`&"inspector"`,
  `SIDE_LEFT` — `reserved_width()` / `reserved_width_changed` on show/hide + live
  drag-resize) and the **Band/City panel** (`&"band_panel"`, its currently-docked
  edge — see below).
- **`Main.RESERVER_PRIORITY` = `{event_dock: 0, inspector: 1, band_panel: 2}`** — the
  stable stacking order for co-edge reservers, LOWER sitting against the screen edge.
  It is read by `_update_band_panel_edge_offset()`, which offsets the Band panel
  inboard by the Σ sizes of lower-priority reservers on its own edge. A new reserver
  therefore only has to pick a number; the offset falls out. The event dock is 0
  because a thin strip on the rim reads as chrome and it keeps the band panel's
  position relative to the map fixed when the bar grows a row.
- **`Main._update_event_dock_insets()` is the PERPENDICULAR axis, and it is NOT
  priority.** `RESERVER_PRIORITY` orders reservers stacked ALONG one shared edge;
  a top/bottom strip and a left/right column are never co-edge, so priority has
  nothing to say about them and `_update_band_panel_edge_offset` correctly ignores
  the pairing. What the horizontal bar needs instead is to be pulled IN from the
  vertical columns — it starts right of whatever is docked left and stops left of
  whatever is docked right. Recomputed on every `_apply_reservation`, so a panel
  changing edge or collapsing moves the bar. It changes where the strip is DRAWN,
  never what it reserves. Conflating the two axes is the easy mistake, and it is
  the one that shipped: a `SIDE_TOP` bar spanning the raw window drew straight
  over the `SIDE_LEFT` band panel's tab bar.
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
  content grows without bound caps itself against the dock via the shared
  `ui/hud/DockScrollFit.gd` (the subject drawer is the remaining caller, the command
  feed having been retired); the Telling panel grows to fit its own bounded page
  capped at `PAGE_MAX_HEIGHT` and needs neither.

### PanelDock (`ui/PanelDock.gd`)
Ordered controller for one dock region's `VBoxContainer`. Panels `add(panel,
priority)` to register; the dock reparents them in priority order. Visibility is
data-driven — `set_relevant(panel, false)` (or `panel.visible = false`) removes a
panel from layout flow and the stack reflows with no gap. Hud builds `left_dock`
and `right_dock` in `_ready()`.

**The current roster:** LEFT = Tile 10 · Stockpile 20. **The command feed card is
gone** (issue #272): its events are the event dock's now (`event-dock.md`), `R` toggles
that instead, and the left column is the selection card's again — which is what its
40%-of-dock `DockScrollFit` cap existed to protect in the first place.
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

**Migration status:** `TilePanel` (the one selection card), `TellingPanel`, and
`TerrainLegendPanel` are now `PanelCard`s (the last two dropped the bespoke
`AutoSizingPanel` height math and the legend's absolute `PRESET_TOP_RIGHT`
positioning that used to overlap the Victory panel). `StockpilePanel` and
`VictoryPanel` are still plain `PanelContainer`s (correctly container-sized, but
not yet cards). `AutoSizingPanel.gd` remains only for the Inspector.

---

