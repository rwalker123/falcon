extends Control
class_name ConfigDriftNotice

## **WHAT THE LOADED WORLD WILL RUN ON, when that is not what it was saved under.**
##
## The sim deliberately does not save config (`.claude/rules/core_sim/save-game.md`): a loaded world
## is restored exactly as it was written, but every turn *from here* resolves against whatever tuning
## this process has live. So a save made before a balance change plays differently, and nothing in the
## snapshot says so — `SaveOpReply.config_drift` is the only place that fact exists.
##
## **IT NAMES THE FILES, one per line.** "Config changed" is not actionable and would be ignored;
## `fauna_config.json` and `recipes.json` tell a player which numbers to distrust. `builtin` vs `file`
## is kept as a real difference too — a shipped default appearing or a file being deleted changes what
## the sim runs on just as an edit does, and collapsing the two would report the change as an edit
## that never happened.
##
## Built in code and styled through `HudStyle`, like the menu shell it echoes. It owns no socket and
## no timer: `Main` shows it once, after the loaded world reveals, and frees it when dismissed.

## The player closed the notice. The owner frees it.
signal dismissed

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")

# ---- wire tokens (mirror `sim_runtime::commands::ConfigDigestKind`) -------------------------------
const KIND_ABSENT := "absent"
const KIND_BUILTIN := "builtin"
const KIND_FILE := "file"

const TITLE := "Tuning has changed since this save"
const EYEBROW := "Loaded"
## Says what drift IS before listing it, because the list alone reads like an error.
const BODY := "The world was restored exactly as it was saved. The rules it runs under from here are this build's, and these config files no longer match the ones the save was written with:"
const FOOTER := "Nothing is broken — expect different numbers where these files are read."
const DISMISS_LABEL := "Understood"

## One phrasing per (saved → live) pair. **A pair is a fact about WHERE tuning came from**, so each
## gets its own sentence rather than a shared "changed": a file that was edited, a file that appeared,
## and a file that was deleted are three different things to go and check.
const CHANGE_EDITED := "edited since the save"
const CHANGE_NOW_FILE := "now loaded from a file (the save used the shipped default)"
const CHANGE_NOW_BUILTIN := "the file is gone; the shipped default is loaded"
const CHANGE_NEW := "not recorded in the save; this build loads it"
const CHANGE_DROPPED := "recorded in the save; this build does not load it"
const CHANGE_UNKNOWN := "differs from the save"

const ROW_FORMAT := "%s — %s"
const ROW_BULLET := "• "

# ---- layout (named; no bare literals) ------------------------------------------------------------
## The card's nominal width. Prose at this measure wraps to a readable line; a narrower window pulls
## it in (`_fit`), and it never grows past it — this is a paragraph and a short list, not a table.
const CARD_WIDTH := 640.0
const CARD_MIN_WIDTH := 360.0
const CARD_MIN_HEIGHT := 140.0
const CARD_MARGIN := 40.0
const CARD_PAD := 20
const CARD_RADIUS := 10
const BODY_SEPARATION := 12
const HEADER_SEPARATION := 3
const LIST_SEPARATION := 4
const LIST_INDENT := 10

# ---- font sizes ----------------------------------------------------------------------------------
const EYEBROW_SIZE := 11
const TITLE_SIZE := 17
const BODY_SIZE := 13
const ROW_SIZE := 13
const FOOTER_SIZE := 11

var _scrim: ColorRect
## **THE SANCTIONED FREE-FLOATING CARD** (`.claude/rules/client/panel-framework.md`): a plain
## `Control` that is sized to its measured content against the viewport, rather than bespoke
## height arithmetic. Measuring it here by hand was the first shape and it was wrong in the way that
## helper exists to prevent — an autowrap Label's minimum height is only right once the container has
## been sorted at the card's real width, so a same-pass measurement wrapped the prose at zero width
## and returned a card the height of the screen.
var _card: AutoSizingPanel
var _panel: PanelContainer
var _scroll: ScrollContainer
var _body: VBoxContainer
var _list: VBoxContainer
var _built := false

## The scrim's opacity. Lighter than the pause menu's: this notice interrupts a world the player has
## just asked to see, so the map stays legible behind it.
const SCRIM_ALPHA := 0.62


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_build()
	resized.connect(_fit)
	_fit()


## **SHOW THE DRIFT.** `rows` is `SaveOpReply.config_drift` as the bridge hands it over — an array of
## `{file_name, saved, live}`. An EMPTY array is the good case and the owner must not call this at
## all for it; called anyway, the notice says nothing rather than rendering an empty accusation.
func show_drift(rows: Array) -> void:
	if not _built:
		_build()
	for child in _list.get_children():
		# Removed as well as freed: `queue_free` alone leaves the node a child until the end of the
		# frame, and the "is the list empty" test below would then count the PREVIOUS load's rows.
		_list.remove_child(child)
		child.queue_free()
	var rendered := 0
	for row_variant in rows:
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		var label := Label.new()
		label.text = ROW_BULLET + (ROW_FORMAT % [
			String(row.get("file_name", "")),
			describe_change(String(row.get("saved", KIND_ABSENT)), String(row.get("live", KIND_ABSENT))),
		])
		label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		label.add_theme_font_size_override("font_size", ROW_SIZE)
		label.add_theme_color_override("font_color", HudStyle.WARN)
		_list.add_child(label)
		rendered += 1
	visible = rendered > 0
	_fit()


