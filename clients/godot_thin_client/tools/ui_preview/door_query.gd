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

# ---- THE TRAILING WORKERS MARK (the worker figure + crew count every door row ends in) -------------------
# Read off the RENDERED markup of one row, never the producer's registry. A row is cut out of the markup from its
# key cell to the end of its value cell.

const IMG_OPEN := "[img="
const MARK_PATH_SPRITE_ID := "workers"
const IMG_CLOSE := "[/img]"
const NBSP := " "

## The markup of the `nth` row named `key` -- both cells, door or readout. `""` when there is no such row.
static func row_markup(hud: Node, key: String, nth: int = 0) -> String:
	var markup := _markup(hud)
	var needle := "[color=#%s]%s[/color]" % [HudStyle.INK_DIM_HEX, key]
	var at := markup.find(needle)
	for _i in range(nth):
		if at < 0:
			break
		at = markup.find(needle, at + needle.length())
	if at < 0:
		return ""
	# key, value and mark cells (the land card is a `[table=3]`).
	var end := at
	for _i in range(3):
		end = markup.find("[/cell]", end + 1)
		if end < 0:
			return ""
	return markup.substr(at, end - at)

## How many workers images the row carries.
static func mark_images(hud: Node, key: String, nth: int = 0) -> int:
	return row_markup(hud, key, nth).count(_mark_path())

## The row's image tint as the 8-digit `RRGGBBAA` hex, `""` for no image.
static func mark_tint(hud: Node, key: String, nth: int = 0) -> String:
	var row := row_markup(hud, key, nth)
	var img := row.find(IMG_OPEN + "")
	if img < 0 or not row.contains(_mark_path()):
		return ""
	var at := row.find("color=#", img)
	return row.substr(at + 7, 8) if at >= 0 else ""

## The plain text after the row's image (BBCode tags stripped, spaces trimmed): the crew count and any warn mark.
static func mark_text(hud: Node, key: String, nth: int = 0) -> String:
	var row := row_markup(hud, key, nth)
	var at := row.find(IMG_CLOSE)
	if at < 0:
		return ""
	var tail := row.substr(at + IMG_CLOSE.length())
	var plain := ""
	var in_tag := false
	for ch in tail:
		if ch == "[":
			in_tag = true
		elif ch == "]":
			in_tag = false
		elif not in_tag:
			plain += ch
	return plain.replace(NBSP, " ").strip_edges()

## True when the warn glyph is wrapped in the WARN ink.
static func warn_mark_in_warn_ink(hud: Node, key: String, nth: int = 0) -> bool:
	return row_markup(hud, key, nth).contains("[color=#%s]%s[/color]" % [
		HudStyle.WARN_HEX, HudSelectionVocab.DOOR_WARN_SUFFIX.replace(" ", NBSP)])

## True when the image sits in the row's THIRD cell (the mark column), i.e. after two closed cells: every figure shares
## that column's left edge.
static func mark_in_column(hud: Node, key: String, nth: int = 0) -> bool:
	var row := row_markup(hud, key, nth)
	var img := row.find(IMG_OPEN)
	return img > 0 and row.substr(0, img).count("[/cell]") == 2

## The card-wide claims every chapter that renders door rows makes: each named door row carries the workers image
## EXACTLY once and sits behind its pad, and the whole card carries as many images as it has doors, so a readout
## row carries none.
static func assert_marks(h: Object, label: String, door_keys: Array, aligned: bool = true,
		crewed_readouts: int = 0) -> void:
	for key in door_keys:
		var k := String(key)
		h._assert_hud("%s: door row `%s` carries the workers image exactly once (%d)" % [label, k,
				mark_images(h._hud, k)],
			mark_images(h._hud, k) == 1)
		if aligned:
			h._assert_hud("%s: door row `%s` wears its mark in the shared mark column" % [label, k],
				mark_in_column(h._hud, k))
	var images: int = _markup(h._hud).count(_mark_path())
	var doors: int = all_metas(h._hud).size()
	h._assert_hud("%s: one workers image per door, plus one per readout that states a crew, none on any other readout (%d images, %d doors, %d crewed readouts)"
			% [label, images, doors, crewed_readouts],
		images == doors + crewed_readouts)

## The workers sprite's `res://` path, the needle that tells a door's mark from the basket rows' `[img]`s.
static func _mark_path() -> String:
	return String(HudSprites.SPRITE_PATHS.get(MARK_PATH_SPRITE_ID, "no-such-mark"))
