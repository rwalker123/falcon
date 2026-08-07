extends CanvasLayer
class_name BandCityPanel

## The dockable Band / City panel (docs/plan_band_city_dock.md §"Architecture 2").
##
## A CanvasLayer that renders a card against one screen edge and *reserves* that
## strip via the slice-1 reservation API (Main fans `reservation_changed` out to
## `MapView`/`Hud`, so the map + HUD reflow off the edge rather than being
## overlaid). Chrome: settlement header (stage glyph + name + stage label), a
## settlement cycler, a 4-cell dock chooser, and a collapse toggle, plus dock +
## tab persistence.
##
## THE BODY IS **THREE NAMED ZONES AT A FIXED CROSS-AXIS SIZE** (`set_zones`):
## `band` (vitals), `work` (the paged work board) and `parties`. Nothing is
## balanced, so no content can migrate between zones; nothing is fitted to
## content, so the reservation this panel reports changes ONLY on dock / collapse
## / hide / viewport-resize — never on a content edit. That is the whole point of
## the model: the previous block-packing body re-measured on every render and
## re-emitted `reservation_changed`, which invalidated the map cache and flickered
## the map on every `+` press.
##
## Two SHELLS host those zones, chosen by the panel's own WIDTH (never by dock
## edge, so a resizable dock needs no special case):
##   * WIDE  (width >= `WIDE_SHELL_MIN_WIDTH`, DERIVED from what the wide shell needs — in practice
##     a T/B dock on a wide window): the three
##     zones side by side, band + parties fixed-width, work taking the rest,
##     hairline separators between. No tab bar.
##   * NARROW (otherwise, in practice a L/R dock): a tab bar under the header and
##     exactly one zone beneath it filling the panel.
##
## There is deliberately **no ScrollContainer anywhere in this panel** — the design
## is no-scroll (the work zone pages itself against `work_zone_size()`), and a
## scroll container would silently reintroduce content-dependent sizing.
##
## All geometry/typography flows from named constants + `HudStyle` (no magic
## numbers, one visual-language source).

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

# ---- geometry (canvas-space px) --------------------------------------------
## Cross-axis size of the expanded panel when docked L/R. FIXED — it is never fitted to the band
## content, so the reserved strip (and therefore the map inset) cannot move when a section grows.
const PANEL_WIDTH := 380.0
## Cross-axis size of the expanded panel when docked T/B. Likewise fixed; tall enough for the three
## zones' rows without eating the map.
##
## **IT IS THE BODY'S BUDGET, NOT THE STRIP'S HEIGHT** — `_horizontal_panel_height()` adds whatever the
## ACTIVE SHELL spends on its own chrome (the narrow shell's tab bar) before clamping to
## `MAX_WIDE_HEIGHT_FRACTION`, so both shells hand their zones the same box. Read as a flat strip
## height it is 35px short in the narrow shell, which is the zone the tab bar used to be paid out of.
const PANEL_HEIGHT_WIDE := 360.0
## Cross-axis size when collapsed to a thin rail (both orientations).
const COLLAPSED_SIZE := 46.0
## Render above the map (and the HUD/Inspector) so the panel owns its reserved strip.
const LAYER_INDEX := 103
## Accent seam thickness on the panel's map-facing edge (the prototype's SIGNAL_DEEP border).
const SEAM_THICKNESS := 2.0

# ---- chrome typography / sizing --------------------------------------------
const STAGE_GLYPH_FONT_SIZE := 20
## Bundled stage sprite box, sized to the glyph label's font size so swapping a `Label` for a
## `TextureRect` leaves the header's height and the rows beside it exactly where they were.
const STAGE_SPRITE_SIZE := Vector2(STAGE_GLYPH_FONT_SIZE, STAGE_GLYPH_FONT_SIZE)
const NAME_FONT_SIZE := 15
const STAGE_LABEL_FONT_SIZE := 10
## Gap between the stage word and the band's hex coordinates on the header's second line. Its own
## const rather than the borrowed `HEADER_SEPARATION`: that one spaces the header's top-level
## CLUSTERS (subject / cycler / dock chooser / collapse), and this spaces two words inside one of
## them, which reads too loose at the cluster gap.
const STAGE_ROW_SEPARATION := 6
const CYCLER_FONT_SIZE := 13
const COUNT_FONT_SIZE := 11
const ICON_BUTTON_FONT_SIZE := 13
const HEADER_SEPARATION := 8
const COLUMN_SEPARATION := 0
# Clickable subject cluster ("jump to my band"): a subtle rounded hover tint (transparent
# otherwise); same content margins in both states so hover doesn't shift the header layout.
const SUBJECT_HOVER_CORNER_RADIUS := 5
const SUBJECT_HOVER_PADDING_H := 4
const SUBJECT_HOVER_PADDING_V := 2
const ICON_BUTTON_SIZE := 24.0
const DOCK_CELL_SIZE := 16.0
const DOCK_CELL_SEPARATION := 3
const DOCK_ACCENT_WIDTH := 4
const CORNER_RADIUS := 3
const COUNT_MIN_WIDTH := 30.0
const BODY_EMPTY_TEXT := "No band selected"
const BODY_SEPARATION := 8
## Card inner padding (the PanelContainer content margins). Named so the wide-dock fit-to-content
## height math reuses the exact same paddings the card draws with (no magic 12/10 duplicated).
const PANEL_CONTENT_MARGIN_H := 12
const PANEL_CONTENT_MARGIN_V := 10
## Card border thickness (`panel_card_stylebox`), subtracted alongside the content margins when the
## panel reports the interior box its Work zone may fill. Declared here, beside the margins it is
## always summed with, so `PANEL_CHROME_H` below can be a `const`.
const PANEL_BORDER_WIDTH := 1.0
## What the card's own horizontal chrome costs — the border plus the content margins the card draws
## with, i.e. exactly what `_interior_size()` subtracts from `_panel_extent().x`. Named so the shell
## threshold (which is tested against the panel's OUTER width) can add it back, and so
## `ZONE_PARTY_WIDTH` below can state the narrow shell's zone width as a derivation. Declared HERE,
## with the two terms it sums, because a `const` may not reference one declared further down.
const PANEL_CHROME_H := 2.0 * (float(PANEL_CONTENT_MARGIN_H) + PANEL_BORDER_WIDTH)
# ---- responsive body layout (wide 3-column shell vs narrow tabbed shell) -----
## Fixed widths of the two flanking zones in the wide shell; Work takes whatever is left.
## **THE BAND ZONE IS `PANEL_WIDTH` WIDE, DELIBERATELY THE SAME NUMBER**: the NARROW shell hands its
## one zone the panel's strip less chrome (`ZONE_PARTY_WIDTH` below is exactly that), so a band column
## narrower than that gave the layout with a whole screen to spend LESS width for the same rows than
## the layout squeezed into a side dock — and the band zone CLIPS rather than scrolls, so the width it
## lacks comes straight off its vitals rows as wraps. It takes the full 380 rather than that floor
## because it is the zone whose rows are widest (the merged Food line measures 353px) and, unlike the
## parties zone, 380 here still leaves the work board two columns at 1920 on every shipped map. 380 is
## already this file's vocabulary (`PANEL_WIDTH`, `ZONE_WORK_MIN_WIDTH`): one readable column of rows.
const ZONE_BAND_WIDTH := 380.0
## The most columns the BAND flank will lay its blocks out across on a horizontal dock.
##
## **GROW THE LONG AXIS, NEVER THE RESERVED ONE.** The panel reserves its CROSS axis — width on a
## vertical dock, height on a horizontal one — so growth along the reserved axis re-emits
## `reservation_changed`, re-insets `MapView` and invalidates its cache, which is the map flicker the
## fixed cross-axis size exists to prevent. Growth along the LONG axis costs nothing. A horizontal
## dock therefore spends a wide monitor on band COLUMNS rather than on a taller strip. It does NOT buy
## the strip's height back — see `PANEL_HEIGHT_WIDE` for the two constraints that turned out to pin
## that, neither of them this flank.
##
## **TWO, because the split is AUTHORED** (`BandPanelController.build_band_zone`): the blocks are
## heterogeneous — a wrapped vitals label, two composition bars, a row of role cards — so a generic
## reflow produces nonsense, and a third column would need a third authored split with nothing
## measured to put in it. Measured at 380px on the SHORT tier: vitals 52, PEOPLE 58, WORKFORCE 139.
const BAND_ZONE_MAX_COLUMNS := 2
## **THE PARTIES ZONE IS EXACTLY THE NARROW SHELL'S ZONE WIDTH** — the panel's strip less the card
## chrome — which IS the requirement: the wide shell must never hand a zone LESS room than the side
## dock does for the same content, and this zone's four-rung compose picker is already 2×2 at that
## width because 3-across does not fit. It is deliberately NOT `ZONE_BAND_WIDTH`'s 380, and the
## difference is MEASURED, not taste: the flanks come out of the work zone, and at 1920 in a bottom
## dock on the widest shipped map (Large, whose chrome rail is 308px against Standard's 296) a 380px
## parties zone leaves the board 751px — under 2 × `ZONE_WORK_MIN_WIDTH`, so the work board drops to
## ONE column, which is the very thing the wide shell exists to prevent. The ceiling that keeps two
## columns there is 371; 354 clears it by 17px. Raise this only with that measurement re-run.
const ZONE_PARTY_WIDTH := PANEL_WIDTH - PANEL_CHROME_H
## The NARROWEST the WORK zone may be for the wide shell to be worth choosing: one readable board
## column. MIRRORS Hud's `WORK_COLUMN_MIN_WIDTH` (380) — the width below which `_work_board_capacity`
## clamps to a single column, and a single column crammed into less than that clips its row labels.
## Kept here rather than read from Hud — the panel must not depend on its content's internals — but
## this and `ZONE_WORK_MAX_WIDTH` are a PAIR with Hud's column consts: change the board's column
## width or cap and change both of these with it.
const ZONE_WORK_MIN_WIDTH := 380.0
## The widest the WORK zone can use: Hud's board stops adding columns at `WORK_MAX_COLUMNS` (4) of
## `WORK_COLUMN_MIN_WIDTH` (380), so past this a wider zone only stretches the same rows. Kept here
## rather than read from Hud — the panel must not depend on its content's internals — but the two are
## a PAIR: change the board's column cap and change this with it (and `ZONE_WORK_MIN_WIDTH` with it).
const ZONE_WORK_MAX_WIDTH := 1520.0
## The most board columns the work zone will ever draw — the pair above stated as a count, since since
## issue #377 the card's width is built UP from a column count rather than clamped DOWN from a width.
## Derived, so it cannot drift from the two widths it comes from.
const WORK_MAX_COLUMNS := int(ZONE_WORK_MAX_WIDTH / ZONE_WORK_MIN_WIDTH)
## Hairline separator drawn between adjacent zones in the wide shell.
const ZONE_SEPARATOR_THICKNESS := 1.0
## Gap either side of a zone separator, so the hairline is not flush against zone content.
const ZONE_SEPARATION := 12
## What ONE separator + its gaps cost — the hairline plus a `ZONE_SEPARATION` gap either side. Named
## because the trailing chrome rail is separated from the content column by exactly one of these
## (`_rail_span()`), so the rail's gutter costs the same as any inter-zone gutter by construction
## rather than by a matching pair of literals.
const RAIL_SEPARATOR_SPAN := ZONE_SEPARATOR_THICKNESS + 2.0 * float(ZONE_SEPARATION)
## What the TWO separators + their gaps cost the wide shell's interior width — written as two of the
## above so the two can never disagree. A `const` (not just `_wide_separator_span()`) because
## `WIDE_SHELL_MIN_WIDTH` below is itself a const and cannot call a function; the function returns this
## so the threshold, the content cap and `work_zone_size` can never disagree about how much width the
## chrome eats.
const WIDE_SEPARATOR_SPAN := 2.0 * RAIL_SEPARATOR_SPAN
## The panel switches to the wide (3-zones-side-by-side) shell once its own WIDTH reaches this;
## below it the narrow (tabbed, one-zone) shell is used. A WIDTH test, never a dock-edge test, so a
## resizable dock or a narrow window needs no special case. DERIVED, never hand-picked: the wide
## shell is only worth choosing when it can still give the work zone one readable column, so this is
## exactly what the three zones + the separators + the card chrome need (380 + 354 + 380 + 50 + 26 =
## 1190). It is compared against the OUTER `_panel_extent().x`, hence the chrome term — below it the
## narrow shell would hand the board the panel's whole interior, so flipping wide too early makes the
## board several times NARROWER, degrading the very thing the wide shell exists to improve.
const WIDE_SHELL_MIN_WIDTH := ZONE_BAND_WIDTH + ZONE_PARTY_WIDTH + ZONE_WORK_MIN_WIDTH \
	+ WIDE_SEPARATOR_SPAN + PANEL_CHROME_H
