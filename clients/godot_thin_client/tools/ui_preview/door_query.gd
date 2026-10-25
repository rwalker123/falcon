extends RefCounted

## The tile card's DOOR ROWS, read the way a player meets them: through the RENDERED markup of
## `%TileDetail` and its real `meta_clicked` / `meta_hover_*` signals. Every helper takes the HUD node and
## reads the markup the label actually holds, so a claim made through here is about what is drawn, never
## about the producer's own registry.
##
## A row is found by its KEY (`Foraging`, `Wood`, `Road`, `Ancestors`, ...) and an occurrence, because keys
## repeat (a hex with two roads carries two `Road` rows).

const URL_OPEN := "[url="

## Every door meta wrapping the key cell of a row named `key`, in row order.
static func metas_for(hud: Node, key: String) -> Array[String]:
	var out: Array[String] = []
	var markup := _markup(hud)
	var needle := "][color=#%s]%s[/color][/url]" % [HudStyle.INK_DIM_HEX, key]
	var at := markup.find(needle)
	while at >= 0:
		var open := markup.rfind(URL_OPEN, at)
		var meta := markup.substr(open + URL_OPEN.length(), at - open - URL_OPEN.length())
		if meta.begins_with(HudDisclosureVocab.DOOR_META_PREFIX):
			out.append(meta)
		at = markup.find(needle, at + needle.length())
	return out

## The door meta of the `nth` row named `key`, `""` where that row is not a door.
static func meta_for(hud: Node, key: String, nth: int = 0) -> String:
	var metas := metas_for(hud, key)
	return metas[nth] if nth < metas.size() else ""

static func is_door(hud: Node, key: String, nth: int = 0) -> bool:
	return meta_for(hud, key, nth) != ""

## Every door meta the card carries.
static func all_metas(hud: Node) -> Array[String]:
	var out: Array[String] = []
	var markup := _markup(hud)
	var at := markup.find(URL_OPEN + HudDisclosureVocab.DOOR_META_PREFIX)
	while at >= 0:
		var close := markup.find("]", at)
		var meta := markup.substr(at + URL_OPEN.length(), close - at - URL_OPEN.length())
		if not out.has(meta):
			out.append(meta)
		at = markup.find(URL_OPEN + HudDisclosureVocab.DOOR_META_PREFIX, close)
	return out

## Click a door the way the engine does: the label's own `meta_clicked`. `false` when the row is no door.
static func press(hud: Node, key: String, nth: int = 0) -> bool:
	var meta := meta_for(hud, key, nth)
	if meta == "":
		return false
	hud.tile_detail.meta_clicked.emit(meta)
	return true

## The sentence the row shows over its own area: hover it (the label's real `meta_hover_started`), read
## the label's tooltip, un-hover. `""` for a row with nothing to say or no door.
static func hover_text(hud: Node, key: String, nth: int = 0) -> String:
	var meta := meta_for(hud, key, nth)
	if meta == "":
		return ""
	hud.tile_detail.meta_hover_started.emit(meta)
	var text: String = hud.tile_detail.tooltip_text
	hud.tile_detail.meta_hover_ended.emit(meta)
	return text

## The row's standing-summary line (the first line of its hover): `♻ 3 harvesters · +2.74 /turn`.
static func summary_line(hud: Node, key: String, nth: int = 0) -> String:
	return hover_text(hud, key, nth).split("\n")[0]

## The `[cell` opening tag worn by BOTH cells of the row named `key` (a door row), `""` for none. Wash
## claims compare it to `HudStyle.DOOR_*_WASH_HEX`.
static func cell_tags(hud: Node, key: String, nth: int = 0) -> Array[String]:
	var out: Array[String] = []
	var meta := meta_for(hud, key, nth)
	if meta == "":
		return out
	var markup := _markup(hud)
	var at := markup.find(URL_OPEN + meta + "]")
	var seen := 0
	# The key cell's url opens right after its `[cell...]` tag; the value cell's follows `[/cell]`.
	while at >= 0 and seen < 2:
		var tag_open := markup.rfind("[cell", at)
		out.append(markup.substr(tag_open, markup.find("]", tag_open) - tag_open + 1))
		seen += 1
		at = markup.find(URL_OPEN + meta + "]", at + 1)
	return out

## Which door rows (by meta) currently wear the given wash hex.
static func washed(hud: Node, wash_hex: String) -> Array[String]:
	var out: Array[String] = []
	for meta in all_metas(hud):
		var markup := _markup(hud)
		if markup.contains("[cell bg=#%s][url=%s]" % [wash_hex, meta]) and not out.has(meta):
			out.append(meta)
	return out

static func _markup(hud: Node) -> String:
	return hud.tile_detail.text
