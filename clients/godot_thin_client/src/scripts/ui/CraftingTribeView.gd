extends RefCounted

## The Crafting panel's TRIBE body (#864): what each member band's bench is making or still needs to
## make for the tribe. Read-only; `#863`'s picker calls `load(path).build(members)` when the tribe is
## selected. Every figure is the sim's (`for_tribe` on bench orders, `for_tribe_count` on suggestions).

const HudStyle = preload("res://src/scripts/ui/HudStyle.gd")
const HudCraftingVocab = preload("res://src/scripts/ui/hud/hud_crafting_vocab.gd")

## `members`: full member-band cohort dicts, in member order.
static func build(members: Array[Dictionary]) -> Control:
	var root := VBoxContainer.new()
	root.add_theme_constant_override("separation", HudCraftingVocab.TRIBE_VIEW_SEPARATION)
	var any := false
	for band in members:
		var lines := _lines_for(band)
		var band_name := String(band.get(HudCraftingVocab.BAND_NAME_KEY, ""))
		if lines.is_empty():
			root.add_child(_label(HudCraftingVocab.TRIBE_VIEW_NOTHING_FORMAT % band_name,
				HudCraftingVocab.TRIBE_VIEW_LINE_FONT_SIZE, HudStyle.INK_DIM))
			continue
		any = true
		var box := VBoxContainer.new()
		box.add_theme_constant_override("separation", HudCraftingVocab.TRIBE_VIEW_LINE_SEPARATION)
		box.add_child(_label(band_name, HudCraftingVocab.TRIBE_VIEW_NAME_FONT_SIZE, HudStyle.INK))
		for line in lines:
			var margin := MarginContainer.new()
			margin.add_theme_constant_override("margin_left", HudCraftingVocab.TRIBE_VIEW_ITEM_INDENT)
			margin.add_child(_label(String(line), HudCraftingVocab.TRIBE_VIEW_LINE_FONT_SIZE, HudStyle.INK_DIM))
			box.add_child(margin)
		root.add_child(box)
	if not any:
		root.add_child(_label(HudCraftingVocab.TRIBE_VIEW_NOBODY,
			HudCraftingVocab.TRIBE_VIEW_LINE_FONT_SIZE, HudStyle.INK_DIM))
	return root

## One line per (item, state): "making" from the queued orders, "needed" from the suggestions.
static func _lines_for(band: Dictionary) -> Array:
	var lines: Array = []
	var making := {}
	var bench: Dictionary = band.get(HudCraftingVocab.BAND_BENCH_KEY, {})
	for order in bench.get("orders", []):
		var n := int(order.get(HudCraftingVocab.ORDER_FOR_TRIBE_KEY, 0))
		if n > 0:
			var key := _order_name(order)
			making[key] = int(making.get(key, 0)) + n
	for key in making:
		lines.append(HudCraftingVocab.TRIBE_VIEW_MAKING_FORMAT % [key, making[key]])
	for suggestion in band.get(HudCraftingVocab.BAND_CRAFT_SUGGESTIONS_KEY, []):
		var n := int(suggestion.get(HudCraftingVocab.SUGGESTION_FOR_TRIBE_COUNT_KEY, 0))
		if n > 0:
			var shown := String(suggestion.get(HudCraftingVocab.ORDER_DISPLAY_NAME_KEY, ""))
			if shown == "":
				shown = String(suggestion.get(HudCraftingVocab.SUGGESTION_ITEM_ID_KEY, ""))
			lines.append(HudCraftingVocab.TRIBE_VIEW_NEEDED_FORMAT % [shown, n])
	return lines

static func _order_name(order: Dictionary) -> String:
	var shown := String(order.get(HudCraftingVocab.ORDER_DISPLAY_NAME_KEY, ""))
	return shown if shown != "" else String(order.get("recipe_id", ""))

static func _label(text: String, font_size: int, color: Color) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", font_size)
	label.add_theme_color_override("font_color", color)
	return label