## **THE STRIP DOES NOT GET SHORTER WHEN THE BAND FLANK WIDENS, AND THAT IS MEASURED.**
##
## The width growth was expected to buy height back — a two-column flank needs 139px where one needs
## 273 — so a `PANEL_HEIGHT_WIDE_TWO_COLUMN` was built and tried at 230. It cannot exist: the band
## flank was never what pinned this budget. **TWO other things bind above it, both unrelated to the
## band zone**, and 360 is already their minimum:
##
##   * **the PARTIES zone's worst case, 294px of body** — a hunt party lighting all seven inspector
##     lines at once (`band_panel_worst_case_party`; the budget is spelled out in
##     `band-city-panel.md` → "The parties strip's SEVEN lines"). At a 230px strip its box is 170 and
##     it overflows by 124.
##   * **the PARKED CHROME STACK, ~322px of strip** — `DockRowController._required_height()` is the
##     nav cluster (164) + the turn cluster (128) + `RAIL_SLOT_SEPARATION` + the card's own chrome, and
##     the reflow gate is `reserved >= that`. Below it the gate DECLINES, `BottomBar` keeps the minimap
##     and orb, and a bottom dock silently loses the whole issue-#324 dock-row reflow.
##
## 294 + the header + the card's vertical chrome IS 360. So the flank's saved height is real and has
## nowhere to go, and this const stays a single budget for both column counts.
## Safety net so a short window can never let the T/B strip eat the screen: the reserved wide-dock
## height is the body budget clamped to this fraction of the window height.
const MAX_WIDE_HEIGHT_FRACTION := 0.6
## Header height used for the interior maths before the header has laid out once (it is pure chrome —
## two text rows beside `ICON_BUTTON_SIZE` controls — so this is a bootstrap value, not a guess about
## content).
const HEADER_HEIGHT_FALLBACK := 44.0

# ---- narrow-shell tab bar ---------------------------------------------------
## Zone keys. The same keys index `set_zones`' slots, the tab bar and `set_tab_badge`.
const ZONE_BAND := &"band"
const ZONE_WORK := &"work"
const ZONE_PARTIES := &"parties"
## Tab order + display labels, in the prototype's order (Band · Work · Parties).
const TAB_ORDER: Array[StringName] = [ZONE_BAND, ZONE_WORK, ZONE_PARTIES]
const TAB_LABELS := {
	ZONE_BAND: "Band",
	ZONE_WORK: "Work",
	ZONE_PARTIES: "Parties",
}
## The tab a fresh session opens on: work is the zone the player acts in.
const DEFAULT_TAB := ZONE_WORK
const TAB_FONT_SIZE := 12
const TAB_BADGE_FONT_SIZE := 10
const TAB_SEPARATION := 4
const TAB_PADDING_H := 10
const TAB_PADDING_V := 5
## Thickness of the active tab's underline (the prototype's SIGNAL rule under the selected tab).
const TAB_UNDERLINE_THICKNESS := 2
const TAB_BADGE_CORNER_RADIUS := 7
const TAB_BADGE_PADDING_H := 5
const TAB_BADGE_PADDING_V := 1
const CYCLE_PREV := -1
const CYCLE_NEXT := 1

# ---- chrome glyphs (geometric — render reliably, unlike emoji magnifiers) ---
const COLLAPSE_GLYPH := "▾"   # ▾  minimize
const EXPAND_GLYPH := "▸"     # ▸  restore
const CYCLE_PREV_GLYPH := "◀" # ◀
const CYCLE_NEXT_GLYPH := "▶" # ▶
const DEFAULT_STAGE_GLYPH := "⛺" # ⛺  nomadic fallback

# ---- persistence (decision 5 — first client user-pref file) ----------------
const CONFIG_PATH := "user://band_city_dock.cfg"
const CONFIG_SECTION := "dock"
const CONFIG_KEY_EDGE := "edge"
const CONFIG_KEY_COLLAPSED := "collapsed"
## The narrow shell's selected tab, so a reopened session lands where the player left it.
const CONFIG_KEY_TAB := "tab"
## The WORK board's chosen sort, stored as an OPAQUE string: the sort vocabulary belongs to
## `BandPanelController`, so this panel persists the word without ever knowing what it means.
const CONFIG_KEY_WORK_SORT := "work_sort"
## Preview harnesses point this at a scratch file so a render can neither READ nor WRITE the
## player's real dock prefs — the same isolation `NarrativeForkPanel.config_path_override` gives the
## HUD-panel prefs, and for the same reason: without it a harness renders whatever tab the LAST run
## happened to leave selected, and then saves its own tab walk back over the player's.
static var config_path_override: String = ""

## The four dock edges, in the prototype's 2×2 chooser order (row-major:
## left/top on the first row, bottom/right on the second).
const DOCK_EDGES: Array[int] = [SIDE_LEFT, SIDE_TOP, SIDE_BOTTOM, SIDE_RIGHT]
## The two SLOTS of the row's trailing chrome rail (issue #324), stacked top-to-bottom: the HUD parks
## its nav cluster in the top one and its turn cluster in the bottom one. ONE column at the trailing
## end, never a gutter at each end — two opposite gutters cost ~562px of row, pushed the band zone
## inward AND stranded dead space around the orb; one column costs `max(nav, turn)` ≈ 296–302 depending
## on map aspect (296 Standard, 302 Large) instead.
const RAIL_SLOT_TOP := 0
const RAIL_SLOT_BOTTOM := 1
## Slot order, top-to-bottom — also what `_apply_rail` and the HUD's restore iterate.
const RAIL_SLOT_ORDER: Array[int] = [RAIL_SLOT_TOP, RAIL_SLOT_BOTTOM]
## Gap between the two stacked clusters. Its own const rather than a borrowed `HEADER_SEPARATION` /
## `BODY_SEPARATION`: those are the header's and the tab bar's gaps, and this is read by
## `DockRowController._required_height` as part of the stack's measured height.
const RAIL_SLOT_SEPARATION := 8

signal reservation_changed(edge: int, size: float)
signal cycle_requested(delta: int)
## The header subject cluster (stage glyph + name + stage label) was clicked — "jump to my band".
signal subject_activated
## `work_zone_size()` changed — a shell flip, dock change, collapse or viewport resize. Hud re-pages
## its work board on this rather than re-rendering everything.
signal zones_resized

var _dock_edge: int = SIDE_LEFT
var _collapsed: bool = false
var _shown: bool = true
## The cross-axis size last published through `reservation_changed`, so `_republish_reservation_if_changed`
## can tell a size the panel merely re-derived from one nobody downstream has been told about. Seeded to a
## value no reservation can take, so the first republish after a declared input arrives is never suppressed
## by a coincidence with an unset member.
var _published_reservation: float = -1.0
# Leading (inboard) offset from the docked edge, pushed by Main = Σ sizes of co-edge reservers
# inboard of this panel (today: the Inspector's strip when both dock left). Keeps co-edge panels
# stacked, not overlapping. Does NOT change what this panel reserves (the map/HUD inset is the
# per-edge SUM), only where its own Control anchors.
var _edge_offset: float = 0.0

# nodes
var _root: Control
var _panel: PanelContainer
var _seam: ColorRect
var _header_full: HBoxContainer
var _header_rail: VBoxContainer
var _subject_cluster: PanelContainer
var _stage_glyph_label: Label
var _rail_glyph_label: Label
## Bundled-sprite siblings of the two glyph labels. Exactly one of each pair is visible at a time
## (see `set_header`): the sprite when the stage has bundled art, else the emoji label.
var _stage_glyph_sprite: TextureRect
var _rail_glyph_sprite: TextureRect
var _name_label: Label
var _stage_label: Label
## The band's hex coordinates, beside the stage word on the header's second line. IDENTITY, not a
## vital: it answers "which band am I looking at", exactly as the name and the stage do, so it lives
## in the header rather than as a row in the band zone's vitals grid (where it cost that
## height-capped zone a row it could not spare, and rendered only on the map-click path). Hidden when
## the caller passes no coordinates, so an empty value costs no gap.
var _position_label: Label
var _count_label: Label
var _collapse_button: Button
var _rail_expand_button: Button
# Body layout: `_body_host` holds the two alternative SHELLS, exactly one visible at a time (chosen by
# panel width — see `_shell_is_wide`). The wide shell is an HBox of three zone hosts (band + parties
# fixed-width, work expanding) with hairline separators; the narrow shell is a tab bar over a single
# zone host. `_zones` holds the three Hud-built zone Controls the panel OWNS (freed on the next
# `set_zones`); `_reparent_zones` homes them into whichever hosts are active, so a shell flip needs no
# Hud re-render. Nothing here measures content — the shells fill a card whose size is fixed per dock.
## The card's whole content column (header + body). It always FILLS its card: since issue #377 the CARD
## is the thing sized to its content, and the centring happens one level up on the card as a whole.
var _panel_column: VBoxContainer
## The card's row, holding just the content column. The chrome rail was its trailing cell until issue
## #377 made the two separate islands, so the row is a single-cell wrapper now — see `_build`.
var _card_row: HBoxContainer
## The strip's TRAILING chrome rail (issue #324): one column at the strip's right end holding the HUD's
## bottom-bar chrome stacked vertically — nav cluster on top, turn cluster below — so it shares the
## panel's row instead of stacking against it. A SIBLING of the card under `_root` since issue #377, not
## a cell of `_card_row`. The panel owns the rail, its stack and the two slot HOSTS, and
## NOTHING inside those hosts — contrast `set_zones`, which owns and frees the zone contents it is
## handed. Never add to, read, or free a slot's children here.
## `_rail` is a PLAIN `Control` and that is load-bearing: it blocks the stack's minimum size from
## propagating out to the card, which is what lets everything INSIDE it be ordinary containers.
var _rail: Control
## The rail's two slots, stacked and centred inside it (`_build_rail` states why the centring is a
## container's job and not anchor arithmetic).
var _rail_stack: VBoxContainer
var _rail_slots: Dictionary = {}          # slot:int (RAIL_SLOT_*) -> Control host
## The rail column's width, DECLARED by the HUD (`set_rail_width`) — never measured from the content.
var _rail_declared_width: float = 0.0
## What the card must LEAVE at each end of a horizontal strip, declared by `Main` (`set_lateral_bounds`)
## — the HUD's left and right column widths. Only a TOP dock has any, because it is the only edge where
## the HUD does NOT yield its strip (see `Main._reserver_overlays_hud`); a bottom dock's HUD moves out of
## the way, so the card has the whole row.
var _bound_leading: float = 0.0
var _bound_trailing: float = 0.0
## How many columns the WORK board wants, DECLARED by `BandPanelController` (`set_work_columns`). It is
## what the card's wide-shell width is built from. Seeded at the maximum so the first layout pass — which
## happens before any controller has counted anything — draws the widest card rather than a one-column
## sliver that then jumps wider.
var _work_columns: int = WORK_MAX_COLUMNS
var _body_host: VBoxContainer
var _wide_shell: HBoxContainer
var _wide_zone_hosts: Dictionary = {}   # zone:StringName -> Control (a plain, clipping zone host)
var _narrow_shell: VBoxContainer
var _tab_bar: HBoxContainer
var _narrow_zone_host: Control
var _body_is_wide: bool = false
var _band_present: bool = false
var _empty_state: Label
## The three zone contents the panel currently owns (zone:StringName -> Control). A zone may be absent
## or null → that zone renders empty.
var _zones: Dictionary = {}
## Narrow-shell tab state: the selected zone key (persisted) and each tab's badge (`{text, hot}`).
var _active_tab: StringName = DEFAULT_TAB
## The WORK board's persisted sort, opaque to this panel. `""` = the player has never chosen one, so
## the controller keeps its own default.
var _work_sort_pref: String = ""
var _tab_badges: Dictionary = {}
var _tab_buttons: Dictionary = {}   # zone:StringName -> Control (the tab cell)
## The last `work_zone_size()` reported, so `zones_resized` fires on a real change only.
var _last_work_zone_size: Vector2 = Vector2.ZERO
## The last `band_zone_columns()` reported, beside it — see `_notify_zones_resized` for why the work
## box alone is not enough of a trigger.
var _last_band_columns: int = 0
var _dock_cells: Dictionary = {}   # edge:int -> Button

