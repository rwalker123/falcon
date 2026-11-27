extends RefCounted
class_name ScenarioForm

## THE SCHEMA-WALKED FORM — draws one scenario document from the scenario file's JSON Schema, BLIND.
##
## **The property that matters: a key added to any section of the file appears here with no client
## edit.** Nothing in this file names a scenario key; it reads the SHAPE of each schema node
## (`ScenarioSchema`) and draws the control for that shape, keyed by the file's own spelling — never
## prettified, because the designer's next move is to search the file for what they read.
##
## Every edit mutates the document IN PLACE (the containers are references) and calls back; a change of
## SHAPE (a key added or cleared, an item added or removed, a pick that fills keys) calls back a second
## time so the owner rebuilds. A rebuild keeps `_open` — which blocks the designer had open — so adding
## an item does not collapse everything.
##
## It owns no document and no file: the page hands both in.

# ---- wiring ------------------------------------------------------------------
## `() -> void` — a value in the document changed.
var _on_edit: Callable = Callable()
## `() -> void` — the shape of the document changed; rebuild.
var _on_structure: Callable = Callable()
## `(on_pick: Callable) -> bool` — arm a one-shot map pick. Invalid when no map is attached.
var _arm_pick: Callable = Callable()

# ---- state -------------------------------------------------------------------
var _schema: Dictionary = {}
var _doc: Variant = null
## path -> bool: the blocks the designer opened or closed. Survives rebuilds; cleared when a different
## file is opened.
var _open: Dictionary = {}
## path -> {body, button, title}: every collapsible block of the current build.
var _blocks: Dictionary = {}
## path -> VBoxContainer: where a refusal for that node is attached.
var _notes: Dictionary = {}
## The declared band/herd ids at the last build, so a rename is noticed on focus-out.
var _signature := ""


func configure(on_edit: Callable, on_structure: Callable, arm_pick: Callable) -> void:
	_on_edit = on_edit
	_on_structure = on_structure
	_arm_pick = arm_pick


func set_document(schema: Dictionary, doc: Variant) -> void:
	_schema = schema
	_doc = doc


## Forget which blocks were open — a different file starts from the defaults.
func reset_open_state() -> void:
	_open.clear()


## Open or close every block of the current build (the designer's expand-all / collapse-all).
func set_all_open(open: bool) -> void:
	for path in _blocks:
		_set_open(String(path), open)


func open_paths() -> Dictionary:
	return _open


## The block registry of the last build (path -> {body, button, title}) — read by the preview harness.
func blocks() -> Dictionary:
	return _blocks


func notes() -> Dictionary:
	return _notes


## Draw the document into `host`, then attach each refusal (`ScenarioRefusals.parse` dictionaries) to the
## deepest node drawn whose path is a prefix of the refusal's, opening its ancestors. Returns the
## refusals that named no drawn node, for the page's banner.
func build(host: Control, refusals: Array) -> Array:
	_blocks.clear()
	_notes.clear()
	_signature = _declared_signature()
	var unmatched: Array = []
	if _schema.is_empty() or not (_doc is Dictionary):
		return refusals.duplicate()
	_add_object_properties(host, _schema, _doc, "", 0)
	for refusal in refusals:
		if not _attach(refusal):
			unmatched.append(refusal)
	return unmatched


func _attach(refusal: Dictionary) -> bool:
	var path: String = refusal.get("path", "")
	if path.is_empty():
		return false
	var best := ""
	for node_path in _notes:
		if ScenarioRefusals.is_prefix(String(node_path), path) and String(node_path).length() > best.length():
			best = String(node_path)
	if best.is_empty():
		return false
	var box: VBoxContainer = _notes[best]
	box.add_child(WorkbenchWidgets.build_caption(String(refusal.get("reason", "")), HudStyle.WARN))
	for block_path in _blocks:
		if ScenarioRefusals.is_prefix(String(block_path), best):
			_set_open(String(block_path), true)
	return true


# ---- objects and properties ----------------------------------------------------

