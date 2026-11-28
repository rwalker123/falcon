class_name TribePicker
extends RefCounted

## **THE TRIBE ON A PICKER** - all-`static` (`.claude/rules/client/tribe-panel.md`). Once a player band is in a
## tribe, every list that picks "who" lists the TRIBE in place of its members, with a fold beside it; opening the
## fold lists the members indented beneath it, and a lone band is listed on its own. The same entry list serves the
## tile card's `Band:` row and Materials & Crafting's picker, so the two cannot disagree about it.

const ENTRY_LABEL := "label"
const ENTRY_KIND := "kind"
const ENTRY_KIND_BAND := "band"
const ENTRY_KIND_TRIBE := "tribe"
const ENTRY_ENTITY := "entity"
const ENTRY_TRIBE_ID := "tribe_id"
const ENTRY_INDENT := "indent"

## The list's entries in picker order: `{label, kind, entity | tribe_id, indent}`. `fold_open` lists each
## tribe's members beneath it, indented; a closed fold lists the tribe alone.
static func entries(bands: Array, fold_open: bool) -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	for subject in TribeRoster.subjects(bands):
		if String(subject[TribeRoster.SUBJECT_KIND]) == TribeRoster.SUBJECT_BAND:
			var band: Dictionary = subject[TribeRoster.SUBJECT_BAND_DICT]
			out.append({ENTRY_LABEL: HudFormat.band_name(band), ENTRY_KIND: ENTRY_KIND_BAND,
				ENTRY_ENTITY: int(band.get("entity", -1)), ENTRY_INDENT: 0})
			continue
		out.append({ENTRY_LABEL: String(subject[TribeRoster.SUBJECT_NAME]), ENTRY_KIND: ENTRY_KIND_TRIBE,
			ENTRY_TRIBE_ID: int(subject[TribeRoster.SUBJECT_TRIBE_ID]), ENTRY_INDENT: 0})
		if not fold_open:
			continue
		for member in subject[TribeRoster.SUBJECT_MEMBERS]:
			out.append({ENTRY_LABEL: HudFormat.band_name(member), ENTRY_KIND: ENTRY_KIND_BAND,
				ENTRY_ENTITY: int((member as Dictionary).get("entity", -1)),
				ENTRY_INDENT: HudTribeVocab.PICKER_MEMBER_INDENT})
	return out

static func has_tribe(bands: Array) -> bool:
	for band in bands:
		if band is Dictionary and TribeRoster.is_member(band):
			return true
	return false

## The picker's closed face for the current selection: the tribe's name, or the band's own.
static func face_of(bands: Array, selected_band: Dictionary, selected_tribe: int) -> String:
	if selected_tribe != HudTribeVocab.NO_TRIBE:
		for member in bands:
			if member is Dictionary and TribeRoster.tribe_id_of(member) == selected_tribe:
				return String((member as Dictionary).get(HudTribeVocab.KEY_TRIBE_NAME, ""))
	return HudFormat.band_name(selected_band) if not selected_band.is_empty() else ""

## The `Band:` row: key, option picker, and the fold button beside it while a tribe exists. `on_band` takes
## the picked band's entity, `on_tribe` a tribe id, and `on_fold` flips the fold (the caller re-renders).
static func build_row(bands: Array, selected_band: Dictionary, selected_tribe: int, fold_open: bool,
		on_band: Callable, on_tribe: Callable, on_fold: Callable) -> HBoxContainer:
	var row := HBoxContainer.new()
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
	row.set_meta(HudTribeVocab.PICKER_META, selected_tribe)
	row.add_child(HudWidgets.build_field_key(HudWorkVocab.BAND_PICKER_LABEL))
	var list := entries(bands, fold_open)
	var picks: Array = []
	var selected_index := HudWidgets.NO_ENTRY_SELECTED
	var selected_entity := int(selected_band.get("entity", -1))
	for i in range(list.size()):
		var entry: Dictionary = list[i]
		var tribe_id := int(entry.get(ENTRY_TRIBE_ID, HudTribeVocab.NO_TRIBE))
		var entity := int(entry.get(ENTRY_ENTITY, -1))
		if String(entry[ENTRY_KIND]) == ENTRY_KIND_TRIBE:
			if tribe_id == selected_tribe and selected_tribe != HudTribeVocab.NO_TRIBE:
				selected_index = i
			picks.append({ENTRY_LABEL: String(entry[ENTRY_LABEL]), "on_pick": func() -> void: on_tribe.call(tribe_id)})
		else:
			if selected_tribe == HudTribeVocab.NO_TRIBE and entity == selected_entity:
				selected_index = i
			picks.append({ENTRY_LABEL: String(entry[ENTRY_LABEL]), "on_pick": func() -> void: on_band.call(entity)})
	var picker := HudWidgets.build_option_picker(picks, selected_index,
		face_of(bands, selected_band, selected_tribe), "")
	var popup := picker.get_popup()
	for i in range(list.size()):
		if int((list[i] as Dictionary)[ENTRY_INDENT]) > 0:
			popup.set_item_indent(i, int((list[i] as Dictionary)[ENTRY_INDENT]))
	row.add_child(picker)
	if has_tribe(bands):
		var fold := TribePage.build_fold_button(fold_open,
			HudTribeVocab.PICKER_FOLD_OPEN_TIP if fold_open else HudTribeVocab.PICKER_FOLD_CLOSED_TIP,
			on_fold)
		fold.custom_minimum_size.x = HudTribeVocab.PICKER_FOLD_WIDTH
		fold.set_meta(HudTribeVocab.PICKER_FOLD_META, fold_open)
		row.add_child(fold)
	return row
