extends WorkbenchPage
class_name ConfigTuningPage

## PAGE ONE OF THE WORKBENCH — edit the sim's tunables and start a run on them.
##
## The page is driven ENTIRELY by `src/config/tuning_manifest.json`: which configs exist, which
## parameters each exposes, their bounds, steps, defaults, units and hints. Nothing about a
## parameter is written in GDScript, so exposing a new tunable is a manifest row and no client edit
## — the same property `WorkbenchPages` buys the surface as a whole.
##
## **A row's DEFAULT is the manifest's, not the running server's.** The client never asks the sim
## what its config currently holds, so "modified" here means *differs from the shipped default*, and
## the patch this page emits is written against that baseline. A snapshot field carrying the live
## config would let the two be told apart; there is none, so the page does not pretend.
##
## The overrides are **restart-scoped** (`WorkbenchVocab.TUNING_BANNER` says so on the surface):
## every config kind is loaded by the sim at world generation, so a patch changes the NEXT New Game
## and never retunes the running world. That is why `Apply` both sends the patches and asks the host
## for a new game — an apply that only staged them would leave the designer watching an unchanged
## world with no way to tell success from failure.

## Emitted by `Apply`. `patches` maps a config KIND (the manifest's `kind`) to a SPARSE nested
## dictionary holding only the parameters the designer actually moved — see `build_patches`.
signal overrides_requested(patches: Dictionary)

## One entry per parameter row: its config kind, its JSON pointer, the typing needed to read the
## control back, and the two nodes whose state IS the row (the field and its dot).
var _rows: Array[Dictionary] = []
## The pinned action bar's widgets, held here because they live in the SHELL's footer region rather
## than in this page's body — `_refresh_state` has to be able to reach them from outside the subtree.
var _status: Label = null
var _apply_button: Button = null
var _revert_button: Button = null


## Restore every row to its manifest default. Reached by `Revert all` and by the shell's page reset.
func reset() -> void:
	for row in _rows:
		var spin: SpinBox = row["spin"]
		spin.set_value_no_signal(row["default"])
	_refresh_state()


# ---- body ------------------------------------------------------------------

func build() -> void:
	add_theme_constant_override("separation", WorkbenchVocab.CONTENT_GAP)
	add_child(WorkbenchWidgets.build_banner(WorkbenchVocab.TUNING_BANNER, HudStyle.WARN))

	var kinds := _load_manifest_kinds()
	if kinds.is_empty():
		add_child(WorkbenchWidgets.build_caption(
			WorkbenchVocab.TUNING_MANIFEST_UNREADABLE % WorkbenchVocab.TUNING_MANIFEST_PATH,
			HudStyle.DANGER))
		return

	for kind in kinds:
		add_child(_build_group(kind))
	_refresh_state()


## The manifest's `kinds` array, or an empty array when the file is missing, unparseable or shaped
## unexpectedly. Every failure lands in the same place so the page has ONE degraded state to render.
func _load_manifest_kinds() -> Array:
	var text := FileAccess.get_file_as_string(WorkbenchVocab.TUNING_MANIFEST_PATH)
	if text.is_empty():
		push_warning("[Workbench] tuning manifest unreadable: %s" % WorkbenchVocab.TUNING_MANIFEST_PATH)
		return []
	var parsed: Variant = JSON.parse_string(text)
	if typeof(parsed) != TYPE_DICTIONARY:
		push_warning("[Workbench] tuning manifest is not a JSON object")
		return []
	var kinds: Variant = (parsed as Dictionary).get("kinds", [])
	if typeof(kinds) != TYPE_ARRAY:
		push_warning("[Workbench] tuning manifest has no `kinds` array")
		return []
	return kinds


## One config kind: a sunk well headed by the kind's label, holding a row per parameter.
func _build_group(kind: Dictionary) -> PanelContainer:
	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", WorkbenchWidgets.group_stylebox())

	var body := VBoxContainer.new()
	body.add_theme_constant_override("separation", WorkbenchWidgets.GROUP_HEADER_GAP)
	panel.add_child(body)

	body.add_child(WorkbenchWidgets.build_section_label(kind.get("label", "")))
	var rule := PanelContainer.new()
	rule.custom_minimum_size.y = 1.0
	rule.add_theme_stylebox_override("panel", HudStyle.hairline_stylebox())
	body.add_child(rule)

	var rows := VBoxContainer.new()
	rows.add_theme_constant_override("separation", WorkbenchWidgets.ROW_GAP)
	body.add_child(rows)

	var kind_id: String = kind.get("kind", "")
	for param in kind.get("params", []):
		rows.add_child(_build_row(kind_id, param))
	return panel


