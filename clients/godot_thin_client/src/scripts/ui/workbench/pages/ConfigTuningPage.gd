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

## Emitted by `Apply`. `patches` maps a config KIND (the manifest's `kind`) to a nested dictionary
## holding the parameters this apply is taking ownership of — see `build_patches`.
signal overrides_requested(patches: Dictionary)

## One entry per parameter row: its config kind, its JSON pointer, the typing needed to read the
## control back, the nodes whose state IS the row (the field and its dot), and `sent` — **the value
## the SERVER was last told**, which is the second half of the page's state.
##
## **TWO FACTS PER ROW, NOT ONE.** Tracking only "is this edited?" made the surface lie: edit a row,
## Apply (the server now holds an override file), then type the default back, and every row reads
## clean — so the status line said "no overrides", both buttons went dead, and `Revert all` (the only
## thing that clears the server) became unreachable while the next new game still booted on the
## staged value. `sent` is what makes "the server holds something I no longer want" a state the page
## can see. It starts at the manifest default: before the first apply, the server has been told
## nothing, which is the same thing as having been told the defaults.
var _rows: Array[Dictionary] = []
## True while the server holds a `set_config_override` this page issued and has not since cleared.
## Independent of any row's value, because that is exactly the case the row values cannot express.
var _staged := false
## The pinned action bar's widgets, held here because they live in the SHELL's footer region rather
## than in this page's body — `_refresh_state` has to be able to reach them from outside the subtree.
var _status: Label = null
var _apply_button: Button = null
var _revert_button: Button = null
## An outcome to show in the status line INSTEAD of the computed state, until the designer moves a
## control. It is how a failed Apply reaches the eye: the log service writes to the command feed,
## which ships hidden, so without this a refused Apply changed nothing on screen at all.
var _outcome := ""
var _outcome_tint := HudStyle.INK_FAINT


## WORLD BOUNDARY — deliberately a NO-OP, and the exception the base contract names.
##
## This page holds no world-derived state: its rows come from the manifest and its `sent`/`_staged`
## values describe what the SERVER is holding, which a new world does not change (the override file
## survives the restart that Apply asked for). Clearing here would drop the record of the overrides
## the world in front of the designer was just built from — the same lie the `sent` tracking exists
## to prevent, arriving one frame later.
func reset() -> void:
	pass


## Return every row to its manifest default and forget what the server was told. The REVERT path's
## local half; `_on_revert_pressed` pairs it with the command that clears the server.
func _reset_rows() -> void:
	_outcome = ""
	for row in _rows:
		var spin: SpinBox = row["spin"]
		spin.set_value_no_signal(row["default"])
		row["sent"] = row["default"]
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
	var rows := VBoxContainer.new()
	rows.add_theme_constant_override("separation", WorkbenchWidgets.ROW_GAP)

	var kind_id: String = kind.get("kind", "")
	for param in kind.get("params", []):
		rows.add_child(_build_row(kind_id, param))
	return WorkbenchWidgets.build_group(kind.get("label", ""), rows)


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
		# Nothing has been sent yet, which is indistinguishable from having sent the defaults.
		"sent": default_value,
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

## Any control movement retires a reported outcome: the message described the press the designer has
## just moved past, and leaving it up would let a stale failure sit over a fresh edit.
func _on_value_changed(_value: float) -> void:
	_outcome = ""
	_refresh_state()


## Repaint every dot and settle the action bar on the page's THREE states, in the order they take
## precedence:
##
##   1. UNSENT — some row differs from what the server was told. Apply and Revert both live; this is
##      the only state in which Apply does anything.
##   2. STAGED — nothing unsent, but the server is holding overrides from an earlier Apply. Apply is
##      dead (there is nothing new to send) and **Revert stays live**, because it is the only way to
##      clear the server, and the bug this replaces was exactly a page that disabled it here.
##   3. CLEAN — nothing unsent, nothing staged. Both dead.
##
## The DOTS are unchanged and answer a different question: a dot marks a row that differs from the
## shipped default, whether or not the server has been told. So a staged page shows its dots with a
## dead Apply, which is what "already applied" looks like.
func _refresh_state() -> void:
	var overridden := 0
	var unsent := 0
	for row in _rows:
		var modified := _is_modified(row)
		WorkbenchWidgets.set_modified(row["dot"], modified)
		if modified:
			overridden += 1
		if _is_unsent(row):
			unsent += 1
	if _apply_button != null:
		_apply_button.disabled = unsent == 0
	if _revert_button != null:
		_revert_button.disabled = unsent == 0 and not _staged
	if _status == null:
		return
	if not _outcome.is_empty():
		_status.text = _outcome
		_status.add_theme_color_override("font_color", _outcome_tint)
		return
	var text := WorkbenchVocab.TUNING_CLEAN_STATUS
	var tint := HudStyle.INK_FAINT
	if unsent > 0:
		text = WorkbenchVocab.TUNING_UNSENT_STATUS % unsent
		tint = HudStyle.WARN
	elif _staged:
		text = WorkbenchVocab.TUNING_STAGED_CLEARED_STATUS if overridden == 0 \
			else WorkbenchVocab.TUNING_STAGED_STATUS % overridden
		tint = HudStyle.SIGNAL
	_status.text = text
	_status.add_theme_color_override("font_color", tint)


