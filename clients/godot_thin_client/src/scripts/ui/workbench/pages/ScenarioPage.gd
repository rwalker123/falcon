extends WorkbenchPage
class_name ScenarioPage

## THE SCENARIO PAGE — edit a scenario JSON file, save it through the server, load it as the world.
##
## **State is the FILE and nothing else**: `_path`, the parsed document `_doc` (insertion order kept), and
## `_saved_canon` — what the server last read or accepted, in the form the page would write it back — for
## the dirty check. `_schema` and `_files` are the SERVER's (one schema, one directory), not the world's.
## The page consumes no snapshot.
##
## **The form is generated from the schema** (`pages/scenario/ScenarioForm.gd`, blind — it names no
## scenario key), and the server's refusals attach to the node they name
## (`pages/scenario/ScenarioRefusals.gd`). This file is the glue: the header (file list, new file), the
## pinned Save / Load bar, and the round trips.
##
## **Save writes only a file the server could load** (a trial world build); **Load** swaps the live world
## for the file on DISK, so it is live only when the page is clean and the file has been saved — the file
## on disk must be what is on screen.

## WORLD BOUNDARY — a documented NO-OP, like `ConfigTuningPage.reset`. The document is the designer's
## file, not the world's: Load rebuilds the world FROM it, and wiping it on the epoch change that Load
## itself causes would blank the page one frame after the world it describes appeared.
func reset() -> void:
	pass


# ---- state ---------------------------------------------------------------------
## The file being edited, as the server spells it (`core_sim/scenarios/<name>.json`); `""` = none open.
var _path := ""
## The parsed document. Dictionary, or null while no file is open.
var _doc: Variant = null
## What the server last read or accepted for `_path`, as `_stringify` writes it; `""` for a new file.
var _saved_canon := ""
## True for a file created in-page and never saved.
var _is_new := false
## The export path the open document was read from, when it was not a fixture; `""` otherwise.
var _opened_from := ""
var _schema: Dictionary = {}
var _files: Array = []
var _list_pending := false
var _busy := false
## The last Save/Load reply's refusals, parsed; kept until the next reply.
var _refusals: Array = []
## Banner lines for what the form cannot show (unmatched refusals, a parse failure, a transport failure).
var _banner_lines: Array = []
var _banner_tint := HudStyle.WARN
var _pick_armed := false
## While a switch-while-dirty is waiting on its second press: what it will do, and who asked.
var _pending_open := ""
var _pending_new := false
var _outcome := ""

var _form := ScenarioForm.new()

# ---- widgets -------------------------------------------------------------------
var _services_caption: Label = null
var _file_picker: OptionButton = null
var _refresh_button: Button = null
var _confirm_button: Button = null
var _name_edit: LineEdit = null
var _new_button: Button = null
var _banner_host: VBoxContainer = null
var _form_host: VBoxContainer = null
var _status: Label = null
var _save_button: Button = null
var _load_button: Button = null
var _dot: Label = null


# ---- build ---------------------------------------------------------------------

func build() -> void:
	add_theme_constant_override("separation", WorkbenchVocab.CONTENT_GAP)
	_services_caption = WorkbenchWidgets.build_caption("", HudStyle.WARN)
	add_child(_services_caption)
	add_child(WorkbenchWidgets.build_group(WorkbenchVocab.SCENARIO_HEADING_FILE, _build_file_rows()))
	_banner_host = VBoxContainer.new()
	add_child(_banner_host)
	_form_host = VBoxContainer.new()
	_form_host.add_theme_constant_override("separation", WorkbenchVocab.FORM_ROW_GAP)
	add_child(_form_host)
	_configure_form()
	tree_entered.connect(_on_tree_entered)
	_render()
	_request_list()


