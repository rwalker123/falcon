extends CanvasLayer
class_name BandCityPanel

## The dockable Band / City panel (docs/plan_band_city_dock.md §"Architecture 2").
##
## A CanvasLayer that renders a card against one screen edge and *reserves* that
## strip via the slice-1 reservation API (Main fans `reservation_changed` out to
## `MapView`/`Hud`, so the map + HUD reflow off the edge rather than being
## overlaid). This slice is the **chrome scaffold**: settlement header (stage
## glyph + name + stage label), a settlement cycler, a 4-cell dock chooser, and a
## collapse toggle, plus dock persistence. The body hosts the relocated band detail as an ordered
## list of **section blocks** Hud hands over via `set_band_sections` (summary, active-expeditions,
## then the allocation sections); the panel owns those blocks and arranges them by dock aspect —
## a vertical stack when tall (L/R), a column-flow that fills the strip when wide (T/B).
##
## All geometry/typography flows from named constants + `HudStyle` (no magic
## numbers, one visual-language source).

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

# ---- geometry (canvas-space px) --------------------------------------------
## Cross-axis size of the expanded panel: width when docked L/R, height when T/B.
const PANEL_WIDTH := 380.0
const PANEL_HEIGHT := 260.0
## Cross-axis size when collapsed to a thin rail (both orientations).
const COLLAPSED_SIZE := 46.0
## Render above the map (and the HUD/Inspector) so the panel owns its reserved strip.
const LAYER_INDEX := 103
## Accent seam thickness on the panel's map-facing edge (the prototype's SIGNAL_DEEP border).
const SEAM_THICKNESS := 2.0

# ---- chrome typography / sizing --------------------------------------------
const STAGE_GLYPH_FONT_SIZE := 20
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
# ---- responsive body layout (tall L/R stack vs wide T/B manual columns) -----
## In the wide (T/B) dock the section blocks are packed by hand into fixed-width columns that FILL the
## strip width, and the panel reserves exactly the height its tallest column needs (fit-to-content, so
## nothing clips). Each block is capped to this width so a column is a tidy, readable measure and the
## stepper `−/+` controls stay beside their labels (≈ the tall dock's content width).
const SECTION_COLUMN_WIDTH := 340.0
## Gap between the packed columns AND between blocks within a column, in wide mode.
const WIDE_FLOW_SEPARATION := 16
## Safety net for a pathological single column taller than most of the screen: the reserved wide-dock
## height never exceeds this fraction of the window height; past it the columns gain a vertical scroll
## rather than the panel eating the whole screen. Fit-to-content is the primary behaviour; this caps it.
const MAX_WIDE_HEIGHT_FRACTION := 0.6
## A few px of slack on the computed wide-dock height so sub-pixel min-size vs rendered-height rounding
## never clips the last row of the tallest column.
const WIDE_CONTENT_SAFETY_PAD := 4.0
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

## The four dock edges, in the prototype's 2×2 chooser order (row-major:
## left/top on the first row, bottom/right on the second).
const DOCK_EDGES: Array[int] = [SIDE_LEFT, SIDE_TOP, SIDE_BOTTOM, SIDE_RIGHT]

