extends Control
class_name MenuShell

## The one shared menu surface (DRY). A single registry-driven shell renders BOTH the
## boot landing screen and the in-game ESC pause menu; the only thing that differs is
## `mode` ("landing" | "pause"), which re-filters the nav and re-lays-out the frame.
##
## - landing: full-bleed, no scrim (the owning LandingScreen paints the ground).
## - pause: a centered card floating over a dark scrim.
##
## Styled entirely through `HudStyle` so it matches the in-game command console. Functional
## items (New Game, Resume, Abandon, Exit) emit signals the owner acts on; the rest
## (Map Selection, Load, Save, Options) render inert placeholder panes.
##
## Signals out — the owner (LandingScreen / Main's PauseLayer) wires these:
##   new_game_requested(preset_id, width, height, seed, profile_id)
##   resume_requested / abandon_requested / exit_requested

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")
const MapSizes = preload("res://src/scripts/MapSizes.gd")

signal new_game_requested(preset_id: String, width: int, height: int, seed: int, profile_id: String)
signal resume_requested
signal abandon_requested
signal exit_requested

const LANDING := "landing"
const PAUSE := "pause"

## The only shipped start profile — the seed the New Game command carries.
const DEFAULT_PROFILE_ID := "late_forager_tribe"

# ---- registry: ONE list drives both surfaces; `modes` is the only per-surface difference ----
const ITEMS := [
	{"id": "resume", "label": "Resume", "hint": "ESC", "modes": ["pause"], "danger": false},
	{"id": "new_game", "label": "New Game", "hint": "N", "modes": ["landing"], "danger": false},
	{"id": "map_selection", "label": "Map Selection", "hint": "M", "modes": ["landing", "pause"], "danger": false},
	{"id": "load", "label": "Load Game", "hint": "L", "modes": ["landing", "pause"], "danger": false},
	{"id": "save", "label": "Save Game", "hint": "S", "modes": ["pause"], "danger": false},
	{"id": "options", "label": "Options", "hint": "O", "modes": ["landing", "pause"], "danger": false},
	{"id": "abandon", "label": "Abandon Run", "hint": "", "modes": ["pause"], "danger": true},
	{"id": "exit", "label": "Exit to Desktop", "hint": "", "modes": ["landing"], "danger": true},
]

# The two real world presets (mirrors core_sim/src/data/map_presets.json); blurbs copied from
# the shell prototype. `pinned` presets carry their own seed server-side, so the seed field is
# advisory for them (surfaced as "pinned" in the summary).
const PRESETS := [
	{
		"id": "earthlike",
		"name": "Earthlike (Oceans & Continents)",
		"seed_policy": "uses run seed",
		"pinned": false,
		"blurb": "Large oceans with continental shelves, 2–5 major landmasses, moderate-to-high river density, temperate/tropical mix with polar caps.",
	},
	{
		"id": "polar_contrast",
		"name": "Polar Microplate Contrast",
		"seed_policy": "seed pinned by preset",
		"pinned": true,
		"blurb": "Fragmented microplates under a hard climate gradient. Narrow habitable band, heavy ice at both caps, scarce river networks.",
	},
]

# Static fixtures for the placeholder Load/Save panes (display-only; wiring is server-side work).
const SAVE_SLOTS := [
	{"who": "Trail Sovereigns", "meta": "Turn 47 · Late Thaw · 3 bands, 94 souls", "when": "12 min ago", "auto": true},
	{"who": "Trail Sovereigns", "meta": "Turn 31 · First Frost · 2 bands, 61 souls", "when": "Yesterday 22:14", "auto": false},
	{"who": "Trail Sovereigns", "meta": "Turn 12 · High Sun · 1 band, 30 souls", "when": "18 Jul, 09:02", "auto": false},
	{"empty": true},
]