func _build_file_rows() -> Control:
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", WorkbenchWidgets.ROW_COLUMN_GAP)

	var pick_row := HBoxContainer.new()
	pick_row.add_theme_constant_override("separation", WorkbenchWidgets.ROW_COLUMN_GAP)
	_file_picker = OptionButton.new()
	HudStyle.apply_option_button(_file_picker)
	_file_picker.add_theme_font_size_override("font_size", WorkbenchVocab.FORM_BUTTON_FONT_SIZE)
	_file_picker.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_file_picker.clip_text = true
	_file_picker.item_selected.connect(_on_file_chosen)
	pick_row.add_child(_file_picker)
	_refresh_button = _button(WorkbenchVocab.SCENARIO_REFRESH_LABEL, "ghost")
	_refresh_button.pressed.connect(_request_list)
	pick_row.add_child(_refresh_button)
	column.add_child(pick_row)

	_confirm_button = _button(WorkbenchVocab.SCENARIO_DISCARD_LABEL, "armed")
	_confirm_button.pressed.connect(_on_confirm_pressed)
	_confirm_button.visible = false
	column.add_child(_confirm_button)

	var new_row := HBoxContainer.new()
	new_row.add_theme_constant_override("separation", WorkbenchWidgets.ROW_COLUMN_GAP)
	_name_edit = LineEdit.new()
	_name_edit.placeholder_text = WorkbenchVocab.SCENARIO_NEW_PLACEHOLDER
	_name_edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	HudStyle.apply_line_edit(_name_edit)
	_name_edit.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_VALUE)
	new_row.add_child(_name_edit)
	_new_button = _button(WorkbenchVocab.SCENARIO_NEW_LABEL, "ghost")
	_new_button.pressed.connect(_on_new_pressed)
	new_row.add_child(_new_button)
	column.add_child(new_row)
	return column


func build_actions() -> Control:
	var footer := VBoxContainer.new()
	footer.add_theme_constant_override("separation", WorkbenchWidgets.GROUP_HEADER_GAP)
	_status = WorkbenchWidgets.build_caption("")
	footer.add_child(_status)
	var actions := HBoxContainer.new()
	actions.add_theme_constant_override("separation", WorkbenchWidgets.ROW_COLUMN_GAP)
	_dot = WorkbenchWidgets.build_modified_dot()
	actions.add_child(_dot)
	_save_button = _button(WorkbenchVocab.SCENARIO_SAVE_LABEL, "primary")
	_save_button.pressed.connect(_on_save_pressed)
	actions.add_child(_save_button)
	_load_button = _button(WorkbenchVocab.SCENARIO_LOAD_LABEL, "ghost")
	_load_button.pressed.connect(_on_load_pressed)
	actions.add_child(_load_button)
	footer.add_child(actions)
	_refresh_state()
	return footer


func _button(text: String, variant: String) -> Button:
	var button := Button.new()
	button.text = text
	button.focus_mode = Control.FOCUS_NONE
	button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	button.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_BODY)
	HudStyle.apply_button(button, variant)
	return button


func _configure_form() -> void:
	_form.configure(_on_form_edited, _render_form,
		Callable(self, "_arm_pick") if service(WorkbenchVocab.SERVICE_PICK_TILE).is_valid() else Callable())


## The host re-supplied its services: the pick buttons' enabled state follows them.
func set_services(services: Dictionary) -> void:
	super.set_services(services)
	if _form_host != null:
		_configure_form()
		_render()


func _on_tree_entered() -> void:
	# A reopen with no list (the server was not there on first show) asks again.
	if _files.is_empty() and not _list_pending:
		_request_list()


# ---- the file list -------------------------------------------------------------

func _request_list() -> void:
	var ask := service(WorkbenchVocab.SERVICE_SCENARIO_REQUEST)
	if not ask.is_valid():
		return
	_list_pending = true
	if not bool(ask.call(WorkbenchVocab.SCENARIO_KIND_LIST, {}, _on_list_reply)):
		_list_pending = false


func _on_list_reply(reply: Dictionary) -> void:
	_list_pending = false
	if not bool(reply.get(WorkbenchVocab.SCENARIO_REPLY_OK, false)):
		_show_failure(reply, "")
		return
	_files = reply.get(WorkbenchVocab.SCENARIO_REPLY_FILES, []) as Array
	var parsed: Variant = JSON.parse_string(String(reply.get(WorkbenchVocab.SCENARIO_REPLY_SCHEMA, "")))
	_schema = parsed if parsed is Dictionary else {}
	if _doc is Dictionary and not _schema.is_empty():
		# A document held before the schema arrived is made honest about integers now.
		_doc = ScenarioSchema.normalize(_doc, _schema)
	_fill_file_picker()
	_render()


func _fill_file_picker() -> void:
	if _file_picker == null:
		return
	_file_picker.clear()
	_file_picker.add_item(WorkbenchVocab.SCENARIO_FILE_PLACEHOLDER)
	var selected := 0
	for i in _files.size():
		var listed := String(_files[i])
		_file_picker.add_item(listed.trim_prefix(WorkbenchVocab.SCENARIO_DIR_PREFIX)
			if _is_fixture_path(listed) else WorkbenchVocab.SCENARIO_EXPORT_ITEM_FORMAT % listed.get_file())
		_file_picker.set_item_metadata(i + 1, listed)
		if listed == (_opened_from if not _opened_from.is_empty() else _path):
			selected = i + 1
	_file_picker.select(selected)