signal reservation_changed(edge: int, size: float)
signal cycle_requested(delta: int)
## The header subject cluster (stage glyph + name + stage label) was clicked — "jump to my band".
signal subject_activated

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
var _name_label: Label
var _stage_label: Label
var _count_label: Label
var _collapse_button: Button
var _rail_expand_button: Button
# Body layout: `_body_host` holds two alternative layout containers, one visible at a time — a
# vertical `ScrollContainer`→VBox stack (`_tall_*`, for the tall L/R docks) and a MANUAL column pack
# (`_wide_*`, for the wide T/B docks): an HBox of column VBoxes the panel fills by hand. Hud hands the
# panel an ordered list of self-contained **section blocks** via `set_band_sections`; the panel OWNS
# them (frees the previous set, arranges the new one). On a tall↔wide dock flip the SAME block nodes
# are reparented into the other container (`_relayout_body`) — no Hud re-render needed. Wide mode packs
# blocks height-balanced into as many `SECTION_COLUMN_WIDTH` columns as the width allows and reserves
# exactly the tallest column's height (fit-to-content — see `_pack_wide_columns`), so a short fixed
# strip can never clip a tall column. The column VBoxes are transient scaffolding, rebuilt each pack.
var _body_host: VBoxContainer
var _tall_scroll: ScrollContainer
var _tall_vbox: VBoxContainer
var _wide_scroll: ScrollContainer
var _wide_columns_row: HBoxContainer
var _body_is_wide: bool = false
var _band_present: bool = false
var _empty_state: Label
var _section_blocks: Array = []   # the Hud-built section blocks the panel currently owns
# Wide-dock fit-to-content state: the reserved cross-axis HEIGHT `_cross_axis_size` reports for T/B
# (derived from the tallest packed column, capped by MAX_WIDE_HEIGHT_FRACTION), and a re-measure guard
# so the deferred settle pass (fit_content RichTextLabel heights) is queued at most once.
var _wide_content_height: float = PANEL_HEIGHT
var _wide_capped: bool = false
var _wide_remeasure_queued: bool = false
# Tall-dock (L/R) fit-to-content WIDTH — the mirror of `_wide_content_height` for the vertical docks.
# The reserved cross-axis WIDTH tracks the section content so `_root`, the seam (`_position_seam`), and
# the reservation (`current_reservation_size`) all agree on the true card edge. The fixed PANEL_WIDTH
# used to freeze the seam mid-card whenever a section (a long Hunt row, the send-expedition button, …)
# grew the content-sized PanelContainer past 380. PANEL_WIDTH stays the floor (a sparse band still gets
# the nominal strip). `_tall_remeasure_queued` mirrors `_wide_remeasure_queued` (one deferred settle
# pass per burst).
var _tall_content_width: float = PANEL_WIDTH
var _tall_remeasure_queued: bool = false
var _dock_cells: Dictionary = {}   # edge:int -> Button

func _ready() -> void:
	layer = LAYER_INDEX
	_load_prefs()
	_build()
	_apply_dock_layout()
	_refresh_collapse_state()
	_refresh_dock_cells()
	# A window resize changes the available width → the wide-dock column count (and thus the
	# fit-to-content height), so re-pack + re-report the reservation when the viewport resizes.
	var vp := get_viewport()
	if vp != null:
		vp.size_changed.connect(_on_viewport_resized)

# ---- public API ------------------------------------------------------------

## Push the header subject: settlement stage glyph, display name, stage label.
func set_header(glyph: String, subject_name: String, stage_label: String) -> void:
	var resolved_glyph := glyph if not glyph.is_empty() else DEFAULT_STAGE_GLYPH
	if _stage_glyph_label != null:
		_stage_glyph_label.text = resolved_glyph
	if _rail_glyph_label != null:
		_rail_glyph_label.text = resolved_glyph
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

## Hand the panel the ordered list of Hud-built section blocks (summary, active-expeditions, then the
## allocation section blocks). The panel takes OWNERSHIP: it frees the blocks from the previous render
## and arranges the new ones in the active layout (tall stack / wide column-flow). An empty array →
## the empty-state placeholder.
func set_band_sections(blocks: Array) -> void:
	_free_section_blocks()
	for b in blocks:
		if b is Control:
			_section_blocks.append(b)
	if _section_blocks.is_empty():
		set_band_present(false)
		return
	_band_present = true
	if _empty_state != null:
		_empty_state.visible = false
	_arrange_sections()

## Toggle between the band-detail content and the empty-state placeholder. `false` also frees any
## owned section blocks (no band → nothing to show).
func set_band_present(present: bool) -> void:
	_band_present = present
	if not present:
		_free_section_blocks()
	if _empty_state != null:
		_empty_state.visible = not present
	_update_body_visibility()

