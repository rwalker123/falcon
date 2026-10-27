class_name HudTileVocab
extends RefCounted

## **THE TILE-SOURCED BRANCHES' VOCABULARY** -- `source_kind == "tile"` in `intensification_ladder.json`
## (belief's monument, food stores' pit, and whatever the config adds next). A tile branch is an
## improvement standing ON a hex that a crew keeps: the crew line places it at the branch's root rung
## and, on a bare tile, declares the branch's first rung itself (`assign_labor <f> <band> tile <branch> <x> <y>
## <n>`: the row IS the order), the band's builders raise it (`build <verb> <f> <x> <y>` remains a valid press).
##
## ⛔ **NOTHING HERE NAMES A BRANCH.** Every noun, verb, cost and gate is read off the rung CATALOG
## (`SubsistenceSection.ladderRungs`), so a branch added to the config reaches the tile card, the crew
## sheet and the step card with no client edit. The only per-rung facts this file holds are the art marks (`RUNG_MARKS`); the
## only per-SITE-TERM facts are the words for a site requirement (`min_belief`), keyed by the term.
##
## The catalog readers (`catalog_*`, `branch_ladder`, `ladder_order_of`) are `HudDepositVocab`'s: they
## read the same wire row and a second copy would be a second spelling of every key.

const SOURCE_KIND_TILE := "tile"

## The `tile_improvements` row's keys (`RouteSection.tileImprovements`).
const ROW_TILE_X := "tile_x"
const ROW_TILE_Y := "tile_y"
const ROW_BRANCH := "branch"
const ROW_CURRENT_RUNG := "current_rung"

## No crew on the tile. The sim refuses a tile build with no crew ("Assign a crew first").
const CREW_NONE := 0

## A crew of one is the least that places the improvement, and what a first crew opens the sheet on
## when the band has no standing row to seed from. It is a starting point, not a cap: the stepper's
## ceiling is the band's working-age people and the count the player sets is what is sent.
const CREW_DEFAULT := 1

# ---- THE CATALOG, FILTERED TO THE TILE BRANCHES ---------------------------------------------------

## The tile-kind rows of a catalog, in the catalog's own order. `[]` before the first snapshot.
static func tile_ladder(catalog: Array) -> Array[Dictionary]:
	var rows: Array[Dictionary] = []
	for entry_variant in catalog:
		if entry_variant is Dictionary \
				and String((entry_variant as Dictionary).get("source_kind", "")) == SOURCE_KIND_TILE:
			rows.append(entry_variant as Dictionary)
	return rows

## The tile branches the catalog carries, once each, in the order their first rung appears.
static func branches(ladder: Array[Dictionary]) -> Array[String]:
	var out: Array[String] = []
	for entry in ladder:
		var branch := HudDepositVocab.catalog_branch(entry)
		if branch != "" and not out.has(branch):
			out.append(branch)
	return out

## The first rung a crew-and-builders can raise on this branch: the lowest rung that declares a verb.
## `{}` for a branch that offers nothing to order.
static func first_buildable(ladder: Array[Dictionary], branch: String) -> Dictionary:
	for entry in HudDepositVocab.branch_ladder(ladder, branch):
		if HudDepositVocab.catalog_verb(entry) != HudDepositVocab.RUNG_CATALOG_NONE:
			return entry
	return {}

## The branch's root rung: where a crew's placed improvement stands before anything is raised.
static func root_entry(ladder: Array[Dictionary], branch: String) -> Dictionary:
	return HudDepositVocab.branch_floor_entry(ladder, branch)

# ---- THE BUILT RUNG'S MARK --------------------------------------------------------------------------

## Rung wire key (`branch:id`, the catalog's `rung_key`) -> `HudSprites` mark id. Art is bundled per RUNG, so
## the table is keyed by the key the catalog carries and a rung without art has no entry (the verb glyph is
## the fallback). No code path branches on a branch: the branch is resolved to its rung through the catalog.
const RUNG_MARKS := {
	"food_stores:storage_pit": "storage_pit",
	"belief:monument": "monument",
}