func _add_object_properties(host: Control, node: Dictionary, obj: Dictionary, path: String,
		depth: int) -> void:
	var props := ScenarioSchema.properties(node)
	for key in ScenarioSchema.ordered_keys(node):
		_add_property(host, node, props[key], obj, String(key), path, depth)
	# A key the schema does not declare: shown, with a remove, so the form never hides part of the file.
	for key in obj.keys():
		if not props.has(key):
			host.add_child(_unknown_row(obj, String(key), path))


func _add_property(host: Control, parent_node: Dictionary, prop: Dictionary, obj: Dictionary,
		key: String, path: String, depth: int) -> void:
	var p := _join(path, key)
	if obj.has(key):
		var remove := Callable()
		if not ScenarioSchema.is_required(parent_node, key):
			remove = func() -> void:
				obj.erase(key)
				_structure()
		_add_value(host, prop, obj, key, p, depth, key, remove)
		return
	var add := Button.new()
	add.text = WorkbenchVocab.FORM_ADD_FORMAT % key
	add.tooltip_text = ScenarioSchema.description(prop)
	add.alignment = HORIZONTAL_ALIGNMENT_LEFT
	add.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	_style_button(add)
	add.pressed.connect(func() -> void:
		obj[key] = ScenarioSchema.skeleton(prop)
		ScenarioSchema.reorder(obj, parent_node)
		_open[p] = true
		_structure())
	host.add_child(add)


## One value of any shape, drawn into `host`. `container[slot]` IS the value — the controls write there.
func _add_value(host: Control, node: Dictionary, container: Variant, slot: Variant, path: String,
		depth: int, label: String, remove: Callable) -> void:
	if depth > WorkbenchVocab.FORM_MAX_DEPTH:
		host.add_child(WorkbenchWidgets.build_caption(WorkbenchVocab.CONFIG_TOO_DEEP))
		return
	var value: Variant = container[slot]
	if ScenarioSchema.is_const_choice(node):
		host.add_child(_choice_row(label, node, container, slot, path, remove))
		return
	if ScenarioSchema.is_variant_choice(node):
		_add_variant(host, node, container, slot, path, depth, label, remove)
		return
	if value == null:
		host.add_child(_unset_row(label, node, container, slot, path, remove))
		return
	match ScenarioSchema.base_type(node):
		ScenarioSchema.TYPE_OBJECT:
			if not (value is Dictionary):
				host.add_child(_raw_row(label, value, path, remove))
			elif ScenarioSchema.is_map(node):
				_add_map(host, node, value, path, depth, label, remove)
			else:
				_add_struct(host, node, value, path, depth, label, remove)
		ScenarioSchema.TYPE_ARRAY:
			if not (value is Array):
				host.add_child(_raw_row(label, value, path, remove))
			elif ScenarioSchema.pick_hint(node) == WorkbenchVocab.SCHEMA_PICK_TILE:
				host.add_child(_tile_row(label, node, container, slot, path, remove))
			else:
				_add_array(host, node, value, path, depth, label, remove)
		ScenarioSchema.TYPE_STRING:
			host.add_child(_string_row(label, node, container, slot, path, remove))
		ScenarioSchema.TYPE_INTEGER, ScenarioSchema.TYPE_NUMBER:
			if ScenarioBigInt.is_big(value):
				host.add_child(_big_int_row(label, node, container, slot, path, remove))
			else:
				host.add_child(_number_row(label, node, container, slot, path, remove))
		ScenarioSchema.TYPE_BOOLEAN:
			host.add_child(_bool_row(label, node, container, slot, path, remove))
		_:
			host.add_child(_raw_row(label, value, path, remove))


func _add_struct(host: Control, node: Dictionary, obj: Dictionary, path: String, depth: int,
		label: String, remove: Callable) -> void:
	var extras: Array = []
	var tile_keys := ScenarioSchema.pick_tile_keys(node)
	if not tile_keys.is_empty():
		extras.append(_tile_keys_button(obj, String(tile_keys[0]), String(tile_keys[1])))
	var body := _block(host, label, ScenarioSchema.description(node), path, depth, extras, remove, "")
	_add_object_properties(body, node, obj, path, depth + 1)


