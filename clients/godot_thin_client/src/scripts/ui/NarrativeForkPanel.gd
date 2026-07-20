extends Control
class_name NarrativeForkPanel

## The Telling (docs/plan_the_telling.md) — the narrative fork decision surface.
##
## A fork is the moment the game asks the player who their people ARE, so this is deliberately
## not a dialog box: the narration is the hero element (prose, generous, never truncated), the
## choices sit under it as equals, and the sampled signals behind the question are available —
## collapsed — as the standing proof that the voice never lies.
##
## Structure follows the two patterns this HUD already has:
##   • the targeting banner's centered-overlay skeleton (Hud._ensure_targeting_banner), and
##   • the TurnOrb popover's CATCHER NESTING (TurnOrb._open_popover) — the card is a CHILD of the
##     full-screen dismiss layer, never its sibling. A child renders and picks ABOVE its parent, so
##     the card's own buttons consume their clicks and only clicks OUTSIDE the card dismiss. As
##     siblings, the ordering is ambiguous and the catcher swallows the buttons ("clicking the
##     choice did nothing") — a bug that has already been paid for once.
##
## This node IS the catcher; `_card` (an AutoSizingPanel, per the project CLAUDE.md rule against
## bespoke height logic) is the nested card that grows to fit the wardrobe entry's prose.

const HudStyle := preload("res://src/scripts/ui/HudStyle.gd")

## The player picked an answer. Payload keys: { beat_id, choice_id }. Hud adds the faction and
## re-emits `answer_fork_requested`; Main formats `answer_fork <faction> <beat> <choice>`.
signal answer_selected(payload: Dictionary)
## The panel closed without an answer (catcher click / ✕). The fork is still pending — the orb
## keeps its row and the end-turn gate keeps holding.
signal dismissed

# ---- voice register preference --------------------------------------------
# `VoiceLine.register` is a FREE-FORM string precisely so a new register needs no schema change,
# so nothing here may hardcode "mythic"/"warm": the toggle is built from the registers actually
# present in the fork, and an unknown/absent stored preference falls back to the FIRST available.
const CONFIG_PATH := "user://narrative.cfg"
const CONFIG_SECTION := "narrative"
const CONFIG_KEY_VOICE_REGISTER := "voice_register"

# ---- geometry / typography (named constants; no magic literals) ------------
# A dim scrim over the rest of the HUD. A fork is modal in intent — the panel must read as the
# only thing on screen — and the card's own stylebox is translucent, so without this the tile card
# and command feed show THROUGH the narration and make the prose hard to read.
const SCRIM_COLOR := Color(0.0, 0.0, 0.0, 0.55)
const CARD_WIDTH := 660.0
const CARD_MIN_HEIGHT := 220.0
const CARD_MAX_HEIGHT := 720.0
# The card is pinned this far below the top edge, and keeps the same clearance at the bottom —
# which is exactly the `bottom_margin` AutoSizingPanel measures its available height against.
const CARD_TOP_MARGIN := 96.0
# Breathing room below the last row, on top of the card stylebox's own margins — a fork is the
# game asking who your people are, and a card whose footer sits flush on its border reads rushed.
const CARD_EXTRA_PADDING := 12.0
const BODY_SEPARATION := 16
const EYEBROW_FONT_SIZE := 12
# The narration is prose at paragraph length, so it is set noticeably larger than UI copy and
# given real leading — cramped 14px body text is what makes a story beat read like a tooltip.
const NARRATION_FONT_SIZE := 19
const NARRATION_LINE_SPACING := 7
const NARRATION_MIN_HEIGHT := 76.0
const CHOICE_SEPARATION := 8
const CHOICE_FONT_SIZE := 15
const CHOICE_MIN_HEIGHT := 44.0
const GLOSS_FONT_SIZE := 13
const GLOSS_TOGGLE_FONT_SIZE := 12
const FOOTER_FONT_SIZE := 12
const CLOSE_FONT_SIZE := 16
# Gloss values are sampled signals, shown as numbers, not prettified into prose.
const GLOSS_ROW_FORMAT := "%s = %s"
const GLOSS_DECIMALS := 2

const EYEBROW_TEXT := "A QUESTION AT THE FIRE"
const GLOSS_LABEL_COLLAPSED := "▸  beneath the telling"
const GLOSS_LABEL_EXPANDED := "▾  beneath the telling"
const VOICE_LABEL := "Voice"
const CLOSE_TEXT := "✕"
const CLOSE_TOOLTIP := "Set the question aside — it will keep until you answer it."

