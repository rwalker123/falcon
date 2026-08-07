extends WorkbenchPage
class_name EquipmentPage

## THE EQUIPMENT PAGE — every top-level block of the sim's effective equipment config EXCEPT the kit
## roster and the job defaults, which are the Kits page's (`.claude/rules/core_sim/equipment.md`,
## `docs/plan_early_game_labor.md` → "Equipment / TOE").
##
## **IT NAMES NO FIELD.** The page is defined by subtraction — "everything that is not
## `WorkbenchVocab.CONFIG_KITS_KEY` or `CONFIG_DEFAULT_KITS_KEY`" — and the tree under each entry is
## walked blind by `WorkbenchWidgets.build_config_object`. A fourth gear block added to
## `equipment.json` therefore appears here with no edit, a renamed one renames itself on screen, and
## a new field inside an existing block arrives with its own row. The page it replaced listed the
## fields by hand, which is a list that goes stale silently: a renamed key simply stops drawing.
##
## The keys are printed EXACTLY as the config spells them (`wear_per_biomass_hauled`, never
## "Wear per biomass hauled"), because the reader's next move is to search the config file for the
## string they just read.
##
## Pure read side: no command, no schema, no sim. The live kit state a band resolves to is the Band
## panel's, not this surface's.

# ---- state -----------------------------------------------------------------
## The parsed config. ALL of the page's state, and all of it the world's — which is what makes
## `reset()` real here.
var _config: Dictionary = {}

var _body: VBoxContainer = null


# ---- body ------------------------------------------------------------------

func build() -> void:
	add_theme_constant_override("separation", WorkbenchVocab.CONTENT_GAP)

	_body = VBoxContainer.new()
	_body.add_theme_constant_override("separation", WorkbenchWidgets.ROW_GAP)
	add_child(WorkbenchWidgets.build_group(WorkbenchVocab.EQUIPMENT_HEADING, _body))
	# Built before any frame can arrive, so the well opens on its degraded line rather than empty.
	_render()


# ---- ingest ----------------------------------------------------------------

## **PRESENCE IS NOT A CHANGE SIGNAL; `changed_sections` IS.** `SnapshotDecoder::decode_delta` builds
## a merged frame as `cache.dict.duplicate_shallow()` overwritten with the delta's keys, so **every
## baseline key rides every merged frame** — `data.has(CONFIG_JSON_KEY)` is true on all of them, and a
## gate resting on absence never skips. The manifest is the replacement signal
## (`SnapshotSections.changed`, which reads a frame carrying no manifest — a full snapshot — as
## "everything changed", so the frame that must repaint is never starved). The config is a per-world
## CONSTANT: its manifest entry moves only on a world rebuild, which is what makes `reset()` real.
##
## **THE "…OR I AM HOLDING NOTHING" CLAUSE IS LOAD-BEARING — it is what makes the shell's page-switch
## replay work.** A replayed cached frame reports the section unchanged, so under a `changed`-only
## gate a page activated between turns would sit empty until the next one; the same clause re-seeds
## the page after `reset()`. The four cases it resolves: first full snapshot (changed → ingest),
## steady delta (unchanged, holding data → skip), page-switch replay or post-reset (unchanged,
## holding nothing → ingest), world rebuild (changed → ingest).
func apply_update(data: Dictionary, _full_snapshot: bool) -> void:
	var key := WorkbenchVocab.CONFIG_JSON_KEY
	if not data.has(key):
		return
	if not (SnapshotSections.changed(data, key) or _config.is_empty()):
		return
	var parsed: Variant = JSON.parse_string(String(data.get(key, "")))
	_config = parsed if parsed is Dictionary else {}
	_render()


## WORLD BOUNDARY — and unlike `ConfigTuningPage`, this one is NOT a no-op. The config came from the
## world that has just ended, and it is only re-sent on a rebuild, so a page holding it would show the
## next world the previous one's tunables until its first frame landed.
func reset() -> void:
	_config = {}
	_render()


# ---- render ----------------------------------------------------------------

## Every top-level entry the Kits page does not own, in the config's own key order.
func _render() -> void:
	if _body == null:
		return
	HudWidgets.clear_children(_body)
	if _config.is_empty():
		_body.add_child(WorkbenchWidgets.build_caption(WorkbenchVocab.EQUIPMENT_NO_CONFIG))
		return
	var drawn := 0
	for key in _config:
		var name := String(key)
		if name == WorkbenchVocab.CONFIG_KITS_KEY or name == WorkbenchVocab.CONFIG_DEFAULT_KITS_KEY:
			continue
		for control in WorkbenchWidgets.build_config_entries(name, _config[key],
				WorkbenchWidgets.CONFIG_TOP_LEVEL_DEPTH):
			_body.add_child(control)
		drawn += 1
	if drawn == 0:
		_body.add_child(WorkbenchWidgets.build_caption(WorkbenchVocab.EQUIPMENT_NO_BLOCKS))