## One parameter: its control and its dot, built here and threaded into the shared row drawing, plus
## the entry in `_rows` that is how this page reads the control back.
func _build_row(kind_id: String, param: Dictionary) -> Control:
	var is_int: bool = param.get("type", "float") == "int"
	var step: float = float(param.get("step", 1.0))
	var default_value: float = float(param.get("default", 0.0))
	var spin := WorkbenchWidgets.build_number_field(default_value, float(param.get("min", 0.0)),
		float(param.get("max", 0.0)), step, is_int)
	var dot := WorkbenchWidgets.build_modified_dot()
	var default_readout := WorkbenchVocab.TUNING_DEFAULT_PREFIX + WorkbenchWidgets.format_value(
		default_value, step, is_int, param.get("unit", ""))
	var label_text: String = param.get("label", "")
	var hint: String = param.get("hint", "")

	var row := WorkbenchWidgets.build_param_row(label_text, hint, default_readout, dot, spin)
	_rows.append({
		"kind": kind_id,
		"pointer": param.get("pointer", ""),
		"is_int": is_int,
		"step": step,
		"default": default_value,
		"spin": spin,
		"dot": dot,
	})
	spin.value_changed.connect(_on_value_changed)
	return row


## THE PINNED ACTION BAR — the status line and the two actions.
##
## It goes to the shell's footer region rather than into the body because the body is 25 rows long:
## an action bar at the end of it would be off screen at every scroll position but the last, which
## is exactly where the state of the page (how many rows are overridden) most needs to be readable.
##
## Both buttons are dead while nothing is dirty — there is nothing to apply and nothing to revert,
## and a live button that does neither teaches the wrong thing about what "modified" means here.
func build_actions() -> Control:
	var footer := VBoxContainer.new()
	footer.add_theme_constant_override("separation", WorkbenchWidgets.GROUP_HEADER_GAP)

	_status = WorkbenchWidgets.build_caption("")
	footer.add_child(_status)

	var actions := HBoxContainer.new()
	actions.add_theme_constant_override("separation", WorkbenchWidgets.ROW_COLUMN_GAP)

	_apply_button = Button.new()
	_apply_button.text = WorkbenchVocab.TUNING_APPLY_LABEL
	_apply_button.focus_mode = Control.FOCUS_NONE
	_apply_button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	_apply_button.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_BODY)
	HudStyle.apply_button(_apply_button, "primary")
	_apply_button.pressed.connect(_on_apply_pressed)
	actions.add_child(_apply_button)

	_revert_button = Button.new()
	_revert_button.text = WorkbenchVocab.TUNING_REVERT_LABEL
	_revert_button.focus_mode = Control.FOCUS_NONE
	_revert_button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	_revert_button.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_BODY)
	HudStyle.apply_button(_revert_button, "ghost")
	_revert_button.pressed.connect(_on_revert_pressed)
	actions.add_child(_revert_button)

	footer.add_child(actions)
	# The rows were built before this bar existed, so the state they settled on is re-applied now
	# that there is somewhere to say it.
	_refresh_state()
	return footer


# ---- state -----------------------------------------------------------------

func _on_value_changed(_value: float) -> void:
	_refresh_state()


## Re-read every row's control, repaint its dot, and settle the footer on the resulting dirty count.
## One pass over the rows drives all three, so a dot and the status line cannot disagree.
func _refresh_state() -> void:
	var dirty := 0
	for row in _rows:
		var modified := _is_modified(row)
		WorkbenchWidgets.set_modified(row["dot"], modified)
		if modified:
			dirty += 1
	if _status != null:
		_status.text = WorkbenchVocab.TUNING_CLEAN_STATUS if dirty == 0 \
			else WorkbenchVocab.TUNING_DIRTY_STATUS % dirty
		_status.add_theme_color_override("font_color",
			HudStyle.INK_FAINT if dirty == 0 else HudStyle.WARN)
	if _apply_button != null:
		_apply_button.disabled = dirty == 0
	if _revert_button != null:
		_revert_button.disabled = dirty == 0