func _ready() -> void:
	layer = LAYER_INDEX
	_load_prefs()
	_build()
	_apply_dock_layout()
	_refresh_collapse_state()
	_refresh_dock_cells()
	# A window resize changes the T/B panel width (hence the shell) and the clamped wide height, so
	# re-choose the shell and re-report both the reservation and the work-zone box.
	var vp := get_viewport()
	if vp != null:
		vp.size_changed.connect(_on_viewport_resized)
	_notify_zones_resized()

# ---- public API ------------------------------------------------------------

## Push the header subject: settlement stage id (the server's stable key), its emoji glyph
## fallback, display name, stage label, and the band's preformatted hex coordinates. The stage
## renders as bundled art when `StageSprites` has a texture for the id; a stage with no bundled art
## (the config is user-editable) keeps its emoji.
## `position_label` is a preformatted `String` the CALLER resolves (the panel never reads a band
## dict), and `""` renders nothing — the caller could not resolve the coordinates.
func set_header(stage_id: String, glyph: String, subject_name: String, stage_label: String,
		position_label: String = "") -> void:
	var resolved_glyph := glyph if not glyph.is_empty() else DEFAULT_STAGE_GLYPH
	var sprite := StageSprites.for_stage(stage_id)
	_apply_stage_visual(_stage_glyph_label, _stage_glyph_sprite, sprite, resolved_glyph)
	_apply_stage_visual(_rail_glyph_label, _rail_glyph_sprite, sprite, resolved_glyph)
	if _name_label != null:
		_name_label.text = subject_name
	if _stage_label != null:
		_stage_label.text = stage_label
	if _position_label != null:
		_position_label.text = position_label
		_position_label.visible = not position_label.is_empty()

## Update the cycler readout ("index+1 / count"). count <= 0 blanks it.
func set_cycler(index: int, count: int) -> void:
	if _count_label == null:
		return
	if count <= 0:
		_count_label.text = "–"   # en-dash placeholder
	else:
		_count_label.text = "%d / %d" % [index + 1, count]

## Hand the panel its three zone contents. The panel takes OWNERSHIP (frees the previous set) and
## parents them into whichever shell is active. Any may be null → that zone renders empty.
func set_zones(band: Control, work: Control, parties: Control) -> void:
	_free_zones()
	_zones[ZONE_BAND] = band
	_zones[ZONE_WORK] = work
	_zones[ZONE_PARTIES] = parties
	if band == null and work == null and parties == null:
		set_band_present(false)
		return
	_band_present = true
	if _empty_state != null:
		_empty_state.visible = false
	if not _body_is_wide:
		_rebuild_tab_bar()   # which zones exist can move `_effective_tab`, hence the highlight
	_reparent_zones()
	_update_body_visibility()

## The box the Work zone's content may fill, in canvas px — the zone's INTERIOR, after the panel's own
## chrome (card border + content margins + header, and in the narrow shell the tab bar). Hud sizes its
## paged work board from this. Purely a function of the dock edge, the collapse state and the window;
## it never consults the content.
func work_zone_size() -> Vector2:
	if _collapsed or not _shown:
		return Vector2.ZERO
	var interior := _interior_size()
	var body_height: float = maxf(interior.y - _header_height(), 0.0)
	if _shell_is_wide():
		var flanks := _band_flank_width() + ZONE_PARTY_WIDTH
		# The card is built UP from the declared column count now (issue #377), so its interior is
		# flanks + separators + the board — no cap to clamp against, and this comes back as exactly
		# `_work_columns × ZONE_WORK_MIN_WIDTH`. The `max` still guards the clamped-card case, where a
		# window narrower than the content leaves less than the flanks alone want.
		return Vector2(maxf(interior.x - flanks - _wide_separator_span(), 0.0), body_height)
	return Vector2(interior.x, maxf(body_height - _tab_bar_height(), 0.0))

## The box the PARTIES zone's content may fill, in canvas px. Its HEIGHT is the body's, i.e. exactly
## what `work_zone_size` reports — every wide-shell zone shares the card's one body height — and its
## WIDTH is this zone's own FIXED flank rather than the work board's expanding column, which is the
## whole difference between the two answers. `BandPanelController` measures the compose sheet against
## it (see `BandComposeFloat`), so reading the work board's width there would tell a 380px-wide sheet
## it had a 1520px column.
func parties_zone_size() -> Vector2:
	var box := work_zone_size()
	if box == Vector2.ZERO:
		return box
	return Vector2(ZONE_PARTY_WIDTH if _shell_is_wide() else box.x, box.y)

## The CARD's global rect — the island the strip holds, not the strip (`_root`) itself. Published for
## the free-floating compose card, which anchors itself to the card's map-facing edge and must never
## overlap it; every other reader of this geometry lives inside this file. See `_position_card_and_rail`
## for why the two rects stopped being the same one.
func card_rect() -> Rect2:
	return _panel.get_global_rect() if _panel != null else Rect2()

## Push a tab's badge (narrow shell only; ignored in the wide shell, which has no tab bar).
## `hot` tints it WARN. An empty `text` clears the badge.
func set_tab_badge(zone: StringName, text: String, hot: bool) -> void:
	if not TAB_LABELS.has(zone):
		return
	_tab_badges[zone] = {"text": text, "hot": hot}
	if not _body_is_wide:
		_rebuild_tab_bar()

## Toggle between the band-detail content and the empty-state placeholder. `false` also frees any
## owned zones (no band → nothing to show).
func set_band_present(present: bool) -> void:
	_band_present = present
	if not present:
		_free_zones()
	if _empty_state != null:
		_empty_state.visible = not present
	_update_body_visibility()

## Free (and detach) the zone contents from the previous render. Ownership is unambiguous: the panel
## owns exactly what it was last handed, and drops it here before taking the next set.
func _free_zones() -> void:
	for key in _zones:
		var zone_variant: Variant = _zones[key]
		if zone_variant is Node:
			_detach(zone_variant)
			(zone_variant as Node).queue_free()
	_zones.clear()

## Dock the panel to an edge (a Godot SIDE_* const). Re-anchors, persists, and
## re-emits the reservation so the map + HUD reflow.
func set_dock(edge: int) -> void:
	if not DOCK_EDGES.has(edge):
		return
	if edge == _dock_edge:
		return
	_dock_edge = edge
	_apply_dock_layout()
	_refresh_dock_cells()
	_save_prefs()
	_emit_reservation()
	_notify_zones_resized()

func get_dock() -> int:
	return _dock_edge

## Set the leading (inboard) offset from the docked edge so this panel stacks outboard of any
## co-edge reserver (Main computes it = Σ sizes of inboard co-edge reservers). Re-anchors only;
## does NOT re-emit the reservation (the size this panel reserves is unchanged).
func set_edge_offset(px: float) -> void:
	var offset: float = maxf(px, 0.0)
	if is_equal_approx(offset, _edge_offset):
		return
	_edge_offset = offset
	_apply_dock_layout()

## Rail the panel to a thin strip (or restore it). Persists + re-emits the
## reservation so the map + HUD reflow to the collapsed size.
func set_collapsed(collapsed: bool) -> void:
	if collapsed == _collapsed:
		return
	_collapsed = collapsed
	_refresh_collapse_state()
	_apply_dock_layout()
	_save_prefs()
	_emit_reservation()
	_notify_zones_resized()

func is_collapsed() -> bool:
	return _collapsed

## Show/hide the panel; hiding releases its reserved strip (slice 3 gates this on
## band selection). Emits the reservation change.
func set_shown(shown: bool) -> void:
	if shown == _shown:
		return
	_shown = shown
	if _root != null:
		_root.visible = shown
	_emit_reservation()
	_notify_zones_resized()

## The strip the panel currently reserves (0 hidden, COLLAPSED_SIZE collapsed,
## else the cross-axis size). Main queries this to seed the initial reservation.
func current_reservation_size() -> float:
	if not _shown:
		return 0.0
	return _cross_axis_size()

# ---- construction ----------------------------------------------------------