# ---- layout constants (named; no bare literals) ----
const LANDING_PAD_X := 72.0
const LANDING_PAD_Y := 56.0
# Cap the landing shell width so the setup pane doesn't stretch edge-to-edge on
# an ultra-wide monitor (rail + gap + ~780px pane + shell padding). Left-aligned;
# shrinks below this on narrower windows.
const LANDING_MAX_WIDTH := 1180.0
const PAUSE_OUTER_MARGIN := 40.0
const PAUSE_MAX_WIDTH := 960.0
const PAUSE_MIN_WIDTH := 520.0
const PAUSE_MAX_HEIGHT := 720.0
const RAIL_WIDTH_LANDING := 330.0
const RAIL_WIDTH_PAUSE := 250.0
const COLUMN_GAP := 40
const RAIL_GAP := 34
const NAV_GAP := 3
const SHELL_PAD := 26
const CARD_RADIUS := 10
const CTRL_RADIUS := 7
const NAV_PAD_X := 13
const NAV_PAD_Y := 10
const CARD_PAD := 13
const SEED_FIELD_MIN_WIDTH := 160.0
const SEED_MAX_LENGTH := 12

# ---- font sizes ----
const TITLE_SIZE_LANDING := 44
const TITLE_SIZE_PAUSE := 26
const SUBTITLE_SIZE := 13
const CAMPAIGN_SIZE := 12
const PANE_TITLE_SIZE := 16
const EYEBROW_SIZE := 11
const BODY_SIZE := 14
const NAV_LABEL_SIZE := 17
const HINT_SIZE := 11
const CARD_NAME_SIZE := 14
const CARD_BLURB_SIZE := 13
const SIZE_NAME_SIZE := 13
const SIZE_DIM_SIZE := 11
const SUMMARY_KEY_SIZE := 11
const SUMMARY_VAL_SIZE := 12
const NOTE_SIZE := 11

# The masthead title tone — warm parchment, the one place the dark console admits a light accent
# (mirrors the prototype's --parchment / #f2e6bf). Not in HudStyle because nothing else uses it.
const TITLE_COLOR := Color(0.949, 0.902, 0.749, 1.0)

@export var mode: String = LANDING:
	set = set_mode

var _built := false

# structural nodes
var _scrim: ColorRect
var _shell: PanelContainer
var _columns: HBoxContainer
var _rail: VBoxContainer
var _pane_panel: PanelContainer
var _pane_body: VBoxContainer
var _title_label: Label
var _nav_box: VBoxContainer

# selection state (shared across setup/map_selection panes, like the prototype)
var _active_pane := "new_game"
var _selected_preset := "earthlike"
var _selected_size := MapSizes.DEFAULT_KEY
var _seed_edit: LineEdit
var _summary_box: HBoxContainer
var _nav_rows := {}   # id -> {row, item, hover}


func set_mode(value: String) -> void:
	mode = value
	if _built:
		_apply_mode()


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_build()
	_built = true
	resized.connect(_on_resized)
	_apply_mode()


func _build() -> void:
	_scrim = ColorRect.new()
	_scrim.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_scrim.color = Color(HudStyle.GROUND.r, HudStyle.GROUND.g, HudStyle.GROUND.b, 0.82)
	_scrim.mouse_filter = Control.MOUSE_FILTER_STOP  # swallow clicks behind the pause card
	add_child(_scrim)

	_shell = PanelContainer.new()
	add_child(_shell)

	var shell_margin := MarginContainer.new()
	shell_margin.add_theme_constant_override("margin_left", SHELL_PAD)
	shell_margin.add_theme_constant_override("margin_right", SHELL_PAD)
	shell_margin.add_theme_constant_override("margin_top", SHELL_PAD)
	shell_margin.add_theme_constant_override("margin_bottom", SHELL_PAD)
	_shell.add_child(shell_margin)

	_columns = HBoxContainer.new()
	_columns.add_theme_constant_override("separation", COLUMN_GAP)
	shell_margin.add_child(_columns)

	_rail = VBoxContainer.new()
	_rail.add_theme_constant_override("separation", RAIL_GAP)
	_rail.size_flags_vertical = Control.SIZE_FILL
	_columns.add_child(_rail)
	_build_rail()

	_pane_panel = PanelContainer.new()
	_pane_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_pane_panel.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_columns.add_child(_pane_panel)

	var pane_scroll := ScrollContainer.new()
	pane_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	pane_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	pane_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_pane_panel.add_child(pane_scroll)

	_pane_body = VBoxContainer.new()
	_pane_body.add_theme_constant_override("separation", 14)
	_pane_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	pane_scroll.add_child(_pane_body)