## The mark id for one catalog rung, `""` when the rung has no art.
static func rung_mark(entry: Dictionary) -> String:
	return String(RUNG_MARKS.get(HudDepositVocab.catalog_rung_key(entry), ""))

## The mark of a branch's BUILT rung -- the first rung above the root that declares a verb. `""` for none.
static func branch_mark(ladder: Array[Dictionary], branch: String) -> String:
	return rung_mark(first_buildable(ladder, branch))

# ---- THE TILE'S STATE OF A BRANCH ------------------------------------------------------------------

## A `tile_improvements` row normalised to the shape the shared deposit readers take: `rung` is the
## rung held (the branch root where no improvement is placed yet), and the identity keys are filled so
## `HudDepositVocab.tile_of` / `branch_of` answer for a tile with no row at all.
static func site_of(row: Dictionary, ladder: Array[Dictionary], branch: String, x: int, y: int) -> Dictionary:
	var site := row.duplicate()
	site["tile_x"] = x
	site["tile_y"] = y
	site["branch"] = branch
	var held := String(row.get(ROW_CURRENT_RUNG, ""))
	if held == "":
		held = HudDepositVocab.catalog_rung_key(root_entry(ladder, branch))
	site["rung"] = held
	return site

# ---- THE TILE CARD'S ROW ---------------------------------------------------------------------------

## ⛔ **THE BRANCH'S ROW ON THE TILE CARD IS ITS DOOR** (`SubjectDrawerController` registers it): the crew
## sheet opens from the row that states the branch, not from a button beneath the card. Which row that
## is comes off the CATALOG and nothing here names a branch:
##   * a branch whose site term asks the tile's belief (`min_belief`) is stated by the row the dead
##     already have, `DetailFormat.ANCESTORS_ROW` -- a tile branch whose readout row exists keeps it;
##   * any other tile branch gets a row of its own, keyed by the branch's display name
##     (`HudRouteVocab.branch_name`) and valued by the rung the tile stands at.
static func readout_row_key(entry: Dictionary, branches: Array) -> String:
	if catalog_min_belief(entry) > BELIEF_NONE:
		return DetailFormat.ANCESTORS_ROW
	return HudRouteVocab.branch_name(branches, HudDepositVocab.catalog_branch(entry))

## `Storage pit`, `Unstored`: the rung the tile STANDS at on a branch -- the placed improvement's, else
## the branch root's -- named as the catalog names it. `""` where the catalog carries no rung for it.
static func standing_rung_name(ladder: Array[Dictionary], branch: String, improvement: Dictionary,
		x: int, y: int) -> String:
	var site := site_of(improvement, ladder, branch, x, y)
	return HudDepositVocab.ladder_rung_name(ladder, String(site.get("rung", "")))

## The refusal's hover: the short reason, then the long one, so the row says on hover everything the
## greyed amber line beside the retired step button said.
const REFUSAL_HOVER_SEPARATOR := "\n"

static func refusal_hover(refusal: Dictionary) -> String:
	var parts: Array[String] = []
	for key in ["short", "long"]:
		var part := String(refusal.get(key, "")).strip_edges()
		if part != "":
			parts.append(part)
	return REFUSAL_HOVER_SEPARATOR.join(parts)

## Sheet: eyebrow `ASSIGN CREW` (through `HudComposeVocab.COMPOSE_SHEET_EYEBROW_FORMAT`), the branch's
## catalog noun as the title, a `Crew` stepper, one hint line, the commit.
const CREW_SHEET_NOUN := "crew"
const CREW_STEPPER_LABEL := "Crew"
const CREW_HINT := "Adding the crew starts the first build. They keep it; the band's builders raise it."
const COMMIT_LABEL := "Assign crew"

## Handle the harnesses find the crew sheet's stepper row by (the value is the `x,y:branch` subject key).
const CREW_ROW_META := "tile_branch_crew_row"
## ...and the crew sheet's work strip (its ladder chip is the way to the step card).
const STRIP_META := "tile_branch_strip"