func _build() -> void:
	_root = Control.new()
	_root.name = "PanelRoot"
	_root.visible = _shown
	# **THE LAYOUT REGION IS TRANSPARENT TO THE POINTER** (issue #377) — the exact OPPOSITE call from
	# `EventDockPanel._build`, and for the same mechanism read the other way round. `_root` spans the
	# WHOLE reserved strip (`_apply_root_anchors`), but since the card became a floating island most of
	# that strip is LIVE MAP. `MapView` picks hexes out of `_unhandled_input`, and a `STOP` control under
	# the pointer — the `Control` default — makes the Viewport mark the press handled before it ever gets
	# there. Without `IGNORE` the ~1929px of open map either side of the card on a 3440 bottom dock can be
	# neither clicked to select a hex, nor right/middle-dragged to pan, nor wheel-zoomed. The two things
	# that must still eat their own clicks are `STOP` in their own right below — the card and the chrome
	# cluster — so this costs the islands nothing; an `IGNORE` parent does not stop its children being
	# picked. A vertical dock is unaffected either way: the card fills that strip edge to edge.
	_root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_root)

	_panel = PanelContainer.new()
	_panel.name = "PanelCard"
	_panel.add_theme_stylebox_override("panel", panel_card_stylebox())
	_panel.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	# The card DOES eat its own clicks — a press on the panel must never also select the hex behind it.
	# `STOP` is the `Control` default, set explicitly because with `_root` on `IGNORE` it is the surface
	# that carries the whole claim, exactly as `EventDockPanel` sets its own card's filter.
	_panel.mouse_filter = Control.MOUSE_FILTER_STOP
	_root.add_child(_panel)

	# The card's row. It has exactly ONE child — the content column — since issue #377 moved the chrome
	# rail out to be a sibling of the card rather than this row's trailing cell. The HBox is kept because
	# the column's fill/expand flags and the card's inner padding are stated through it, and because a
	# second cell inside the card is a thing this row can grow again without a structural change.
	_card_row = HBoxContainer.new()
	_card_row.name = "CardRow"
	# `ZONE_SEPARATION`, the same gutter the wide shell puts either side of a zone separator. With one
	# child it costs nothing today; it is here so a second cell would be spaced like every other region.
	_card_row.add_theme_constant_override("separation", ZONE_SEPARATION)
	_card_row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_card_row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_panel.add_child(_card_row)

	var column := VBoxContainer.new()
	column.name = "PanelColumn"
	column.add_theme_constant_override("separation", COLUMN_SEPARATION)
	# The column simply FILLS its card — the card is the thing that narrows now (`_card_width`), and the
	# centring happens one level up on the whole card (`_position_card_and_rail`). Set once, at
	# construction: no layout pass changes it.
	column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	column.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_card_row.add_child(column)
	_panel_column = column

	_build_rail()

	_header_full = _build_header_full()
	column.add_child(_header_full)

	_header_rail = _build_header_rail()
	column.add_child(_header_rail)

	# The body host holds both alternative shells + the empty-state; only one shell is visible at a
	# time. Collapse hides the whole host.
	_body_host = VBoxContainer.new()
	_body_host.name = "BandBodyHost"
	_body_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body_host.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(_body_host)

	# Empty state (shown only when no band is resolved — the panel otherwise hides outright when
	# there are zero player bands). First body child so it occupies the body when no band is present.
	_empty_state = Label.new()
	_empty_state.text = BODY_EMPTY_TEXT
	_empty_state.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_empty_state.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	_empty_state.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body_host.add_child(_empty_state)

	# WIDE shell: the three zones side by side, band + parties fixed-width, work taking the rest,
	# hairline separators between. No tab bar — every zone is visible at once.
	_wide_shell = HBoxContainer.new()
	_wide_shell.name = "WideShell"
	_wide_shell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_wide_shell.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_wide_shell.add_theme_constant_override("separation", ZONE_SEPARATION)
	_wide_shell.visible = false
	_body_host.add_child(_wide_shell)
	_wide_zone_hosts[ZONE_BAND] = _add_wide_zone_host(ZONE_BAND, ZONE_BAND_WIDTH)
	_wide_shell.add_child(_make_zone_separator())
	_wide_zone_hosts[ZONE_WORK] = _add_wide_zone_host(ZONE_WORK, 0.0)
	_wide_shell.add_child(_make_zone_separator())
	_wide_zone_hosts[ZONE_PARTIES] = _add_wide_zone_host(ZONE_PARTIES, ZONE_PARTY_WIDTH)

	# NARROW shell: a tab bar directly under the header + exactly one zone filling the rest.
	_narrow_shell = VBoxContainer.new()
	_narrow_shell.name = "NarrowShell"
	_narrow_shell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_narrow_shell.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_narrow_shell.add_theme_constant_override("separation", BODY_SEPARATION)
	_narrow_shell.visible = false
	_body_host.add_child(_narrow_shell)
	_tab_bar = HBoxContainer.new()
	_tab_bar.name = "ZoneTabs"
	_tab_bar.add_theme_constant_override("separation", TAB_SEPARATION)
	_narrow_shell.add_child(_tab_bar)
	_narrow_zone_host = _make_zone_host("NarrowZoneHost", 0.0)
	_narrow_shell.add_child(_narrow_zone_host)
	_rebuild_tab_bar()

	# The accent seam sits on the map-facing edge, above the card fill.
	_seam = ColorRect.new()
	_seam.name = "AccentSeam"
	_seam.color = HudStyle.SIGNAL_DEEP
	_seam.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_root.add_child(_seam)

func _build_header_full() -> HBoxContainer:
	var header := HBoxContainer.new()
	header.name = "HeaderFull"
	header.add_theme_constant_override("separation", HEADER_SEPARATION)

	# The subject cluster (stage glyph + name + stage label) is a clickable "jump to my band"
	# affordance: a PanelContainer (STOP + hand cursor + subtle hover tint) wrapping a
	# mouse-transparent HBox so a click anywhere on it reaches `_on_subject_gui_input`. It expands to
	# fill (pushing the cycler/dock-chooser right, as the plain subject VBox used to).
	_subject_cluster = PanelContainer.new()
	_subject_cluster.name = "SubjectCluster"
	_subject_cluster.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_subject_cluster.mouse_filter = Control.MOUSE_FILTER_STOP
	_subject_cluster.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	_subject_cluster.tooltip_text = "Jump to this band on the map"
	_subject_cluster.add_theme_stylebox_override("panel", _subject_stylebox(false))
	_subject_cluster.gui_input.connect(_on_subject_gui_input)
	_subject_cluster.mouse_entered.connect(func(): _set_subject_hover(true))
	_subject_cluster.mouse_exited.connect(func(): _set_subject_hover(false))

	var cluster_row := HBoxContainer.new()
	cluster_row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	cluster_row.add_theme_constant_override("separation", HEADER_SEPARATION)
	_subject_cluster.add_child(cluster_row)

	_stage_glyph_label = Label.new()
	_stage_glyph_label.add_theme_font_size_override("font_size", STAGE_GLYPH_FONT_SIZE)
	_stage_glyph_label.text = DEFAULT_STAGE_GLYPH
	_stage_glyph_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	_stage_glyph_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	cluster_row.add_child(_stage_glyph_label)

	_stage_glyph_sprite = _make_stage_glyph_sprite()
	cluster_row.add_child(_stage_glyph_sprite)

	var subject := VBoxContainer.new()
	subject.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	subject.add_theme_constant_override("separation", 0)
	subject.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_name_label = Label.new()
	_name_label.add_theme_font_size_override("font_size", NAME_FONT_SIZE)
	_name_label.add_theme_color_override("font_color", HudStyle.INK)
	_name_label.text = ""
	_name_label.clip_text = true
	_name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_stage_label = Label.new()
	_stage_label.add_theme_font_size_override("font_size", STAGE_LABEL_FONT_SIZE)
	_stage_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	_stage_label.text = ""
	_stage_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	# The stage word and the band's coordinates share the header's second line: both are secondary
	# IDENTITY, so they wear the same quiet ink and size, and the coordinates sit AFTER the stage
	# (a band is "Camp" first and "at (68, 30)" second).
	var stage_row := HBoxContainer.new()
	stage_row.add_theme_constant_override("separation", STAGE_ROW_SEPARATION)
	stage_row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_position_label = Label.new()
	_position_label.add_theme_font_size_override("font_size", STAGE_LABEL_FONT_SIZE)
	_position_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	_position_label.text = ""
	_position_label.visible = false
	_position_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	stage_row.add_child(_stage_label)
	stage_row.add_child(_position_label)
	subject.add_child(_name_label)
	subject.add_child(stage_row)
	cluster_row.add_child(subject)

	header.add_child(_subject_cluster)

	header.add_child(_build_cycler())

	var dock_chooser := _build_dock_chooser()
	header.add_child(dock_chooser)

	_collapse_button = _make_icon_button(COLLAPSE_GLYPH, "Collapse")
	_collapse_button.pressed.connect(_on_collapse_pressed)
	header.add_child(_collapse_button)

	return header

## Subject-cluster background: transparent normally, a subtle SIGNAL_WASH tint on hover. Same
## content margins in both states so hovering never shifts the header.
func _subject_stylebox(hover: bool) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudStyle.SIGNAL_WASH if hover else Color(0.0, 0.0, 0.0, 0.0)
	sb.set_corner_radius_all(SUBJECT_HOVER_CORNER_RADIUS)
	sb.content_margin_left = SUBJECT_HOVER_PADDING_H
	sb.content_margin_right = SUBJECT_HOVER_PADDING_H
	sb.content_margin_top = SUBJECT_HOVER_PADDING_V
	sb.content_margin_bottom = SUBJECT_HOVER_PADDING_V
	return sb

func _set_subject_hover(hover: bool) -> void:
	if _subject_cluster != null:
		_subject_cluster.add_theme_stylebox_override("panel", _subject_stylebox(hover))

## Left-click anywhere on the subject cluster → "jump to my band".
func _on_subject_gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
		subject_activated.emit()

func _build_cycler() -> HBoxContainer:
	var cycler := HBoxContainer.new()
	cycler.name = "Cycler"
	cycler.add_theme_constant_override("separation", 4)

	var prev := _make_icon_button(CYCLE_PREV_GLYPH, "Previous settlement")
	prev.pressed.connect(func(): _on_cycle_pressed(CYCLE_PREV))
	cycler.add_child(prev)

	_count_label = Label.new()
	_count_label.add_theme_font_size_override("font_size", COUNT_FONT_SIZE)
	_count_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	_count_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_count_label.custom_minimum_size = Vector2(COUNT_MIN_WIDTH, 0.0)
	_count_label.text = "–"
	cycler.add_child(_count_label)

	var nxt := _make_icon_button(CYCLE_NEXT_GLYPH, "Next settlement")
	nxt.pressed.connect(func(): _on_cycle_pressed(CYCLE_NEXT))
	cycler.add_child(nxt)

	return cycler

func _build_dock_chooser() -> GridContainer:
	var grid := GridContainer.new()
	grid.name = "DockChooser"
	grid.columns = 2
	grid.add_theme_constant_override("h_separation", DOCK_CELL_SEPARATION)
	grid.add_theme_constant_override("v_separation", DOCK_CELL_SEPARATION)
	for edge in DOCK_EDGES:
		var cell := Button.new()
		cell.custom_minimum_size = Vector2(DOCK_CELL_SIZE, DOCK_CELL_SIZE)
		cell.focus_mode = Control.FOCUS_NONE
		cell.tooltip_text = "Dock %s" % _edge_name(edge)
		cell.pressed.connect(func(): set_dock(edge))
		_dock_cells[edge] = cell
		grid.add_child(cell)
	return grid

func _build_header_rail() -> VBoxContainer:
	var rail := VBoxContainer.new()
	rail.name = "HeaderRail"
	rail.alignment = BoxContainer.ALIGNMENT_CENTER
	rail.add_theme_constant_override("separation", HEADER_SEPARATION)

	_rail_glyph_label = Label.new()
	_rail_glyph_label.add_theme_font_size_override("font_size", STAGE_GLYPH_FONT_SIZE)
	_rail_glyph_label.text = DEFAULT_STAGE_GLYPH
	_rail_glyph_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	rail.add_child(_rail_glyph_label)

	_rail_glyph_sprite = _make_stage_glyph_sprite()
	rail.add_child(_rail_glyph_sprite)

	_rail_expand_button = _make_icon_button(EXPAND_GLYPH, "Expand")
	_rail_expand_button.pressed.connect(_on_collapse_pressed)
	rail.add_child(_rail_expand_button)

	rail.visible = false
	return rail

# ---- layout ----------------------------------------------------------------

func _apply_dock_layout() -> void:
	_apply_root_anchors()
	# BEFORE `_relayout_body`: a dock change can retire the rail (a vertical strip has none), and the
	# shell is chosen from the width the rail leaves.
	_apply_rail()
	_relayout_body()

## Re-anchor `_root` to the active edge at the current cross-axis size, and pin the seam. Split out of
## `_apply_dock_layout` so a wide-dock fit-to-content height recompute can resize the card WITHOUT
## re-arranging the body (which would recurse back into the packer).
func _apply_root_anchors() -> void:
	if _root == null:
		return
	var cross := _cross_axis_size()
	# `_edge_offset` shifts the panel INBOARD from the docked edge, so a co-edge reserver
	# (e.g. the Inspector, which is always the inboard screen-edge reserver) sits between the
	# screen edge and this panel — the two stack instead of overlapping. The near offset is
	# `_edge_offset`, the far offset `_edge_offset + cross`.
	var near := _edge_offset
	var far := _edge_offset + cross
	# Re-anchor _root to the active edge, fixed on the cross axis, filling the rest.
	match _dock_edge:
		SIDE_LEFT:
			_set_root_anchors(0.0, 0.0, 0.0, 1.0)
			_set_root_offsets(near, 0.0, far, 0.0)
		SIDE_RIGHT:
			_set_root_anchors(1.0, 0.0, 1.0, 1.0)
			_set_root_offsets(-far, 0.0, -near, 0.0)
		SIDE_TOP:
			_set_root_anchors(0.0, 0.0, 1.0, 0.0)
			_set_root_offsets(0.0, near, 0.0, far)
		SIDE_BOTTOM:
			_set_root_anchors(0.0, 1.0, 1.0, 1.0)
			_set_root_offsets(0.0, -far, 0.0, -near)
	_position_card_and_rail()
	_position_seam()