## Does this row differ from the SHIPPED DEFAULT? Drives the dot.
func _is_modified(row: Dictionary) -> bool:
	return _differs(row, float(row["default"]))


## Does this row differ from what the SERVER WAS LAST TOLD? Drives Apply — and it is the predicate
## that survives a row being edited back to its default after an apply, which "modified" cannot.
func _is_unsent(row: Dictionary) -> bool:
	return _differs(row, float(row["sent"]))


## A row's value has moved off `baseline` once it clears half a step. A `SpinBox` snaps to its step,
## so any real edit clears that, and a float baseline off the step grid cannot read as a change.
func _differs(row: Dictionary, baseline: float) -> bool:
	var spin: SpinBox = row["spin"]
	var step: float = row["step"]
	return absf(spin.value - baseline) > step * WorkbenchVocab.TUNING_MODIFIED_STEP_FRACTION


## Show `text` in the action bar in place of the computed state, until the next control movement.
## The command feed gets the same line — but it ships hidden, so it cannot be the only place an
## outcome lands.
func _report(text: String, tint: Color) -> void:
	_outcome = text
	_outcome_tint = tint
	log_line(text)
	_refresh_state()


# ---- patches ---------------------------------------------------------------

## The overrides to hand the sim: config kind -> a nested dictionary carrying every parameter this
## page has TAKEN OWNERSHIP OF — the ones edited off their default, plus the ones it previously sent
## and has since edited back.
##
## **Sparse in the parameters it never touched.** Each config kind is a file the sim deep-merges over
## its shipped defaults, so a parameter absent from the patch is one the designer has not claimed:
## writing the untouched ones through at their current default would freeze them against any later
## change to the shipped config, and the diff would stop being readable as "what I tuned".
##
## **But a returned-to-default row must be written EXPLICITLY, not omitted** — that is the half this
## was missing. The server merges onto the file it already has, so omitting a row that an earlier
## apply put there leaves the old value in force while the panel shows the default. Sending it back
## at its default value overwrites it; nothing else the client can say does, because the command
## channel has no "unset this key".
func build_patches() -> Dictionary:
	var patches: Dictionary = {}
	for row in _rows:
		# `sent != default` is how a row that was applied and then edited back stays in the payload.
		var was_sent := absf(float(row["sent"]) - float(row["default"])) > 0.0
		if not _is_modified(row) and not was_sent:
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
	_outcome = ""
	var patches := build_patches()
	if patches.is_empty():
		return
	overrides_requested.emit(patches)

	var landed: Array = []
	for kind in patches:
		var line := "%s %s %s" % [WorkbenchVocab.COMMAND_SET_CONFIG_OVERRIDE, kind,
			JSON.stringify(patches[kind])]
		if not send_command(line):
			# Whatever went BEFORE this failed one did reach the server, so those kinds are recorded
			# as sent and the page stays staged — a partial apply is still an apply.
			_mark_sent(landed)
			_report(WorkbenchVocab.TUNING_OFFLINE_LOG
				% WorkbenchVocab.COMMAND_SET_CONFIG_OVERRIDE, HudStyle.DANGER)
			return
		landed.append(kind)

	# Every patch landed: the server is now holding exactly what the rows say.
	_mark_sent(landed)

	var new_game := service(WorkbenchVocab.SERVICE_NEW_GAME)
	if not new_game.is_valid():
		_report(WorkbenchVocab.TUNING_NO_NEW_GAME_LOG, HudStyle.DANGER)
		return
	new_game.call()

	var overridden := 0
	for row in _rows:
		if _is_modified(row):
			overridden += 1
	log_line(WorkbenchVocab.TUNING_APPLY_LOG_FORMAT % [overridden, ", ".join(patches.keys())])
	_refresh_state()


## Record that the server has been told the current value of every row in `kinds`, and that it is
## therefore holding overrides. This is the write that turns "edited" into "applied" — without it a
## row stays forever unsent and Apply never goes quiet.
func _mark_sent(kinds: Array) -> void:
	if kinds.is_empty():
		return
	_staged = true
	for row in _rows:
		if kinds.has(row["kind"]):
			var spin: SpinBox = row["spin"]
			row["sent"] = spin.value


## Return every row to its default AND drop the server's staged overrides, so the two cannot
## disagree: a cleared page over a server still holding the last patch would generate the next world
## on overrides the surface says are gone.
##
## **It stays reachable while anything is staged**, even with every row already reading its default —
## that is the state in which it is the only control that can do anything.
func _on_revert_pressed() -> void:
	_outcome = ""
	if send_command(WorkbenchVocab.COMMAND_CLEAR_CONFIG_OVERRIDES):
		_staged = false
		_reset_rows()
		log_line(WorkbenchVocab.TUNING_REVERT_LOG)
		_refresh_state()
	else:
		# The rows are LEFT ALONE: the server still holds the overrides, so clearing the surface
		# would be the same lie in the opposite direction.
		_report(WorkbenchVocab.TUNING_OFFLINE_LOG
			% WorkbenchVocab.COMMAND_CLEAR_CONFIG_OVERRIDES, HudStyle.DANGER)