func _add_variant(host: Control, node: Dictionary, container: Variant, slot: Variant, path: String,
		depth: int, label: String, remove: Callable) -> void:
	var value: Variant = container[slot]
	if not (value is Dictionary) or (value as Dictionary).size() != 1:
		host.add_child(_raw_row(label, value, path, remove))
		return
	var obj: Dictionary = value
	var variant_name := String(obj.keys()[0])
	var variant := ScenarioSchema.variant_named(node, variant_name)
	if variant.is_empty() or not (obj[variant_name] is Dictionary):
		host.add_child(_raw_row(label, value, path, remove))
		return
	var inner: Dictionary = ScenarioSchema.properties(variant)[variant_name]
	var extras: Array = []
	var tile_keys := ScenarioSchema.pick_tile_keys(inner)
	if not tile_keys.is_empty():
		extras.append(_tile_keys_button(obj[variant_name], String(tile_keys[0]), String(tile_keys[1])))
	var title := WorkbenchVocab.FORM_VARIANT_TITLE_FORMAT % [label, variant_name]
	var description := ScenarioSchema.description(variant)
	if description.is_empty():
		description = ScenarioSchema.description(inner)
	var body := _block(host, title, description, path, depth, extras, remove, "")
	_add_object_properties(body, inner, obj[variant_name], _join(path, variant_name), depth + 1)


func _add_array(host: Control, node: Dictionary, list: Array, path: String, depth: int,
		label: String, remove: Callable) -> void:
	var body := _block(host, label, ScenarioSchema.description(node), path, depth, [], remove,
		WorkbenchVocab.FORM_COUNT_FORMAT % list.size())
	var item_schema := ScenarioSchema.items(node)
	for i in list.size():
		var item_remove := func() -> void:
			list.remove_at(i)
			_structure()
		_add_value(body, item_schema, list, i, _index(path, i), depth + 1,
			WorkbenchVocab.FORM_ITEM_FORMAT % [label, i], item_remove)
	body.add_child(_add_item_row(item_schema, list, path))


## The `+ add` row under an array — a plain button, or, for a oneOf-of-objects, an OptionButton of the
## variant keys beside it.
func _add_item_row(item_schema: Dictionary, list: Array, path: String) -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", WorkbenchWidgets.ROW_COLUMN_GAP)
	var picker: OptionButton = null
	if ScenarioSchema.is_variant_choice(item_schema):
		picker = OptionButton.new()
		_style_picker(picker)
		for variant_name in ScenarioSchema.variant_keys(item_schema):
			picker.add_item(String(variant_name))
		row.add_child(picker)
	var add := Button.new()
	add.text = WorkbenchVocab.FORM_ADD_ROW_LABEL
	_style_button(add)
	add.pressed.connect(func() -> void:
		var schema := item_schema
		if picker != null:
			schema = ScenarioSchema.variant_named(item_schema, picker.get_item_text(picker.selected))
		list.append(ScenarioSchema.skeleton(schema))
		_open[_index(path, list.size() - 1)] = true
		_structure())
	row.add_child(add)
	return row


func _add_map(host: Control, node: Dictionary, obj: Dictionary, path: String, depth: int,
		label: String, remove: Callable) -> void:
	var body := _block(host, label, ScenarioSchema.description(node), path, depth, [], remove,
		WorkbenchVocab.FORM_COUNT_FORMAT % obj.size())
	var value_schema := ScenarioSchema.map_value_schema(node)
	for key in obj.keys():
		var row_remove := func() -> void:
			obj.erase(key)
			_structure()
		_add_value(body, value_schema, obj, key, _join(path, String(key)), depth + 1, String(key),
			row_remove)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", WorkbenchWidgets.ROW_COLUMN_GAP)
	var key_edit := LineEdit.new()
	key_edit.placeholder_text = WorkbenchVocab.FORM_MAP_KEY_PLACEHOLDER
	key_edit.custom_minimum_size.x = WorkbenchVocab.FORM_VALUE_MIN_WIDTH
	key_edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_style_line_edit(key_edit)
	row.add_child(key_edit)
	var add := Button.new()
	add.text = WorkbenchVocab.FORM_ADD_ROW_LABEL
	_style_button(add)
	add.pressed.connect(func() -> void:
		var key := key_edit.text.strip_edges()
		if key.is_empty() or obj.has(key):
			return
		obj[key] = ScenarioSchema.skeleton(value_schema)
		_structure())
	row.add_child(add)
	body.add_child(row)