var _fork: Dictionary = {}
var _register: String = ""
var _gloss_expanded: bool = false
# The narrator's medium (The Telling §7). Presentational ONLY — it tints this panel's eyebrow so a
# fork wears the same-aged voice as the Telling panel, and never selects different copy. The
# accent table + its `oral` fallback live in TellingPanel, so the two surfaces cannot drift.
var _medium_accent: Color = TellingPanel.accent_for(TellingPanel.MEDIUM_ORAL)

var _card: AutoSizingPanel = null
var _body: VBoxContainer = null
var _scroll: ScrollContainer = null
var _gloss_box: VBoxContainer = null
var _scrim: ColorRect = null


func _ready() -> void:
	# The catcher: full-screen, STOP, so a click anywhere outside the card dismisses.
	set_anchors_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_STOP
	gui_input.connect(_on_catcher_input)
	resized.connect(_reposition_card)
	visible = false

	_scrim = ColorRect.new()
	_scrim.name = "Scrim"
	_scrim.color = SCRIM_COLOR
	_scrim.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_scrim)

	_card = AutoSizingPanel.new()
	_card.name = "ForkCard"
	_card.target_width = CARD_WIDTH
	_card.min_height = CARD_MIN_HEIGHT
	_card.max_height = CARD_MAX_HEIGHT
	_card.bottom_margin = CARD_TOP_MARGIN
	add_child(_card)

	var panel := PanelContainer.new()
	panel.set_anchors_preset(Control.PRESET_FULL_RECT)
	panel.add_theme_stylebox_override("panel", HudStyle.card_stylebox())
	_card.add_child(panel)

	_scroll = ScrollContainer.new()
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	panel.add_child(_scroll)

	_body = VBoxContainer.new()
	_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_body.add_theme_constant_override("separation", BODY_SEPARATION)
	_scroll.add_child(_body)

# ---- public API ------------------------------------------------------------

## Show `fork` (a pending-fork dict as decoded by the native layer). Re-showing the SAME fork
## just re-renders — safe to call from a snapshot path.
func show_fork(fork: Dictionary) -> void:
	if fork.is_empty():
		close()
		return
	_fork = fork
	_register = _resolve_register(fork)
	_gloss_expanded = false
	visible = true
	_sync_to_viewport()
	_render()

func close() -> void:
	visible = false

func is_open() -> bool:
	return visible

## Age this panel's header to the faction's narrator medium. Unknown/absent ids fall back to
## `oral` (see `TellingPanel.accent_for`). Re-renders only when already open — the accent is read
## fresh on every `_render()`, so a closed panel needs no work.
func set_voice_medium(medium_id: String) -> void:
	var accent := TellingPanel.accent_for(medium_id)
	if accent == _medium_accent:
		return
	_medium_accent = accent
	if visible:
		_render()

## The beat this panel is currently showing ("" when closed / empty) — lets the Hud decide
## whether an incoming snapshot still concerns what the player is looking at.
func current_beat_id() -> String:
	return String(_fork.get("beat_id", ""))

# ---- voice register (shared with the Hud's attention-row label) ------------

## The stored voice-register preference, or "" when none is stored / it cannot be read.
## Fails silently: a missing or malformed pref file must never surface to the player.
static func load_voice_register() -> String:
	var cfg := ConfigFile.new()
	if cfg.load(CONFIG_PATH) != OK:
		return ""
	return String(cfg.get_value(CONFIG_SECTION, CONFIG_KEY_VOICE_REGISTER, ""))

static func save_voice_register(register: String) -> void:
	if register == "":
		return
	var cfg := ConfigFile.new()
	cfg.load(CONFIG_PATH)   # preserve any other sections; ignore load errors
	cfg.set_value(CONFIG_SECTION, CONFIG_KEY_VOICE_REGISTER, register)
	cfg.save(CONFIG_PATH)

## The registers a `[VoiceLine]` array actually carries, in catalog order, de-duplicated.
static func registers_in(lines_variant: Variant) -> Array:
	var out: Array = []
	if not (lines_variant is Array):
		return out
	for line_variant in (lines_variant as Array):
		if not (line_variant is Dictionary):
			continue
		var register := String((line_variant as Dictionary).get("register", ""))
		if register != "" and not out.has(register):
			out.append(register)
	return out

## Render a `[VoiceLine]` array in `register`, falling back to the FIRST line present so a line
## authored only in some other register still says something rather than rendering blank.
static func text_in_register(lines_variant: Variant, register: String) -> String:
	if not (lines_variant is Array):
		return ""
	var lines: Array = lines_variant
	var first := ""
	for line_variant in lines:
		if not (line_variant is Dictionary):
			continue
		var line: Dictionary = line_variant
		var text := String(line.get("text", ""))
		if first == "" and text != "":
			first = text
		if String(line.get("register", "")) == register and text != "":
			return text
	return first

