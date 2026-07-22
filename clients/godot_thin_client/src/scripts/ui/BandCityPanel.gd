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
##   * WIDE  (width >= `WIDE_SHELL_MIN_WIDTH`, in practice a T/B dock): the three
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
# ---- responsive body layout (wide 3-column shell vs narrow tabbed shell) -----
## The panel switches to the wide (3-zones-side-by-side) shell once its own WIDTH reaches this;
## below it the narrow (tabbed, one-zone) shell is used. A WIDTH test, never a dock-edge test, so a
## resizable dock or a narrow window needs no special case. Three zones at their fixed widths plus a
## work zone worth reading need roughly this much room.
const WIDE_SHELL_MIN_WIDTH := 900.0
## Fixed widths of the two flanking zones in the wide shell; Work takes whatever is left.
const ZONE_BAND_WIDTH := 300.0
const ZONE_PARTY_WIDTH := 300.0
## The widest the WORK zone can use: Hud's board stops adding columns at `WORK_MAX_COLUMNS` (4) of
## `WORK_COLUMN_MIN_WIDTH` (380), so past this a wider zone only stretches the same rows. Kept here
## rather than read from Hud — the panel must not depend on its content's internals — but the two are
## a PAIR: change the board's column cap and change this with it.
const ZONE_WORK_MAX_WIDTH := 1520.0
## Hairline separator drawn between adjacent zones in the wide shell.
const ZONE_SEPARATOR_THICKNESS := 1.0
## Gap either side of a zone separator, so the hairline is not flush against zone content.
const ZONE_SEPARATION := 12
## Safety net so a short window can never let the T/B strip eat the screen: the reserved wide-dock
## height is `PANEL_HEIGHT_WIDE` clamped to this fraction of the window height.
const MAX_WIDE_HEIGHT_FRACTION := 0.6
## Card border thickness (`_panel_stylebox`), subtracted alongside the content margins when the
## panel reports the interior box its Work zone may fill.
const PANEL_BORDER_WIDTH := 1.0
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

## The four dock edges, in the prototype's 2×2 chooser order (row-major:
## left/top on the first row, bottom/right on the second).
const DOCK_EDGES: Array[int] = [SIDE_LEFT, SIDE_TOP, SIDE_BOTTOM, SIDE_RIGHT]

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
var _count_label: Label
var _collapse_button: Button
var _rail_expand_button: Button
# Body layout: `_body_host` holds the two alternative SHELLS, exactly one visible at a time (chosen by
# panel width — see `_shell_is_wide`). The wide shell is an HBox of three zone hosts (band + parties
# fixed-width, work expanding) with hairline separators; the narrow shell is a tab bar over a single
# zone host. `_zones` holds the three Hud-built zone Controls the panel OWNS (freed on the next
# `set_zones`); `_reparent_zones` homes them into whichever hosts are active, so a shell flip needs no
# Hud re-render. Nothing here measures content — the shells fill a card whose size is fixed per dock.
## The card's whole content column (header + body). It is what CENTRES on an ultrawide — the card
## fill and the accent seam stay full-bleed, since the panel still reserves the entire edge.
var _panel_column: VBoxContainer
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
var _tab_badges: Dictionary = {}
var _tab_buttons: Dictionary = {}   # zone:StringName -> Control (the tab cell)
## The last `work_zone_size()` reported, so `zones_resized` fires on a real change only.
var _last_work_zone_size: Vector2 = Vector2.ZERO
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
## fallback, display name, stage label. The stage renders as bundled art when `StageSprites` has
## a texture for the id; a stage with no bundled art (the config is user-editable) keeps its emoji.
func set_header(stage_id: String, glyph: String, subject_name: String, stage_label: String) -> void:
	var resolved_glyph := glyph if not glyph.is_empty() else DEFAULT_STAGE_GLYPH
	var sprite := StageSprites.for_stage(stage_id)
	_apply_stage_visual(_stage_glyph_label, _stage_glyph_sprite, sprite, resolved_glyph)
	_apply_stage_visual(_rail_glyph_label, _rail_glyph_sprite, sprite, resolved_glyph)
	if _name_label != null:
		_name_label.text = subject_name
	if _stage_label != null:
		_stage_label.text = stage_label

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
		var flanks := ZONE_BAND_WIDTH + ZONE_PARTY_WIDTH
		# The shell is CENTRED at `_wide_content_cap()` once the panel exceeds it, so the work zone stops
		# growing there too — measure the capped width, not the panel's.
		var usable: float = minf(interior.x, _wide_content_cap())
		return Vector2(maxf(usable - flanks - _wide_separator_span(), 0.0), body_height)
	return Vector2(interior.x, maxf(body_height - _tab_bar_height(), 0.0))

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
	add_child(_root)

	_panel = PanelContainer.new()
	_panel.name = "PanelCard"
	_panel.add_theme_stylebox_override("panel", _panel_stylebox())
	_panel.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_root.add_child(_panel)

	var column := VBoxContainer.new()
	column.name = "PanelColumn"
	column.add_theme_constant_override("separation", COLUMN_SEPARATION)
	_panel.add_child(column)
	_panel_column = column

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
	subject.add_child(_name_label)
	subject.add_child(_stage_label)
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
	_position_seam()