# ---- rows ----------------------------------------------------------------------

## One leaf: a wrapper holding the row and, under it, the notes box refusals attach to.
func _row(label: String, node: Dictionary, path: String, controls: Array, remove: Callable) -> VBoxContainer:
	var wrapper := VBoxContainer.new()
	wrapper.add_theme_constant_override("separation", WorkbenchVocab.FORM_NOTE_GAP)
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", WorkbenchWidgets.ROW_COLUMN_GAP)
	var name_label := WorkbenchWidgets.build_caption(label, HudStyle.INK_DIM)
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_label.custom_minimum_size.x = WorkbenchVocab.FORM_KEY_MIN_WIDTH
	name_label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	name_label.mouse_filter = Control.MOUSE_FILTER_PASS
	name_label.tooltip_text = ScenarioSchema.description(node)
	line.add_child(name_label)
	for control in controls:
		line.add_child(control)
	if remove.is_valid():
		line.add_child(_remove_button(remove))
	wrapper.add_child(line)
	var notes := VBoxContainer.new()
	notes.add_theme_constant_override("separation", WorkbenchVocab.FORM_NOTE_GAP)
	wrapper.add_child(notes)
	_notes[path] = notes
	return wrapper


func _string_row(label: String, node: Dictionary, container: Variant, slot: Variant, path: String,
		remove: Callable) -> Control:
	var value: Variant = container[slot]
	var pick := ScenarioSchema.pick_hint(node)
	if pick == WorkbenchVocab.SCHEMA_PICK_BAND or pick == WorkbenchVocab.SCHEMA_PICK_HERD:
		return _row(label, node, path, [_id_picker(node, container, slot, pick)], remove)
	var edit := LineEdit.new()
	edit.text = String(value)
	edit.custom_minimum_size.x = WorkbenchVocab.FORM_VALUE_MIN_WIDTH
	edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_style_line_edit(edit)
	edit.text_changed.connect(func(text: String) -> void:
		container[slot] = text
		_edited())
	# A rename of a band or herd id changes what the picks offer, so the form rebuilds once the field
	# is left — not per keystroke, which would take the caret away from it.
	edit.focus_exited.connect(func() -> void:
		if _declared_signature() != _signature:
			_structure())
	return _row(label, node, path, [edit], remove)


## An integer beyond 2^53, which a SpinBox (a double) cannot hold: its digits in a text field, validated
## against the node's bounds and `u64`/`i64` on every keystroke. An invalid entry is tinted and NOT written.
func _big_int_row(label: String, node: Dictionary, container: Variant, slot: Variant, path: String,
		remove: Callable) -> Control:
	var edit := LineEdit.new()
	edit.text = ScenarioBigInt.digits(String(container[slot]))
	edit.custom_minimum_size.x = WorkbenchVocab.FORM_VALUE_MIN_WIDTH
	edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_style_line_edit(edit)
	edit.text_changed.connect(func(text: String) -> void:
		var parsed: Variant = ScenarioBigInt.parse_edit(text, node)
		edit.add_theme_color_override("font_color", HudStyle.INK if parsed != null else HudStyle.DANGER)
		if parsed != null:
			container[slot] = parsed
			_edited())
	return _row(label, node, path, [edit], remove)


func _number_row(label: String, node: Dictionary, container: Variant, slot: Variant, path: String,
		remove: Callable) -> Control:
	var spin := _number_field(node, float(container[slot]))
	var is_int := ScenarioSchema.types(node).has(ScenarioSchema.TYPE_INTEGER) \
		and not ScenarioSchema.types(node).has(ScenarioSchema.TYPE_NUMBER)
	spin.value_changed.connect(func(v: float) -> void:
		container[slot] = int(v) if is_int else v
		_edited())
	return _row(label, node, path, [WorkbenchWidgets.wrap_number_field(spin, WorkbenchWidgets.CONTROL_WIDTH)],
		remove)