func _build_rail() -> void:
	var wordmark := VBoxContainer.new()
	wordmark.add_theme_constant_override("separation", 8)
	_rail.add_child(wordmark)

	_title_label = Label.new()
	_title_label.text = "Shadow & Scale"
	_title_label.add_theme_color_override("font_color", TITLE_COLOR)
	_title_label.add_theme_font_size_override("font_size", TITLE_SIZE_LANDING)
	wordmark.add_child(_title_label)

	var subtitle := Label.new()
	subtitle.text = "TRAIL  SOVEREIGNS"
	subtitle.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	subtitle.add_theme_font_size_override("font_size", SUBTITLE_SIZE)
	wordmark.add_child(subtitle)

	var campaign := Label.new()
	campaign.text = "Nomadic strategy saga"
	campaign.add_theme_color_override("font_color", HudStyle.INK_DIM)
	campaign.add_theme_font_size_override("font_size", CAMPAIGN_SIZE)
	wordmark.add_child(campaign)

	_nav_box = VBoxContainer.new()
	_nav_box.add_theme_constant_override("separation", NAV_GAP)
	_rail.add_child(_nav_box)

	var spacer := Control.new()
	spacer.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_rail.add_child(spacer)

	var footer := Label.new()
	footer.text = "Shadow & Scale — thin client"
	footer.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	footer.add_theme_font_size_override("font_size", HINT_SIZE)
	_rail.add_child(footer)


# ---- mode application -------------------------------------------------------
func _apply_mode() -> void:
	var is_pause := mode == PAUSE
	_scrim.visible = is_pause
	_title_label.add_theme_font_size_override(
		"font_size", TITLE_SIZE_PAUSE if is_pause else TITLE_SIZE_LANDING
	)
	_rail.custom_minimum_size.x = RAIL_WIDTH_PAUSE if is_pause else RAIL_WIDTH_LANDING
	if is_pause:
		_shell.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
		_pane_panel.add_theme_stylebox_override("panel", HudStyle.empty_stylebox())
	else:
		_shell.add_theme_stylebox_override("panel", HudStyle.empty_stylebox())
		_pane_panel.add_theme_stylebox_override("panel", _card_stylebox())
	if not _pane_visible_in_mode(_active_pane):
		_active_pane = "resume" if is_pause else "new_game"
	_apply_shell_layout()
	_rebuild_nav()
	_show_pane(_active_pane)


func _on_resized() -> void:
	_apply_shell_layout()


func _apply_shell_layout() -> void:
	if _shell == null:
		return
	var sz := size
	_shell.anchor_left = 0.0
	_shell.anchor_top = 0.0
	_shell.anchor_right = 0.0
	_shell.anchor_bottom = 0.0
	if mode == PAUSE:
		var avail_w: float = max(PAUSE_MIN_WIDTH, sz.x - 2.0 * PAUSE_OUTER_MARGIN)
		var w: float = min(PAUSE_MAX_WIDTH, avail_w)
		var h: float = min(PAUSE_MAX_HEIGHT, sz.y - 2.0 * PAUSE_OUTER_MARGIN)
		_shell.position = Vector2((sz.x - w) * 0.5, (sz.y - h) * 0.5)
		_shell.size = Vector2(w, h)
	else:
		var landing_w: float = min(LANDING_MAX_WIDTH, max(0.0, sz.x - 2.0 * LANDING_PAD_X))
		_shell.position = Vector2(max(LANDING_PAD_X, (sz.x - landing_w) * 0.5), LANDING_PAD_Y)
		_shell.size = Vector2(
			landing_w,
			max(0.0, sz.y - 2.0 * LANDING_PAD_Y)
		)


func _pane_visible_in_mode(pane_id: String) -> bool:
	for item in ITEMS:
		if String(item["id"]) == pane_id:
			return mode in item["modes"]
	return false


# ---- nav --------------------------------------------------------------------
func _rebuild_nav() -> void:
	for child in _nav_box.get_children():
		child.queue_free()
	_nav_rows.clear()
	for item in ITEMS:
		if not (mode in item["modes"]):
			continue
		_nav_box.add_child(_make_nav_row(item))
	_refresh_nav_active()


