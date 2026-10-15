class_name PeoplesDiplomacy
extends RefCounted

## **THE PEOPLES TAB'S DIPLOMACY BLOCK** (early diplomacy C2, issue #233, `docs/plan_early_diplomacy.md` §3):
## one block per OTHER PEOPLE the client holds any published fact about, above the ties list in the
## Peoples tab and its wide-shell section. All-`static`, stateless -- the roster idiom
## (`ConnectionsRoster`, `DealingsRoster`): the model is a parameter, the actions are Callables.
##
## A block is two lines and an action strip:
##   * line 1 `Brackfolk ……… They: friendly · We: wary` -- the people's name, then what THEY think of us
##     (the last report, never live) and what WE think of them (live). Each is a word cut from the
##     PUBLISHED cuts (`goodwill.cuts`), so the client never restates a threshold; the numbers and their
##     reasons ride the hover. **A people with no row has no word** -- never a default neutral, which
##     would state an opinion nobody reported;
##   * line 2 the treaty in force in words, or `No treaty`;
##   * the strip `Propose a treaty · End treaty · Borders open`.
## Every line is one line, clipped with an ellipsis, so a block cannot wrap into a third; the hover
## carries the whole sentence.
##
## **WHO IS LISTED** is `HudBandLaborState.known_peoples`: every faction named by a goodwill row, a
## treaty, a standing link or a dealing, plus the border overrides and the foreign bands in sight. A
## people the wire has never mentioned is not a row (there is nothing to say about it).
##
## **THE BORDER STATE IS THE SIM'S, NEVER GUESSED**: the override row if one exists, else the general
## `open_borders`; an open-borders TERM of a treaty in force wins over both (the server's own rule), and
## the hover says which of the three is speaking.

## The block's people, ascending: the model's known peoples plus the policy's overrides and the visible
## foreign bands.
static func people(band_labor: HudBandLaborState, policy: Dictionary) -> Array[int]:
	var extra: Array = band_labor.foreign_factions()
	for override_variant in Array(policy.get("border_overrides", [])):
		if override_variant is Dictionary:
			extra.append(int((override_variant as Dictionary).get("other_faction", HudConst.NO_FACTION_ID)))
	return band_labor.known_peoples(extra)

## What the block is built from, so a refresh that changes none of it touches nothing.
static func signature(band_labor: HudBandLaborState, policy: Dictionary) -> Array:
	return [band_labor.goodwill(), band_labor.treaties(), policy.get("open_borders", true),
		policy.get("border_overrides", []), band_labor.foreign_factions(), band_labor.current_turn()]

## `{open, source}` for one people: the effective border state toward it and whose word it is
## (`SOURCE_TREATY` / `SOURCE_OVERRIDE` / `SOURCE_DEFAULT`).
const SOURCE_TREATY := "treaty"
const SOURCE_OVERRIDE := "override"
const SOURCE_DEFAULT := "default"

static func border_state(faction: int, band_labor: HudBandLaborState, policy: Dictionary) -> Dictionary:
	var treaty := band_labor.treaty_with(faction)
	if not treaty.is_empty() and bool(Dictionary(treaty.get("terms", {})).get("open_borders", false)):
		return {"open": true, "source": SOURCE_TREATY}
	for override_variant in Array(policy.get("border_overrides", [])):
		if override_variant is Dictionary \
				and int((override_variant as Dictionary).get("other_faction", HudConst.NO_FACTION_ID)) == faction:
			return {"open": bool((override_variant as Dictionary).get("open", true)),
				"source": SOURCE_OVERRIDE}
	return {"open": bool(policy.get("open_borders", true)), "source": SOURCE_DEFAULT}

## The block, or `null` when there is nobody to list. `actions` maps `HudDealingsVocab.PEOPLE_ACTION_*` to
## a Callable: `propose(faction)`, `end_treaty(faction)`, `borders(faction, open)`.
static func build_block(band_labor: HudBandLaborState, policy: Dictionary,
		actions: Dictionary) -> VBoxContainer:
	var ids := people(band_labor, policy)
	if ids.is_empty():
		return null
	var block := HudWidgets.make_zone_block()
	block.name = HudDealingsVocab.PEOPLES_BLOCK_NAME
	block.add_child(HudWidgets.alloc_section_label(HudDealingsVocab.PEOPLES_SECTION))
	var cuts := band_labor.goodwill_cuts()
	for faction in ids:
		block.add_child(_build_row(faction, band_labor, policy, cuts, actions))
	return block

static func _build_row(faction: int, band_labor: HudBandLaborState, policy: Dictionary, cuts: Dictionary,
		actions: Dictionary) -> VBoxContainer:
	var people_name := FactionMark.faction_name(faction)
	var treaty := band_labor.treaty_with(faction)
	var row := VBoxContainer.new()
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_theme_constant_override("separation", HudDealingsVocab.ROW_LINE_SEPARATION)
	row.set_meta(HudDealingsVocab.PEOPLE_ROW_META, faction)

	var head := HBoxContainer.new()
	head.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_theme_constant_override("separation", HudWorkVocab.ZONE_HEAD_SEPARATION)
	var name_label := _line_label(people_name, HudStyle.INK, HudWorkVocab.WORK_ROW_FONT_SIZE)
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_child(name_label)
	var theirs := _regard_cell(band_labor.goodwill_row("known", faction), cuts, true)
	if theirs != null:
		head.add_child(theirs)
	var ours := _regard_cell(band_labor.goodwill_row("own", faction), cuts, false)
	if ours != null:
		if theirs != null:
			head.add_child(_cell(HudDealingsVocab.ROW_SEPARATOR, HudStyle.INK_FAINT))
		head.add_child(ours)
	row.add_child(head)

	var terms_text := HudDealingsVocab.TREATY_NONE
	var terms_tip := HudDealingsVocab.TREATY_NONE
	var terms_ink := HudStyle.INK_FAINT
	if not treaty.is_empty():
		terms_text = HudDealingsVocab.terms_text(Dictionary(treaty.get("terms", {})))
		terms_tip = "\n".join(PackedStringArray([terms_text,
			HudDealingsVocab.TREATY_SINCE_FORMAT % int(treaty.get("formed_turn", 0))]))
		terms_ink = HudStyle.INK_DIM
	var second := _line_label(terms_text, terms_ink, HudWorkVocab.ALLOC_SECTION_FONT_SIZE)
	HudWidgets.set_label_tooltip(second, terms_tip)
	row.add_child(second)
	row.add_child(_build_strip(faction, not treaty.is_empty(),
		border_state(faction, band_labor, policy), actions))
	return row

