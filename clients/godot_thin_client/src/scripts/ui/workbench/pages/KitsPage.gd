extends WorkbenchPage
class_name KitsPage

## THE KITS PAGE — the kit roster a party may be sent out with, and what each verb reaches for when
## the player names none (`.claude/rules/core_sim/equipment.md`).
##
## It draws exactly two top-level entries of the sim's effective equipment config,
## `WorkbenchVocab.CONFIG_KITS_KEY` and `CONFIG_DEFAULT_KITS_KEY`; the Equipment page draws every
## other one. **Those two names are the only config knowledge either page holds** — what is UNDER
## them is walked blind by `WorkbenchWidgets.build_config_object`, so a field added to a roster entry
## arrives with its own row and a renamed one renames itself on screen.
##
## The two groups are separate wells rather than one tree because they answer different questions: the
## roster is what EXISTS, the defaults are what is CHOSEN, and a job default is one line that would
## otherwise sit below three screens of roster.

# ---- state -----------------------------------------------------------------
## The parsed config. ALL of the page's state, and all of it the world's — which is what makes
## `reset()` real here.
var _config: Dictionary = {}

var _roster_body: VBoxContainer = null
var _defaults_body: VBoxContainer = null


# ---- body ------------------------------------------------------------------

func build() -> void:
	add_theme_constant_override("separation", WorkbenchVocab.CONTENT_GAP)

	_roster_body = _block_column()
	add_child(WorkbenchWidgets.build_group(WorkbenchVocab.KITS_ROSTER_HEADING, _roster_body))
	_defaults_body = _block_column()
	add_child(WorkbenchWidgets.build_group(WorkbenchVocab.KITS_DEFAULTS_HEADING, _defaults_body))
	# Built before any frame can arrive, so both wells open on their degraded line rather than empty.
	_render()


## The column one group's blocks stack in.
static func _block_column() -> VBoxContainer:
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", WorkbenchWidgets.ROW_GAP)
	return column


# ---- ingest ----------------------------------------------------------------

## The same gate `EquipmentPage` carries, for the same reasons, on the same key — see its docstring.
## **PRESENCE IS NOT A CHANGE SIGNAL** (every baseline key rides every merged delta), and the "…or I
## am holding nothing" clause is what makes the shell's page-switch replay reach a page activated
## between two frames, as well as what re-seeds this page after `reset()`.
func apply_update(data: Dictionary, _full_snapshot: bool) -> void:
	var key := WorkbenchVocab.CONFIG_JSON_KEY
	if not data.has(key):
		return
	if not (SnapshotSections.changed(data, key) or _config.is_empty()):
		return
	var parsed: Variant = JSON.parse_string(String(data.get(key, "")))
	_config = parsed if parsed is Dictionary else {}
	_render()


## WORLD BOUNDARY — real here, exactly as on `EquipmentPage`: the roster is the ended world's config
## and is only re-sent on a rebuild, so holding it would show the next world the previous one's kits.
func reset() -> void:
	_config = {}
	_render()


# ---- render ----------------------------------------------------------------

func _render() -> void:
	_render_roster()
	_render_defaults()


## The roster, one block per entry, in the config's own order — so `none` sorts last because
## `equipment.json` authors it last and not because anything here says so.
func _render_roster() -> void:
	if _roster_body == null:
		return
	HudWidgets.clear_children(_roster_body)
	if _config.is_empty():
		_roster_body.add_child(WorkbenchWidgets.build_caption(WorkbenchVocab.KITS_NO_CONFIG))
		return
	var kits: Variant = _config.get(WorkbenchVocab.CONFIG_KITS_KEY, [])
	if not (kits is Array) or (kits as Array).is_empty():
		# Not the shape this page titles — hand it straight back to the generic walker, which states
		# an empty array as `—` and any other shape as exactly whatever it is.
		_append_generic(WorkbenchVocab.CONFIG_KITS_KEY, kits)
		return
	var roster: Array = kits
	for index in roster.size():
		var entry: Variant = roster[index]
		if entry is Dictionary:
			_roster_body.add_child(_kit_block(entry, index))
			continue
		# A roster element that is not an object at all — there is nothing to promote into a title,
		# so the walker states it under its coordinate.
		_append_generic(_indexed_name(index), entry)