## Place the CARD and the CHROME CLUSTER inside `_root`'s strip (issue #377).
##
## **A horizontal dock is TWO FLOATING ISLANDS, not one bar.** The card used to be `PRESET_FULL_RECT`
## with the chrome as its last cell, deliberately, so a bottom dock read as one continuous bar — and
## that is exactly what made it span two feet of an ultrawide with the work zone stretched across the
## middle. The card is now sized to its CONTENT (`_card_width`) and centred in the room the chrome
## leaves; the chrome is pinned to the strip's trailing edge with a bare gutter between them. The
## reference is the tile bar at the top of the screen: a card as wide as what it has to say, over live
## map, with the readouts as their own cluster beside it.
##
## **A VERTICAL dock is untouched** — the card fills its strip exactly as before and there is no rail.
func _position_card_and_rail() -> void:
	if _root == null or _panel == null:
		return
	if _is_vertical_edge(_dock_edge):
		_panel.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		return
	# The chrome cluster: flush to the strip's TRAILING edge, full strip height. Anchored by hand rather
	# than laid out, because it is no longer any container's child.
	var rail_width := _rail_width()
	if _rail != null:
		_rail.anchor_left = 1.0
		_rail.anchor_right = 1.0
		_rail.anchor_top = 0.0
		_rail.anchor_bottom = 1.0
		_rail.offset_left = -rail_width
		_rail.offset_right = 0.0
		_rail.offset_top = 0.0
		_rail.offset_bottom = 0.0
	# The card: its content width, centred in what the chrome leaves. Clamped to the available room so a
	# window narrower than the content can never slide the card under the chrome.
	# Centred in the room the chrome cluster and the HUD columns leave — and OFFSET past the leading
	# bound, so "centred" means centred in the gap rather than centred on the screen with a column
	# underneath one end of it.
	var available: float = _available_card_span()
	var card_width: float = minf(_card_width(), available)
	var lead: float = _bound_leading + 0.5 * maxf(available - card_width, 0.0)
	_panel.anchor_left = 0.0
	_panel.anchor_right = 0.0
	_panel.anchor_top = 0.0
	_panel.anchor_bottom = 1.0
	_panel.offset_left = lead
	_panel.offset_right = lead + card_width
	_panel.offset_top = 0.0
	_panel.offset_bottom = 0.0

## Choose the shell for the panel's current WIDTH and home the zones into it. Called on every
## dock-layout pass; cheap and idempotent.
func _relayout_body() -> void:
	if _wide_shell == null or _narrow_shell == null:
		return
	var was_wide := _body_is_wide
	_body_is_wide = _shell_is_wide()
	if _body_is_wide != was_wide:
		_rebuild_tab_bar()
	# The band flank is the ONE wide-shell host whose width is not a constant — it is
	# `band_zone_columns()` of `ZONE_BAND_WIDTH` — so the host's pinned minimum is re-declared on every
	# layout pass rather than only at `_build`. The parties host beside it is genuinely fixed and is
	# left where `_build` put it.
	var band_host: Control = _wide_zone_hosts.get(ZONE_BAND)
	if band_host != null:
		band_host.custom_minimum_size.x = _band_flank_width()
	_reparent_zones()
	_update_body_visibility()

## What the two separators + their gaps cost. The value lives in `WIDE_SEPARATOR_SPAN` (a `const`, so
## `WIDE_SHELL_MIN_WIDTH` can use it too); this is the call-site form, shared by `_card_width` and
## `work_zone_size`, so the three can never disagree about how much width the chrome eats.
func _wide_separator_span() -> float:
	return WIDE_SEPARATOR_SPAN

## Declare the room the card must leave at each end of a horizontal strip — the HUD columns it must not
## be drawn over. `Main` owns the widths; the panel owns what to do with them.
##
## **They are the columns' LIVE widths, not the HUD's authored minimums** (`HudLayer.lateral_column_widths`),
## which is the opposite of the bound the event dock takes and deliberately so. That one fixes an EDGE
## that must not jitter from turn to turn, and a column drawing wider than its minimum merely overlaps it
## a little. This one decides whether a CARD is drawn THROUGH the readouts, where being a little wrong is
## not cosmetic: measured at 1920 they render 419px against a 344px authored minimum, because the metrics
## line is simply longer than the minimum allows for. So the bound moves when the columns do — `Main`
## re-pushes it per snapshot, and this early-outs on an unchanged pair.
##
## **Without this the top-dock HUD exemption is only correct for a SPARSE band.** A band with no worked
## sources makes a narrow card with room either side, which is what made the fix look complete; a band
## with 34 sources makes a 1570px card in a 1920px strip and puts it straight through the readouts.
func set_lateral_bounds(leading: float, trailing: float) -> void:
	var lead: float = maxf(leading, 0.0)
	var trail: float = maxf(trailing, 0.0)
	if is_equal_approx(lead, _bound_leading) and is_equal_approx(trail, _bound_trailing):
		return
	_bound_leading = lead
	_bound_trailing = trail
	_apply_dock_layout()
	_republish_reservation_if_changed()
	_notify_zones_resized()

## The span of strip a horizontal card may actually use: the whole row less the chrome cluster and less
## whatever HUD column sits at either end. The ONE definition, so `_card_width`, `_interior_size` and
## `_position_card_and_rail` cannot disagree about how much room there is.
func _available_card_span() -> float:
	return maxf(_panel_width_extent() - _rail_span() - _bound_leading - _bound_trailing, 0.0)

## How wide the CARD draws. A vertical dock is the fixed strip; a horizontal one is exactly what its
## content needs, which is what stops a bottom dock spanning an ultrawide.
##
## **The wide shell's width is DECLARED, not measured** — the flanks are fixed, and the work board's
## column count arrives through `set_work_columns` from the controller that knows how many sources
## there are. That direction is the `set_rail_width` contract again, and it is what keeps this
## acyclic: the SHELL is still chosen from the room the strip has (`_shell_is_wide`), never from the
## card, so nothing here can feed back into the choice that produced it.
##
## The narrow shell takes the whole available strip: it is reached only when there is too little room
## for three zones, i.e. exactly when there is nothing to give back.
func _card_width() -> float:
	if _is_vertical_edge(_dock_edge):
		return PANEL_WIDTH
	var available: float = _available_card_span()
	if not _shell_is_wide():
		return available
	var work: float = float(clampi(_work_columns, 1, WORK_MAX_COLUMNS)) * ZONE_WORK_MIN_WIDTH
	return _band_flank_width() + ZONE_PARTY_WIDTH + work + _wide_separator_span() + PANEL_CHROME_H

## Declare how many columns the WORK board wants — the count `BandPanelController` derives from its
## source list and the zone's HEIGHT. The panel then draws a card exactly that wide.
##
## **DECLARED, never measured here**, the `set_rail_width` contract: the controller owns the sources, so
## the controller counts them. It is also what keeps the handshake ACYCLIC — the column count follows
## from the zone's height and the source count, neither of which depends on the width this sets. A
## repeat declaration early-outs, so the `zones_resized` → repage → declare loop settles in one pass.
##
## It can NEVER re-emit `reservation_changed`: the reservation is the CROSS axis, and this spends the
## long one.
##
## **AND IT MUST NOT EMIT `zones_resized` EITHER**, which is the one non-obvious part. The caller is the
## controller in the middle of BUILDING the board, and `zones_resized` is what makes it re-page — so
## emitting here would re-enter `_fill_work_zone` from inside itself. It is also unnecessary: the
## controller already holds the column count (it is the call's return value) and is about to fill the
## resized host with exactly that many columns. So the cached size is refreshed SILENTLY, which both
## breaks the loop and stops the next genuine resize firing a spurious re-page against a stale value.
## **It RETURNS the count it actually applied, and the caller must build to that** — a want is not a
## grant. The board can ask for four columns on a band with 34 sources while the strip can only pay for
## one (a narrow side dock, or the wide shell at its own minimum width), and a board built to the want
## rather than the grant overflows its zone by ~190px in a 380px dock and ~725px at the shell
## threshold — silently, since the zone hosts CLIP.
func set_work_columns(columns: int) -> int:
	var want := clampi(columns, 1, WORK_MAX_COLUMNS)
	want = maxi(mini(want, _affordable_work_columns()), 1)
	if want != _work_columns:
		_work_columns = want
		_apply_dock_layout()
		_last_work_zone_size = work_zone_size()
	return _work_columns

## The most board columns the STRIP can pay for, whatever the source count wants. The card grows to fit
## its content, but only up to the room it actually has — past that the board has to page instead.
##
## The narrow shell hands its ONE zone the whole interior, so the question there is simply how many
## readable columns that interior holds. The wide shell has to pay for both flanks and the separators
## out of the same strip first, and the chrome cluster before any of it.
func _affordable_work_columns() -> int:
	if not _shell_is_wide():
		return maxi(int(_interior_size().x / ZONE_WORK_MIN_WIDTH), 1)
	# `_available_card_span()`, not the raw strip: the HUD columns a top dock must keep clear of come off
	# the card's room BEFORE the board gets any of it, so counting columns against the whole row builds a
	# board the clamped card cannot hold — measured as 135px of it hanging out of a clipping zone host.
	var room: float = _available_card_span() - PANEL_CHROME_H - _band_flank_width() - ZONE_PARTY_WIDTH \
		- _wide_separator_span()
	return maxi(int(room / ZONE_WORK_MIN_WIDTH), 1)

## True when the panel is wide enough for the three zones side by side. A WIDTH test, never a
## dock-edge test — see `WIDE_SHELL_MIN_WIDTH`.
## The panel must NEVER enter the wide shell with a work zone below `ZONE_WORK_MIN_WIDTH` — the exact
## failure the hand-picked 900 threshold caused. `WIDE_SHELL_MIN_WIDTH` is zones + separators +
## `PANEL_CHROME_H` and is tested against the OUTER width, so the chrome rail (which spends that same
## outer width before the zones see any of it — its column AND its separator gutter) must come off first.
func _shell_is_wide() -> bool:
	if _is_vertical_edge(_dock_edge):
		return _panel_width_extent() - _rail_span() >= WIDE_SHELL_MIN_WIDTH
	# `_available_card_span()` on a horizontal dock, because the HUD columns a top dock keeps clear of
	# come off the CARD's room before any zone sees it (issue #377). Testing the raw strip put the panel
	# into the wide shell on a 1920 top dock whose card could only have 1141 — a 331px work zone against
	# a 380px minimum, i.e. exactly the invariant `WIDE_SHELL_MIN_WIDTH` was derived to protect.
	return _available_card_span() >= WIDE_SHELL_MIN_WIDTH

## The RESERVED STRIP for the current dock: fixed on the cross axis, the window on the other. It was the
## card's outer size until issue #377, when the card stopped filling a horizontal strip; it is the region
## `_root` spans, and what `_available_card_span()` subtracts the chrome cluster and the HUD columns from
## to get the room the card may actually use. On a VERTICAL dock the card still fills it, so the two
## readings coincide there.
func _panel_extent() -> Vector2:
	var window := _viewport_size()
	if _is_vertical_edge(_dock_edge):
		return Vector2(PANEL_WIDTH, window.y)
	return Vector2(window.x, _horizontal_panel_height())