## Free (and detach) the section blocks from the previous render. Ownership is unambiguous: the panel
## owns exactly what it was last handed, and drops it here before taking the next set.
func _free_section_blocks() -> void:
	for block_variant in _section_blocks:
		if block_variant is Node:
			_detach(block_variant)
			(block_variant as Node).queue_free()
	_section_blocks.clear()

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

	_header_full = _build_header_full()
	column.add_child(_header_full)

	_header_rail = _build_header_rail()
	column.add_child(_header_rail)

	# The body host holds both alternative layout containers + the empty-state; only one layout is
	# visible per dock. Collapse hides the whole host.
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

	# Tall layout (L/R docks): a single vertical stack of section blocks in a vertical scroll.
	_tall_scroll = ScrollContainer.new()
	_tall_scroll.name = "TallScroll"
	_tall_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_tall_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_tall_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_tall_scroll.visible = false
	_body_host.add_child(_tall_scroll)
	_tall_vbox = VBoxContainer.new()
	_tall_vbox.name = "TallColumn"
	_tall_vbox.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_tall_vbox.add_theme_constant_override("separation", BODY_SEPARATION)
	_tall_scroll.add_child(_tall_vbox)

	# Wide layout (T/B docks): a MANUAL column pack. The panel reserves exactly the tallest column's
	# height (fit-to-content), so the columns normally fit with no scroll; the ScrollContainer is the
	# cap safety net — vertical scroll flips to AUTO only when a pathological column exceeds
	# MAX_WIDE_HEIGHT_FRACTION of the window, and horizontal AUTO is defensive against column overrun.
	_wide_scroll = ScrollContainer.new()
	_wide_scroll.name = "WideScroll"
	_wide_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_wide_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	_wide_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_wide_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_wide_scroll.visible = false
	_body_host.add_child(_wide_scroll)
	# The row of hand-packed column VBoxes (begin-aligned, so columns fill from the left and any
	# leftover width stays empty on the right). `_pack_wide_columns` rebuilds its children each pack.
	_wide_columns_row = HBoxContainer.new()
	_wide_columns_row.name = "WideColumns"
	_wide_columns_row.add_theme_constant_override("separation", WIDE_FLOW_SEPARATION)
	_wide_scroll.add_child(_wide_columns_row)

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

## Switch the body between the tall single-stack layout (L/R docks) and the wide manual-column layout
## (T/B docks). Called on every dock-layout pass; the actual work is cheap and idempotent, so it just
## re-arranges into the active mode (the wide packer re-runs on width/content change anyway).
func _relayout_body() -> void:
	if _tall_vbox == null or _wide_columns_row == null:
		return
	_body_is_wide = not _is_vertical_edge(_dock_edge)
	_arrange_sections()

## Home the owned section blocks into the active layout — a single vertical stack when tall, the
## hand-packed balanced columns when wide — then refresh which container is visible.
func _arrange_sections() -> void:
	if _body_is_wide:
		_arrange_wide()
	else:
		_arrange_tall()
	_update_body_visibility()

## Tall (L/R): every block stacks in the vertical scroll's VBox, filling the strip width (min-x 0 →
## the VBox's EXPAND_FILL stretches them). Reparents only blocks not already homed there.
func _arrange_tall() -> void:
	for block_variant in _section_blocks:
		if not (block_variant is Control):
			continue
		var block: Control = block_variant
		if block.get_parent() != _tall_vbox:
			_detach(block)
			_tall_vbox.add_child(block)
		block.custom_minimum_size.x = 0.0
	# Fit the reserved WIDTH to the just-homed content (deferred a frame so the summary RichTextLabel's
	# fit_content settles first), so the seam sits on the true card edge rather than a fixed PANEL_WIDTH.
	_schedule_tall_remeasure()

## Wide (T/B): cap each block to a tidy column measure, pack the balanced columns now (using current
## min sizes), then queue one deferred re-pack so the fit_content summary height settles before the
## reserved height is finalised.
func _arrange_wide() -> void:
	if _section_blocks.is_empty():
		return   # nothing to pack yet; `_cross_axis_size` falls back to PANEL_HEIGHT until a band arrives
	for block_variant in _section_blocks:
		if block_variant is Control:
			(block_variant as Control).custom_minimum_size.x = SECTION_COLUMN_WIDTH
	_pack_wide_columns()
	_schedule_wide_remeasure()

## Show the active layout container (tall or wide) when a band is present, else neither (the
## empty-state placeholder shows instead). Collapse is handled separately by `_refresh_collapse_state`
## hiding the whole `_body_host`.
func _update_body_visibility() -> void:
	if _tall_scroll != null:
		_tall_scroll.visible = _band_present and not _body_is_wide
	if _wide_scroll != null:
		_wide_scroll.visible = _band_present and _body_is_wide