func _make_nav_row(item: Dictionary) -> PanelContainer:
	var row := PanelContainer.new()
	row.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	var hb := HBoxContainer.new()
	hb.mouse_filter = Control.MOUSE_FILTER_IGNORE
	hb.add_theme_constant_override("separation", 12)
	row.add_child(hb)

	var tick := Label.new()
	tick.name = "Tick"
	tick.text = "▸"
	tick.add_theme_font_size_override("font_size", HINT_SIZE)
	tick.add_theme_color_override("font_color", Color(0, 0, 0, 0))
	hb.add_child(tick)

	var label := Label.new()
	label.text = String(item["label"])
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.add_theme_font_size_override("font_size", NAV_LABEL_SIZE)
	label.add_theme_color_override(
		"font_color", HudStyle.DANGER if bool(item["danger"]) else HudStyle.INK_DIM
	)
	hb.add_child(label)

	var hint := Label.new()
	hint.text = String(item["hint"])
	hint.add_theme_font_size_override("font_size", HINT_SIZE)
	hint.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	hb.add_child(hint)

	row.gui_input.connect(_on_nav_input.bind(item))
	row.mouse_entered.connect(_on_nav_hover.bind(item, true))
	row.mouse_exited.connect(_on_nav_hover.bind(item, false))
	_nav_rows[String(item["id"])] = {"row": row, "item": item, "hover": false}
	return row


func _on_nav_input(event: InputEvent, item: Dictionary) -> void:
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		_activate_item(String(item["id"]))


func _on_nav_hover(item: Dictionary, entered: bool) -> void:
	var record: Variant = _nav_rows.get(String(item["id"]), null)
	if record is Dictionary:
		(record as Dictionary)["hover"] = entered
	_refresh_nav_active()


func _activate_item(id: String) -> void:
	if id == "resume":
		emit_signal("resume_requested")
		return
	_active_pane = id
	_refresh_nav_active()
	_show_pane(id)


func _refresh_nav_active() -> void:
	for id in _nav_rows:
		var record: Dictionary = _nav_rows[id]
		var row: PanelContainer = record["row"]
		var item: Dictionary = record["item"]
		var active := String(id) == _active_pane
		var hover := bool(record.get("hover", false))
		row.add_theme_stylebox_override(
			"panel", _nav_stylebox(active, hover, bool(item["danger"]))
		)
		var tick: Label = row.get_node("HBoxContainer/Tick") if row.has_node("HBoxContainer/Tick") else null
		if tick == null:
			# HBox is unnamed; fetch the first Label child's sibling reliably.
			var hb := row.get_child(0)
			tick = hb.get_node("Tick")
		tick.add_theme_color_override(
			"font_color", HudStyle.SIGNAL if active else Color(0, 0, 0, 0)
		)


func _nav_stylebox(active: bool, hover: bool, danger: bool) -> StyleBox:
	if not active and not hover:
		var empty := StyleBoxFlat.new()
		empty.draw_center = false
		empty.set_corner_radius_all(CTRL_RADIUS)
		_pad_stylebox(empty, NAV_PAD_X, NAV_PAD_Y)
		return empty
	var sb := StyleBoxFlat.new()
	sb.set_corner_radius_all(CTRL_RADIUS)
	sb.set_border_width_all(1)
	_pad_stylebox(sb, NAV_PAD_X, NAV_PAD_Y)
	if active:
		sb.bg_color = HudStyle.SIGNAL_WASH
		sb.border_color = HudStyle.SIGNAL_DEEP
	else:  # hover
		sb.bg_color = Color(0.075, 0.129, 0.122, 1.0)
		sb.border_color = HudStyle.DANGER if danger else HudStyle.SIGNAL_DEEP
	return sb


# ---- panes ------------------------------------------------------------------
func _show_pane(pane_id: String) -> void:
	for child in _pane_body.get_children():
		child.queue_free()
	match pane_id:
		"new_game":
			_build_setup_pane()
		"map_selection":
			_build_map_selection_pane()
		"load":
			_build_saves_pane("Load Game", "Saved runs", false)
		"save":
			_build_saves_pane("Save Game", "Turn 47 · Late Thaw", true)
		"options":
			_build_options_pane()
		"abandon":
			_build_abandon_pane()
		"exit":
			_build_exit_pane()
		"resume":
			pass  # empty pane — the nav Resume row is the whole affordance