## The strip's extent along the axis the CARD's WIDTH is measured on — `_panel_extent().x`, split out
## as its own reader.
##
## **The split is what keeps the layout ACYCLIC**, not tidiness: the cross axis now depends on which
## SHELL is active (`_shell_chrome_height`), and the shell is chosen from this width — so a shell test
## that built the whole extent would call the height, which calls the shell test, which builds the
## extent. Every width reader takes this; only `_panel_extent()` itself pairs it with the height.
func _panel_width_extent() -> float:
	return PANEL_WIDTH if _is_vertical_edge(_dock_edge) else _viewport_size().x

## The T/B cross-axis size: the fixed body budget `PANEL_HEIGHT_WIDE` PLUS whatever the ACTIVE SHELL
## spends on its own chrome, clamped to a fraction of the window so a short window can never let the
## strip eat the screen.
##
## **THE SHELL TERM IS THE FIX FOR A ZONE SLICED BY THE STRIP'S OWN TAB BAR.** `PANEL_HEIGHT_WIDE` is
## the budget the zones' content is tuned against — the band zone's SHORT tier reads 299px of the
## 300px box a wide horizontal dock offers — and it was spent as a FLAT strip height, so the narrow
## shell paid for its tab bar out of the zone: measured, a 265px box for content that needs 273
## (`band_panel_scale_bottom`), which the `clip_contents` zone host then sliced with no scrollbar and
## no affordance. On a BOTTOM dock that cut lands at the window's own edge, which is what makes it
## read as the panel "running off the bottom of the screen" rather than as a clipped zone.
##
## It is deliberately NOT a scale question — the narrow shell is reached at `ui_scale` 1.0 in a window
## under ~1511px too, and paid the same 35px there. The two shells now hand their zones the SAME box,
## which is the only arrangement under which one set of tier thresholds can be right for both.
func _horizontal_panel_height() -> float:
	return minf(PANEL_HEIGHT_WIDE + _shell_chrome_height(),
		_viewport_size().y * MAX_WIDE_HEIGHT_FRACTION)

## How many `ZONE_BAND_WIDTH` columns the BAND flank lays its blocks out across.
##
## **PURELY GEOMETRIC — what the span AFFORDS, never what the content holds.** That is the whole
## safety argument. Every term below is a function of the viewport, the dock edge, the rail's declared
## span and the lateral bounds; not one of them is content, so the strip's height (which keys off this)
## stays content-independent and the reservation cannot move when the player edits the band. A count
## that grew with the roster would put `MapView`'s inset on the snapshot's critical path — the flicker
## bug the fixed cross-axis size exists to prevent.
##
## The room measured is what is left AFTER the other two zones are paid: the wide shell exists to give
## the work board a readable column, so a second band column is only affordable once `ZONE_PARTY_WIDTH`
## and one `ZONE_WORK_MIN_WIDTH` are already covered. `_affordable_work_columns()`'s idiom, one flank
## over.
##
## ONE on a vertical dock and one in the narrow shell, both by construction: those layouts hand the
## band zone a single strip-width column, which is what `PANEL_WIDTH` means.
func band_zone_columns() -> int:
	if _is_vertical_edge(_dock_edge) or not _shell_is_wide():
		return 1
	var room: float = _available_card_span() - PANEL_CHROME_H - ZONE_PARTY_WIDTH \
		- ZONE_WORK_MIN_WIDTH - _wide_separator_span()
	return clampi(int(room / ZONE_BAND_WIDTH), 1, BAND_ZONE_MAX_COLUMNS)

## What the BAND flank actually spends of the row — its column count at one readable column each. The
## ONE definition, so `_card_width`, `work_zone_size` and `_affordable_work_columns` cannot disagree
## about how much of the row the flank has taken, exactly as `_wide_separator_span()` is the one
## definition of what the separators cost.
func _band_flank_width() -> float:
	return float(band_zone_columns()) * ZONE_BAND_WIDTH

## The box the BAND zone's content may fill. Its HEIGHT is the body's, like every wide-shell zone; its
## WIDTH is `band_zone_columns()` of `ZONE_BAND_WIDTH`. `BandPanelController` authors its split across
## exactly that many columns, so the two cannot disagree about how many there are.
func band_zone_size() -> Vector2:
	var box := work_zone_size()
	if box == Vector2.ZERO:
		return box
	if not _shell_is_wide():
		return box
	return Vector2(float(band_zone_columns()) * ZONE_BAND_WIDTH, box.y)

## What the ACTIVE shell spends on the strip's CROSS axis before any zone sees the box. The wide shell
## spends none — its separators are vertical hairlines, paid out of the width — while the narrow shell
## carries the tab bar and the gap beneath it. Read through `_shell_is_wide()` rather than the cached
## `_body_is_wide`, so a dock layout that arrives before `_relayout_body` has re-chosen the shell still
## sizes the strip for the shell it is about to get.
func _shell_chrome_height() -> float:
	return 0.0 if _shell_is_wide() else _tab_bar_height()

## The card's INTERIOR box — what the card DRAWS AT, less the border and the content margins it draws
## with (`panel_card_stylebox`). Chrome only; never content.
## `work_zone_size()` and `_affordable_work_columns()` read this, so both follow the card's width with no
## edit of their own.
func _interior_size() -> Vector2:
	var chrome_v := 2.0 * (PANEL_CONTENT_MARGIN_V + PANEL_BORDER_WIDTH)
	# The WIDTH comes off the CARD, not the strip (issue #377): the card no longer spans a horizontal
	# dock, so the strip's width stopped being what the zones have to spend. `_card_width()` has already
	# subtracted the chrome cluster and its gutter, which is why `_rail_span()` does not appear again
	# here. The HEIGHT is still the strip's — the card is full-height on both axes of a horizontal dock.
	var card_width: float = minf(_card_width(), _available_card_span())
	return Vector2(maxf(card_width - PANEL_CHROME_H, 0.0), maxf(_panel_extent().y - chrome_v, 0.0))

## Height of the header row — pure chrome (two text rows beside the icon controls), so measuring it
## keeps the interior maths content-independent. Falls back before the first layout pass.
func _header_height() -> float:
	if _header_full == null:
		return HEADER_HEIGHT_FALLBACK
	var measured := _header_full.get_combined_minimum_size().y
	return measured if measured > 0.0 else HEADER_HEIGHT_FALLBACK

## Height the narrow shell's tab bar takes off the body (plus the gap under it).
func _tab_bar_height() -> float:
	if _tab_bar == null:
		return 0.0
	var measured := _tab_bar.get_combined_minimum_size().y
	return measured + float(BODY_SEPARATION)

## The tab the narrow shell actually shows: the selected one when it has content, else the first zone
## that does. A selected tab whose zone was handed in as null must not black the panel out — and this
## is what keeps the Part-1 shim (which fills only the BAND zone) previewable under the `work` default.
func _effective_tab() -> StringName:
	if _zones.get(_active_tab) is Control:
		return _active_tab
	for zone in TAB_ORDER:
		if _zones.get(zone) is Control:
			return zone
	return _active_tab

## Home each owned zone Control into the active shell's host: all three side by side in the wide
## shell, only the selected tab's zone in the narrow one (the other two are detached but still owned,
## so a tab switch is a reparent rather than a Hud re-render).
func _reparent_zones() -> void:
	for zone in TAB_ORDER:
		var zone_variant: Variant = _zones.get(zone)
		if not (zone_variant is Control):
			continue
		var control: Control = zone_variant
		var host: Control = null
		if _body_is_wide:
			host = _wide_zone_hosts.get(zone)
		elif zone == _effective_tab():
			host = _narrow_zone_host
		if host == null:
			_detach(control)
			continue
		if control.get_parent() != host:
			_detach(control)
			host.add_child(control)
		# The host is a plain Control, so the zone content anchors itself to fill it.
		control.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

## Show the active shell when a band is present, else neither (the empty-state placeholder shows
## instead). Collapse is handled separately by `_refresh_collapse_state` hiding the whole `_body_host`.
func _update_body_visibility() -> void:
	if _wide_shell != null:
		_wide_shell.visible = _band_present and _body_is_wide
	if _narrow_shell != null:
		_narrow_shell.visible = _band_present and not _body_is_wide

# ---- wide shell scaffolding ------------------------------------------------

## One wide-shell zone column. `fixed_width > 0` pins the column (band / parties); 0 makes it the
## expanding one (work).
func _add_wide_zone_host(zone: StringName, fixed_width: float) -> Control:
	var host := _make_zone_host("Zone_%s" % String(zone), fixed_width)
	_wide_shell.add_child(host)
	return host

## A zone host. Deliberately a PLAIN `Control`, not a container: a container reports its children's
## combined minimum size, so an over-wide zone content would push the whole card past its FIXED
## cross-axis size (a 380 L/R strip rendering 456px wide, spilling over the map) — the very
## content-dependence this rework removes. A plain Control reports no minimum, so the zone stays the
## size the shell gave it, and `clip_contents` keeps anything that does not fit inside its own zone
## instead of painting over its neighbour. Zone content is anchored full-rect into it by
## `_reparent_zones`.
func _make_zone_host(host_name: String, fixed_width: float) -> Control:
	var host := Control.new()
	host.name = host_name
	host.clip_contents = true
	host.size_flags_vertical = Control.SIZE_EXPAND_FILL
	if fixed_width > 0.0:
		host.custom_minimum_size = Vector2(fixed_width, 0.0)
		host.size_flags_horizontal = Control.SIZE_FILL
	else:
		host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return host

# ---- the trailing chrome rail (the HUD's bottom-bar chrome shares the row) ----

## Build the row's trailing rail: a plain clipping `Control` holding a centred vertical stack of the two
## slot hosts. THREE deliberate choices, each load-bearing:
## 1. `_rail` is a PLAIN `Control`, not a container, for the same reason `_make_zone_host` is one — a
##    container reports its children's combined minimum size, so the HUD's chrome could push the card
##    past its FIXED cross-axis size. Its width is DECLARED by the HUD (`set_rail_width`), never
##    measured from the content, which is what keeps `work_zone_size()` content-independent.
## 2. Because that wrapper blocks propagation, everything INSIDE it can be an ordinary container — which
##    is why the slots are `MarginContainer`s rather than plain Controls. A slot must report the height
##    of the cluster parked in it or the stack collapses to nothing, and a container reading its own
##    child's minimum is exactly the mechanism for that. Nobody has to measure the chrome twice.
## 3. The stack CENTRES via `ALIGNMENT_CENTER`, not by anchor arithmetic. Anchors were tried and are a
##    trap here: `set_anchors_and_offsets_preset` derives its offsets from `get_minimum_size()` — the
##    *virtual* one, which ignores `custom_minimum_size` — so it centres a `Container` and does NOT
##    centre a plain `Control`. Measured on the bottom dock: `PRESET_HCENTER_WIDE` gave `NavBacking` (a
##    `PanelContainer`) offsets `[0, -76, 0, +76]` = ±152/2, correctly centred, and gave `TurnCluster` (a
##    plain `Control` whose 128px height is only `custom_minimum_size`) offsets `[0, 0, 0, 0]` — its TOP
##    edge pinned to the mid-line, then grown DOWNWARD by `_size_changed`'s minimum clamp, rendering 64px
##    low (rect y 900–1028 in a host spanning 730–1070). A container-driven stack has no such asymmetry.
func _build_rail() -> void:
	_rail = Control.new()
	_rail.name = "ChromeRail"
	_rail.clip_contents = true
	_rail.visible = false
	# The chrome island eats its own clicks, for the same reason the card does: `_root` is `IGNORE` now,
	# so a press on the rail's bare column would otherwise fall straight through to the hex behind it.
	# `STOP` is the `Control` default; explicit because here it is load-bearing rather than incidental.
	_rail.mouse_filter = Control.MOUSE_FILTER_STOP
	# **A SIBLING OF THE CARD, NOT A CHILD OF IT** (issue #377). The rail used to be the last cell of
	# `_card_row`, i.e. chrome sitting ON the card — which is what made a horizontal dock read as ONE
	# continuous bar spanning the monitor. The card is an ISLAND now (`_position_card_and_rail`), so the
	# chrome is its own island beside it: parented to `_root`, right-anchored, and positioned by hand.
	# It keeps `NavBacking`'s own backing panel, so it reads as chrome floating over the map exactly as
	# the top-bar readouts do.
	_root.add_child(_rail)

	_rail_stack = VBoxContainer.new()
	_rail_stack.name = "ChromeRailStack"
	_rail_stack.alignment = BoxContainer.ALIGNMENT_CENTER
	_rail_stack.add_theme_constant_override("separation", RAIL_SLOT_SEPARATION)
	_rail_stack.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_rail.add_child(_rail_stack)

	for slot in RAIL_SLOT_ORDER:
		var host := MarginContainer.new()
		host.name = "ChromeRailSlot_%d" % slot
		# SHRINK_CENTER so a cluster narrower than the rail (the rail is the WIDER of the two) sits
		# centred in the column rather than ragged against its leading edge.
		host.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
		host.size_flags_vertical = Control.SIZE_FILL
		_rail_slots[slot] = host
		_rail_stack.add_child(host)