func _on_file_chosen(index: int) -> void:
	release_field_focus()
	if index <= 0:
		_fill_file_picker()
		return
	var path := String(_file_picker.get_item_metadata(index))
	if path == (_opened_from if not _opened_from.is_empty() else _path):
		return
	if _is_dirty():
		# The press that asked is undone and a second one is asked for.
		_pending_open = path
		_pending_new = false
		_fill_file_picker()
		_refresh_state()
		return
	_open_file(path)


func _on_confirm_pressed() -> void:
	var path := _pending_open
	_pending_open = ""
	if not path.is_empty():
		_open_file(path)
	_refresh_state()


func _open_file(path: String) -> void:
	var ask := service(WorkbenchVocab.SERVICE_SCENARIO_REQUEST)
	if not ask.is_valid():
		return
	_busy = true
	if not bool(ask.call(WorkbenchVocab.SCENARIO_KIND_READ, {"path": path}, _on_read_reply.bind(path))):
		_busy = false
	_refresh_state()


func _on_read_reply(reply: Dictionary, path: String) -> void:
	_busy = false
	if not bool(reply.get(WorkbenchVocab.SCENARIO_REPLY_OK, false)):
		_show_failure(reply, path)
		return
	var text := String(reply.get(WorkbenchVocab.SCENARIO_REPLY_TEXT, ""))
	adopt_text(path, text)


## Parse `text` as the content of `path` and make it the open document. A parse failure keeps the
## previous document and says so in the banner. Public so a harness can seat a document without a server.
func adopt_text(path: String, text: String) -> bool:
	var json := JSON.new()
	if json.parse(text) != OK or not (json.data is Dictionary):
		var why := "line %d: %s" % [json.get_error_line(), json.get_error_message()] \
			if json.get_error_message() != "" else "not a JSON object"
		_banner_lines = [WorkbenchVocab.SCENARIO_UNREADABLE % [path, why]]
		_banner_tint = HudStyle.DANGER
		_render_banner([])
		_refresh_state()
		return false
	var doc: Dictionary = json.data
	if not _schema.is_empty():
		doc = ScenarioSchema.normalize(doc, _schema)
	# A file outside the fixture directory is an export: the document is what was read, but the path is
	# the fixture one it would save to, and nothing is saved there yet — so it is dirty and Load waits.
	_opened_from = "" if _is_fixture_path(path) else path
	_path = path if _opened_from.is_empty() else WorkbenchVocab.SCENARIO_DIR_PREFIX + path.get_file()
	_doc = doc
	_is_new = not _opened_from.is_empty()
	_saved_canon = "" if _is_new else _stringify(doc)
	_refusals = []
	_banner_lines = []
	_pending_open = ""
	_pending_new = false
	_outcome = ""
	_form.reset_open_state()
	_fill_file_picker()
	_render()
	return true


# ---- a new file ----------------------------------------------------------------

func _on_new_pressed() -> void:
	release_field_focus()
	if _is_dirty() and not _pending_new:
		_pending_new = true
		_pending_open = ""
		_refresh_state()
		return
	_pending_new = false
	_pending_open = ""
	var file_name := _name_edit.text.strip_edges()
	var valid := not file_name.is_empty()
	for i in file_name.length():
		var code := file_name.unicode_at(i)
		var ok := (code >= 48 and code <= 57) or (code >= 65 and code <= 90) \
			or (code >= 97 and code <= 122) or code == 45 or code == 95
		valid = valid and ok
	if not valid:
		_banner_lines = [WorkbenchVocab.SCENARIO_BAD_NAME]
		_banner_tint = HudStyle.WARN
		_render_banner([])
		_refresh_state()
		return
	var path := WorkbenchVocab.SCENARIO_DIR_PREFIX + file_name + WorkbenchVocab.SCENARIO_FILE_SUFFIX
	if _files.has(path):
		_banner_lines = [WorkbenchVocab.SCENARIO_NAME_EXISTS % path]
		_banner_tint = HudStyle.WARN
		_render_banner([])
		_refresh_state()
		return
	if _schema.is_empty():
		_banner_lines = [WorkbenchVocab.SCENARIO_NO_SCHEMA]
		_banner_tint = HudStyle.WARN
		_render_banner([])
		return
	_path = path
	_opened_from = ""
	_doc = ScenarioSchema.skeleton(_schema)
	_is_new = true
	_saved_canon = ""
	_refusals = []
	_banner_lines = []
	_form.reset_open_state()
	_fill_file_picker()
	_render()


