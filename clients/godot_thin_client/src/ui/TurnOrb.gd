extends Control
class_name TurnOrb

## The bottom-right "turn orb": a 4X-style circular widget that replaces the old
## default-theme "Advance Turn" button.
##
## It **calm-pulses only when nothing needs the player** (an empty attention
## registry) and otherwise stops pulsing, wears a count badge tinted by the
## highest-severity item, and becomes the hub for typed **attention reasons** —
## each a popover row that jumps the camera to the thing on the map.
##
## The orb is deliberately generic: it renders a list of `Attention` dictionaries
## (see the contract below) and knows nothing about who produced them, so new
## producers (wars / decisions / awaiting expeditions) slot in with no orb change.
##   Attention := {
##     kind:     String   # "idle_workers" | "war" | "decision" | …
##     severity: String   # "info" | "warn" | "critical"  → color + badge tint
##     label:    String   # "3 idle workers"      one-line summary
##     detail:   String   # "Band 2"              secondary context
##     x: int, y: int     # map focus for the jump; (-1, -1) = non-locating
##   }
##
## Palette comes entirely from HudStyle (no hardcoded hexes).

const HudStyle := preload("res://src/scripts/ui/HudStyle.gd")

## Jump the camera to (x, y) — reuses the Alerts-panel focus wiring in Hud/Main.
signal focus_requested(x: int, y: int)
## Advance the turn (the popover footer's Advance button).
signal advance_requested

# ---- severity model --------------------------------------------------------
const SEVERITY_INFO := "info"
const SEVERITY_WARN := "warn"
const SEVERITY_CRITICAL := "critical"
const SEVERITY_RANK := {SEVERITY_CRITICAL: 3, SEVERITY_WARN: 2, SEVERITY_INFO: 1}

const KIND_IDLE_WORKERS := "idle_workers"
const KIND_ICON := {KIND_IDLE_WORKERS: "🛠"}
const KIND_ICON_FALLBACK := "●"

# ---- geometry (named constants; no magic literals) -------------------------
const CLUSTER_WIDTH := 260.0
const CLUSTER_HEIGHT := 128.0
const CAPTION_GAP := 12
# The cluster is the last, right-flush BottomBar child, sitting on the window's
# bottom-right corner. Inset the orb + caption from those edges so the full ring
# and count badge stay on-screen with a comfortable margin.
const EDGE_MARGIN_RIGHT := 16
const EDGE_MARGIN_TOP := 14
const EDGE_MARGIN_BOTTOM := 14
const ORB_DIAMETER := 100.0
const FACE_DIAMETER := 74.0
const FACE_BORDER_WIDTH := 2
const RING_RADIUS := 47.0
const RING_WIDTH := 2.0
const RING_SEGMENTS := 64
const GLYPH := "‣‣"
const GLYPH_FONT_SIZE := 26

# Calm pulse (only while the registry is empty).
const PULSE_PERIOD := 2.6            # seconds for a full breath
const PULSE_ALPHA_MIN := 0.30
const PULSE_ALPHA_MAX := 0.85
const PULSE_RADIUS_MIN := 44.0
const PULSE_RADIUS_MAX := 47.0
const PULSE_WIDTH := 1.5
const PULSE_DASH_COUNT := 22
const PULSE_DASH_FRACTION := 0.42    # portion of each dash slot that's stroked
const PULSE_ARC_SEGMENTS := 4

# Count badge (only while the registry is non-empty).
const BADGE_RADIUS := 13.0
const BADGE_INSET := 3.0
const BADGE_FONT_SIZE := 13

# Popover.
const POPOVER_WIDTH := 320
const POPOVER_GAP := 14.0
const ROW_MIN_HEIGHT := 52.0
const ROW_H_PADDING := 12
const ROW_SEPARATION := 12
const SEV_STRIPE_WIDTH := 3
const ROW_ICON_SIZE := 30