func _build_setup_pane() -> void:
	_add_pane_header("New Game", "Campaign setup")
	_add_paragraph("Every run begins with one scout band of thirty on unequal land. What the land gives is what you get — choose where that argument starts.")

	_add_field_label("World preset")
	for preset in PRESETS:
		_pane_body.add_child(_make_preset_card(preset))

	_add_field_label("Map size")
	_pane_body.add_child(_make_size_row())

	_add_field_label("Seed")
	_seed_edit = LineEdit.new()
	_seed_edit.text = "0"
	_seed_edit.max_length = SEED_MAX_LENGTH
	_seed_edit.custom_minimum_size.x = SEED_FIELD_MIN_WIDTH
	_seed_edit.size_flags_horizontal = Control.SIZE_FILL
	_style_line_edit(_seed_edit)
	_seed_edit.text_changed.connect(func(_t): _refresh_summary())
	_pane_body.add_child(_seed_edit)
	_add_note("0 = derive from clock")

	_summary_box = HBoxContainer.new()
	_summary_box.add_theme_constant_override("separation", 22)
	var summary_panel := PanelContainer.new()
	summary_panel.add_theme_stylebox_override("panel", _summary_stylebox())
	summary_panel.add_child(_summary_box)
	_pane_body.add_child(summary_panel)
	_refresh_summary()

	var actions := _make_actions_row()
	var begin := Button.new()
	begin.text = "Begin the trail"
	HudStyle.apply_button(begin, "primary")
	begin.pressed.connect(_on_begin_pressed)
	actions.add_child(begin)

	var preview := Button.new()
	preview.text = "Preview map"
	HudStyle.apply_button(preview, "ghost")
	preview.pressed.connect(_activate_item.bind("map_selection"))
	actions.add_child(preview)
	_pane_body.add_child(actions)


func _build_map_selection_pane() -> void:
	_add_pane_header("Map Selection", "World preset")
	_add_paragraph("Presets shape the generating inputs — sea level, continent scale, river density — and let the world fall out of them. Nothing here paints a result directly.")
	for preset in PRESETS:
		_pane_body.add_child(_make_preset_card(preset))
	var actions := _make_actions_row()
	var regen := Button.new()
	regen.text = "Regenerate world"
	HudStyle.apply_button(regen, "primary")
	regen.disabled = true  # inert placeholder — regeneration is server-side work
	actions.add_child(regen)
	var back := Button.new()
	back.text = "Back to setup"
	HudStyle.apply_button(back, "ghost")
	back.pressed.connect(_activate_item.bind("new_game"))
	actions.add_child(back)
	_pane_body.add_child(actions)


func _build_saves_pane(title: String, eyebrow: String, is_save: bool) -> void:
	_add_pane_header(title, eyebrow)
	if is_save:
		_add_paragraph("Write the current run to a slot. The autosave slot is rewritten at the end of every turn and cannot be overwritten by hand.")
	for slot in SAVE_SLOTS:
		_pane_body.add_child(_make_slot_row(slot))
	var actions := _make_actions_row()
	var primary := Button.new()
	primary.text = "Save to slot" if is_save else "Load selected"
	HudStyle.apply_button(primary, "primary")
	primary.disabled = true  # inert placeholder — save/load is server-side work
	actions.add_child(primary)
	if not is_save:
		var del := Button.new()
		del.text = "Delete"
		HudStyle.apply_button(del, "armed")
		del.disabled = true
		actions.add_child(del)
	_pane_body.add_child(actions)


func _build_options_pane() -> void:
	_add_pane_header("Options", "Client settings")
	_add_paragraph("Client settings are not wired yet — this pane is a placeholder for interface scale, terrain rendering, and the sim endpoint.")
	var actions := _make_actions_row()
	var apply := Button.new()
	apply.text = "Apply"
	HudStyle.apply_button(apply, "primary")
	apply.disabled = true
	actions.add_child(apply)
	var restore := Button.new()
	restore.text = "Restore defaults"
	HudStyle.apply_button(restore, "ghost")
	restore.disabled = true
	actions.add_child(restore)
	_pane_body.add_child(actions)