## Does this tile's card offer the branch at all? The first buildable rung's lesson is known, or the tile
## already holds an improvement of the branch -- so a fresh game's tile card does not carry two greyed
## rows it can do nothing about.
static func is_offered(ladder: Array[Dictionary], branch: String, knowledge: Dictionary,
		holds_improvement: bool) -> bool:
	if holds_improvement:
		return true
	var entry := first_buildable(ladder, branch)
	if entry.is_empty():
		return false
	var unlock := HudDepositVocab.catalog_unlock_knowledge(entry)
	return unlock == HudDepositVocab.RUNG_CATALOG_NONE \
		or float(knowledge.get(unlock, 0.0)) >= HudConst.KNOWLEDGE_COMPLETE

# ---- THE SITE REQUIREMENT ---------------------------------------------------------------------------

## ⛔ **A SITE TERM IS WORDED BY THE TERM, NEVER BY THE BRANCH.** `min_belief` is compared with the belief
## standing on the tile, which is the dead buried there and the gatherings held there (the card's
## `Ancestors: N dead` row). `0` is "asks nothing of the tile's belief".
const BELIEF_NONE := 0.0
const SITE_NO_BELIEF_SHORT := "No dead are buried here"
const SITE_FEW_BELIEF_SHORT := "Too few dead are buried here"
const SITE_BELIEF_LONG_FORMAT := "%s needs ground where people have buried their dead and gathered. %s"
const SITE_BELIEF_HAVE_NONE := "None are buried here."
const SITE_BELIEF_HAVE_SOME := "Not enough are buried here yet."

static func catalog_min_belief(entry: Dictionary) -> float:
	return float(entry.get("min_belief", BELIEF_NONE))

static func tile_belief(tile_info: Dictionary) -> float:
	return float(tile_info.get("belief", BELIEF_NONE))

## The refusal record `{short, long}` for a rung's site requirement on this tile, `{}` where the tile
## meets it (or the rung asks nothing). The caller wraps it in its gate record.
static func site_refusal(entry: Dictionary, tile_info: Dictionary) -> Dictionary:
	var wanted := catalog_min_belief(entry)
	if wanted <= BELIEF_NONE:
		return {}
	var have := tile_belief(tile_info)
	if have >= wanted:
		return {}
	var none := have <= BELIEF_NONE
	return {
		"short": SITE_NO_BELIEF_SHORT if none else SITE_FEW_BELIEF_SHORT,
		"long": SITE_BELIEF_LONG_FORMAT % [
			HudDepositVocab.catalog_display_name(entry),
			SITE_BELIEF_HAVE_NONE if none else SITE_BELIEF_HAVE_SOME],
	}

## The first site refusal among the branch's buildable rungs, for the tile card's greyed row. `{}` when
## the tile suits the branch.
static func branch_site_refusal(ladder: Array[Dictionary], branch: String, tile_info: Dictionary) -> Dictionary:
	return site_refusal(first_buildable(ladder, branch), tile_info)

# ---- THE GATES' WORDS -------------------------------------------------------------------------------

const GATE_KIND_SITE := "site"
const GATE_KIND_CREW := "crew"
const GATE_KIND_CRAFT := "craft"
const GATE_KIND_GROUND := "ground"

## No crew works the tile: the build only reaches a tile a band of yours works, and the crew row itself declares
## the branch's first rung.
const GATE_SHORT_NO_CREW := "no crew"
const GATE_LONG_NO_CREW := "No crew works this tile. Adding one starts the build, and the band's builders raise it."

## The row-priority order a locked row prefers: the site no learning closes, then the crew that closes in
## one gesture, then the lesson, then the rung beneath.
const GATE_ROW_PRIORITY := [GATE_KIND_SITE, GATE_KIND_CREW, GATE_KIND_CRAFT, GATE_KIND_GROUND]

# ---- THE STEP CARD ------------------------------------------------------------------------------------

const TILE_LADDER_NAME_WIDTH := 106.0
const TILE_LADDER_TIP_SEPARATOR := "\n"