## Pack the owned section blocks into height-balanced fixed-width columns and reserve exactly the
## height the tallest column needs (fit-to-content, so nothing clips). Steps:
##   1. Column count from the available width (full window width for a T/B dock, minus card padding),
##      clamped to 1..blocks so a wide screen fills with many columns but never more than blocks.
##   2. Greedy balance: each block (in order) joins the currently-SHORTEST column, minimising the
##      tallest column → minimising the height we must reserve. Column VBoxes are rebuilt each pack.
##   3. Reserved height = card V-padding + header + tallest column (+ a px of slack), capped at
##      MAX_WIDE_HEIGHT_FRACTION of the window (past which the columns gain a vertical scroll). Report
##      it via `reservation_changed` (through `_cross_axis_size`) so the map/HUD reflow to fit.
func _pack_wide_columns() -> void:
	if _wide_columns_row == null:
		return
	# (1) Column count from available width. For a T/B dock `_root` spans the full window width, so the
	# usable content width is the window width minus the card's horizontal padding.
	var window_w := _viewport_size().x
	var avail := window_w - 2.0 * PANEL_CONTENT_MARGIN_H
	var block_count := _section_blocks.size()
	var per_col := SECTION_COLUMN_WIDTH + float(WIDE_FLOW_SEPARATION)
	var num_cols := clampi(int(avail / per_col), 1, maxi(block_count, 1))
	# (2) Rebuild the column scaffolding: detach the owned blocks first (so freeing the old columns
	# never frees a block), then create fresh column VBoxes.
	for block_variant in _section_blocks:
		if block_variant is Node:
			_detach(block_variant)
	for child in _wide_columns_row.get_children():
		child.queue_free()
	var columns: Array[VBoxContainer] = []
	var col_heights: Array[float] = []
	for i in range(num_cols):
		var col := VBoxContainer.new()
		col.size_flags_horizontal = Control.SIZE_FILL
		col.size_flags_vertical = Control.SIZE_FILL
		col.custom_minimum_size = Vector2(SECTION_COLUMN_WIDTH, 0.0)
		col.add_theme_constant_override("separation", WIDE_FLOW_SEPARATION)
		_wide_columns_row.add_child(col)
		columns.append(col)
		col_heights.append(0.0)
	# Greedy: add each block to the shortest column.
	for block_variant in _section_blocks:
		if not (block_variant is Control):
			continue
		var block: Control = block_variant
		var target := _shortest_column_index(col_heights)
		if col_heights[target] > 0.0:
			col_heights[target] += float(WIDE_FLOW_SEPARATION)
		col_heights[target] += block.get_combined_minimum_size().y
		columns[target].add_child(block)
	# (3) Reserved height from the tallest column + chrome, capped by the window fraction.
	var tallest := 0.0
	for h in col_heights:
		tallest = maxf(tallest, h)
	var header_h := _header_full.get_combined_minimum_size().y if _header_full != null else 0.0
	var content_h := 2.0 * PANEL_CONTENT_MARGIN_V + header_h + tallest + WIDE_CONTENT_SAFETY_PAD
	var cap := _viewport_size().y * MAX_WIDE_HEIGHT_FRACTION
	_wide_capped = content_h > cap
	var new_height := minf(content_h, cap) if _wide_capped else content_h
	# Capped → let the columns scroll vertically within the (shorter) reserved height; else they fit
	# exactly, no scroll.
	if _wide_scroll != null:
		_wide_scroll.vertical_scroll_mode = (
			ScrollContainer.SCROLL_MODE_AUTO if _wide_capped else ScrollContainer.SCROLL_MODE_DISABLED)
	# Re-anchor + re-report the reservation only when the height actually moved (avoids reflow churn).
	if not is_equal_approx(new_height, _wide_content_height):
		_wide_content_height = new_height
		if not _collapsed and _shown:
			_apply_root_anchors()
			_emit_reservation()

## Index of the shortest column (ties → the leftmost), for the greedy height balance.
func _shortest_column_index(heights: Array[float]) -> int:
	var best := 0
	for i in range(1, heights.size()):
		if heights[i] < heights[best]:
			best = i
	return best