## Choose the shell for the panel's current WIDTH and home the zones into it. Called on every
## dock-layout pass; cheap and idempotent.
func _relayout_body() -> void:
	if _wide_shell == null or _narrow_shell == null:
		return
	var was_wide := _body_is_wide
	_body_is_wide = _shell_is_wide()
	if _body_is_wide != was_wide:
		_rebuild_tab_bar()
	_apply_wide_content_cap()
	_reparent_zones()
	_update_body_visibility()

## The widest the three zones can USE: both flanks, the work board at its column cap, and the
## separators between them. Past this the board cannot grow another column, so every extra pixel would
## only stretch the zones — which on an ultrawide leaves one row of work strung across two feet of
## screen and pushes the band zone and the parties zone so far apart that reading one loses the other.
func _wide_content_cap() -> float:
	return ZONE_BAND_WIDTH + ZONE_PARTY_WIDTH + ZONE_WORK_MAX_WIDTH + _wide_separator_span()

## What the two separators + their gaps cost. Shared by the cap and `work_zone_size`, so the two can
## never disagree about how much width the chrome eats.
func _wide_separator_span() -> float:
	return 2.0 * (ZONE_SEPARATOR_THICKNESS + 2.0 * float(ZONE_SEPARATION))

## Centre the card's whole CONTENT COLUMN once the panel is wider than the zones can use, leaving equal
## margins either side; below the cap it fills as before. The header goes with it deliberately: capping
## the body alone left the band's name at the far left of the monitor and its cycler at the far right,
## a screen apart, straddling a centred island of content. The card fill and the seam are untouched —
## the panel still reserves the whole edge, it just stops STRETCHING into all of it.
## `SHRINK_CENTER` takes the container's MINIMUM size, so the cap is applied as that minimum — and it
## MUST be cleared when filling, or the column would refuse to shrink below the cap on a narrower
## window and would overflow the card.
func _apply_wide_content_cap() -> void:
	if _panel_column == null:
		return
	var cap := _wide_content_cap()
	if _body_is_wide and _interior_size().x > cap:
		_panel_column.custom_minimum_size.x = cap
		_panel_column.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	else:
		_panel_column.custom_minimum_size.x = 0.0
		_panel_column.size_flags_horizontal = Control.SIZE_EXPAND_FILL

## True when the panel is wide enough for the three zones side by side. A WIDTH test, never a
## dock-edge test — see `WIDE_SHELL_MIN_WIDTH`.
func _shell_is_wide() -> bool:
	return _panel_extent().x >= WIDE_SHELL_MIN_WIDTH

## The panel card's outer size for the current dock: fixed on the cross axis, the window on the other.
func _panel_extent() -> Vector2:
	var window := _viewport_size()
	if _is_vertical_edge(_dock_edge):
		return Vector2(PANEL_WIDTH, window.y)
	return Vector2(window.x, _wide_panel_height())

## The T/B cross-axis size: the fixed `PANEL_HEIGHT_WIDE`, clamped to a fraction of the window so a
## short window can never let the strip eat the screen.
func _wide_panel_height() -> float:
	return minf(PANEL_HEIGHT_WIDE, _viewport_size().y * MAX_WIDE_HEIGHT_FRACTION)

## The card's INTERIOR box — the outer extent less the border and the content margins the card draws
## with (`_panel_stylebox`). Chrome only; never content.
func _interior_size() -> Vector2:
	var outer := _panel_extent()
	var chrome_h := 2.0 * (PANEL_CONTENT_MARGIN_H + PANEL_BORDER_WIDTH)
	var chrome_v := 2.0 * (PANEL_CONTENT_MARGIN_V + PANEL_BORDER_WIDTH)
	return Vector2(maxf(outer.x - chrome_h, 0.0), maxf(outer.y - chrome_v, 0.0))

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
func _notify_zones_resized() -> void:
	var size := work_zone_size()
	if size.is_equal_approx(_last_work_zone_size):
		return
	_last_work_zone_size = size
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
	reservation_changed.emit(_dock_edge, current_reservation_size())

# ---- helpers ---------------------------------------------------------------

## The reserved cross-axis size. **It must never depend on content** — that independence is what
## keeps `current_reservation_size` (and therefore MapView's inset + cache invalidation) constant
## while the player edits the band, so a `+` press cannot flicker the map.
func _cross_axis_size() -> float:
	if _collapsed:
		return COLLAPSED_SIZE
	if _is_vertical_edge(_dock_edge):
		return PANEL_WIDTH
	return _wide_panel_height()

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

func _panel_stylebox() -> StyleBoxFlat:
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
	if cfg.load(CONFIG_PATH) != OK:
		return
	var edge := int(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_EDGE, SIDE_LEFT))
	if DOCK_EDGES.has(edge):
		_dock_edge = edge
	_collapsed = bool(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_COLLAPSED, false))
	var tab := StringName(str(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_TAB, String(DEFAULT_TAB))))
	if TAB_LABELS.has(tab):
		_active_tab = tab

func _save_prefs() -> void:
	var cfg := ConfigFile.new()
	cfg.load(CONFIG_PATH)   # preserve any other sections; ignore load errors
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_EDGE, _dock_edge)
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_COLLAPSED, _collapsed)
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_TAB, String(_active_tab))
	cfg.save(CONFIG_PATH)
