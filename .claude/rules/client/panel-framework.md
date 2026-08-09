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

**A caller that mounts fresh content must apply its nominal width BEFORE the frame it measures in.**
`fit_width(0, 0)` is how: with no content measurement it can only resolve to `target_width`. The
height a later `fit_to_content` reads is a function of the width the content was laid out at, so a
card that spent that frame at its previous width (zero, on a first mount) reports the wrapping of a
column that no longer exists — measured on `BandComposeFloat` as a card left 100px taller than its
own content, which is the same lie as a card fitted too short, upside down.

`_fitted_width` is why the two fits cannot disagree: `fit_to_content` re-asserts the card's
width every pass, and re-asserting `target_width` there would silently undo `fit_width` on
the next height fit.

### "AGAINST THE VIEWPORT" MEANS AGAINST THE ROOM — `room_bounds`

The reserved-edge registry below insets `MapView` and the HUD's `LayoutRoot` by every docked
panel's strip, so once anything is docked the raw viewport is a rectangle **nothing else in
the client is using**. A free-floating card measured against it grows over the dock and over
whatever overlays the edge it just claimed — reported in play on the Materials & Crafting
panel, which ran the full height of the window and covered the top of the screen.

**`room_bounds` is the seam, and it is one rect for both jobs.** Set it to the Control the
registry has already inset (the HUD's `LayoutRoot`) and `available_room(margin)` gives the
caller its placement rect while `fit_to_content` takes its ceiling from the same rect's
bottom edge, so the placement and the height fit cannot disagree about how much room there
is. Take the height fit at the TOP of that room, never while the card is centred — the
ceiling derives from `global_position.y`, so a centred card throws away everything above it
(`crafting-panel.md` records the measured clamp).

It is **opt-in, and `null` is the correct answer for a card that IS a reserver** — the
Inspector reserves its own edge, so it must be measured against the whole window. A card
handed a room it cannot fit does not overflow: `fit_to_content` turns its internal scroll on
exactly when the content did not fit.

### AN OVERLAY IS A SECOND KIND OF NEIGHBOUR — `Hud.set_overlay_inset`

The event dock reserves nothing by design (it overlays the map — `event-dock.md`), so it is
not in the reservation registry and a card bounded by `LayoutRoot` shares a band with it: the
Materials & Crafting header was reported drawn *underneath* a top-docked event bar.

**A docked panel is drawn under the bar by its container; a free-floating card is drawn
through it**, because it places itself by arithmetic against a rect. So the fix is a rect and
not a reservation — making the bar a reserver would push the whole map layout down and undo a
decision that has nothing to do with whoever is colliding with it.

`Hud.set_overlay_inset(id, edge, size)` is the registry for it, the same `{edge, size}` shape
keyed by the same StringName id, and it writes a SECOND node: **`FloatingRoom`**, which is
`LayoutRoot`'s rect pulled further off every overlay. `LayoutRoot` is untouched, so the HUD's
own layout does not move; free-floating cards take `FloatingRoom` as their `room_bounds` and
get both bounds from one rect.

Three properties it is easy to get wrong:

- **`size` is ABSOLUTE — the depth covered measured from the screen edge**, already including
  whatever displacement pushed the surface inboard. The per-edge totals are therefore a
  **maximum**, where reservations are a **sum**: a reserved strip and an overlay drawn inboard
  of it overlap rather than stack.
- **`size <= 0` releases it, which is what a hidden surface publishes.** The event dock's
  `occupied_extent()` answers 0 while suppressed; it does NOT shrink when empty, because its
  height is content-independent by design.
- **The room must re-fit when the overlay moves, not only when a card opens.** The bar is
  toggleable (`R`), flips edge, and grows a row on a preference change — all under an
  already-open card. `set_overlay_inset` therefore calls `CraftingPanelController.refit_room()`,
  which re-fits (never re-renders — the payload is unchanged and a rebuild would lose the
  player's scroll position). `EventDockPanel` publishes `occupancy_changed(edge, extent)` from
  `_apply_dock_layout`, the one choke point every input to its geometry already runs through,
  and `Main` relays it; the initial value is seeded by hand in `_connect_event_dock`, the dock's
  own `_ready` having emitted before the parent could connect.

**A card pinned narrower than its content does not fail; it lies.** The inner
`PanelContainer` — a real Container — grows out of the card and draws the background at the
content's width, so the card *looks* right while its own rect, and every placement decision
made from it, is the nominal number.

**And the same is true of a card fitted too SHORT, with one extra consequence:** the panel grows out
of the bottom instead, and `fit_to_content`'s scroll test — which compares the caller's own
`content_height + extra_height` against the room below the card — under-reports by the same amount, so
a sheet that genuinely does not fit is left with its scroll DISABLED and runs off the screen. Whatever
a caller passes as `extra_height` must therefore be the chrome that is really there. `ComposeSheet`
measured its title label where its header ROW carries a taller ✕ button; the autopsy and the two
assertions that pin it are in `labor-ui.md` → "THE HEIGHT CHROME IS THE HEADER **ROW**".

## Key scripts

| Script | Purpose |
|--------|---------|
| `ui/AutoSizingPanel.gd` | Shared helper for panels that expand to fit content — `fit_to_content` (height, ceiling `max_height`) and `fit_width` (width, ceiling `max_width`), plus `available_room(margin)`, all measured against `room_bounds` where one was set and against the raw viewport where it was not. Callers: the Inspector, `ui/hud/BandComposeFloat.gd` and `ui/hud/CraftingPanel.gd` (the one that sets `room_bounds`, to the HUD's `FloatingRoom`) |
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
  to both surfaces. Three reservers today, all of them panels that span their edge:
  the **Inspector** (`&"inspector"`, `SIDE_LEFT` — `reserved_width()` /
  `reserved_width_changed` on show/hide + live drag-resize), the **Workbench**
  (`&"workbench"`, `SIDE_LEFT`, the designer surface replacing the Inspector) and the **Band/City
  panel** (`&"band_panel"`, its currently-docked edge — see below). **The event dock
  is not one of them** — it overlays (`event-dock.md`).
- **`Main.RESERVER_PRIORITY` = `{inspector: 0, workbench: 0, band_panel: 1}`** — the
  stable stacking order for co-edge reservers, LOWER sitting against the screen edge.
  It is read by `_update_band_panel_edge_offset()`, which offsets the Band panel
  inboard by the Σ sizes of lower-priority reservers on its own edge. A new reserver
  therefore only has to pick a number; the offset falls out. The Inspector and the
  Workbench share a rank because they are alternatives — opening either closes the
  other — so they are never co-edge with each other.
- **A NON-RESERVER CAN STILL BE DISPLACED, and that is a third thing again.**
  `Main._update_event_dock_edge_offset()` sums every reserver on the edge the event
  dock is docked to and pushes the total to `EventDockPanel.set_edge_offset`, so a
  co-edge band panel keeps the screen edge and the bar is drawn just past it. It
  takes **no priority test** where `_update_band_panel_edge_offset` needs one: the
  dock reserves nothing, so nothing can stack against it and it is by construction
  the innermost thing on its edge. Giving it a `RESERVER_PRIORITY` row instead is
  the reflex mistake — it would reintroduce the full-width reservation two bullets
  down. Recomputed on every `_apply_reservation` AND on the dock's own
  `dock_changed`, which is the only thing that can see the bar change edge.
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
- **A reservation is FULL WIDTH, and that is why not every panel wants one.**
  Every reserver insets both surfaces — the map so its content cannot hide under
  the strip, the HUD so its bars and docks reflow beside it — across the whole
  edge. The event dock briefly reserved and then stopped: its strip is bounded to
  the centre band, so the full-width reservation pushed the map down for a bar
  that filled only the middle of it and left bare background at the ends. It
  **overlays** instead (`event-dock.md`), and is not in this registry at all. A
  panel that does not span its edge should ask whether it should be reserving.

### The HUD's own side columns are AUTHORED, and a horizontal panel must respect them

A panel that spans the HUD's width draws over the left dock and the right dock —
neither of which is a reserver, so bounding against the reservation registry
alone is not enough. `Hud.left_column_width()` / `right_column_width()` publish
those widths, and they read `custom_minimum_size.x` off the scene's own regions
rather than measuring content:

| Region | Authored | Why it cannot drift |
|---|---|---|
| `LeftDock` | 360 | `PanelDock` zeroes the stack's horizontal minimum on construction, so no card can widen the column |
| `RightDock` | 344 | its card minimum (320) plus its authored margins (8 + 16), authored as the outer minimum so the published number is the one it renders at |

**THE THIRD REGION IS GONE, AND IT WAS THE ONE THIS RULE WAS WRITTEN FOR.** `TurnBlock` — the
top-right readout column carrying `Turn N`, `Units · Logistics · Sentiment`, the Sedentarization
meter, the `Pop …` demographics line, the discovered-sites strip and the `⚒ Your people know:`
strip — is retired outright (issue #450), and `TopBar` with it, the block having been its only
content besides the spacer that pushed it right. It is why the rule exists: the block had **no**
minimum of its own, was pure text width, and rendered 419px against the 344 it was later authored
to — a measured width deciding a panel's edge, which is exactly what must not happen.

Two consequences: `right_column_width()` is one region's authored minimum rather than the wider of a
pair, and the `ContentRow` now starts at the top of the screen, so **a TOP-edge bar or card shares a
vertical band with the RIGHT DOCK** where it used to share one with the readouts. The clearance
claims moved with it, not away.

**A bar whose edge tracked a MEASURED width is the flicker rule again**: it would
jump when the player selects a tile and the selection card appears, or when a
metric gains a digit — and worse than on the band panel, since an event arrives
every turn. `ui_preview` asserts the live rects never exceed the authored numbers,
so a scene edit that outgrows a column fails loudly instead of the bar quietly
overlapping.
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
- **THE TITLE MAY NEVER SET THE CARD'S WIDTH.** The header is a `PanelContainer`
  over an `HBoxContainer` of two `Label`s — `CardKind` (the eyebrow) and
  `CardTitle`, which is `SIZE_EXPAND_FILL` with `clip_text` +
  `OVERRUN_TRIM_ELLIPSIS` and a `tooltip_text` carrying the untrimmed string
  (set from `resized`, and left EMPTY when the title fits, so no card explains a
  line the player can already read). It was a `bbcode` `RichTextLabel` with
  `fit_content = true` and `AUTOWRAP_OFF`, which made a title's unwrapped width a
  hard card minimum: a 58-character title dragged the right column to **489**,
  137px past `Hud.RIGHT_COLUMN_CEILING` — and that ceiling is an upper bound a
  forward-reasoning predicate consumes, so a card wider than it is drawn THROUGH
  the readouts. **The node type is the fix, not a property**: `RichTextLabel`
  exposes neither `clip_text` nor `text_overrun_behavior` (checked against
  Godot 4.7's `extension_api.json`), and its `fit_content` drives both axes with
  no per-axis switch. `TellingPanel._build_chrome` still depends on the header
  being child index 0 of `CardContent`; it is.
- **The KIND eyebrow is deliberately still unbounded.** An `HBox` pays every
  child its minimum before the expanding one gets anything, so trimming both
  would collapse both to slivers instead of spending the shortfall on the one
  string that can be arbitrarily long. `card_kind` is a one-word authored
  vocabulary with a single value today (`"Tile"`); a long one could widen a card
  again. A conscious limit, not an oversight.
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
not yet cards). `AutoSizingPanel.gd` has **two** callers: the Inspector, and
`ui/hud/BandComposeFloat.gd` — the parties compose sheet floated off the Band
panel when its zone cannot hold it (`band-city-panel.md`). The float is the
free-floating case by the test at the top of this file: its ceiling is the
VIEWPORT, not a dock's remaining height, so `PanelCard` + `DockScrollFit` there
would fight a container that does not exist.

---

