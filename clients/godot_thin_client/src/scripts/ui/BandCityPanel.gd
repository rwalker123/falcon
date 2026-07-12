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
# ---- responsive body layout (tall L/R stack vs wide T/B column-flow) -------
## In the wide (T/B) dock the section blocks flow into a VFlowContainer bounded to the strip height
## and wrap into columns. Each block is capped to this width, so a column is a tidy, readable measure
## and the stepper `−/+` controls stay beside their labels (≈ the tall dock's content width). Bounded,
## NOT expand-fill, so a wide/ultrawide window leaves empty space on the right rather than stretching
## the rows — the flow simply spreads the sections across more columns.
const SECTION_COLUMN_WIDTH := 340.0
## Gap between the wrapped columns AND between blocks within a column, in wide (column-flow) mode.
const WIDE_FLOW_SEPARATION := 16
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
# vertical `ScrollContainer`→VBox stack (`_tall_*`, for the tall L/R docks) and a bounded-height
# `VFlowContainer` (`_wide_*`, for the wide T/B docks) that flows the blocks into columns. Hud hands
# the panel an ordered list of self-contained **section blocks** via `set_band_sections`; the panel
# OWNS them (frees the previous set, arranges the new one). On a tall↔wide dock flip the SAME block
# nodes are reparented into the other container (`_relayout_body`) — no Hud re-render needed. The two
# modes use DIFFERENT container types on purpose: VFlowContainer mis-columns in the tall unbounded
# scroll (confirmed in #103), so tall stays a plain VBox.
var _body_host: VBoxContainer
var _tall_scroll: ScrollContainer
var _tall_vbox: VBoxContainer
var _wide_scroll: ScrollContainer
var _wide_flow: VFlowContainer
var _body_is_wide: bool = false
var _band_present: bool = false
var _empty_state: Label
var _section_blocks: Array = []   # the Hud-built section blocks the panel currently owns
var _dock_cells: Dictionary = {}   # edge:int -> Button

func _ready() -> void:
	layer = LAYER_INDEX
	_load_prefs()
	_build()
	_apply_dock_layout()
	_refresh_collapse_state()
	_refresh_dock_cells()

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

	# Wide layout (T/B docks): a VFlowContainer that flows the section blocks top→bottom and wraps
	# into a new column when a column fills, spreading the sections across the strip width. It MUST be
	# bounded in height to column-flow correctly, so it lives in a ScrollContainer with vertical scroll
	# DISABLED (the scroll fits the flow to the strip height) and horizontal scroll AUTO (defensive:
	# a narrow window whose columns overrun the width scrolls sideways instead of clipping).
	_wide_scroll = ScrollContainer.new()
	_wide_scroll.name = "WideScroll"
	_wide_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_wide_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	_wide_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_wide_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_wide_scroll.visible = false
	_body_host.add_child(_wide_scroll)
	_wide_flow = VFlowContainer.new()
	_wide_flow.name = "WideFlow"
	_wide_flow.add_theme_constant_override("h_separation", WIDE_FLOW_SEPARATION)
	_wide_flow.add_theme_constant_override("v_separation", WIDE_FLOW_SEPARATION)
	_wide_scroll.add_child(_wide_flow)

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
	_relayout_body()

## Switch the body between the tall single-stack layout (L/R docks) and the wide column-flow layout
## (T/B docks) by reparenting the SAME owned section blocks. Idempotent — only re-arranges when the
## tall↔wide orientation actually changes.
func _relayout_body() -> void:
	if _tall_vbox == null or _wide_flow == null:
		return
	var wide := not _is_vertical_edge(_dock_edge)
	if wide == _body_is_wide and _blocks_are_homed():
		return
	_body_is_wide = wide
	_arrange_sections()

## True when every owned block already lives in the active layout container (so a re-arrange is a
## no-op). Guards the first layout pass (before any blocks exist, `_body_is_wide` may match the dock
## but the containers are empty — that's fine, an empty arrange just toggles visibility).
func _blocks_are_homed() -> bool:
	var host: Node = _wide_flow if _body_is_wide else _tall_vbox
	for block in _section_blocks:
		if block is Node and (block as Node).get_parent() != host:
			return false
	return true

## Home every owned section block into the active layout container and size it for that mode, then
## refresh which container is visible. Wide caps each block to a tidy column measure
## (`SECTION_COLUMN_WIDTH`) so the flow spreads them across columns; tall lets blocks fill the strip
## width (min 0 → the VBox's EXPAND_FILL stretches them). Reparents only blocks not already homed.
func _arrange_sections() -> void:
	var host: Node = _wide_flow if _body_is_wide else _tall_vbox
	for block_variant in _section_blocks:
		if not (block_variant is Control):
			continue
		var block: Control = block_variant
		if block.get_parent() != host:
			_detach(block)
			host.add_child(block)
		block.custom_minimum_size.x = SECTION_COLUMN_WIDTH if _body_is_wide else 0.0
	_update_body_visibility()

## Show the active layout container (tall or wide) when a band is present, else neither (the
## empty-state placeholder shows instead). Collapse is handled separately by `_refresh_collapse_state`
## hiding the whole `_body_host`.
func _update_body_visibility() -> void:
	if _tall_scroll != null:
		_tall_scroll.visible = _band_present and not _body_is_wide
	if _wide_scroll != null:
		_wide_scroll.visible = _band_present and _body_is_wide

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
	return PANEL_WIDTH if _is_vertical_edge(_dock_edge) else PANEL_HEIGHT

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
	sb.content_margin_left = 12
	sb.content_margin_right = 12
	sb.content_margin_top = 10
	sb.content_margin_bottom = 10
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
