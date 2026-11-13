class_name HudStorageVocab

## **THE BAND'S SPOILAGE AND THE STORES THAT WOULD SAVE IT** — the wire keys, the words and the
## readings behind the band panel's amber food-side line.
##
## **EVERY NUMBER IS THE SIM'S** (`spoil_ahead` / `storage_saves` on an own band's cohort dict):
## food a turn of each keeping class projected to spoil uneaten, and per store the band does not hold
## the food a turn it would save, whether its lesson is known and whether a build is declared. Nothing
## here derives a rate; it joins the published rows to the rung roster's own name and says them.

# ---- wire keys ------------------------------------------------------------------------------------
const SPOIL_AHEAD_KEY := "spoil_ahead"
const STORAGE_SAVES_KEY := "storage_saves"
const HELD_STORES_KEY := "held_stores"
const BAND_IMPROVEMENTS_KEY := "band_improvements"
const SHELF_LIFE_KEY := "shelf_life_turns"
const KEEPING_KEY := "keeping"
const FOOD_KEY := "food"
const RUNG_KEY := "rung"
const FOOD_SAVED_KEY := "food_saved"
const LESSON_KNOWN_KEY := "lesson_known"
const DECLARED_KEY := "declared"
const TILE_KEPT_KEY := "tile_kept"
## The ladder catalog's own keys (`ladder_rungs`): the rung's bare key, its branch and its name.
const CATALOG_RUNG_KEY := "rung_key"
const CATALOG_BRANCH_KEY := "branch"
const CATALOG_DISPLAY_NAME_KEY := "display_name"
## A wire rung id is `<branch>:<rung key>` (`camp_stores:drying_rack`).
const RUNG_ID_SEPARATOR := ":"

# ---- the words ------------------------------------------------------------------------------------
## The keeping classes in the words the panel already uses for food (`meat lasts 4`), not the ids.
const KEEPING_WORDS := {
	"flesh": "meat",
	"fresh_plant": "greens",
	"dry": "grain",
}
## The line's lead: the shared hazard mark, so the amber is the one every warning in this client wears.
const SPOIL_LINE_LEAD := "⚠ "
const SPOIL_PHRASE := "food a turn spoils uneaten"
const SPOIL_LINE_FORMAT := SPOIL_LINE_LEAD + "%s " + SPOIL_PHRASE
const SPOIL_FIX_JOIN := " · "
const SPOIL_FIX_SAVES_FORMAT := "a %s would save %s"
const SPOIL_FIX_DECLARED_FORMAT := "%s being built"
const SPOIL_FIX_NEEDS_LESSON := "needs Storage"
const SPOIL_TOOLTIP_LEAD := "Food a turn projected to spoil uneaten in the larder:"
const SPOIL_TOOLTIP_ROW_FORMAT := "%s %s"
const SPOIL_TOOLTIP_ROW_JOIN := "\n"
## The detail label's hover key for that line (`DetailFormat.Context.row_tooltips`).
const SPOIL_HOVER_KEY := "Spoilage"

## Decimals a food rate is said to (the client's one food-rate resolution).
const FOOD_DECIMALS := 1
## Below this a published figure is rounding noise, not a spoil to name.
const FOOD_EPSILON := 0.05

static func keeping_word(keeping: String) -> String:
	return String(KEEPING_WORDS.get(keeping, keeping.replace("_", " ")))

static func format_food(value: float) -> String:
	return String.num(value, FOOD_DECIMALS)

## The band's total spoilage a turn, summed over the non-zero classes.
static func spoil_total(band: Dictionary) -> float:
	var total := 0.0
	for row_variant in band.get(SPOIL_AHEAD_KEY, []):
		if row_variant is Dictionary:
			total += maxf(float((row_variant as Dictionary).get(FOOD_KEY, 0.0)), 0.0)
	return total

## One rung's name as the ladder roster says it, lower-cased; the bare key (never the id) for a rung
## the roster does not carry. `catalog` is the `ladder_rungs` array.
static func rung_name(catalog: Array, rung_id: String) -> String:
	var parts := rung_id.split(RUNG_ID_SEPARATOR)
	var branch := parts[0] if parts.size() > 1 else ""
	var key := parts[parts.size() - 1]
	for entry_variant in catalog:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		if String(entry.get(CATALOG_RUNG_KEY, "")) == key \
				and (branch == "" or String(entry.get(CATALOG_BRANCH_KEY, "")) == branch):
			var name := String(entry.get(CATALOG_DISPLAY_NAME_KEY, "")).strip_edges()
			return (name if name != "" else key.replace("_", " ")).to_lower()
	return key.replace("_", " ")