var _entries: Array = []
var _accent_color: Color = HudStyle.SIGNAL
var _turn: int = 0
var _pulse_time: float = 0.0

var _layout: HBoxContainer
var _caption: VBoxContainer
var _turn_label: Label
var _status_label: Label
var _orb_area: Control
var _face: Button

var _popover: PanelContainer = null
var _catcher: Control = null
var _popover_open: bool = false


func _ready() -> void:
	custom_minimum_size = Vector2(CLUSTER_WIDTH, CLUSTER_HEIGHT)
	# Fill the bottom bar's height so the orb can center within it (the bar grows to
	# the tallest corner widget); the edge margins below keep it off the window edges.
	size_flags_vertical = Control.SIZE_FILL
	mouse_filter = Control.MOUSE_FILTER_IGNORE

	_layout = HBoxContainer.new()
	_layout.set_anchors_preset(Control.PRESET_FULL_RECT)
	_layout.offset_top = EDGE_MARGIN_TOP
	_layout.offset_bottom = -EDGE_MARGIN_BOTTOM
	_layout.offset_right = -EDGE_MARGIN_RIGHT
	_layout.alignment = BoxContainer.ALIGNMENT_END
	_layout.add_theme_constant_override("separation", CAPTION_GAP)
	_layout.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_layout)

	_caption = VBoxContainer.new()
	_caption.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_caption.add_theme_constant_override("separation", 2)
	_caption.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_turn_label = Label.new()
	_turn_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	_turn_label.add_theme_color_override("font_color", HudStyle.INK)
	_status_label = Label.new()
	_status_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	_caption.add_child(_turn_label)
	_caption.add_child(_status_label)
	_layout.add_child(_caption)

	_orb_area = Control.new()
	_orb_area.custom_minimum_size = Vector2(ORB_DIAMETER, ORB_DIAMETER)
	_orb_area.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_orb_area.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_orb_area.draw.connect(_on_orb_area_draw)
	_orb_area.resized.connect(_position_face)
	_layout.add_child(_orb_area)

	_face = Button.new()
	_face.text = GLYPH
	_face.focus_mode = Control.FOCUS_NONE
	_face.custom_minimum_size = Vector2(FACE_DIAMETER, FACE_DIAMETER)
	_face.size = Vector2(FACE_DIAMETER, FACE_DIAMETER)
	_face.add_theme_font_size_override("font_size", GLYPH_FONT_SIZE)
	_face.pressed.connect(toggle_popover)
	_orb_area.add_child(_face)
	_position_face()

	set_turn(_turn)
	_recompute()

func _process(delta: float) -> void:
	# Only animate (and redraw) while the calm pulse is active — i.e. nothing
	# needs the player. When there are entries, _recompute() stops processing.
	_pulse_time += delta
	_orb_area.queue_redraw()

func _position_face() -> void:
	if _face == null or _orb_area == null:
		return
	_face.position = (_orb_area.size - Vector2(FACE_DIAMETER, FACE_DIAMETER)) * 0.5

# ---- public API ------------------------------------------------------------

## Replace the attention registry, recompute ready/badge/tint, restart or stop
## the pulse, and (if open) rebuild the popover.
func set_attention(entries: Array) -> void:
	_entries = entries.duplicate(true)
	_entries.sort_custom(_sort_by_severity_desc)
	_recompute()
	if _popover_open:
		_rebuild_popover()

func set_turn(turn: int) -> void:
	_turn = turn
	if _turn_label != null:
		_turn_label.text = "Turn %d" % turn

## Open the popover programmatically (used by the ui_preview harness).
func open_popover() -> void:
	if not _popover_open:
		toggle_popover()

func toggle_popover() -> void:
	if _popover_open:
		_close_popover()
	else:
		_open_popover()

# ---- recompute + draw ------------------------------------------------------

func _sort_by_severity_desc(a: Variant, b: Variant) -> bool:
	return _rank(a) > _rank(b)