## ONE ROSTER ENTRY, TITLED BY WHAT IT CALLS ITSELF.
##
## **This is the page's one piece of field knowledge and it is bounded on purpose** (see
## `WorkbenchVocab.CONFIG_KIT_DISPLAY_NAME_KEY`). The title is composed from the entry's own values
## and the keys it CONSUMED are then suppressed in the body — nothing else is. The body is still the
## generic tree, so a key added to a kit definition arrives with its own row and no edit here.
##
## The four cases are the four shapes a hand-edited config can be in, and each suppresses only what it
## used: both keys → `display_name (id)` with both rows gone; `display_name` alone → the name, with
## the `display_name` row gone and nothing else; `id` alone → the id, likewise; neither → the walker's
## own `kits[N]`, with the whole entry still in the body.
func _kit_block(entry: Dictionary, index: int) -> Control:
	var display := _promotable(entry, WorkbenchVocab.CONFIG_KIT_DISPLAY_NAME_KEY)
	var id := _promotable(entry, WorkbenchVocab.CONFIG_KIT_ID_KEY)
	var title := ""
	var promoted := PackedStringArray()
	if not display.is_empty() and not id.is_empty():
		title = WorkbenchVocab.KITS_TITLE_FORMAT % [display, id]
		promoted.append(WorkbenchVocab.CONFIG_KIT_DISPLAY_NAME_KEY)
		promoted.append(WorkbenchVocab.CONFIG_KIT_ID_KEY)
	elif not display.is_empty():
		title = display
		promoted.append(WorkbenchVocab.CONFIG_KIT_DISPLAY_NAME_KEY)
	elif not id.is_empty():
		title = id
		promoted.append(WorkbenchVocab.CONFIG_KIT_ID_KEY)
	else:
		title = _indexed_name(index)
	return WorkbenchWidgets.build_config_block(title, entry,
		WorkbenchWidgets.CONFIG_TOP_LEVEL_DEPTH, promoted)


## `key`'s value if the entry states it as a NON-EMPTY STRING, else `""`.
##
## Strict on purpose. A config edited to carry a number under `id` has nothing this page can put in a
## title, so it falls through to the next case and **the row stays in the body** — suppressing a key
## whose value the title could not carry would hide it and say nothing in its place.
static func _promotable(entry: Dictionary, key: String) -> String:
	var value: Variant = entry.get(key, null)
	if not (value is String):
		return ""
	return value


## The coordinate the walker itself would use, for the entries that have no name of their own.
static func _indexed_name(index: int) -> String:
	return WorkbenchVocab.CONFIG_INDEX_FORMAT % [WorkbenchVocab.CONFIG_KITS_KEY, index]


func _append_generic(name: String, value: Variant) -> void:
	for control in WorkbenchWidgets.build_config_entries(
			name, value, WorkbenchWidgets.CONFIG_TOP_LEVEL_DEPTH):
		_roster_body.add_child(control)


## The job defaults, rendered as their own object's rows — the group heading already names them, so
## restating `default_kits` above them would print the same word twice.
func _render_defaults() -> void:
	if _defaults_body == null:
		return
	HudWidgets.clear_children(_defaults_body)
	if _config.is_empty():
		_defaults_body.add_child(WorkbenchWidgets.build_caption(WorkbenchVocab.KITS_NO_DEFAULTS))
		return
	var defaults: Variant = _config.get(WorkbenchVocab.CONFIG_DEFAULT_KITS_KEY, {})
	_defaults_body.add_child(WorkbenchWidgets.build_config_object(
		defaults if defaults is Dictionary else {}, WorkbenchWidgets.CONFIG_TOP_LEVEL_DEPTH))