## The band panel's spoilage sentence, or `""` when nothing spoils. The fix clause names the best
## saving row: a declared build reads `<rung> being built`, an unlearned one `needs Storage`, else what
## it would save.
static func spoil_line(band: Dictionary, catalog: Array) -> String:
	var total := spoil_total(band)
	if total < FOOD_EPSILON:
		return ""
	var line := SPOIL_LINE_FORMAT % format_food(total)
	var best := {}
	for row_variant in band.get(STORAGE_SAVES_KEY, []):
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		if float(row.get(FOOD_SAVED_KEY, 0.0)) <= FOOD_EPSILON:
			continue
		# A declared build outranks an open one: it is the news the player is waiting on.
		if best.is_empty() or (bool(row.get(DECLARED_KEY, false)) and not bool(best.get(DECLARED_KEY, false))) \
				or (bool(row.get(DECLARED_KEY, false)) == bool(best.get(DECLARED_KEY, false))
					and float(row.get(FOOD_SAVED_KEY, 0.0)) > float(best.get(FOOD_SAVED_KEY, 0.0))):
			best = row
	if best.is_empty():
		return line
	var name := rung_name(catalog, String(best.get(RUNG_KEY, "")))
	if bool(best.get(DECLARED_KEY, false)):
		return line + SPOIL_FIX_JOIN + SPOIL_FIX_DECLARED_FORMAT % name
	if not bool(best.get(LESSON_KNOWN_KEY, false)):
		return line + SPOIL_FIX_JOIN + SPOIL_FIX_NEEDS_LESSON
	return line + SPOIL_FIX_JOIN + SPOIL_FIX_SAVES_FORMAT % [
		name, format_food(float(best.get(FOOD_SAVED_KEY, 0.0)))]

## The hover: the per-class breakdown in the food words the panel already uses.
static func spoil_tooltip(band: Dictionary) -> String:
	var rows: PackedStringArray = []
	for row_variant in band.get(SPOIL_AHEAD_KEY, []):
		if not (row_variant is Dictionary):
			continue
		var row: Dictionary = row_variant
		var food := float(row.get(FOOD_KEY, 0.0))
		if food < FOOD_EPSILON:
			continue
		rows.append(SPOIL_TOOLTIP_ROW_FORMAT % [keeping_word(String(row.get(KEEPING_KEY, ""))),
			format_food(food)])
	if rows.is_empty():
		return ""
	return SPOIL_TOOLTIP_LEAD + SPOIL_TOOLTIP_ROW_JOIN + SPOIL_TOOLTIP_ROW_JOIN.join(rows)

# ---- the Work tab's store rows (GROUNDWORK: the drying rack, the storage pit) ---------------------
## A store's three states, said on the row's subline.
enum StoreState { NOT_BUILT, BUILDING, BUILT }
const STORE_NOT_BUILT_LINE := "not built"
const STORE_BUILDING_LINE := "building"
const STORE_BUILDING_PERCENT_FORMAT := "building, %d%%"
const STORE_BUILT_LINE := "built"
const STORE_KEEPS_FORMAT := "%s keeps %d turns"
## A build fraction the caller has no meter for (the subline then says `building` alone).
const NO_BUILD_FRACTION := -1.0
const PERCENT_SCALE := 100.0
## The pit's place word when it stands on the band's own tile (`Storage pit · in camp`).
const STORE_IN_CAMP := "in camp"
const BUILD_BUTTON_LABEL := "Build"
## The Build button's meta, valued with its branch.
const BUILD_BUTTON_META := &"storage_build_button"

## The wire rung id (`camp_stores:drying_rack`) of the store the band's published rows name for `branch`:
## an unheld rung is a `storage_saves` row, a held one a `held_stores` row. `""` when neither names the branch,
## which is how a branch that is not a store reads.
static func rung_id_of(band: Dictionary, branch: String) -> String:
	for key in [STORAGE_SAVES_KEY, HELD_STORES_KEY]:
		for row_variant in band.get(key, []):
			if not (row_variant is Dictionary):
				continue
			var rung := String((row_variant as Dictionary).get(RUNG_KEY, ""))
			if rung.begins_with(branch + RUNG_ID_SEPARATOR):
				return rung
	return ""