## The sentence for one (saved → live) pair. Static so the wording can be asserted without a tree.
static func describe_change(saved: String, live: String) -> String:
	if saved == KIND_FILE and live == KIND_FILE:
		return CHANGE_EDITED
	if saved == KIND_BUILTIN and live == KIND_FILE:
		return CHANGE_NOW_FILE
	if saved == KIND_FILE and live == KIND_BUILTIN:
		return CHANGE_NOW_BUILTIN
	if saved == KIND_ABSENT and live != KIND_ABSENT:
		return CHANGE_NEW
	if saved != KIND_ABSENT and live == KIND_ABSENT:
		return CHANGE_DROPPED
	return CHANGE_UNKNOWN


func _build() -> void:
	if _built:
		return
	_built = true
	_scrim = ColorRect.new()
	_scrim.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_scrim.color = Color(HudStyle.GROUND.r, HudStyle.GROUND.g, HudStyle.GROUND.b, SCRIM_ALPHA)
	_scrim.mouse_filter = Control.MOUSE_FILTER_STOP
	add_child(_scrim)

	_card = AutoSizingPanel.new()
	_card.target_width = CARD_WIDTH
	_card.max_width = CARD_WIDTH
	_card.min_height = CARD_MIN_HEIGHT
	# The card is CENTRED after it is fitted, so the whole room is its height ceiling rather than
	# whatever happens to lie below its current top edge.
	_card.centred_in_room = true
	add_child(_card)

	_panel = PanelContainer.new()
	_panel.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	var card_style := HudStyle.card_stylebox()
	card_style.bg_color = HudStyle.PANEL
	card_style.set_corner_radius_all(CARD_RADIUS)
	card_style.content_margin_left = CARD_PAD
	card_style.content_margin_right = CARD_PAD
	card_style.content_margin_top = CARD_PAD
	card_style.content_margin_bottom = CARD_PAD
	_panel.add_theme_stylebox_override("panel", card_style)
	_card.add_child(_panel)

	# The drift list is short in practice (one row per config file that moved) but it is not bounded,
	# so the card scrolls rather than growing off a small window — the same bargain every fitted card
	# in this client strikes.
	_scroll = ScrollContainer.new()
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_panel.add_child(_scroll)

	var body := VBoxContainer.new()
	body.add_theme_constant_override("separation", BODY_SEPARATION)
	body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.add_child(body)
	_body = body

	var header := VBoxContainer.new()
	header.add_theme_constant_override("separation", HEADER_SEPARATION)
	body.add_child(header)

	var eyebrow := Label.new()
	eyebrow.text = EYEBROW.to_upper()
	eyebrow.add_theme_font_size_override("font_size", EYEBROW_SIZE)
	eyebrow.add_theme_color_override("font_color", HudStyle.SIGNAL)
	header.add_child(eyebrow)

	var title := Label.new()
	title.text = TITLE
	title.add_theme_font_size_override("font_size", TITLE_SIZE)
	title.add_theme_color_override("font_color", HudStyle.INK)
	header.add_child(title)

	body.add_child(HSeparator.new())

	var blurb := Label.new()
	blurb.text = BODY
	blurb.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	blurb.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	blurb.add_theme_font_size_override("font_size", BODY_SIZE)
	blurb.add_theme_color_override("font_color", HudStyle.INK_DIM)
	body.add_child(blurb)

	var indent := MarginContainer.new()
	indent.add_theme_constant_override("margin_left", LIST_INDENT)
	body.add_child(indent)
	_list = VBoxContainer.new()
	_list.add_theme_constant_override("separation", LIST_SEPARATION)
	indent.add_child(_list)

	var footer := Label.new()
	footer.text = FOOTER
	footer.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	footer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	footer.add_theme_font_size_override("font_size", FOOTER_SIZE)
	footer.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	body.add_child(footer)

	var actions := HBoxContainer.new()
	actions.alignment = BoxContainer.ALIGNMENT_END
	body.add_child(actions)
	var dismiss := Button.new()
	dismiss.text = DISMISS_LABEL
	HudStyle.apply_button(dismiss, "primary")
	dismiss.pressed.connect(func(): dismissed.emit())
	actions.add_child(dismiss)


## **FIT, THEN CENTRE — and the fit is a FRAME LATE, deliberately.** The card's content is prose that
## autowraps, so its height is only knowable once the layout under it has been sorted at the card's
## real width; measuring in the same pass that mounted the rows reads the wrap at whatever width the
## container had a moment ago. So the width goes on now and the height is taken next frame.
func _fit() -> void:
	if _card == null:
		return
	var room := _card.available_room(CARD_MARGIN)
	_card.target_width = clampf(room.size.x, CARD_MIN_WIDTH, CARD_WIDTH)
	_card.max_width = _card.target_width
	_card.max_height = room.size.y
	_card.fit_width(0.0)
	await get_tree().process_frame
	if _card == null or not is_instance_valid(_card):
		return
	_card.fit_to_content(_body.get_combined_minimum_size().y, 2.0 * float(CARD_PAD), _scroll)
	room = _card.available_room(CARD_MARGIN)
	# `available_room` answers in GLOBAL coordinates and `position` is the parent's, so the notice's
	# own origin comes off it — this Control is full-rect over the viewport today, and the two would
	# silently agree until it was not.
	_card.position = room.position - global_position + (room.size - _card.size) * 0.5