func _rank(entry: Variant) -> int:
	if entry is Dictionary:
		return int(SEVERITY_RANK.get(String(entry.get("severity", SEVERITY_INFO)), 0))
	return 0

func _severity_color(severity: String) -> Color:
	match severity:
		SEVERITY_CRITICAL:
			return HudStyle.DANGER
		SEVERITY_WARN:
			return HudStyle.WARN
		_:
			return HudStyle.SIGNAL

func _highest_severity_color() -> Color:
	var best_rank := 0
	var color := HudStyle.SIGNAL
	for entry in _entries:
		var r := _rank(entry)
		if r > best_rank:
			best_rank = r
			color = _severity_color(String(entry.get("severity", SEVERITY_INFO)))
	return color

func _kind_icon(kind: String) -> String:
	return String(KIND_ICON.get(kind, KIND_ICON_FALLBACK))

func _recompute() -> void:
	var ready := _entries.is_empty()
	_accent_color = HudStyle.SIGNAL if ready else _highest_severity_color()
	_style_face(_accent_color)
	if ready:
		_status_label.text = "▸ all clear"
		_status_label.add_theme_color_override("font_color", HudStyle.HEALTHY)
	else:
		var n := _entries.size()
		var noun := "item needs you" if n == 1 else "items need you"
		_status_label.text = "%d %s" % [n, noun]
		_status_label.add_theme_color_override("font_color", _accent_color)
	# The pulse only breathes while all-clear; otherwise the badge tells the story.
	set_process(ready)
	if _orb_area != null:
		_orb_area.queue_redraw()

func _style_face(accent: Color) -> void:
	if _face == null:
		return
	var normal := StyleBoxFlat.new()
	normal.bg_color = HudStyle.PANEL_SOLID
	normal.set_corner_radius_all(int(FACE_DIAMETER * 0.5))
	normal.set_border_width_all(FACE_BORDER_WIDTH)
	normal.border_color = accent
	# Subtle cyan inner glow: a faint signal-wash highlight sitting inside the face.
	normal.shadow_color = HudStyle.SIGNAL_WASH
	normal.shadow_size = 4
	var hover := normal.duplicate() as StyleBoxFlat
	hover.border_color = HudStyle.SIGNAL if accent == HudStyle.SIGNAL else accent
	hover.shadow_size = 8
	_face.add_theme_stylebox_override("normal", normal)
	_face.add_theme_stylebox_override("hover", hover)
	_face.add_theme_stylebox_override("pressed", hover)
	_face.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
	_face.add_theme_color_override("font_color", accent)
	_face.add_theme_color_override("font_hover_color", accent)
	_face.add_theme_color_override("font_pressed_color", accent)

func _on_orb_area_draw() -> void:
	var center := _orb_area.size * 0.5
	# Static base ring behind the face.
	_orb_area.draw_arc(center, RING_RADIUS, 0.0, TAU, RING_SEGMENTS, HudStyle.LINE_SOFT, RING_WIDTH, true)
	if _entries.is_empty():
		_draw_pulse(center)
	else:
		_draw_badge()

func _draw_pulse(center: Vector2) -> void:
	# 0..1 breath from a cosine so it eases at both ends.
	var t := 0.5 - 0.5 * cos(_pulse_time * TAU / PULSE_PERIOD)
	var col := HudStyle.SIGNAL
	col.a = lerpf(PULSE_ALPHA_MIN, PULSE_ALPHA_MAX, t)
	var radius := lerpf(PULSE_RADIUS_MIN, PULSE_RADIUS_MAX, t)
	var span := TAU / float(PULSE_DASH_COUNT)
	for i in PULSE_DASH_COUNT:
		var a0 := float(i) * span
		_orb_area.draw_arc(center, radius, a0, a0 + span * PULSE_DASH_FRACTION, PULSE_ARC_SEGMENTS, col, PULSE_WIDTH, true)