func _is_modified(row: Dictionary) -> bool:
	var spin: SpinBox = row["spin"]
	var step: float = row["step"]
	return absf(spin.value - float(row["default"])) \
		> step * WorkbenchVocab.TUNING_MODIFIED_STEP_FRACTION


# ---- patches ---------------------------------------------------------------

## The overrides to hand the sim: config kind -> a nested dictionary carrying ONLY the parameters
## whose value has moved.
##
## **Sparse is the whole contract.** Each config kind is a file the sim merges over its shipped
## defaults, so a parameter present in the patch is a parameter the designer has taken ownership of:
## writing the untouched ones through at their current default would silently freeze them against
## any later change to the shipped config, and the diff would stop being readable as "what I tuned".
func build_patches() -> Dictionary:
	var patches: Dictionary = {}
	for row in _rows:
		if not _is_modified(row):
			continue
		var kind: String = row["kind"]
		if not patches.has(kind):
			patches[kind] = {}
		var spin: SpinBox = row["spin"]
		var value: Variant = int(round(spin.value)) if row["is_int"] else spin.value
		_write_pointer(patches[kind], row["pointer"], value)
	return patches


## Write `value` into `target` at a JSON pointer, creating the intermediate objects. The pointer is
## the manifest's addressing of a parameter within its config file, so the patch this builds is
## shaped exactly like the config it overrides.
static func _write_pointer(target: Dictionary, pointer: String, value: Variant) -> void:
	var tokens := _pointer_tokens(pointer)
	if tokens.is_empty():
		return
	var cursor := target
	for i in range(tokens.size() - 1):
		var token := tokens[i]
		if typeof(cursor.get(token)) != TYPE_DICTIONARY:
			cursor[token] = {}
		cursor = cursor[token]
	cursor[tokens[tokens.size() - 1]] = value


## Split a JSON pointer (RFC 6901) into its unescaped tokens. `~1` is a literal `/` inside a key and
## `~0` a literal `~`, and they are unescaped in that order — the reverse order would turn an
## escaped tilde followed by a `1` into a slash.
static func _pointer_tokens(pointer: String) -> PackedStringArray:
	var tokens := PackedStringArray()
	if not pointer.begins_with("/"):
		return tokens
	for raw in pointer.substr(1).split("/"):
		tokens.append(raw.replace("~1", "/").replace("~0", "~"))
	return tokens


# ---- actions ---------------------------------------------------------------

## Stage the overrides and restart the world on them: one `set_config_override <kind> <json>` per
## dirty kind, then the host's `new_game`.
##
## **The two halves are ordered and the second is conditional**, because the failure they guard
## against is asymmetric: a new game started before the patches land generates a world on the OLD
## config and looks exactly like a working apply. So every command goes first, and the restart is
## only asked for once they have all gone. If any of them did not, the page says so and stops —
## leaving the rows dirty, because the edits are still there to re-apply once a server is attached
## and silently reverting them would look like the apply had worked.
func _on_apply_pressed() -> void:
	var patches := build_patches()
	if patches.is_empty():
		return
	overrides_requested.emit(patches)

	for kind in patches:
		var line := "%s %s %s" % [WorkbenchVocab.COMMAND_SET_CONFIG_OVERRIDE, kind,
			JSON.stringify(patches[kind])]
		if not send_command(line):
			log_line(WorkbenchVocab.TUNING_OFFLINE_LOG % WorkbenchVocab.COMMAND_SET_CONFIG_OVERRIDE)
			return

	var new_game := service(WorkbenchVocab.SERVICE_NEW_GAME)
	if not new_game.is_valid():
		log_line(WorkbenchVocab.TUNING_NO_NEW_GAME_LOG)
		return
	new_game.call()

	var overridden := 0
	for row in _rows:
		if _is_modified(row):
			overridden += 1
	log_line(WorkbenchVocab.TUNING_APPLY_LOG_FORMAT % [overridden, ", ".join(patches.keys())])


## Return every row to its default AND drop the server's staged overrides, so the two cannot
## disagree: a cleared page over a server still holding the last patch would generate the next world
## on overrides the surface says are gone.
func _on_revert_pressed() -> void:
	reset()
	if send_command(WorkbenchVocab.COMMAND_CLEAR_CONFIG_OVERRIDES):
		log_line(WorkbenchVocab.TUNING_REVERT_LOG)
	else:
		log_line(WorkbenchVocab.TUNING_OFFLINE_LOG % WorkbenchVocab.COMMAND_CLEAR_CONFIG_OVERRIDES)