## The slot the HUD parks one bottom-bar chrome cluster into on a horizontal dock: `RAIL_SLOT_TOP` (the
## nav cluster) above `RAIL_SLOT_BOTTOM` (the turn cluster). The panel owns this host and NOTHING inside
## it: the HUD adds and removes its own nodes. Always non-null for a valid slot (the hosts exist from
## `_build`); the rail as a whole is hidden while it carries no declared width.
func rail_slot_host(slot: int) -> Control:
	return _rail_slots.get(slot)

## Declare the rail column's width — the `max` over the parked clusters, computed by the HUD, which owns
## them. NOT measured here. `width <= 0` retires the rail. Re-chooses the shell and re-reports
## `work_zone_size()`.
##
## **IT COULD ONCE NEVER RE-EMIT `reservation_changed`, AND THAT IS NO LONGER TRUE.** The claim rested
## on the reservation being a pure function of the collapse flag, the dock edge and the viewport — the
## rail spending only the LONG axis while the reservation is the CROSS one. Since the cross axis carries
## the active shell's own chrome (`_shell_chrome_height`), and the shell is chosen from the span the rail
## comes out of, a rail width can flip the shell and move the strip by the tab bar's height. So it
## republishes through `_republish_reservation_if_changed`, which read the old guarantee's PURPOSE rather
## than its letter: the loop it prevented (HUD pushes a width -> panel relayouts -> emit -> `Main` fan-out
## -> HUD reflow -> pushes a width) still terminates, because that second push lands on the same number
## and is dropped by the early-out above, and the republish itself is silent on an unchanged size.
func set_rail_width(width: float) -> void:
	var declared: float = maxf(width, 0.0)
	if is_equal_approx(declared, _rail_declared_width):
		return
	_rail_declared_width = declared
	# The FULL dock layout, not just the rail: since issue #377 the rail's own rect is written by
	# `_position_card_and_rail` (it is anchored, not laid out by a container), and the CARD's width and
	# centring are both computed against `_rail_span()`. Calling `_apply_rail` alone left the cluster
	# anchored at whatever width it had when the dock was last applied — measured as a 296px rail
	# hanging 180px off the end of a 1920px strip.
	_apply_dock_layout()
	_republish_reservation_if_changed()
	_notify_zones_resized()

## Size + show the rail for the current dock. The rail exists only on a HORIZONTAL dock: a vertical strip
## is `PANEL_WIDTH` (380) wide and has no room beside the zones for a ~300px chrome column.
## The rail's RECT is written by `_position_card_and_rail`, which runs on the same layout pass.
func _apply_rail() -> void:
	if _rail == null:
		return
	var width := _rail_width()
	_rail.custom_minimum_size.x = width
	_rail.visible = width > 0.0

## The rail's effective width: the declared value on a horizontal dock, 0 on a vertical one. Forcing 0 by
## EDGE rather than trusting the declared value keeps the panel correct whatever order the dock change
## and the HUD's push arrive in.
## Only the BOTTOM dock carries a rail: a vertical strip has no room beside its zones, and a TOP dock
## never displaces `BottomBar` in the first place, so its chrome stays home (`DockRowController.REFLOW_EDGES`).
## Forcing 0 by EDGE rather than trusting the declared value keeps the panel correct whatever order the
## dock change and the HUD's push arrive in.
func _rail_width() -> float:
	if _dock_edge != SIDE_BOTTOM:
		return 0.0
	return maxf(_rail_declared_width, 0.0)

## What the rail takes off the strip ALTOGETHER — its declared width PLUS the gutter beside it.
## The long-axis twin of `_wide_separator_span()`, and the value the width maths must use: subtracting
## `_rail_width()` alone would silently over-report the usable width by `RAIL_SEPARATOR_SPAN` (25px).
## **A retired rail contributes ZERO on both terms**, which is what keeps a vertical dock bit-identical
## to before the rail existed.
## Since issue #377 the gutter is BARE — the room between two floating islands, not a drawn hairline
## between two regions of one card. The `ChromeRailSeparator` `ColorRect` went with the merged bar; a
## rule down the gap between the card and the chrome would re-assert the very join that was removed.
func _rail_span() -> float:
	var width := _rail_width()
	if width <= 0.0:
		return 0.0
	return width + RAIL_SEPARATOR_SPAN

## The hairline rule between two adjacent zones in the wide shell.
func _make_zone_separator() -> ColorRect:
	var rule := ColorRect.new()
	rule.name = "ZoneSeparator"
	rule.color = HudStyle.LINE_SOFT
	rule.custom_minimum_size = Vector2(ZONE_SEPARATOR_THICKNESS, 0.0)
	rule.size_flags_horizontal = Control.SIZE_FILL
	rule.size_flags_vertical = Control.SIZE_EXPAND_FILL
	rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return rule

# ---- narrow shell tab bar --------------------------------------------------

## Rebuild the tab row (Band · Work · Parties) from the current selection + badges. Cheap enough to
## redo wholesale, and it keeps the active/inactive styling in exactly one place.
func _rebuild_tab_bar() -> void:
	if _tab_bar == null:
		return
	for child in _tab_bar.get_children():
		child.queue_free()
	_tab_buttons.clear()
	for zone in TAB_ORDER:
		var tab: Control = _make_tab_button(zone)
		_tab_bar.add_child(tab)
		_tab_buttons[zone] = tab

## One tab. A `PanelContainer` (not a `Button`) wrapping a mouse-transparent row, exactly as the
## header's subject cluster does — a Button is not a Container, so a label+badge row parented to one
## is never laid out and the tabs pile up at the origin.
func _make_tab_button(zone: StringName) -> Control:
	var active := zone == _effective_tab()
	var tab := PanelContainer.new()
	tab.name = "Tab_%s" % String(zone)
	tab.mouse_filter = Control.MOUSE_FILTER_STOP
	tab.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	tab.tooltip_text = TAB_LABELS[zone]
	tab.add_theme_stylebox_override("panel", _tab_stylebox(active))
	tab.gui_input.connect(func(event: InputEvent): _on_tab_gui_input(event, zone))

	# A mouse-transparent row inside the tab so the label + badge read (and click) as one tab.
	var row := HBoxContainer.new()
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_theme_constant_override("separation", TAB_SEPARATION)
	tab.add_child(row)

	var label := Label.new()
	label.text = TAB_LABELS[zone]
	label.add_theme_font_size_override("font_size", TAB_FONT_SIZE)
	label.add_theme_color_override("font_color", HudStyle.SIGNAL if active else HudStyle.INK_FAINT)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(label)

	var badge := _make_tab_badge(zone)
	if badge != null:
		row.add_child(badge)
	return tab

## Left-click anywhere on a tab selects it.
func _on_tab_gui_input(event: InputEvent, zone: StringName) -> void:
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.pressed:
		set_active_tab(zone)

## The tab's small rounded count pill — WARN-filled when the caller marked it hot, else a quiet
## LINE_SOFT chip. Returns null when this tab carries no badge.
func _make_tab_badge(zone: StringName) -> Control:
	var badge_variant: Variant = _tab_badges.get(zone)
	if not (badge_variant is Dictionary):
		return null
	var badge_data: Dictionary = badge_variant
	var text := String(badge_data.get("text", ""))
	if text.is_empty():
		return null
	var hot := bool(badge_data.get("hot", false))
	var pill := PanelContainer.new()
	pill.mouse_filter = Control.MOUSE_FILTER_IGNORE
	pill.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	pill.add_theme_stylebox_override("panel", _tab_badge_stylebox(hot))
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", TAB_BADGE_FONT_SIZE)
	label.add_theme_color_override("font_color", HudStyle.GROUND if hot else HudStyle.INK_DIM)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	pill.add_child(label)
	return pill

## Tab background: transparent either way (the tab is text, not a box); the ACTIVE one wears a SIGNAL
## underline. Identical content margins in both states so selection never shifts the row.
func _tab_stylebox(active: bool) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0.0, 0.0, 0.0, 0.0)
	sb.content_margin_left = TAB_PADDING_H
	sb.content_margin_right = TAB_PADDING_H
	sb.content_margin_top = TAB_PADDING_V
	sb.content_margin_bottom = TAB_PADDING_V
	if active:
		sb.border_width_bottom = TAB_UNDERLINE_THICKNESS
		sb.border_color = HudStyle.SIGNAL
	return sb

func _tab_badge_stylebox(hot: bool) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudStyle.WARN if hot else HudStyle.LINE_SOFT
	sb.set_corner_radius_all(TAB_BADGE_CORNER_RADIUS)
	sb.content_margin_left = TAB_BADGE_PADDING_H
	sb.content_margin_right = TAB_BADGE_PADDING_H
	sb.content_margin_top = TAB_BADGE_PADDING_V
	sb.content_margin_bottom = TAB_BADGE_PADDING_V
	return sb

## Select a narrow-shell tab. Persisted, so a reopened session lands where the player left it. The
## wide shell shows all three zones, so this only changes what the narrow shell will show.
func set_active_tab(zone: StringName) -> void:
	if not TAB_LABELS.has(zone) or zone == _active_tab:
		return
	_active_tab = zone
	_save_prefs()
	_rebuild_tab_bar()
	_reparent_zones()
	_notify_zones_resized()

## A window resize changes the T/B panel width (hence the shell) and the clamped wide height, so
## re-choose the shell, re-anchor and re-report both the reservation and the work-zone box.
func _on_viewport_resized() -> void:
	_apply_dock_layout()
	_emit_reservation()
	_notify_zones_resized()

## Re-report `work_zone_size()` when it actually moved, so Hud re-pages its work board once per real
## geometry change rather than on every layout pass.
## **THE BAND FLANK'S COLUMN COUNT IS ITS OWN TRIGGER, beside the work box.** The two nearly always
## move together — a wider flank leaves the board less — but not always: at the cap the flank stops
## growing while the board keeps taking the rest, so a span change can hold `work_zone_size()` fixed
## across a count change, and the band zone would then keep a split authored for the other layout.
func _notify_zones_resized() -> void:
	var size := work_zone_size()
	var columns := band_zone_columns()
	if size.is_equal_approx(_last_work_zone_size) and columns == _last_band_columns:
		return
	_last_work_zone_size = size
	_last_band_columns = columns
	zones_resized.emit()