## The register to render `fork` in: the stored preference when the fork actually carries it,
## else the first register present. Never assumes a particular register exists.
static func resolve_register_for(fork: Dictionary) -> String:
	var available := registers_in(fork.get("narration", []))
	if available.is_empty():
		return ""
	var stored := load_voice_register()
	if stored != "" and available.has(stored):
		return stored
	return String(available[0])

func _resolve_register(fork: Dictionary) -> String:
	return resolve_register_for(fork)

# ---- rendering -------------------------------------------------------------

func _render() -> void:
	if _body == null:
		return
	for child in _body.get_children():
		child.queue_free()
		_body.remove_child(child)

	_body.add_child(_build_header())
	_body.add_child(_build_narration())
	_body.add_child(_build_choices())
	_body.add_child(_build_gloss_section())
	var footer := _build_voice_toggle()
	if footer != null:
		_body.add_child(footer)

	# The card grows to fit the wardrobe entry's prose (they vary a lot in length), so the fit
	# needs a frame for the wrapped narration label to report its real height.
	call_deferred("_fit_card")

func _build_header() -> Control:
	var header := HBoxContainer.new()
	var eyebrow := Label.new()
	eyebrow.text = EYEBROW_TEXT
	eyebrow.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	eyebrow.add_theme_font_size_override("font_size", EYEBROW_FONT_SIZE)
	# The eyebrow carries the voice's AGE (see `set_voice_medium`) — the one place a fork shows the
	# medium, since the narration itself is medium-independent by design.
	eyebrow.add_theme_color_override("font_color", _medium_accent)
	header.add_child(eyebrow)

	var close_button := Button.new()
	close_button.text = CLOSE_TEXT
	close_button.tooltip_text = CLOSE_TOOLTIP
	close_button.add_theme_font_size_override("font_size", CLOSE_FONT_SIZE)
	HudStyle.apply_link_button(close_button, HudStyle.INK_FAINT)
	close_button.pressed.connect(_on_dismiss)
	header.add_child(close_button)
	return header

func _build_narration() -> Control:
	var narration := Label.new()
	narration.text = text_in_register(_fork.get("narration", []), _register)
	narration.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	narration.custom_minimum_size = Vector2(0, NARRATION_MIN_HEIGHT)
	narration.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	narration.add_theme_font_size_override("font_size", NARRATION_FONT_SIZE)
	narration.add_theme_constant_override("line_spacing", NARRATION_LINE_SPACING)
	narration.add_theme_color_override("font_color", HudStyle.INK)
	return narration

## The choices, in CATALOG order. Every one is always enabled — the defer choice is the explicit
## out the end-turn gate depends on, so it can never be unavailable; it is styled `ghost` (an
## answer, but the quiet one) while the rest are `primary`.
func _build_choices() -> Control:
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", CHOICE_SEPARATION)
	var beat_id := String(_fork.get("beat_id", ""))
	var choices_variant: Variant = _fork.get("choices", [])
	if not (choices_variant is Array):
		return box
	for choice_variant in (choices_variant as Array):
		if not (choice_variant is Dictionary):
			continue
		var choice: Dictionary = choice_variant
		var choice_id := String(choice.get("choice_id", ""))
		# `is_defer` is computed SERVER-side, exactly one per fork — read the flag, never re-derive
		# which choice writes nothing.
		var is_defer := bool(choice.get("is_defer", false))
		var button := Button.new()
		button.text = text_in_register(choice.get("label", []), _register)
		button.focus_mode = Control.FOCUS_NONE
		button.custom_minimum_size = Vector2(0, CHOICE_MIN_HEIGHT)
		button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		button.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		button.add_theme_font_size_override("font_size", CHOICE_FONT_SIZE)
		HudStyle.apply_button(button, "ghost" if is_defer else "primary")
		button.pressed.connect(_on_choice_pressed.bind(beat_id, choice_id))
		box.add_child(button)
	return box