func _build_abandon_pane() -> void:
	_add_pane_header("Abandon Run", "Return to the main menu")
	_add_paragraph("Leaving now returns you to the main menu, where a new run can be started. The current run is not saved.")
	var actions := _make_actions_row()
	var abandon := Button.new()
	abandon.text = "Abandon and return to menu"
	HudStyle.apply_button(abandon, "armed")
	abandon.pressed.connect(func(): emit_signal("abandon_requested"))
	actions.add_child(abandon)
	_pane_body.add_child(actions)


func _build_exit_pane() -> void:
	_add_pane_header("Exit to Desktop", "Close the client")
	_add_paragraph("The simulation server keeps running in the background. Restart the client to reconnect to the same world.")
	var actions := _make_actions_row()
	var quit := Button.new()
	quit.text = "Quit"
	HudStyle.apply_button(quit, "armed")
	quit.pressed.connect(func(): emit_signal("exit_requested"))
	actions.add_child(quit)
	_pane_body.add_child(actions)


## The seed entered in the New Game field, clamped to a non-negative value — the single read
## point for the seed. The server parses the seed as a u64, so a negative seed fails the parse
## and the world never generates (the client is stranded on the loading overlay); 0 still means
## "derive from the run clock".
func _seed_value() -> int:
	if _seed_edit == null or not _seed_edit.text.strip_edges().is_valid_int():
		return 0
	return maxi(0, int(_seed_edit.text.strip_edges()))


func _on_begin_pressed() -> void:
	var dims := MapSizes.option_for(_selected_size)
	var seed_value := _seed_value()
	emit_signal(
		"new_game_requested",
		_selected_preset,
		int(dims["width"]),
		int(dims["height"]),
		seed_value,
		DEFAULT_PROFILE_ID
	)


# ---- selectable cards -------------------------------------------------------
func _make_preset_card(preset: Dictionary) -> PanelContainer:
	var pid := String(preset["id"])
	var card := PanelContainer.new()
	card.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	card.add_theme_stylebox_override("panel", _selectable_stylebox(pid == _selected_preset))
	var body := VBoxContainer.new()
	body.mouse_filter = Control.MOUSE_FILTER_IGNORE
	body.add_theme_constant_override("separation", 5)
	card.add_child(body)

	var name_row := HBoxContainer.new()
	name_row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	body.add_child(name_row)
	var name_label := Label.new()
	name_label.text = String(preset["name"])
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_label.add_theme_font_size_override("font_size", CARD_NAME_SIZE)
	name_label.add_theme_color_override("font_color", HudStyle.INK)
	name_row.add_child(name_label)
	var policy := Label.new()
	policy.text = String(preset["seed_policy"])
	policy.add_theme_font_size_override("font_size", HINT_SIZE)
	policy.add_theme_color_override("font_color", HudStyle.WARN)
	name_row.add_child(policy)

	var blurb := Label.new()
	blurb.text = String(preset["blurb"])
	blurb.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	blurb.mouse_filter = Control.MOUSE_FILTER_IGNORE
	blurb.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	blurb.add_theme_font_size_override("font_size", CARD_BLURB_SIZE)
	blurb.add_theme_color_override("font_color", HudStyle.INK_DIM)
	body.add_child(blurb)

	card.gui_input.connect(_on_preset_input.bind(pid, card))
	card.set_meta("preset_id", pid)
	return card


func _on_preset_input(event: InputEvent, pid: String, _card: Control) -> void:
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		if pid == _selected_preset:
			return
		_selected_preset = pid
		_restyle_selectables()
		_refresh_summary()


func _make_size_row() -> HBoxContainer:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 6)
	for option in MapSizes.OPTIONS:
		row.add_child(_make_size_card(option))
	return row


func _make_size_card(option: Dictionary) -> PanelContainer:
	var key := String(option["key"])
	var selected := key == _selected_size
	var card := PanelContainer.new()
	card.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	card.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	card.add_theme_stylebox_override("panel", _selectable_stylebox(selected))
	var body := VBoxContainer.new()
	body.mouse_filter = Control.MOUSE_FILTER_IGNORE
	body.alignment = BoxContainer.ALIGNMENT_CENTER
	body.add_theme_constant_override("separation", 3)
	card.add_child(body)

	var name_label := Label.new()
	name_label.text = String(option["label"])
	name_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	name_label.add_theme_font_size_override("font_size", SIZE_NAME_SIZE)
	name_label.add_theme_color_override(
		"font_color", HudStyle.SIGNAL if selected else HudStyle.INK_DIM
	)
	body.add_child(name_label)

	var dim_label := Label.new()
	dim_label.text = "%d × %d" % [int(option["width"]), int(option["height"])]
	dim_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	dim_label.add_theme_font_size_override("font_size", SIZE_DIM_SIZE)
	dim_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	body.add_child(dim_label)

	card.gui_input.connect(_on_size_input.bind(key))
	card.set_meta("size_key", key)
	return card