func _draw_badge() -> void:
	var badge_center := Vector2(_orb_area.size.x - BADGE_RADIUS - BADGE_INSET, BADGE_RADIUS + BADGE_INSET)
	_orb_area.draw_circle(badge_center, BADGE_RADIUS, _accent_color)
	var font := _orb_area.get_theme_default_font()
	if font == null:
		return
	var text := str(_entries.size())
	var text_size := font.get_string_size(text, HORIZONTAL_ALIGNMENT_CENTER, -1, BADGE_FONT_SIZE)
	var origin := badge_center + Vector2(-text_size.x * 0.5, BADGE_FONT_SIZE * 0.35)
	_orb_area.draw_string(font, origin, text, HORIZONTAL_ALIGNMENT_LEFT, -1, BADGE_FONT_SIZE, HudStyle.GROUND)

# ---- popover ---------------------------------------------------------------

func _open_popover() -> void:
	_close_popover()
	# Full-screen catcher so a click anywhere outside the popover dismisses it.
	_catcher = Control.new()
	_catcher.top_level = true
	_catcher.mouse_filter = Control.MOUSE_FILTER_STOP
	_catcher.global_position = Vector2.ZERO
	_catcher.size = get_viewport_rect().size
	_catcher.gui_input.connect(_on_catcher_input)
	add_child(_catcher)

	_popover = _build_popover()
	_popover.top_level = true
	_popover.resized.connect(_position_popover)
	add_child(_popover)
	_popover_open = true
	_position_popover()

func _close_popover() -> void:
	if _popover != null:
		_popover.queue_free()
		_popover = null
	if _catcher != null:
		_catcher.queue_free()
		_catcher = null
	_popover_open = false

func _on_catcher_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed:
		_close_popover()

func _position_popover() -> void:
	if _popover == null or _orb_area == null:
		return
	var orb_rect := _orb_area.get_global_rect()
	var pw := _popover.size.x
	var ph := _popover.size.y
	_popover.global_position = Vector2(orb_rect.end.x - pw, orb_rect.position.y - ph - POPOVER_GAP)

func _build_popover() -> PanelContainer:
	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
	var body := VBoxContainer.new()
	body.custom_minimum_size = Vector2(POPOVER_WIDTH, 0)
	body.add_theme_constant_override("separation", 0)
	panel.add_child(body)

	if _entries.is_empty():
		body.add_child(_popover_header("Nothing pending", ""))
		body.add_child(_all_clear_block())
	else:
		var n := _entries.size()
		body.add_child(_popover_header("Needs your attention", "%d item%s" % [n, "" if n == 1 else "s"]))
		for entry in _entries:
			body.add_child(_reason_row(entry))
	body.add_child(_popover_footer())
	return panel

func _popover_header(title: String, count_text: String) -> Control:
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 8)
	var title_label := Label.new()
	title_label.text = title.to_upper()
	title_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	title_label.add_theme_color_override("font_color", HudStyle.INK_DIM)
	header.add_child(title_label)
	if count_text != "":
		var count_label := Label.new()
		count_label.text = count_text
		count_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
		header.add_child(count_label)
	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", ROW_H_PADDING)
	margin.add_theme_constant_override("margin_right", ROW_H_PADDING)
	margin.add_theme_constant_override("margin_top", 4)
	margin.add_theme_constant_override("margin_bottom", 8)
	margin.add_child(header)
	return margin

func _all_clear_block() -> Control:
	var box := VBoxContainer.new()
	box.alignment = BoxContainer.ALIGNMENT_CENTER
	box.add_theme_constant_override("separation", 3)
	var glyph := Label.new()
	glyph.text = "✓"
	glyph.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	glyph.add_theme_font_size_override("font_size", 26)
	glyph.add_theme_color_override("font_color", HudStyle.HEALTHY)
	var title := Label.new()
	title.text = "All clear"
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	title.add_theme_color_override("font_color", HudStyle.INK)
	var sub := Label.new()
	sub.text = "Every band is working and no decision awaits. Advance the turn."
	sub.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	sub.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	sub.add_theme_color_override("font_color", HudStyle.INK_DIM)
	box.add_child(glyph)
	box.add_child(title)
	box.add_child(sub)
	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 20)
	margin.add_theme_constant_override("margin_right", 20)
	margin.add_theme_constant_override("margin_top", 18)
	margin.add_theme_constant_override("margin_bottom", 18)
	margin.add_child(box)
	return margin