# ---- save / load ---------------------------------------------------------------

func _on_save_pressed() -> void:
	release_field_focus()
	var ask := service(WorkbenchVocab.SERVICE_SCENARIO_REQUEST)
	if not ask.is_valid() or not (_doc is Dictionary) or _busy:
		return
	var text := _stringify(_doc)
	_busy = true
	var args := {"path": _path, "text": text}
	if not bool(ask.call(WorkbenchVocab.SCENARIO_KIND_WRITE, args, _on_save_reply.bind(text, _path))):
		_busy = false
		_outcome = WorkbenchVocab.SCENARIO_NO_SERVICES
	_refresh_state()


func _on_save_reply(reply: Dictionary, text: String, path: String) -> void:
	_busy = false
	if bool(reply.get(WorkbenchVocab.SCENARIO_REPLY_OK, false)):
		_saved_canon = text
		_is_new = false
		_opened_from = ""
		_refusals = []
		_banner_lines = []
		log_line(WorkbenchVocab.SCENARIO_SAVED_LOG % path)
		_request_list()
		_render()
		return
	_apply_refusals(reply, path)


func _on_load_pressed() -> void:
	var ask := service(WorkbenchVocab.SERVICE_SCENARIO_REQUEST)
	if not ask.is_valid() or not _load_is_live():
		return
	_busy = true
	if not bool(ask.call(WorkbenchVocab.SCENARIO_KIND_LOAD, {"path": _path}, _on_load_reply.bind(_path))):
		_busy = false
		_outcome = WorkbenchVocab.SCENARIO_NO_SERVICES
	_refresh_state()


func _on_load_reply(reply: Dictionary, path: String) -> void:
	_busy = false
	if bool(reply.get(WorkbenchVocab.SCENARIO_REPLY_OK, false)):
		_refusals = []
		_banner_lines = []
		log_line(WorkbenchVocab.SCENARIO_LOADED_LOG % path)
		_render()
		return
	_apply_refusals(reply, path)


## Take a refused Save/Load: the refusals REPLACE the previous reply's and stay until the next one. A
## reply with none (a dead socket) says so in the banner instead.
func _apply_refusals(reply: Dictionary, path: String) -> void:
	var lines: Array = reply.get(WorkbenchVocab.SCENARIO_REPLY_REFUSALS, []) as Array
	if lines.is_empty():
		_refusals = []
		_show_failure(reply, path)
		return
	_refusals = ScenarioRefusals.parse_all(lines)
	_banner_lines = []
	log_line(WorkbenchVocab.SCENARIO_REFUSED_LOG % [path, lines.size()])
	_render()


## A reply that failed without refusals — the transport token, or a read/list refusal.
func _show_failure(reply: Dictionary, path: String) -> void:
	var lines: Array = []
	for line in reply.get(WorkbenchVocab.SCENARIO_REPLY_REFUSALS, []) as Array:
		lines.append(String(line))
	if lines.is_empty():
		lines.append(WorkbenchVocab.SCENARIO_TRANSPORT_LOG
			% String(reply.get(WorkbenchVocab.SCENARIO_REPLY_ERROR, "")))
	_banner_lines = lines
	_banner_tint = HudStyle.DANGER
	_render_banner([])
	_refresh_state()


# ---- document ------------------------------------------------------------------

## The text Save writes and the dirty check compares: two-space indent, keys in document order, full
## float precision, and the trailing newline the checked-in files carry.
static func _stringify(doc: Variant) -> String:
	return JSON.stringify(doc, WorkbenchVocab.SCENARIO_INDENT, false, true) \
		+ WorkbenchVocab.SCENARIO_TRAILING_NEWLINE


static func _is_fixture_path(path: String) -> bool:
	return path.begins_with(WorkbenchVocab.SCENARIO_DIR_PREFIX)


func _is_dirty() -> bool:
	return _doc is Dictionary and _stringify(_doc) != _saved_canon


## Load swaps the world for the file on DISK, so it is live only when that is what is on screen.
func _load_is_live() -> bool:
	return not _path.is_empty() and not _is_new and not _is_dirty() and not _busy \
		and service(WorkbenchVocab.SERVICE_SCENARIO_REQUEST).is_valid()


func _on_form_edited() -> void:
	_pending_open = ""
	_pending_new = false
	_outcome = ""
	_refresh_state()


# ---- pick ----------------------------------------------------------------------