func _on_size_input(event: InputEvent, key: String) -> void:
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		if key == _selected_size:
			return
		_selected_size = key
		_restyle_selectables()
		_refresh_summary()


## Recolour the selectable preset/size cards in place (they live in the active pane).
func _restyle_selectables() -> void:
	_restyle_selectable_children(_pane_body)


func _restyle_selectable_children(node: Node) -> void:
	for child in node.get_children():
		if child is PanelContainer and child.has_meta("preset_id"):
			(child as PanelContainer).add_theme_stylebox_override(
				"panel", _selectable_stylebox(String(child.get_meta("preset_id")) == _selected_preset)
			)
		elif child is PanelContainer and child.has_meta("size_key"):
			var key := String(child.get_meta("size_key"))
			var selected := key == _selected_size
			(child as PanelContainer).add_theme_stylebox_override("panel", _selectable_stylebox(selected))
			# retint the size-name label
			var body := child.get_child(0)
			if body != null and body.get_child_count() > 0:
				var name_label := body.get_child(0) as Label
				if name_label != null:
					name_label.add_theme_color_override(
						"font_color", HudStyle.SIGNAL if selected else HudStyle.INK_DIM
					)
		if child.get_child_count() > 0:
			_restyle_selectable_children(child)


func _make_slot_row(slot: Dictionary) -> PanelContainer:
	var card := PanelContainer.new()
	var sb := _selectable_stylebox(false)
	if bool(slot.get("auto", false)):
		sb.border_color = HudStyle.WARN
	card.add_theme_stylebox_override("panel", sb)
	if bool(slot.get("empty", false)):
		var empty := Label.new()
		empty.text = "Empty slot"
		empty.add_theme_font_size_override("font_size", CARD_NAME_SIZE)
		empty.add_theme_color_override("font_color", HudStyle.INK_FAINT)
		card.add_child(empty)
		return card
	var hb := HBoxContainer.new()
	card.add_child(hb)
	var left := VBoxContainer.new()
	left.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	hb.add_child(left)
	var who := Label.new()
	who.text = String(slot["who"])
	who.add_theme_font_size_override("font_size", CARD_NAME_SIZE)
	who.add_theme_color_override("font_color", HudStyle.INK)
	left.add_child(who)
	var meta := Label.new()
	meta.text = String(slot["meta"])
	meta.add_theme_font_size_override("font_size", HINT_SIZE)
	meta.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	left.add_child(meta)
	var when := Label.new()
	when.text = String(slot.get("when", "")) + (" · AUTO" if bool(slot.get("auto", false)) else "")
	when.add_theme_font_size_override("font_size", HINT_SIZE)
	when.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	hb.add_child(when)
	return card


# ---- summary ----------------------------------------------------------------
func _refresh_summary() -> void:
	if _summary_box == null:
		return
	for child in _summary_box.get_children():
		child.queue_free()
	var preset := _preset_for(_selected_preset)
	var dims := MapSizes.option_for(_selected_size)
	var world_name := String(preset.get("name", "—"))
	var paren := world_name.find(" (")
	if paren >= 0:
		world_name = world_name.substr(0, paren)
	var seed_text := "clock"
	if bool(preset.get("pinned", false)):
		seed_text = "pinned"
	elif _seed_value() != 0:
		seed_text = str(_seed_value())
	_add_summary_pair("World", world_name)
	_add_summary_pair("Grid", "%s · %d × %d" % [String(dims["label"]), int(dims["width"]), int(dims["height"])])
	_add_summary_pair("Seed", seed_text)