## Queue one deferred re-pack so the fit_content summary RichTextLabel (bounded to SECTION_COLUMN_WIDTH)
## reports its final wrapped height before the reserved height is finalised. Guarded so a burst of
## arrange calls collapses to a single settle pass.
func _schedule_wide_remeasure() -> void:
	if _wide_remeasure_queued:
		return
	_wide_remeasure_queued = true
	_run_wide_remeasure.call_deferred()

func _run_wide_remeasure() -> void:
	_wide_remeasure_queued = false
	if not _body_is_wide or not _band_present:
		return
	# One frame lets the just-packed blocks lay out at their column width so fit_content settles.
	await get_tree().process_frame
	if not is_instance_valid(self) or not _body_is_wide or not _band_present:
		return
	_pack_wide_columns()

## Fit the tall (L/R) reserved WIDTH to the section content, floored at PANEL_WIDTH — the mirror of
## `_pack_wide_columns`'s height fit. The content minimum is read off the PanelContainer's combined
## minimum size (it already folds in the card stylebox margins, the column separation, and the tall
## stack's widest section), which is INDEPENDENT of the current allocated width — so measuring never
## feeds back into a resize loop. `is_equal_approx` guards against reflow churn, exactly like the wide
## height fit. On a real change, re-anchor `_root` (which re-pins the seam) + re-report the reservation
## so the map/HUD reflow to the true card edge.
func _measure_tall_width() -> void:
	if _panel == null or _collapsed or not _shown or not _band_present:
		return
	if not _is_vertical_edge(_dock_edge):
		return
	var content_min := _panel.get_combined_minimum_size().x
	if is_equal_approx(content_min, _tall_content_width):
		return
	_tall_content_width = content_min
	_apply_root_anchors()
	_emit_reservation()

## Queue one deferred tall-width measure so the fit_content summary RichTextLabel reports its final size
## before the reserved width is finalised. Guarded so a burst of arrange calls collapses to a single pass
## (mirrors `_schedule_wide_remeasure`).
func _schedule_tall_remeasure() -> void:
	if _tall_remeasure_queued:
		return
	_tall_remeasure_queued = true
	_run_tall_remeasure.call_deferred()

func _run_tall_remeasure() -> void:
	_tall_remeasure_queued = false
	if _collapsed or not _shown or not _band_present or not _is_vertical_edge(_dock_edge):
		return
	# One frame lets the just-homed blocks lay out so the content minimum (fit_content summary) settles.
	await get_tree().process_frame
	if not is_instance_valid(self):
		return
	if _collapsed or not _shown or not _band_present or not _is_vertical_edge(_dock_edge):
		return
	_measure_tall_width()

## A window resize changes the available width (column count) and thus the fit-to-content height —
## re-pack + re-report when wide and showing a band. When tall, the content minimum width is unchanged
## by a resize, but re-measure defensively (the `is_equal_approx` guard makes a no-op cheap).
func _on_viewport_resized() -> void:
	if not _band_present:
		return
	if _body_is_wide:
		_arrange_wide()
	elif _is_vertical_edge(_dock_edge):
		_schedule_tall_remeasure()

## The current window (viewport) size, the basis for the wide-dock column count + height cap.
func _viewport_size() -> Vector2:
	var vp := get_viewport()
	if vp != null:
		return vp.get_visible_rect().size
	return Vector2(PANEL_WIDTH, PANEL_HEIGHT)

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

func _cross_axis_size() -> float:
	if _collapsed:
		return COLLAPSED_SIZE
	if _is_vertical_edge(_dock_edge):
		# Tall (L/R): the fit-to-content width from the last measure, floored at the nominal PANEL_WIDTH
		# so `_root`/seam/reservation track the real card edge instead of a fixed 380. No band → nominal.
		if _band_present:
			return maxf(PANEL_WIDTH, _tall_content_width)
		return PANEL_WIDTH
	# Wide (T/B): the fit-to-content height from the last column pack. Before the first pack (or with no
	# band) fall back to the nominal PANEL_HEIGHT so the reserved strip is sensible.
	if _band_present and _wide_content_height > 0.0:
		return _wide_content_height
	return PANEL_HEIGHT

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

func _save_prefs() -> void:
	var cfg := ConfigFile.new()
	cfg.load(CONFIG_PATH)   # preserve any other sections; ignore load errors
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_EDGE, _dock_edge)
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_COLLAPSED, _collapsed)
	cfg.save(CONFIG_PATH)