## "Beneath the telling": the signals the voice sampled to ask this question, collapsed by
## default. Formatted PLAINLY (`sedentarization.score = 41`) — the point is that these are the
## real numbers, so prettifying them into prose would defeat the section.
func _build_gloss_section() -> Control:
	var box := VBoxContainer.new()
	var gloss_variant: Variant = _fork.get("gloss", [])
	if not (gloss_variant is Array) or (gloss_variant as Array).is_empty():
		return box

	var toggle := Button.new()
	toggle.text = GLOSS_LABEL_EXPANDED if _gloss_expanded else GLOSS_LABEL_COLLAPSED
	toggle.add_theme_font_size_override("font_size", GLOSS_TOGGLE_FONT_SIZE)
	toggle.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	HudStyle.apply_link_button(toggle, HudStyle.INK_FAINT)
	toggle.pressed.connect(_on_gloss_toggled)
	box.add_child(toggle)

	_gloss_box = VBoxContainer.new()
	_gloss_box.visible = _gloss_expanded
	for entry_variant in (gloss_variant as Array):
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var row := Label.new()
		row.text = GLOSS_ROW_FORMAT % [
			String(entry.get("signal", "")),
			String.num(float(entry.get("value", 0.0)), GLOSS_DECIMALS),
		]
		row.add_theme_font_size_override("font_size", GLOSS_FONT_SIZE)
		row.add_theme_color_override("font_color", HudStyle.INK_DIM)
		_gloss_box.add_child(row)
	box.add_child(_gloss_box)
	return box

## The voice-register toggle, built from the registers this fork ACTUALLY carries — never from a
## hardcoded list. With a single register there is nothing to choose, so nothing renders.
func _build_voice_toggle() -> Control:
	var available := registers_in(_fork.get("narration", []))
	if available.size() < 2:
		return null
	var row := HBoxContainer.new()
	var label := Label.new()
	label.text = VOICE_LABEL
	label.add_theme_font_size_override("font_size", FOOTER_FONT_SIZE)
	label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	row.add_child(label)
	for register_variant in available:
		var register := String(register_variant)
		var button := Button.new()
		button.text = register.capitalize()
		button.add_theme_font_size_override("font_size", FOOTER_FONT_SIZE)
		var active := register == _register
		HudStyle.apply_link_button(button, HudStyle.SIGNAL if active else HudStyle.INK_FAINT)
		button.disabled = active
		if active:
			# `apply_link_button` sets a dim `font_disabled_color`, which would make the ACTIVE
			# register — the one that is disabled precisely because it is already selected — read
			# as the unavailable one. Restate the accent so the selection is legible.
			button.add_theme_color_override("font_disabled_color", HudStyle.SIGNAL)
		button.pressed.connect(_on_register_picked.bind(register))
		row.add_child(button)
	return row

## Grow the card to its content and keep it pinned top-centre. AutoSizingPanel owns the height
## math (and turns the scroll on when the prose outruns the available space).
func _fit_card() -> void:
	if _card == null or _body == null:
		return
	_card.position = Vector2(maxf((_available_width() - CARD_WIDTH) * 0.5, 0.0), CARD_TOP_MARGIN)
	var card_style := HudStyle.card_stylebox()
	var chrome := card_style.content_margin_top + card_style.content_margin_bottom + CARD_EXTRA_PADDING
	_card.fit_to_content(_body.get_combined_minimum_size().y, chrome, _scroll)
	_reposition_card()

## Centre the card horizontally. Measured against the VIEWPORT, not this node's `size`: the catcher
## is anchored full-rect but its size only settles on the next layout pass, so reading `size` in the
## same frame the panel is built centres it against 0 and pins the card to the left edge.
func _available_width() -> float:
	var viewport := get_viewport()
	if viewport != null:
		return viewport.get_visible_rect().size.x
	return size.x

## Pin the catcher (and its scrim) to the viewport EXPLICITLY rather than trusting the full-rect
## anchors: this node is hidden until a fork arrives, and a hidden Control's layout does not
## settle — leaving the scrim a zero-size rect that silently never darkens anything.
func _sync_to_viewport() -> void:
	var rect := Rect2(Vector2.ZERO, Vector2(size))
	var viewport := get_viewport()
	if viewport != null:
		rect = viewport.get_visible_rect()
	position = Vector2.ZERO
	size = rect.size
	if _scrim != null:
		_scrim.position = Vector2.ZERO
		_scrim.size = rect.size

func _reposition_card() -> void:
	if _card == null:
		return
	_sync_to_viewport()
	_card.position = Vector2(maxf((_available_width() - _card.size.x) * 0.5, 0.0), CARD_TOP_MARGIN)

# ---- input -----------------------------------------------------------------

func _on_catcher_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed:
		_on_dismiss()

func _on_dismiss() -> void:
	close()
	emit_signal("dismissed")

func _on_gloss_toggled() -> void:
	_gloss_expanded = not _gloss_expanded
	_render()

func _on_register_picked(register: String) -> void:
	if register == _register:
		return
	_register = register
	save_voice_register(register)
	_render()

func _on_choice_pressed(beat_id: String, choice_id: String) -> void:
	if beat_id == "" or choice_id == "":
		return
	close()
	emit_signal("answer_selected", {"beat_id": beat_id, "choice_id": choice_id})