func _add_summary_pair(key: String, value: String) -> void:
	var pair := HBoxContainer.new()
	pair.add_theme_constant_override("separation", 8)
	var k := Label.new()
	k.text = key.to_upper()
	k.add_theme_font_size_override("font_size", SUMMARY_KEY_SIZE)
	k.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	pair.add_child(k)
	var v := Label.new()
	v.text = value
	v.add_theme_font_size_override("font_size", SUMMARY_VAL_SIZE)
	v.add_theme_color_override("font_color", HudStyle.SIGNAL)
	pair.add_child(v)
	_summary_box.add_child(pair)


# ---- small builders ---------------------------------------------------------
func _add_pane_header(title: String, eyebrow: String) -> void:
	var header := VBoxContainer.new()
	header.add_theme_constant_override("separation", 3)
	header.add_theme_stylebox_override("panel", HudStyle.header_stylebox())
	var eb := Label.new()
	eb.text = eyebrow.to_upper()
	eb.add_theme_font_size_override("font_size", EYEBROW_SIZE)
	eb.add_theme_color_override("font_color", HudStyle.SIGNAL)
	header.add_child(eb)
	var t := Label.new()
	t.text = title
	t.add_theme_font_size_override("font_size", PANE_TITLE_SIZE)
	t.add_theme_color_override("font_color", HudStyle.INK)
	header.add_child(t)
	_pane_body.add_child(header)
	var rule := HSeparator.new()
	_pane_body.add_child(rule)


func _add_paragraph(text: String) -> void:
	var p := Label.new()
	p.text = text
	p.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	p.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	p.add_theme_font_size_override("font_size", BODY_SIZE)
	p.add_theme_color_override("font_color", HudStyle.INK_DIM)
	_pane_body.add_child(p)


func _add_field_label(text: String) -> void:
	var l := Label.new()
	l.text = text.to_upper()
	l.add_theme_font_size_override("font_size", EYEBROW_SIZE)
	l.add_theme_color_override("font_color", HudStyle.INK_DIM)
	_pane_body.add_child(l)


func _add_note(text: String) -> void:
	var l := Label.new()
	l.text = text
	l.add_theme_font_size_override("font_size", NOTE_SIZE)
	l.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	_pane_body.add_child(l)


func _make_actions_row() -> HBoxContainer:
	var actions := HBoxContainer.new()
	actions.add_theme_constant_override("separation", 10)
	return actions


func _preset_for(pid: String) -> Dictionary:
	for preset in PRESETS:
		if String(preset["id"]) == pid:
			return preset
	return PRESETS[0]


# ---- styleboxes -------------------------------------------------------------
func _card_stylebox() -> StyleBoxFlat:
	var sb := HudStyle.card_stylebox()
	sb.bg_color = HudStyle.PANEL
	return sb


func _selectable_stylebox(selected: bool) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = HudStyle.SIGNAL_WASH if selected else HudStyle.GROUND_2
	sb.set_corner_radius_all(CTRL_RADIUS)
	sb.set_border_width_all(1)
	sb.border_color = HudStyle.SIGNAL_DEEP if selected else HudStyle.LINE
	_pad_stylebox(sb, CARD_PAD, CARD_PAD)
	return sb


func _summary_stylebox() -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = Color(0, 0, 0, 0.35)
	sb.set_corner_radius_all(CTRL_RADIUS)
	_pad_stylebox(sb, CARD_PAD, NAV_PAD_Y)
	return sb


func _pad_stylebox(sb: StyleBox, pad_x: int, pad_y: int) -> void:
	sb.content_margin_left = pad_x
	sb.content_margin_right = pad_x
	sb.content_margin_top = pad_y
	sb.content_margin_bottom = pad_y


func _style_line_edit(le: LineEdit) -> void:
	var normal := StyleBoxFlat.new()
	normal.bg_color = HudStyle.GROUND_2
	normal.set_corner_radius_all(CTRL_RADIUS - 2)
	normal.set_border_width_all(1)
	normal.border_color = HudStyle.LINE
	_pad_stylebox(normal, 11, 9)
	var focus := normal.duplicate()
	focus.border_color = HudStyle.SIGNAL
	le.add_theme_stylebox_override("normal", normal)
	le.add_theme_stylebox_override("focus", focus)
	le.add_theme_color_override("font_color", HudStyle.INK)
	le.add_theme_color_override("caret_color", HudStyle.SIGNAL)