## One regard cell (`They: friendly`), or `null` where the sim published no row or no cuts. The hover
## gives the whole sentence and the reasons; the word itself is the only thing on the row.
static func _regard_cell(goodwill: Dictionary, cuts: Dictionary, theirs: bool) -> Label:
	if goodwill.is_empty():
		return null
	var word := HudDealingsVocab.goodwill_word(float(goodwill.get("total", 0.0)), cuts)
	if word == "":
		return null
	var cell := _cell((HudDealingsVocab.THEY_FORMAT if theirs else HudDealingsVocab.WE_FORMAT) % word,
		_word_ink(word))
	cell.set_meta(HudDealingsVocab.PEOPLE_GOODWILL_META, "known" if theirs else "own")
	var headline := HudDealingsVocab.THEY_TOOLTIP_FORMAT % [word, int(goodwill.get("reported_turn", 0))] \
		if theirs else HudDealingsVocab.WE_TOOLTIP_FORMAT % word
	var reasons := HudDealingsVocab.reason_lines(goodwill)
	var lines := PackedStringArray([headline])
	if reasons.is_empty():
		lines.append(HudDealingsVocab.REASONS_NONE)
	else:
		lines.append(HudDealingsVocab.REASONS_HEAD)
		lines.append_array(reasons)
	HudWidgets.set_label_tooltip(cell, "\n".join(lines))
	return cell

## Wary in amber beside the sentence it explains, friendly healthy, neutral dim.
static func _word_ink(word: String) -> Color:
	match word:
		HudDealingsVocab.WORD_WARY:
			return HudStyle.WARN
		HudDealingsVocab.WORD_FRIENDLY:
			return HudStyle.HEALTHY
	return HudStyle.INK_DIM

## `Propose a treaty · End treaty · Borders open`. End treaty exists only where a treaty is in force;
## each link is greyed (never hidden) where its Callable is missing.
static func _build_strip(faction: int, has_treaty: bool, border: Dictionary,
		actions: Dictionary) -> HBoxContainer:
	var strip := HBoxContainer.new()
	strip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	strip.add_theme_constant_override("separation", HudWorkVocab.ZONE_HEAD_SEPARATION)
	strip.set_meta(HudDealingsVocab.PEOPLE_ACTION_META, faction)
	var entries: Array[Dictionary] = [{
		"id": HudDealingsVocab.PEOPLE_ACTION_PROPOSE, "text": HudDealingsVocab.TREATY_PROPOSE_LABEL,
		"tip": HudDealingsVocab.TREATY_PROPOSE_TOOLTIP, "ink": HudStyle.SIGNAL, "args": [faction]}]
	if has_treaty:
		entries.append({"id": HudDealingsVocab.PEOPLE_ACTION_END, "text": HudDealingsVocab.TREATY_END_LABEL,
			"tip": HudDealingsVocab.TREATY_END_TOOLTIP, "ink": HudStyle.DANGER, "args": [faction]})
	var open := bool(border["open"])
	var source := String(border["source"])
	entries.append({"id": HudDealingsVocab.PEOPLE_ACTION_BORDERS,
		"text": HudDealingsVocab.BORDERS_OPEN_LABEL if open else HudDealingsVocab.BORDERS_CLOSED_LABEL,
		"tip": _border_tooltip(source), "ink": HudStyle.INK_DIM, "args": [faction, not open]})
	for entry in entries:
		var callback: Callable = actions.get(entry["id"], Callable())
		var args: Array = entry["args"]
		var link := HudWidgets.build_inline_link(String(entry["text"]), entry["ink"] as Color,
			func() -> void:
				if callback.is_valid():
					callback.callv(args))
		link.tooltip_text = String(entry["tip"])
		link.set_meta(HudDealingsVocab.PEOPLE_ACTION_META, String(entry["id"]))
		link.disabled = not callback.is_valid()
		strip.add_child(link)
	return strip

static func _border_tooltip(source: String) -> String:
	match source:
		SOURCE_TREATY:
			return HudDealingsVocab.BORDERS_TREATY_TOOLTIP
		SOURCE_OVERRIDE:
			return HudDealingsVocab.BORDERS_OVERRIDE_TOOLTIP
	return HudDealingsVocab.BORDERS_DEFAULT_TOOLTIP

static func _line_label(text: String, ink: Color, font_size: int) -> Label:
	var label := Label.new()
	label.text = text
	label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	label.clip_text = true
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.add_theme_color_override("font_color", ink)
	label.add_theme_font_size_override("font_size", font_size)
	return label

static func _cell(text: String, ink: Color) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_color_override("font_color", ink)
	label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
	return label