## A number field honouring the node's `minimum` / `maximum`.
func _number_field(node: Dictionary, value: float) -> SpinBox:
	var is_int := ScenarioSchema.types(node).has(ScenarioSchema.TYPE_INTEGER) \
		and not ScenarioSchema.types(node).has(ScenarioSchema.TYPE_NUMBER)
	var minimum := float(node.get(ScenarioSchema.KEY_MINIMUM, WorkbenchVocab.FORM_FALLBACK_MIN))
	var maximum := float(node.get(ScenarioSchema.KEY_MAXIMUM, WorkbenchVocab.FORM_FALLBACK_MAX))
	var step := WorkbenchVocab.FORM_INT_STEP if is_int else WorkbenchVocab.FORM_FLOAT_STEP
	# A value already outside its own bounds is shown, not silently clamped away — the server is the one
	# to refuse it.
	return WorkbenchWidgets.build_number_field(value, minf(minimum, value), maxf(maximum, value), step,
		is_int)


func _bool_row(label: String, node: Dictionary, container: Variant, slot: Variant, path: String,
		remove: Callable) -> Control:
	var box := CheckBox.new()
	box.button_pressed = bool(container[slot])
	box.focus_mode = Control.FOCUS_NONE
	HudStyle.apply_checkbox(box)
	box.toggled.connect(func(on: bool) -> void:
		container[slot] = on
		_edited())
	return _row(label, node, path, [box], remove)


func _choice_row(label: String, node: Dictionary, container: Variant, slot: Variant, path: String,
		remove: Callable) -> Control:
	var picker := OptionButton.new()
	_style_picker(picker)
	var values := ScenarioSchema.const_values(node)
	var selected := -1
	for i in values.size():
		picker.add_item(str(values[i]))
		picker.set_item_metadata(i, values[i])
		if values[i] == container[slot]:
			selected = i
	if selected < 0:
		# A value the schema does not list is kept and shown as it is, never replaced.
		picker.add_item(str(container[slot]) + WorkbenchVocab.FORM_UNDECLARED_SUFFIX)
		picker.set_item_metadata(values.size(), container[slot])
		selected = values.size()
	picker.select(selected)
	picker.item_selected.connect(func(index: int) -> void:
		container[slot] = picker.get_item_metadata(index)
		_edited())
	return _row(label, node, path, [picker], remove)


## OptionButton of the ids the document declares (`band` / `herd`), a null entry when nullable, and the
## current value as it is when it is not declared.
func _id_picker(node: Dictionary, container: Variant, slot: Variant, pick: String) -> OptionButton:
	var picker := OptionButton.new()
	_style_picker(picker)
	picker.custom_minimum_size.x = WorkbenchVocab.FORM_VALUE_MIN_WIDTH
	var entries: Array = []
	if ScenarioSchema.is_nullable(node):
		entries.append(null)
	var declared := declared_ids(_doc, pick)
	for id in declared:
		entries.append(id)
	var current: Variant = container[slot]
	if not entries.has(current):
		entries.append(current)
	var selected := 0
	for i in entries.size():
		var entry: Variant = entries[i]
		var face := WorkbenchVocab.FORM_UNSET if entry == null else String(entry)
		if entry is String and (entry as String).is_empty():
			face = WorkbenchVocab.FORM_EMPTY_STRING_FACE
		if entry != null and not declared.has(entry):
			face += WorkbenchVocab.FORM_UNDECLARED_SUFFIX
		picker.add_item(face)
		picker.set_item_metadata(i, entry)
		if entry == current:
			selected = i
	picker.select(selected)
	picker.item_selected.connect(func(index: int) -> void:
		container[slot] = picker.get_item_metadata(index)
		_edited())
	return picker