## The rung id's branch (`camp_stores`).
static func rung_branch(rung_id: String) -> String:
	var parts := rung_id.split(RUNG_ID_SEPARATOR)
	return parts[0] if parts.size() > 1 else ""

static func is_store_branch(band: Dictionary, branch: String) -> bool:
	return rung_id_of(band, branch) != ""

## The bare rung key (`drying_rack`) the branch's store raises, `""` for a branch that is not a store.
static func store_rung_key(band: Dictionary, branch: String) -> String:
	var rung := rung_id_of(band, branch)
	return rung.split(RUNG_ID_SEPARATOR)[-1] if rung != "" else ""

## The band's `held_stores` row for the branch (`{rung, keeping, shelf_life_turns}`), `{}` while unheld.
static func held_of(band: Dictionary, branch: String) -> Dictionary:
	for row_variant in band.get(HELD_STORES_KEY, []):
		if row_variant is Dictionary and String((row_variant as Dictionary).get(RUNG_KEY, "")) \
				.begins_with(branch + RUNG_ID_SEPARATOR):
			return row_variant
	return {}

## The band improvement's published build meter (`band_improvements[].build_fraction`), or none.
static func band_build_fraction(band: Dictionary, branch: String) -> float:
	for row_variant in band.get(BAND_IMPROVEMENTS_KEY, []):
		if row_variant is Dictionary and String((row_variant as Dictionary).get("branch", "")) == branch:
			return float((row_variant as Dictionary).get("build_fraction", NO_BUILD_FRACTION))
	return NO_BUILD_FRACTION

## The band's published `storage_saves` entry for the store branch's rung, `{}` once the rung is held
## (the sim lists only the rungs the band does not hold).
static func save_of(band: Dictionary, branch: String) -> Dictionary:
	var wanted := rung_id_of(band, branch)
	for row_variant in band.get(STORAGE_SAVES_KEY, []):
		if row_variant is Dictionary and String((row_variant as Dictionary).get(RUNG_KEY, "")) == wanted:
			return row_variant
	return {}

## Whether the band knows how to raise this band-kind store and has not yet got it: the Work tab lists
## it as a row even before any crew is on it.
static func is_known_unheld_band_store(band: Dictionary, branch: String) -> bool:
	var save := save_of(band, branch)
	return not save.is_empty() and bool(save.get(LESSON_KNOWN_KEY, false)) \
		and not bool(save.get(TILE_KEPT_KEY, false))

## The band-kind store's state: unpublished means held, a declared build means building.
static func band_store_state(band: Dictionary, branch: String) -> int:
	var save := save_of(band, branch)
	if save.is_empty():
		return StoreState.BUILT
	return StoreState.BUILDING if bool(save.get(DECLARED_KEY, false)) else StoreState.NOT_BUILT

## Whether the row offers Build: known, unheld and no build declared yet.
static func offers_build(band: Dictionary, branch: String) -> bool:
	return is_known_unheld_band_store(band, branch) \
		and not bool(save_of(band, branch).get(DECLARED_KEY, false))

## A tile store's state off its published tile-improvement row (`current_rung`, `is_queued`); `rung_key` is
## the bare key the branch's store raises.
static func tile_store_state(tile_row: Dictionary, rung_key: String) -> int:
	var held := String(tile_row.get("current_rung", ""))
	if held.split(RUNG_ID_SEPARATOR)[-1] == rung_key:
		return StoreState.BUILT
	return StoreState.BUILDING if bool(tile_row.get("is_queued", false)) else StoreState.NOT_BUILT

## `meat keeps 20 turns`: the held rung's keeping class in the panel's food words and its shelf life.
static func held_line(held: Dictionary) -> String:
	return STORE_KEEPS_FORMAT % [keeping_word(String(held.get(KEEPING_KEY, ""))),
		roundi(float(held.get(SHELF_LIFE_KEY, 0.0)))]

## `held` is the band's `held_stores` row for a BUILT store: with none, the line is plain `built`.
static func state_line(state: int, fraction: float = NO_BUILD_FRACTION, held: Dictionary = {}) -> String:
	match state:
		StoreState.BUILT:
			return held_line(held) if not held.is_empty() else STORE_BUILT_LINE
		StoreState.BUILDING:
			if fraction < 0.0:
				return STORE_BUILDING_LINE
			return STORE_BUILDING_PERCENT_FORMAT % roundi(fraction * PERCENT_SCALE)
	return STORE_NOT_BUILT_LINE