## The current window (viewport) size, the basis for the panel's long-axis extent + the height clamp.
func _viewport_size() -> Vector2:
	var vp := get_viewport()
	if vp != null:
		return vp.get_visible_rect().size
	return Vector2(PANEL_WIDTH, PANEL_HEIGHT_WIDE)

func _detach(node: Node) -> void:
	if node != null and node.get_parent() != null:
		node.get_parent().remove_child(node)

func _set_root_anchors(left: float, top: float, right: float, bottom: float) -> void:
	_root.anchor_left = left
	_root.anchor_top = top
	_root.anchor_right = right
	_root.anchor_bottom = bottom

func _set_root_offsets(left: float, top: float, right: float, bottom: float) -> void:
	_root.offset_left = left
	_root.offset_top = top
	_root.offset_right = right
	_root.offset_bottom = bottom

## Pin the accent seam to the panel's map-facing edge (opposite the dock edge).
func _position_seam() -> void:
	if _seam == null:
		return
	# **THE SEAM IS A VERTICAL-DOCK THING NOW** (issue #377). It accents the map-facing edge of the
	# reserved STRIP, which is right while the card fills that strip and wrong once it does not: on a
	# horizontal dock it would rule a line across the whole monitor with a small card floating under
	# part of it, re-drawing the full-bleed bar the islands replaced. A floating card states its own
	# edge with its border.
	_seam.visible = _is_vertical_edge(_dock_edge)
	if not _seam.visible:
		return
	match _map_facing_edge():
		SIDE_LEFT:
			_seam.anchor_left = 0.0; _seam.anchor_right = 0.0
			_seam.anchor_top = 0.0; _seam.anchor_bottom = 1.0
			_seam.offset_left = 0.0; _seam.offset_right = SEAM_THICKNESS
			_seam.offset_top = 0.0; _seam.offset_bottom = 0.0
		SIDE_RIGHT:
			_seam.anchor_left = 1.0; _seam.anchor_right = 1.0
			_seam.anchor_top = 0.0; _seam.anchor_bottom = 1.0
			_seam.offset_left = -SEAM_THICKNESS; _seam.offset_right = 0.0
			_seam.offset_top = 0.0; _seam.offset_bottom = 0.0
		SIDE_TOP:
			_seam.anchor_left = 0.0; _seam.anchor_right = 1.0
			_seam.anchor_top = 0.0; _seam.anchor_bottom = 0.0
			_seam.offset_left = 0.0; _seam.offset_right = 0.0
			_seam.offset_top = 0.0; _seam.offset_bottom = SEAM_THICKNESS
		SIDE_BOTTOM:
			_seam.anchor_left = 0.0; _seam.anchor_right = 1.0
			_seam.anchor_top = 1.0; _seam.anchor_bottom = 1.0
			_seam.offset_left = 0.0; _seam.offset_right = 0.0
			_seam.offset_top = -SEAM_THICKNESS; _seam.offset_bottom = 0.0

func _refresh_collapse_state() -> void:
	if _header_full != null:
		_header_full.visible = not _collapsed
	if _body_host != null:
		_body_host.visible = not _collapsed
	if _header_rail != null:
		_header_rail.visible = _collapsed

func _refresh_dock_cells() -> void:
	for edge in _dock_cells:
		var cell: Button = _dock_cells[edge]
		cell.add_theme_stylebox_override("normal", _dock_cell_stylebox(edge, edge == _dock_edge))
		cell.add_theme_stylebox_override("hover", _dock_cell_stylebox(edge, edge == _dock_edge, true))
		cell.add_theme_stylebox_override("pressed", _dock_cell_stylebox(edge, true))

# ---- handlers --------------------------------------------------------------

func _on_collapse_pressed() -> void:
	set_collapsed(not _collapsed)

func _on_cycle_pressed(delta: int) -> void:
	cycle_requested.emit(delta)

func _emit_reservation() -> void:
	_published_reservation = current_reservation_size()
	reservation_changed.emit(_dock_edge, _published_reservation)

## Publish the reservation again IF the cross-axis size has moved since the last emission — and stay
## silent otherwise.
##
## **THE PANEL'S RESERVED SIZE IS WHERE THE EVENT DOCK'S BAR STARTS**, through `_reservations` →
## `Main._update_event_dock_edge_offset`, so a size this panel draws at but never published is a bar
## drawn straight through the card. That could not happen while the cross axis was a pure function of
## the collapse flag, the dock edge and the viewport, because the three paths that change those all
## emit. It can now: the axis carries the active SHELL's chrome, and the shell is chosen from
## `_available_card_span()`, whose other two terms — the lateral bounds and the rail's span — arrive
## DECLARED, on setters that relayout without emitting. Measured on a TOP dock at `ui_scale` 1.35: the
## panel drew 395 while `Main` still held 360, and the bar sat 35px inside the card's lower edge.
##
## **EVERY EXISTING `_emit_reservation()` CALL SITE STAYS UNCONDITIONAL, and that is deliberate.**
## `Main._apply_reservation` is not only how the size travels — it is the hook that re-pushes the
## lateral bounds and recomputes the event dock's perpendicular insets, both of which read live HUD
## geometry that a viewport resize can move without moving this panel at all. Deduplicating those
## emissions would silently stop that recomputation; this is strictly additive.
func _republish_reservation_if_changed() -> void:
	if is_equal_approx(current_reservation_size(), _published_reservation):
		return
	_emit_reservation()

# ---- helpers ---------------------------------------------------------------

## The reserved cross-axis size. **It must never depend on content** — that independence is what
## keeps `current_reservation_size` (and therefore MapView's inset + cache invalidation) constant
## while the player edits the band, so a `+` press cannot flicker the map.
func _cross_axis_size() -> float:
	if _collapsed:
		return COLLAPSED_SIZE
	if _is_vertical_edge(_dock_edge):
		return PANEL_WIDTH
	return _horizontal_panel_height()

## True when the dock reserves a vertical strip (left/right → width on the x-axis).
func _is_vertical_edge(edge: int) -> bool:
	return edge == SIDE_LEFT or edge == SIDE_RIGHT

func _map_facing_edge() -> int:
	match _dock_edge:
		SIDE_LEFT:
			return SIDE_RIGHT
		SIDE_RIGHT:
			return SIDE_LEFT
		SIDE_TOP:
			return SIDE_BOTTOM
		_:
			return SIDE_TOP

func _edge_name(edge: int) -> String:
	match edge:
		SIDE_LEFT:
			return "left"
		SIDE_RIGHT:
			return "right"
		SIDE_TOP:
			return "top"
		_:
			return "bottom"

func _make_icon_button(glyph: String, tooltip: String) -> Button:
	var btn := Button.new()
	btn.text = glyph
	btn.tooltip_text = tooltip
	btn.focus_mode = Control.FOCUS_NONE
	btn.custom_minimum_size = Vector2(ICON_BUTTON_SIZE, ICON_BUTTON_SIZE)
	btn.add_theme_font_size_override("font_size", ICON_BUTTON_FONT_SIZE)
	HudStyle.apply_button(btn, "ghost")
	return btn

## A stage-sprite `TextureRect` sized to the glyph label it sits beside, so it occupies the same
## box in the header flow. Starts hidden — `set_header` decides sprite-vs-emoji per stage.
func _make_stage_glyph_sprite() -> TextureRect:
	var rect := TextureRect.new()
	rect.custom_minimum_size = STAGE_SPRITE_SIZE
	rect.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	rect.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	# Centred in its box on both axes, mirroring the glyph labels it replaces (the cluster label
	# centres vertically, the rail label horizontally).
	rect.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	rect.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	rect.visible = false
	return rect

## Show exactly ONE of the sprite / emoji pair for a stage: the bundled art when it resolved,
## else the emoji label (a stage defined in config with no bundled art keeps its glyph).
func _apply_stage_visual(label: Label, sprite_rect: TextureRect, sprite: Texture2D, glyph: String) -> void:
	if sprite_rect != null:
		sprite_rect.texture = sprite
		sprite_rect.visible = sprite != null
	if label != null:
		label.text = glyph
		label.visible = sprite == null

## The card's own stylebox. PUBLIC and `static` because a second surface draws in it — the
## free-floating compose card (`BandComposeFloat`), which is this panel's content taken off the panel
## and must therefore read as the panel's own surface rather than as a second kind of card. One
## definition, so the two cannot drift into two looks.
static func panel_card_stylebox() -> StyleBoxFlat:
	# Square-edged card (the strip meets the screen edge — no rounding/shadow).
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudStyle.PANEL_SOLID
	sb.set_border_width_all(1)
	sb.border_color = HudStyle.LINE
	sb.content_margin_left = PANEL_CONTENT_MARGIN_H
	sb.content_margin_right = PANEL_CONTENT_MARGIN_H
	sb.content_margin_top = PANEL_CONTENT_MARGIN_V
	sb.content_margin_bottom = PANEL_CONTENT_MARGIN_V
	return sb

func _dock_cell_stylebox(edge: int, active: bool, hovered: bool = false) -> StyleBoxFlat:
	# StyleBoxFlat carries a single border color; a thicker border on the cell's
	# matching side (colored by state) reads as "dock to this edge" like the
	# prototype's edge-cells. Active = SIGNAL wash+border; hover = SIGNAL_DEEP; idle
	# = a faint bar on the LINE frame.
	var sb := StyleBoxFlat.new()
	sb.set_corner_radius_all(CORNER_RADIUS)
	sb.set_border_width_all(1)
	if active:
		sb.bg_color = HudStyle.SIGNAL_WASH
		sb.border_color = HudStyle.SIGNAL
	elif hovered:
		sb.bg_color = HudStyle.GROUND
		sb.border_color = HudStyle.SIGNAL_DEEP
	else:
		sb.bg_color = HudStyle.GROUND
		sb.border_color = HudStyle.INK_FAINT
	match edge:
		SIDE_LEFT:
			sb.border_width_left = DOCK_ACCENT_WIDTH
		SIDE_RIGHT:
			sb.border_width_right = DOCK_ACCENT_WIDTH
		SIDE_TOP:
			sb.border_width_top = DOCK_ACCENT_WIDTH
		SIDE_BOTTOM:
			sb.border_width_bottom = DOCK_ACCENT_WIDTH
	return sb

# ---- persistence -----------------------------------------------------------

func _load_prefs() -> void:
	var cfg := ConfigFile.new()
	if cfg.load(_config_path()) != OK:
		return
	var edge := int(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_EDGE, SIDE_LEFT))
	if DOCK_EDGES.has(edge):
		_dock_edge = edge
	_collapsed = bool(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_COLLAPSED, false))
	var tab := StringName(str(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_TAB, String(DEFAULT_TAB))))
	if TAB_LABELS.has(tab):
		_active_tab = tab
	# Deliberately UNVALIDATED — the work-sort vocabulary is `BandPanelController`'s, and it is what
	# rejects an unknown value when it adopts this.
	_work_sort_pref = str(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_WORK_SORT, ""))

func _save_prefs() -> void:
	var cfg := ConfigFile.new()
	cfg.load(_config_path())   # preserve any other sections; ignore load errors
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_EDGE, _dock_edge)
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_COLLAPSED, _collapsed)
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_TAB, String(_active_tab))
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_WORK_SORT, _work_sort_pref)
	cfg.save(_config_path())

## The WORK board's persisted sort, or `""` when the player has never chosen one.
func work_sort_pref() -> String:
	return _work_sort_pref

## Remember the WORK board's sort. The value is opaque here — see `CONFIG_KEY_WORK_SORT`.
func set_work_sort_pref(value: String) -> void:
	_work_sort_pref = value
	_save_prefs()

## The prefs file actually used — the scratch override when a harness set one, else the player's.
static func _config_path() -> String:
	return config_path_override if config_path_override != "" else CONFIG_PATH