## `[x, y]` — two compact integer fields and a pick button, on one line.
func _tile_row(label: String, node: Dictionary, container: Variant, slot: Variant, path: String,
		remove: Callable) -> Control:
	var tile: Array = container[slot]
	var item_schema := ScenarioSchema.items(node)
	var controls: Array = []
	var holder := HBoxContainer.new()
	holder.add_theme_constant_override("separation", WorkbenchWidgets.ROW_COLUMN_GAP)
	holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	for axis in 2:
		var spin := _number_field(item_schema, float(tile[axis]) if axis < tile.size() else 0.0)
		spin.value_changed.connect(func(v: float) -> void:
			while tile.size() <= axis:
				tile.append(0)
			tile[axis] = int(v)
			_edited())
		holder.add_child(WorkbenchWidgets.wrap_compact_number_field(spin, WorkbenchVocab.FORM_TILE_FIELD_WIDTH))
	var pick := Button.new()
	pick.text = WorkbenchVocab.FORM_PICK_LABEL
	pick.tooltip_text = WorkbenchVocab.SCENARIO_PICK_ARMED
	_style_button(pick)
	pick.disabled = not _arm_pick.is_valid()
	pick.pressed.connect(func() -> void:
		_arm(func(x: int, y: int) -> void:
			container[slot] = [x, y]
			_structure()))
	holder.add_child(pick)
	controls.append(holder)
	return _row(label, node, path, controls, remove)


func _unset_row(label: String, node: Dictionary, container: Variant, slot: Variant, path: String,
		remove: Callable) -> Control:
	var caption := WorkbenchWidgets.build_caption(WorkbenchVocab.FORM_UNSET)
	caption.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var set_button := Button.new()
	set_button.text = WorkbenchVocab.FORM_SET_LABEL
	_style_button(set_button)
	set_button.pressed.connect(func() -> void:
		container[slot] = ScenarioSchema.skeleton(node)
		_open[path] = true
		_structure())
	return _row(label, node, path, [caption, set_button], remove)


func _raw_row(label: String, value: Variant, path: String, remove: Callable) -> Control:
	var caption := WorkbenchWidgets.build_caption(JSON.stringify(value), HudStyle.INK)
	caption.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return _row(label, {}, path, [caption], remove)


func _unknown_row(obj: Dictionary, key: String, path: String) -> Control:
	var remove := func() -> void:
		obj.erase(key)
		_structure()
	var row := _raw_row(key, obj[key], _join(path, key), remove)
	var caption := WorkbenchWidgets.build_caption(WorkbenchVocab.FORM_NOT_IN_SCHEMA, HudStyle.WARN)
	row.add_child(caption)
	row.move_child(caption, 1)
	return row


# ---- blocks --------------------------------------------------------------------

## A collapsible block: header (toggle, count, extra buttons, ×), notes, then the indented body. Returns
## the body column the caller fills.
func _block(host: Control, title: String, description: String, path: String, depth: int,
		extras: Array, remove: Callable, count: String) -> VBoxContainer:
	var wrapper := VBoxContainer.new()
	wrapper.add_theme_constant_override("separation", WorkbenchVocab.FORM_NOTE_GAP)
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", WorkbenchWidgets.ROW_COLUMN_GAP)
	var toggle := Button.new()
	toggle.alignment = HORIZONTAL_ALIGNMENT_LEFT
	toggle.tooltip_text = description
	toggle.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	toggle.focus_mode = Control.FOCUS_NONE
	toggle.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	toggle.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_BODY)
	HudStyle.apply_link_button(toggle, HudStyle.INK)
	header.add_child(toggle)
	if not count.is_empty():
		header.add_child(WorkbenchWidgets.build_caption(count))
	for extra in extras:
		header.add_child(extra)
	if remove.is_valid():
		header.add_child(_remove_button(remove))
	wrapper.add_child(header)
	var notes := VBoxContainer.new()
	notes.add_theme_constant_override("separation", WorkbenchVocab.FORM_NOTE_GAP)
	wrapper.add_child(notes)
	_notes[path] = notes

	var indent := MarginContainer.new()
	indent.add_theme_constant_override("margin_left", WorkbenchVocab.FORM_INDENT)
	var body := VBoxContainer.new()
	body.add_theme_constant_override("separation", WorkbenchVocab.FORM_ROW_GAP)
	indent.add_child(body)
	wrapper.add_child(indent)
	host.add_child(wrapper)

	_blocks[path] = {"body": indent, "button": toggle, "title": title}
	toggle.pressed.connect(func() -> void: _set_open(path, not indent.visible))
	_set_open(path, bool(_open.get(path, depth <= WorkbenchVocab.FORM_OPEN_DEPTH)))
	return body