func _reason_row(entry: Variant) -> Button:
	var severity := String(entry.get("severity", SEVERITY_INFO))
	var color := _severity_color(severity)
	var x := int(entry.get("x", -1))
	var y := int(entry.get("y", -1))
	var locates := x >= 0 and y >= 0

	var button := Button.new()
	button.focus_mode = Control.FOCUS_NONE
	button.custom_minimum_size = Vector2(0, ROW_MIN_HEIGHT)
	HudStyle.apply_button(button, "ghost")

	var row := HBoxContainer.new()
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.set_anchors_preset(Control.PRESET_FULL_RECT)
	row.offset_left = ROW_H_PADDING
	row.offset_right = -ROW_H_PADDING
	row.add_theme_constant_override("separation", ROW_SEPARATION)

	var stripe := ColorRect.new()
	stripe.custom_minimum_size = Vector2(SEV_STRIPE_WIDTH, 0)
	stripe.color = color
	stripe.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(stripe)

	var icon := Label.new()
	icon.text = _kind_icon(String(entry.get("kind", "")))
	icon.custom_minimum_size = Vector2(ROW_ICON_SIZE, ROW_ICON_SIZE)
	icon.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	icon.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	icon.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	icon.add_theme_color_override("font_color", color)
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(icon)

	var text_box := VBoxContainer.new()
	text_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	text_box.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	text_box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	text_box.add_theme_constant_override("separation", 1)
	var label := Label.new()
	label.text = String(entry.get("label", ""))
	label.add_theme_color_override("font_color", HudStyle.INK)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var detail := Label.new()
	detail.text = String(entry.get("detail", ""))
	detail.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	detail.mouse_filter = Control.MOUSE_FILTER_IGNORE
	text_box.add_child(label)
	text_box.add_child(detail)
	row.add_child(text_box)

	var jump := Label.new()
	jump.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	jump.mouse_filter = Control.MOUSE_FILTER_IGNORE
	if locates:
		jump.text = "Jump →"
		jump.add_theme_color_override("font_color", HudStyle.SIGNAL)
	else:
		jump.text = "Open ▸"
		jump.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	row.add_child(jump)

	button.add_child(row)
	button.pressed.connect(_on_reason_pressed.bind(x, y, locates))
	return button

func _popover_footer() -> Control:
	var advance := Button.new()
	advance.text = "Advance ▸"
	advance.focus_mode = Control.FOCUS_NONE
	advance.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	HudStyle.apply_button(advance, "primary")
	advance.pressed.connect(_on_advance_pressed)
	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", ROW_H_PADDING)
	margin.add_theme_constant_override("margin_right", ROW_H_PADDING)
	margin.add_theme_constant_override("margin_top", 8)
	margin.add_theme_constant_override("margin_bottom", 8)
	margin.add_child(advance)
	return margin

func _on_reason_pressed(x: int, y: int, locates: bool) -> void:
	if locates:
		emit_signal("focus_requested", x, y)
		_close_popover()
	# Non-locating entries are a no-op stub for now (only idle_workers exists, and
	# it always locates). Future non-locating kinds (decisions) open a panel here.

func _on_advance_pressed() -> void:
	emit_signal("advance_requested")
	_close_popover()

func _rebuild_popover() -> void:
	if not _popover_open:
		return
	if _popover != null:
		_popover.queue_free()
	_popover = _build_popover()
	_popover.top_level = true
	_popover.resized.connect(_position_popover)
	add_child(_popover)
	_position_popover()