func _arm_pick(on_pick: Callable) -> void:
	var arm := service(WorkbenchVocab.SERVICE_PICK_TILE)
	if not arm.is_valid():
		return
	var picked := func(x: int, y: int) -> void:
		_pick_armed = false
		on_pick.call(x, y)
		_refresh_state()
	var cancelled := func() -> void:
		_pick_armed = false
		_refresh_state()
	_pick_armed = true
	if not bool(arm.call(picked, cancelled)):
		_pick_armed = false
	_refresh_state()


# ---- render --------------------------------------------------------------------

func _render() -> void:
	if _form_host == null:
		return
	_render_form()
	_refresh_state()


func _render_form() -> void:
	if _form_host == null:
		return
	HudWidgets.clear_children(_form_host)
	if _schema.is_empty():
		_form_host.add_child(WorkbenchWidgets.build_caption(WorkbenchVocab.SCENARIO_NO_SCHEMA))
		_render_banner([])
		return
	if not (_doc is Dictionary):
		_form_host.add_child(WorkbenchWidgets.build_caption(WorkbenchVocab.SCENARIO_NO_FILE))
		_render_banner([])
		return
	_form.set_document(_schema, _doc)
	var unmatched := _form.build(_form_host, _refusals)
	_render_banner(unmatched)
	_refresh_state()


## The banner: `_banner_lines` (a failure the page itself reports) and the refusals that named no node.
func _render_banner(unmatched: Array) -> void:
	if _banner_host == null:
		return
	HudWidgets.clear_children(_banner_host)
	var lines: Array = _banner_lines.duplicate()
	for refusal in unmatched:
		lines.append(String(refusal.get("line", "")))
	if lines.is_empty():
		return
	var tint := _banner_tint if not _banner_lines.is_empty() else HudStyle.WARN
	var text := "\n".join(PackedStringArray(lines))
	if _banner_lines.is_empty():
		text = WorkbenchVocab.SCENARIO_BANNER_HEADING % _path + "\n" + text
	_banner_host.add_child(WorkbenchWidgets.build_banner(text, tint))


## Settle every control that depends on state: header buttons, the pinned bar, the dirty dot.
func _refresh_state() -> void:
	var have_ask := service(WorkbenchVocab.SERVICE_SCENARIO_REQUEST).is_valid()
	if _services_caption != null:
		var caption := ""
		if not have_ask:
			caption = WorkbenchVocab.SCENARIO_NO_SERVICES
		elif not service(WorkbenchVocab.SERVICE_PICK_TILE).is_valid():
			caption = WorkbenchVocab.SCENARIO_NO_MAP
		_services_caption.text = caption
		_services_caption.visible = not caption.is_empty()
	if _file_picker != null:
		_file_picker.disabled = not have_ask or _busy
		_refresh_button.disabled = not have_ask or _list_pending
		_new_button.disabled = _schema.is_empty()
		_new_button.text = WorkbenchVocab.SCENARIO_DISCARD_LABEL if _pending_new \
			else WorkbenchVocab.SCENARIO_NEW_LABEL
		_confirm_button.visible = not _pending_open.is_empty()
		_confirm_button.text = WorkbenchVocab.SCENARIO_DISCARD_LABEL
	var dirty := _is_dirty()
	if _dot != null:
		WorkbenchWidgets.set_modified(_dot, dirty)
	if _save_button != null:
		_save_button.disabled = not have_ask or not (_doc is Dictionary) or _busy \
			or (not dirty and not _is_new)
		_load_button.disabled = not _load_is_live()
	if _status != null:
		_status.text = _status_text(dirty)


func _status_text(dirty: bool) -> String:
	if _pick_armed:
		return WorkbenchVocab.SCENARIO_PICK_ARMED
	if not _outcome.is_empty():
		return _outcome
	if _path.is_empty():
		return WorkbenchVocab.SCENARIO_NO_FILE
	if not _opened_from.is_empty():
		return WorkbenchVocab.SCENARIO_EXPORT_STATUS % [_opened_from.get_file(), _path]
	if _is_new:
		return WorkbenchVocab.SCENARIO_NEW_STATUS % _path
	if dirty:
		return WorkbenchVocab.SCENARIO_DIRTY_STATUS % _path
	return WorkbenchVocab.SCENARIO_CLEAN_STATUS % _path


# ---- read by the harness ---------------------------------------------------------

func document() -> Variant:
	return _doc


func current_path() -> String:
	return _path


func form() -> ScenarioForm:
	return _form


## Seat a schema without a server (the preview harness has none).
func adopt_schema(schema: Dictionary, files: Array) -> void:
	_schema = schema
	_files = files
	_fill_file_picker()
	_render()
