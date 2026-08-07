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


## The roster, one block per entry — `kits[0]`, `kits[1]`, … in the config's own order, so `none`
## sorts last because `equipment.json` authors it last and not because anything here says so.
func _render_roster() -> void:
	if _roster_body == null:
		return
	HudWidgets.clear_children(_roster_body)
	if _config.is_empty():
		_roster_body.add_child(WorkbenchWidgets.build_caption(WorkbenchVocab.KITS_NO_CONFIG))
		return
	var kits: Variant = _config.get(WorkbenchVocab.CONFIG_KITS_KEY, [])
	for control in WorkbenchWidgets.build_config_entries(
			WorkbenchVocab.CONFIG_KITS_KEY, kits, 0):
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
		defaults if defaults is Dictionary else {}, 0))