func _set_open(path: String, open: bool) -> void:
	_open[path] = open
	var block: Dictionary = _blocks.get(path, {})
	if block.is_empty():
		return
	(block["body"] as Control).visible = open
	(block["button"] as Button).text = "%s %s" % [
		WorkbenchVocab.FORM_OPEN_GLYPH if open else WorkbenchVocab.FORM_CLOSED_GLYPH, block["title"]]


## The pick button on a block whose object carries its tile as a key pair (`x-pick-tile`).
func _tile_keys_button(obj: Dictionary, x_key: String, y_key: String) -> Button:
	var pick := Button.new()
	pick.text = WorkbenchVocab.FORM_PICK_LABEL
	pick.tooltip_text = WorkbenchVocab.SCENARIO_PICK_ARMED
	_style_button(pick)
	pick.disabled = not _arm_pick.is_valid()
	pick.pressed.connect(func() -> void:
		_arm(func(x: int, y: int) -> void:
			obj[x_key] = x
			obj[y_key] = y
			_structure()))
	return pick


func _remove_button(remove: Callable) -> Button:
	var button := Button.new()
	button.text = WorkbenchVocab.FORM_REMOVE_GLYPH
	_style_button(button)
	button.pressed.connect(remove)
	return button


# ---- helpers -------------------------------------------------------------------

func _arm(on_pick: Callable) -> void:
	if _arm_pick.is_valid():
		_arm_pick.call(on_pick)


func _edited() -> void:
	if _on_edit.is_valid():
		_on_edit.call()


func _structure() -> void:
	_edited()
	if _on_structure.is_valid():
		_on_structure.call()


static func _join(path: String, key: String) -> String:
	return key if path.is_empty() else "%s.%s" % [path, key]


static func _index(path: String, i: int) -> String:
	return "%s[%d]" % [path, i]


static func _style_button(button: Button) -> void:
	button.focus_mode = Control.FOCUS_NONE
	button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	button.add_theme_font_size_override("font_size", WorkbenchVocab.FORM_BUTTON_FONT_SIZE)
	HudStyle.apply_button(button, "ghost")


## An OptionButton that clips its face rather than widening the column to its longest entry.
static func _style_picker(picker: OptionButton) -> void:
	HudStyle.apply_option_button(picker)
	picker.add_theme_font_size_override("font_size", WorkbenchVocab.FORM_BUTTON_FONT_SIZE)
	picker.clip_text = true
	picker.size_flags_horizontal = Control.SIZE_EXPAND_FILL


static func _style_line_edit(edit: LineEdit) -> void:
	HudStyle.apply_line_edit(edit)
	edit.add_theme_font_size_override("font_size", WorkbenchVocab.FONT_SIZE_VALUE)


## The band or herd ids the document declares, in document order.
static func declared_ids(doc: Variant, pick: String) -> Array:
	var path: Array = WorkbenchVocab.DECLARED_BAND_ID_PATH \
		if pick == WorkbenchVocab.SCHEMA_PICK_BAND else WorkbenchVocab.DECLARED_HERD_ID_PATH
	var out: Array = []
	_collect_ids(doc, path, 0, out)
	return out


static func _collect_ids(node: Variant, path: Array, index: int, out: Array) -> void:
	if node is Array:
		for element in node:
			_collect_ids(element, path, index, out)
		return
	if not (node is Dictionary) or index >= path.size() or not (node as Dictionary).has(path[index]):
		return
	var child: Variant = node[path[index]]
	if index == path.size() - 1:
		if child is String and not (child as String).is_empty():
			out.append(child)
		return
	_collect_ids(child, path, index + 1, out)


func _declared_signature() -> String:
	return str(declared_ids(_doc, WorkbenchVocab.SCHEMA_PICK_BAND)) \
		+ str(declared_ids(_doc, WorkbenchVocab.SCHEMA_PICK_HERD))
