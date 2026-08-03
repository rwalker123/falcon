extends RefCounted

## The one-card selection layout, its roster and the compose sheet.
##
## One chapter of the `ui_preview` state walk, run in the order `ui_preview.gd`'s `CHAPTERS`
## lists it. **The order is load-bearing** — states render into one long-lived `HudLayer`, so a
## chapter moved is a set of frames changed. See `.claude/rules/client/test-harnesses.md`.

const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")
const BaseFx := preload("res://tools/ui_preview/fixtures_base.gd")
const ForageFx := preload("res://tools/ui_preview/fixtures_forage.gd")
const HerdFx := preload("res://tools/ui_preview/fixtures_herd.gd")
const Q := preload("res://tools/ui_preview/node_query.gd")
const Readout := preload("res://tools/ui_preview/readouts.gd")
const TileFx := preload("res://tools/ui_preview/fixtures_tile.gd")
const WorldFx := preload("res://tools/ui_preview/fixtures_world.gd")

## The `ui_preview` harness node: the HUD under test, plus `_settle` / `_save` / `_assert_hud`.
var h

# The SECOND player band on the crowded hex (`_crowded_bands_fixture()[1]`, "Band Ash"). The Move
# assertion selects it deliberately: the faction default is the FIRST band, so a Move wired to
# anything but the list selection answers 301 instead.
const TILE_PANEL_MOVE_BAND_ENTITY := 302

# The Move button's face, in both hosts (the drawer's §18 button and the Band/City Orders block).
const MOVE_BUTTON_TEXT := "Move"

# The crowded hex the sticky-land-selection state clicks, and a grid just large enough to contain it
# (the crowded fixtures all sit at 58, 24). Prairie steppe, matching that fixture's biome.
const STICKY_TILE := Vector2i(58, 24)

const STICKY_GRID_W := 64

const STICKY_GRID_H := 32

const STICKY_TERRAIN_ID := 11

# The deselect-keeps-the-tile state's two hexes, on the same grid: a hex carrying the lone herd, and
# an EMPTY land hex a few columns away (far enough that no marker or occupant can bleed into it, so a
# click there resolves as bare land).
const DESELECT_HERD_TILE := Vector2i(30, 16)

const DESELECT_LAND_TILE := Vector2i(34, 16)

# The lone herd on `DESELECT_HERD_TILE`. Named because the land-toggle assertions re-use that same
# one-occupant fixture and have to name the herd the cycle keeps coming back to.
const DESELECT_HERD_ID := "game_deer_405"

# The occupant-cycle state's hex, on the same grid and clear of the other two fixtures' hexes. ONE
# band and TWO herds share it, which is the smallest stack that can prove BOTH halves of issue #429:
# a herd under a band is reachable at all (a band-only prefix used to end the cycle), and a
# multi-herd hex is not stuck on `herds[0]`. The expected cycle order is bands-then-herds, so:
# the band, herd A, herd B, and back to the band.
const CYCLE_TILE := Vector2i(12, 8)

const CYCLE_BAND_ENTITY := 401

const CYCLE_HERD_FIRST_ID := "game_aurochs_429a"

const CYCLE_HERD_SECOND_ID := "game_boar_429b"

const OCCUPANTS_HUNT_LOCAL_WORKERS := 4

const OCCUPANTS_HUNT_PARTY_WORKERS := 6

# The quick-hunt axis guard's herd, and idle workers for the shortcut to have something to send (the
# `quick_hunt_note` state beside it deliberately runs at 0, which is the no-op case).
const QUICK_HUNT_HERD_ID := "game_aurochs_quickhunt"

const QUICK_HUNT_IDLE_WORKERS := 3

# `BandFx.band_fixture` carries no `band_id` (nothing else here emits a command), and `Hud._emit_assign_labor`
# REFUSES a band without one — so the shortcut would no-op silently and the guard would pass on nothing.
const QUICK_HUNT_BAND_ID := 9041

## A synthetic PRESSED mouse-button event, for driving a Control's real `gui_input` handler. The
## harness has no OS input, so this is how a click/wheel gesture is put through the shipped code path
## rather than calling the handler's effect directly.
func _mouse_button_event(button_index: int) -> InputEventMouseButton:
	var event := InputEventMouseButton.new()
	event.button_index = button_index
	event.pressed = true
	return event

## How many Buttons under `root` wear this face — the "is the same order offered twice?" test.
func _count_buttons_by_text(root: Node, text: String) -> int:
	if root == null:
		return 0
	var total := 1 if (root is Button and (root as Button).text == text) else 0
	for child in root.get_children():
		total += _count_buttons_by_text(child, text)
	return total

## **THE STAND IS SILENT WHERE THE VERB IS UNAVAILABLE** (issue #464), asserted over the REAL line
## producer rather than a picture: a `Foraging` row that never rendered and one that rendered off the
## bottom of a scrolled drawer look identical in a PNG, and the claim being made is about which rows
## exist.
##
## **The control half is what makes the rest mean anything.** The two fixtures differ in exactly one
## key (`food_module`), so asserting only the silence would pass just as happily against a producer
## that had stopped emitting food-web rows at all — which is the regression this is most likely to be
## asked to catch. Sabotage-verified: dropping the gate fails the first two, and gating on
## `patch_carrying_capacity` instead of the site fails the fourth.
func _assert_ungathered_stand_is_silent() -> void:
	var ungathered = h._hud._drawer._tile_terrain_lines(ForageFx.floorify(
		_ungathered_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX))
	h._assert_hud("ground nobody gathers states NO Foraging row",
		Readout.detail_row_index(ungathered, HudFloraVocab.FORAGING_KEY) < 0)
	h._assert_hud("…and no basket, since a share is only a share OF a stand you can work",
		ForageFx.flora_basket_rows(ungathered).is_empty())
	# The animal web is untouched, and this is the half that keeps the card from going blank on ground
	# that genuinely feeds herds. Fodder needs no forage action.
	h._assert_hud("…while Grazing still states its stock, because animals eat here regardless",
		Readout.detail_row_index(ungathered, HudFloraVocab.GRAZING_KEY) >= 0)
	# THE CONTROL: the same tile, one key different.
	var gathered = h._hud._drawer._tile_terrain_lines(ForageFx.floorify(
		TileFx.three_role_tile_fixture(), HudComposeVocab.FORAGE_FORECAST_PREFIX))
	h._assert_hud("…and the SAME tile as a gathering site states Foraging and its whole basket",
		Readout.detail_row_index(gathered, HudFloraVocab.FORAGING_KEY) >= 0
			and ForageFx.flora_basket_rows(gathered).size() == 3)

## The instance ids of a container's direct children, so an assertion can prove a restate REUSED the
## same nodes (in-place patch) rather than freeing + recreating them (teardown).
func _child_instance_ids(node: Node) -> Array:
	var ids: Array = []
	if node != null:
		for child in node.get_children():
			ids.append(child.get_instance_id())
	return ids

## The face text of the chip at `index` in the pinned chip strip (each chip is a PanelContainer whose
## first child is its Label).
func _chip_text(strip: Node, index: int) -> String:
	if strip == null or index < 0 or index >= strip.get_child_count():
		return ""
	var chip := strip.get_child(index)
	if chip.get_child_count() == 0:
		return ""
	var label := chip.get_child(0) as Label
	return label.text if label != null else ""

## The forage drawer's standing-summary text (the first child of `%ForageAssignControls` is the
## summary HFlowContainer; its first child is the main status Label).
func _forage_summary_text() -> String:
	var controls = h._hud.forage_assign_controls
	if controls == null or controls.get_child_count() == 0:
		return ""
	var flow = controls.get_child(0)
	if flow.get_child_count() == 0:
		return ""
	var label = flow.get_child(0) as Label
	return label.text if label != null else ""

func _standing_forage_band_fixture() -> Dictionary:
	var band: Dictionary = BandFx.forage_range_bands()[0]
	band["labor_assignments"] = [{
		"kind": "forage", "workers": 4, "target_x": 66, "target_y": 10, "floor": 0.15,
		"actual_yield": 2.74, "sustainable_yield": 0.96, "realized_yield": 2.74,
		"workers_needed": 2, "overdraws": true,
	}]
	return band

## THE FLASH GUARD's tile (docs/plan_hud_decomposition.md §2a): an active, foraged prairie hex with
## the full chip set (sight · habitability · climate · tags · site) and a standing forage patch, so a
## restate with a different `habitability` (Hospitable → Harsh) and `patch_biomass` proves the chips +
## land row + drawer patch in place instead of tearing down. Same coords across restates — the same
## HEX, only its numbers move.
func _no_flash_tile_fixture(habitability: float, biomass: float) -> Dictionary:
	return {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"habitability": habitability,
		"temperature": 18.0,
		"food_module": "savanna_grassland",
		"food_module_label": "Savanna Grassland",
		"site_name": "Verdant Basin",
		"patch_ecology_phase": "thriving",
		"patch_biomass": biomass,
		"patch_carrying_capacity": 120.0,
		"patch_per_worker_yield": 0.32,
		"patch_ceiling_sustain": 0.96,
		"patch_ceiling_surplus": 1.92,
		"patch_ceiling_deplete": 2.88,
		"patch_ceiling_eradicate": 4.80,
	}

## THE FLASH GUARD's band: a player band foraging the no-flash hex with `workers` on it at `yield_val`
## food/turn, so the drawer renders a standing summary (`♻ N foragers · +X /turn`) and the land row a
## `N 🌾` staffing meta — both of which must UPDATE in place (not rebuild) when the numbers change.
## Sustain + `overdraws:false` and no `workers_needed` keep the summary's SHAPE stable across restates
## (no warn/overstaff labels appear/disappear), so only values move.
func _no_flash_band_fixture(workers: int, yield_val: float) -> Dictionary:
	return {
		"id": "Band Steady",
		"size": 30,
		"entity": 909,
		"faction": 0,
		"pos": [66, 10],
		"current_x": 66, "current_y": 10,
		"activity": "forage",
		"working_age": 16,
		"idle_workers": maxi(0, 16 - workers),
		"work_range": 3,
		"settlement_stage_icon": "⛺",
		"settlement_stage_label": "Nomadic band",
		"output_multiplier": 1.0,
		"labor_assignments": [
			{"kind": "forage", "workers": workers, "target_x": 66, "target_y": 10,
				"floor": 0.5, "actual_yield": yield_val, "sustainable_yield": yield_val, "overdraws": false},
		],
	}

## **THE #464 TILE — a full stand on ground nobody gathers.** Rich alluvial plain, a 205-capacity
## patch at Thriving, a three-plant basket led by a staple, pasture beside it — and **no gathering
## site**, so the sim's plant rungs 1–3 all refuse it and no crew can ever be put on it.
##
## It is the `TileFx.three_role_tile_fixture` with its SITE keys cleared together — `food_module` plus the
## two that merely describe that same site (`food_module_label`, `food_kind`), which is one change and
## not three — on its own coordinates, so the saved frame is identifiable and the two fixtures stay
## distinguishable. (`x`/`y` are not inert: `_tile_terrain_lines` resolves the meters' `building_rung`
## through `_band_labor.forage_effort_at`. Neither coordinate carries an assignment in this harness,
## so both resolve to none.) **Every patch, graze and composition key is identical**, and THAT is what
## makes the pair a controlled comparison: everything the food-web rows are built out of is still
## present and still the same, so a `Foraging` row that disappears here disappeared because of the
## site test and not because the fixture went thin. **This is the state the card used to argue with itself in** —
## `Foraging 205 / 205 · Thriving` over a Wild Tubers basket, with `No forage` in the land row two
## rows above and no way to work any of it.
func _ungathered_tile_fixture() -> Dictionary:
	var tile := TileFx.three_role_tile_fixture()
	tile["x"] = 66
	tile["y"] = 9
	tile["food_module"] = ""
	tile["food_module_label"] = ""
	tile.erase("food_kind")
	return tile

## Ground that offers NOTHING to gather: no food module, no patch. The land row's meta must read
## "No forage" (not a blank), and the drawer must carry terrain rows with no compose block.
func _barren_tile_fixture() -> Dictionary:
	return {
		"x": 71, "y": 4,
		"terrain_label": "Rocky Regolith",
		"tags_text": "none",
		"visibility_state": "active",
		"habitability": 0.07,
		"temperature": 2.0,
		"food_module": "",
		"food_module_label": "",
		"height_display": "62 ▮▮▮▮▮▯▯▯",
	}

## THE CROWDED HEX — 3 bands + 2 herds, i.e. six subject rows once the land is counted. The state
## the height cap is judged on: every row visible, the drawer capped, the dock not scrolling.
func _crowded_tile_fixture() -> Dictionary:
	var tile := BaseFx.food_tile_fixture()
	tile["x"] = 58
	tile["y"] = 24
	tile["units"] = _crowded_bands_fixture()
	tile["herds"] = _crowded_herds_fixture()
	return tile

## Three player bands on the crowded hex, spanning the food tiers (green / amber / red dots) and
## carrying real labor so the auto-selected band's drawer renders a full allocation block — which is
## what makes the cap do any work at all.
func _crowded_bands_fixture() -> Array:
	return [
		{"id": "Band Fen", "entity": 301, "faction": 0, "size": 120, "pos": [58, 24],
			"current_x": 58, "current_y": 24, "working_age": 62, "idle_workers": 9,
			"work_range": 2, "hunt_reach": 4, "turns_of_food": 15.0, "morale": 0.72,
			"activity": "forage", "stores": {"provisions": 180.0},
			"food_income": 3.2, "food_consumption": 2.4,
			"labor_assignments": [
				{"kind": "forage", "workers": 5, "target_x": 58, "target_y": 24, "floor": 0.5,
					"actual_yield": 0.96, "sustainable_yield": 0.96, "realized_yield": 0.96,
					"workers_needed": 5, "overdraws": false},
			]},
		{"id": "Band Ash", "entity": 302, "faction": 0, "size": 86, "pos": [58, 24],
			"current_x": 58, "current_y": 24, "working_age": 44, "idle_workers": 4,
			"work_range": 2, "hunt_reach": 4, "turns_of_food": 7.0, "morale": 0.51,
			"activity": "scout", "stores": {"provisions": 40.0}, "labor_assignments": []},
		{"id": "Band Bryn", "entity": 303, "faction": 0, "size": 54, "pos": [58, 24],
			"current_x": 58, "current_y": 24, "working_age": 27, "idle_workers": 0,
			"work_range": 2, "hunt_reach": 4, "turns_of_food": 2.0, "morale": 0.30,
			"activity": "idle", "stores": {"provisions": 8.0}, "labor_assignments": []},
	]

## Two herds sharing the crowded hex — a stressed bison (amber dot) and a thriving boar (green), so
## the Wildlife group is genuinely plural and the ecology dots differ down the list.
func _crowded_herds_fixture() -> Array:
	return [
		HerdFx.occupied_herd_only(),
		{
			"id": "game_boar_04",
			"label": "Wild Boar (game_boar_04)",
			"species": "Wild Boar",
			"size_class": "medium",
			"huntable": true,
			"ecology_phase": "thriving",
			"domestication": 0.0,
			"biomass": 1010.0,
			"carrying_capacity": 1433.0,
			"graze_range_radius": 1,
			"x": 58, "y": 24,
		},
	]

## The MapView snapshot behind `tile_panel_land_sticky` — the crowded hex's OWN bands and herds on a
## grid just big enough to hold it, so MapView's `_tile_info_at` / `_units_on_tile` see exactly what
## the HUD fixture describes. Nothing is redacted because the caller turns FoW OFF explicitly — a
## fresh MapView now defaults to fog ON, and this fixture carries no visibility raster, so every
## occupant would be gated out and the assertion would pass on an empty hex.
func _sticky_map_snapshot() -> Dictionary:
	var terrain: Array = []
	terrain.resize(STICKY_GRID_W * STICKY_GRID_H)
	terrain.fill(STICKY_TERRAIN_ID)
	return {
		"grid": {"width": STICKY_GRID_W, "height": STICKY_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": _crowded_bands_fixture(),
		"herds": _crowded_herds_fixture(),
	}

## The MapView snapshot behind `tile_panel_deselect_keeps_tile` — ONE herd and no bands, so the first
## click resolves as a herd rather than a band (a band would exercise `selected_unit_id`, the other
## half of the same clear branch, but not the herd case the issue was reported on) and `DESELECT_LAND_TILE`
## is genuinely bare. Same grid as the sticky fixture; fog is turned off by the caller.
func _deselect_map_snapshot() -> Dictionary:
	var terrain: Array = []
	terrain.resize(STICKY_GRID_W * STICKY_GRID_H)
	terrain.fill(STICKY_TERRAIN_ID)
	return {
		"grid": {"width": STICKY_GRID_W, "height": STICKY_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [],
		"herds": [{
			"id": DESELECT_HERD_ID,
			"label": "Red Deer (%s)" % DESELECT_HERD_ID,
			"species": "Red Deer",
			"size_class": "big",
			"huntable": true,
			"ecology_phase": "thriving",
			"domestication": 0.0,
			"biomass": 1480.0,
			"carrying_capacity": 2150.0,
			"graze_range_radius": 1,
			"x": DESELECT_HERD_TILE.x, "y": DESELECT_HERD_TILE.y,
		}],
	}

## The MapView snapshot behind `tile_panel_occupant_cycle` — ONE band and TWO herds on a single hex,
## the smallest stack that exercises both kinds and a plural one of the second kind. Same grid as the
## sticky fixture; fog is turned off by the caller. The herds carry neither `herders_needed` half, so
## the field-pair guard skips them (they are wild, and nothing here opens a compose sheet on them).
func _cycle_map_snapshot() -> Dictionary:
	var terrain: Array = []
	terrain.resize(STICKY_GRID_W * STICKY_GRID_H)
	terrain.fill(STICKY_TERRAIN_ID)
	return {
		"grid": {"width": STICKY_GRID_W, "height": STICKY_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [
			{"id": "Band Wold", "entity": CYCLE_BAND_ENTITY, "faction": 0, "size": 94,
				"pos": [CYCLE_TILE.x, CYCLE_TILE.y],
				"current_x": CYCLE_TILE.x, "current_y": CYCLE_TILE.y,
				"working_age": 48, "idle_workers": 6, "work_range": 2, "hunt_reach": 4,
				"turns_of_food": 11.0, "morale": 0.64, "activity": "idle",
				"stores": {"provisions": 120.0}, "labor_assignments": []},
		],
		"herds": [
			{
				"id": CYCLE_HERD_FIRST_ID,
				"label": "Aurochs (%s)" % CYCLE_HERD_FIRST_ID,
				"species": "Aurochs",
				"size_class": "big",
				"huntable": true,
				"ecology_phase": "thriving",
				"domestication": 0.0,
				"biomass": 1620.0,
				"carrying_capacity": 2400.0,
				"graze_range_radius": 1,
				"x": CYCLE_TILE.x, "y": CYCLE_TILE.y,
			},
			{
				"id": CYCLE_HERD_SECOND_ID,
				"label": "Wild Boar (%s)" % CYCLE_HERD_SECOND_ID,
				"species": "Wild Boar",
				"size_class": "medium",
				"huntable": true,
				"ecology_phase": "stressed",
				"domestication": 0.0,
				"biomass": 780.0,
				"carrying_capacity": 1360.0,
				"graze_range_radius": 1,
				"x": CYCLE_TILE.x, "y": CYCLE_TILE.y,
			},
		],
	}

## The occupied hex's herd carrying its tile_info, so show_herd_selection renders
## the full roster with the wildlife row selected.
func _occupied_herd_fixture() -> Dictionary:
	var herd := HerdFx.occupied_herd_only()
	herd["tile_info"] = TileFx.occupied_tile_fixture()
	return herd

func run(harness) -> void:
	h = harness

	# State 3d — a populated hex: the Tile card + the Occupants roster split. Three
	# player bands (turns_of_food 15 / 7 / 2 → green / amber / red vitality dots, with
	# harvest / scout / idle activity glyphs) under Bands (3), and one stressed herd
	# (amber ecology dot) under Wildlife (1). Auto-selects the first band, so the
	# drawer shows its Rations and the Scout verb.
	h._show_tile(TileFx.occupied_tile_fixture())
	await h._settle()
	await h._save("occupants_band")

	# State 3e — the same hex with the wildlife row selected: the drawer swaps to the
	# herd's Species / Biomass and the Hunt / Follow + policy verbs.
	h._show_herd(_occupied_herd_fixture())
	await h._settle()
	await h._save("occupants_herd")

	# State 3e-staffed — the SAME hex, with the bison actually being hunted BOTH ways at once: a
	# standing local hunt (4 workers assigned by Band Fen) and a detached hunting party of 6
	# committed to the same herd. The wildlife row's meta must read the SUM, `10 🏹`, right-aligned
	# exactly like the land row's `N 🌾` — one herd, two mechanisms, one staffing number. The drawer
	# leads with `Size: Big game`, the class that used to ride the row.
	var hunted_bands: Array = WorldFx.occupied_units_fixture()
	hunted_bands[0]["labor_assignments"] = [
		{"kind": "hunt", "workers": OCCUPANTS_HUNT_LOCAL_WORKERS, "fauna_id": "game_bison_02",
			"floor": 0.5, "target_x": 58, "target_y": 24},
	]
	h._hud._band_labor._player_bands = hunted_bands
	h._hud._band_labor._player_band = hunted_bands[0]
	h._hud._band_labor._player_expeditions = [
		{"id": "Party Fen", "entity": 401, "home_band_entity": 301,
			"size": OCCUPANTS_HUNT_PARTY_WORKERS, "expedition_mission": "hunt",
			"expedition_target_herd": "game_bison_02", "expedition_phase": "outbound",
			"current_x": 59, "current_y": 24},
	]
	h._show_herd(_occupied_herd_fixture())
	await h._settle()
	await h._save("occupants_herd_staffed")
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._band_labor._player_expeditions = []

	# ---- ONE CARD, ONE LIST, ONE DRAWER (docs/plan_tile_panel_layout.md) ------------------------
	# The hex is now a single card: a pinned chip strip, one selectable list with the LAND as its
	# first row, and one height-capped drawer that whichever row is lit fills. These six states are
	# the layout's own frames — every other tile/herd/forage state above exercises the same builders
	# through it, which is why their framing changed with this arc.
	h._hud._band_labor._player_band = BandFx.forage_range_bands()[0]
	h._hud._band_labor._player_bands = []
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(-1)
	h._hud._compose.set_forage_count(3)

	# tile_panel_land — the LAND row lit: chips pinned above (In sight · Hospitable · Temperate ·
	# Fertile · Verdant Basin), the land row leading the list with the tile's forage glyph + biome
	# name, and the terrain rows + "Assign foragers" compose block in the drawer beneath.
	h._show_tile(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("tile_panel_land")

	# tile_panel_no_forage — the same layout on ground that offers nothing: the land row's meta
	# reads "No forage" and the drawer carries terrain rows with NO compose block.
	h._show_tile(_barren_tile_fixture())
	await h._settle()
	await h._save("tile_panel_no_forage")

	# tile_panel_ungathered — issue #464: a RICH stand on ground nobody gathers. Distinct from
	# `tile_panel_no_forage` in the only way that matters: there the ground truly carries nothing,
	# here it carries a full patch and a named basket the player can never touch. The card must state
	# `Grazing` and NOT `Foraging` — the plant stand is a verb that is unavailable, the pasture is a
	# fact about ground that feeds herds with no action at all.
	h._show_tile(_ungathered_tile_fixture())
	await h._settle()
	await h._save("tile_panel_ungathered")
	_assert_ungathered_stand_is_silent()

	# tile_panel_herd — a herd row lit: the land row is STILL in the list above it (the land never
	# leaves), and the hunt compose block fills the one drawer.
	h._hud._band_labor._player_band = BandFx.hunt_preview_local_band()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(_occupied_herd_fixture())
	await h._settle()
	await h._save("tile_panel_herd")

	# tile_panel_crowded — THE state this arc exists for: 3 bands + 2 herds. Every row must be
	# visible, the drawer must CAP (scrolling internally on the selected band's allocation block),
	# and the whole card must fit the dock without the dock itself scrolling.
	# The player faction really IS these three bands here, and the first of them forages this very
	# hex — so the land row must report the hex's STAFFING (`5 🌾`), not restate the module name the
	# drawer and the sheet header already carry (§20). Leaving `_player_bands` empty made the row
	# fall back to the module label and ellipsise it, which is the defect, not the fixture's intent.
	h._hud._band_labor._player_bands = _crowded_bands_fixture()
	h._show_tile(_crowded_tile_fixture())
	await h._settle()
	await h._save("tile_panel_crowded")
	# NO Band/City panel is injected here, so this is the legacy fallback path — it renders
	# `%AllocationPanel`, whose Orders block already carries a Move. The drawer's §18 button must NOT
	# be added on top of it, or the player would see the same order offered twice.
	h._assert_hud("the no-panel fallback shows exactly ONE Move button",
		_count_buttons_by_text(h._hud.allocation_panel, MOVE_BUTTON_TEXT) == 1)

	# tile_panel_no_flash — THE FLASH-MECHANISM GUARD (docs/plan_hud_decomposition.md §2a). The
	# tile-inspector "flash" on every turn-advance was `_render_selection_panel` UNCONDITIONALLY
	# tearing down + recreating the card's chips / roster rows / drawer actions even on a same-tile
	# restate where only numbers moved. That teardown is a transient the static PNG harness cannot
	# capture, so this proves the MECHANISM instead: a same-tile restate with CHANGED NUMBERS, fed
	# through the REAL per-snapshot `reapply_selection("tile", …)` path, must PATCH the existing nodes
	# in place — SAME instances, values updated — never free + rebuild them; while a genuine identity
	# change (a band entering the hex) still DOES rebuild, so the diff cannot mask a real update.
	# Proven to FAIL against the pre-fix teardown code.
	h._hud.clear_selection()
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = _no_flash_band_fixture(3, 0.90)
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(-1)
	h._show_tile(_no_flash_tile_fixture(0.01, 84.0))
	await h._settle()
	var flash_chip_ids = _child_instance_ids(h._hud.tile_chips)
	var flash_row_ids = _child_instance_ids(h._hud.subject_list)
	var flash_action_ids = _child_instance_ids(h._hud.forage_assign_controls)
	var flash_chip_before = _chip_text(h._hud.tile_chips, 1)          # the habitability chip
	var flash_summary_before := _forage_summary_text()
	# A SECOND snapshot of the SAME tile with different numbers (habitability rating, patch biomass,
	# forage worker count + rate), replayed the way Main replays MapView's payload every turn.
	h._hud._band_labor._player_band = _no_flash_band_fixture(5, 1.40)
	h._hud.reapply_selection("tile", _no_flash_tile_fixture(0.06, 61.0))
	await h._settle()
	await h._save("tile_panel_no_flash")
	h._assert_hud("same-tile restate REUSES the chip nodes (no teardown)",
		_child_instance_ids(h._hud.tile_chips) == flash_chip_ids and not flash_chip_ids.is_empty())
	h._assert_hud("same-tile restate REUSES the roster-row nodes (no teardown)",
		_child_instance_ids(h._hud.subject_list) == flash_row_ids and not flash_row_ids.is_empty())
	h._assert_hud("same-tile restate REUSES the forage drawer-action nodes (no teardown)",
		_child_instance_ids(h._hud.forage_assign_controls) == flash_action_ids and not flash_action_ids.is_empty())
	h._assert_hud("…and the reused chip's value updated to the new number (Hospitable → Harsh)",
		_chip_text(h._hud.tile_chips, 1) != flash_chip_before and _chip_text(h._hud.tile_chips, 1) != "")
	h._assert_hud("…and the reused drawer summary updated to the new worker count/rate",
		_forage_summary_text() != flash_summary_before and _forage_summary_text() != "")
	# The identity-change half: a band ENTERING the hex must rebuild the roster, so a masked-update
	# bug in the diff cannot survive here.
	var flash_tile_with_band := _no_flash_tile_fixture(0.06, 61.0)
	flash_tile_with_band["units"] = [_no_flash_band_fixture(5, 1.40)]
	h._hud.reapply_selection("tile", flash_tile_with_band)
	await h._settle()
	h._assert_hud("a band entering the hex REBUILDS the roster (membership changed)",
		_child_instance_ids(h._hud.subject_list) != flash_row_ids)
	h._hud.clear_selection()
	h._hud._band_labor._player_band = {}

	# ---- PART 2: THE COMPOSE SHEET (docs/plan_tile_panel_layout.md §10-§17) ----------------------
	# The two ~270px compose blocks left the drawer for a floating sheet. The states above are now the
	# READ state (a standing summary + `Assign … ▸`, and the drawer is visibly shorter for it); these
	# are the WRITE state.

	# tile_panel_compose_forage — the sheet open over the LAND: the full policy grid + band picker +
	# stepper + forecast + button, floating beside the selection card. The MAP MUST STILL BE VISIBLE
	# behind it — an assignment is composed AGAINST the map (work-range ring, hunt reach), so unlike
	# NarrativeForkPanel this sheet draws NO scrim.
	h._hud._band_labor._player_band = BandFx.forage_range_bands()[0]
	h._hud._band_labor._player_bands = []
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(-1)
	h._show_tile(BaseFx.food_tile_fixture())
	h._compose_forage(BaseFx.food_tile_fixture())
	await h._settle()
	h._assert_hud("the Assign button opens the compose sheet", h._hud.is_compose_sheet_open())
	await h._save("tile_panel_compose_forage")

	# tile_panel_compose_herd — the herd sheet on the EXPEDITION branch (the band is beyond hunt
	# reach): the raid forecast + "Send Expedition" must survive the move to the sheet intact.
	h._hud._band_labor._player_bands = [BandFx.hunt_distance_bands()[1]]   # only the FAR band
	h._hud._band_labor._player_band = h._hud._band_labor._player_bands[0]
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(HerdFx.hunt_distance_herd())
	h._compose_herd(HerdFx.hunt_distance_herd())
	await h._settle()
	await h._save("tile_panel_compose_herd")

	# tile_panel_compose_gated — a LOCKED rung inside the sheet: 🐄 Corral greyed AND its gate reasons
	# rendered right beside it. The reasons explain the greyed button, so they had to travel WITH the
	# picker; a reason left behind in the drawer would explain a button that is no longer there.
	h._hud.update_intensification([{
		"faction": 0, "cultivation": 1.0, "herding": 1.0, "seed_selection": 0.0, "penning": 0.35,
	}])
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._show_herd(HerdFx.corral_locked_herd_fixture())
	h._compose_herd(HerdFx.corral_locked_herd_fixture())
	await h._settle()
	await h._save("tile_panel_compose_gated")

	# ---- BEHAVIOURAL ASSERTIONS (§17) -----------------------------------------------------------
	# (2) A SNAPSHOT MUST NOT CLOSE THE SHEET. `reapply_selection` runs every turn; closing on it
	# would make the sheet unusable under autoplay. Driven through the real per-snapshot path — the
	# same `reapply_selection("herd", …)` Main replays from MapView's payload — with the sheet open.
	h._assert_hud("precondition: the herd sheet is open before the snapshot",
		h._hud.is_compose_sheet_open())
	h._hud.reapply_selection("herd", HerdFx.corral_locked_herd_fixture())
	await h._settle()
	h._assert_hud("a snapshot re-render leaves the compose sheet OPEN",
		h._hud.is_compose_sheet_open())
	# …and the SAME refresh DOES close it when the subject it is composing is gone. This half is what
	# proves the half above is not vacuous: the refresh really ran and chose to keep the sheet.
	h._hud.reapply_selection("herd", HerdFx.raid_boar_herd())   # a DIFFERENT herd id
	await h._settle()
	h._assert_hud("a snapshot that swaps the subject closes the sheet",
		not h._hud.is_compose_sheet_open())
	# Re-open on the herd the targeting assertion below needs.
	h._show_herd(HerdFx.corral_locked_herd_fixture())
	h._compose_herd(HerdFx.corral_locked_herd_fixture())
	await h._settle()

	# (3) STARTING A TARGETING FLOW CLOSES THE SHEET — a floating sheet over the map while the player
	# is being asked to click a hex is a trap. Driven through the real Move-band entry point.
	h._hud._targeting.begin_move_band()
	await h._settle()
	h._assert_hud("starting move-band targeting closes the compose sheet",
		not h._hud.is_compose_sheet_open())

	# (1) ESC PRECEDENCE. The chain lives in `Main.escape_claimant`, driven here with the REAL HUD's
	# own `is_compose_sheet_open()` / `is_targeting_active()` rather than hardcoded booleans. It is
	# asserted with BOTH TRUE AT ONCE — targeting is still armed above and the player then opens the
	# sheet on top of it (the drawer stays clickable during targeting, so this is a state the client
	# really reaches). Both-true is the only configuration that can tell the ORDER apart: with the
	# sheet open alone, any ordering answers "compose_sheet".
	h._show_herd(HerdFx.corral_locked_herd_fixture())
	h._compose_herd(HerdFx.corral_locked_herd_fixture())
	h._hud._targeting.begin_move_band()
	h._compose_herd(HerdFx.corral_locked_herd_fixture())
	await h._settle()
	h._assert_hud("precondition: a sheet and targeting are BOTH active",
		h._hud.is_compose_sheet_open() and h._hud.is_targeting_active())
	h._assert_hud("ESC claims the sheet AHEAD of targeting (and never the pause menu)",
		h.MAIN_SCRIPT.escape_claimant(false, h._hud.is_compose_sheet_open(), h._hud.is_targeting_active())
			== h.MAIN_SCRIPT.ESC_COMPOSE_SHEET)
	h._hud.close_compose_sheet()
	await h._settle()
	h._assert_hud("…and with the sheet closed, ESC falls back through to targeting-cancel",
		h.MAIN_SCRIPT.escape_claimant(false, h._hud.is_compose_sheet_open(), h._hud.is_targeting_active())
			== h.MAIN_SCRIPT.ESC_TARGETING)
	h._hud.cancel_active_targeting()
	await h._settle()

	# (4) A WHEEL TICK OVER THE CATCHER MUST NOT DISMISS THE SHEET. The catcher is MOUSE_FILTER_STOP
	# across the whole viewport, so an idle scroll anywhere over the map lands on it — and this sheet
	# has NO SCRIM precisely because the player is still reading that map while composing. Dismissing
	# on a wheel tick would throw the composition away mid-read. Driven through the REAL handler by
	# emitting the catcher's own `gui_input`, and paired with the left-click half, which is what proves
	# the wheel half is not vacuous (i.e. that click-outside dismissal still works at all).
	h._show_herd(HerdFx.corral_locked_herd_fixture())
	h._compose_herd(HerdFx.corral_locked_herd_fixture())
	await h._settle()
	h._assert_hud("precondition: the sheet is open before the wheel tick",
		h._hud.is_compose_sheet_open())
	for wheel_button in [MOUSE_BUTTON_WHEEL_UP, MOUSE_BUTTON_WHEEL_DOWN]:
		h._hud._drawercompose._compose_sheet.gui_input.emit(_mouse_button_event(wheel_button))
	await h._settle()
	h._assert_hud("a wheel tick on the catcher leaves the compose sheet OPEN",
		h._hud.is_compose_sheet_open())
	h._hud._drawercompose._compose_sheet.gui_input.emit(_mouse_button_event(MOUSE_BUTTON_LEFT))
	await h._settle()
	h._assert_hud("a left-click on the catcher still CLOSES the compose sheet",
		not h._hud.is_compose_sheet_open())

	# tile_panel_standing — §14's own frame: the drawer's CLOSED read state on a source the player
	# already works. The summary reuses `SourceForecast.source_yield_readout` verbatim, so it wears the same three
	# parts a Band-panel Current-actions row does — the policy glyph + crew + rate, the ⚠ overdraw
	# flag (ecological) and the "· only N of M working" overstaff note (labor). This fixture crosses
	# the two deliberately: a Deplete patch that DOES overdraw, staffed 4 where only 2 are needed.
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = _standing_forage_band_fixture()
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(-1)
	h._show_tile(BaseFx.food_tile_fixture())
	await h._settle()
	await h._save("tile_panel_standing")

	# tile_panel_land_sticky — THE BEHAVIOURAL ASSERTION for the sticky land selection, driven
	# through the REAL client path, because the bug does not live where a hand-picked
	# `reapply_selection("tile", …)` would put it. MapView holds its OWN occupant selection, and
	# `refresh_selection_payload` answers `kind: "unit"` for as long as `selected_unit_id >= 0` — so on
	# an OCCUPIED hex the tile branch is never even reached. Hence: instance the real MapView, wire the
	# two signals Main wires, click the hex, click the LAND row, then ASK MAPVIEW what the next
	# snapshot's payload is and feed whatever it says into `reapply_selection`. Hardcoding "tile" here
	# would assert a path the bug cannot reach.
	var sticky_map: Node2D = h.MAP_VIEW_SCRIPT.new()
	# Data only — a visible map would render behind the HUD in this and every later frame.
	sticky_map.visible = false
	h.add_child(sticky_map)
	# FoW OFF, stated explicitly — this assertion DIES SILENTLY without it. `_fow_enabled` defaults
	# to `true` (it fails closed for the live client), which fog-gates every band and herd out of
	# `_tile_info_at` / `_units_on_tile` at source: the crowded hex reads "Unexplored / Unknown" and
	# both asserts below pass VACUOUSLY, with no occupant left to fail to stick to. The guard must
	# see the occupants it was written to guard.
	sticky_map.set_fow_enabled(false)
	sticky_map.display_snapshot(_sticky_map_snapshot())
	# Main's wiring, verbatim (Main._on_map_tile_selected / _on_map_unit_selected /
	# _on_hud_roster_occupant_selected).
	sticky_map.tile_selected.connect(h._hud.show_tile_selection)
	sticky_map.unit_selected.connect(h._hud.show_unit_selection)
	h._hud.roster_occupant_selected.connect(sticky_map.select_occupant)
	sticky_map.handle_hex_click(STICKY_TILE.x, STICKY_TILE.y, MOUSE_BUTTON_LEFT)  # lands on a band
	h._hud._selectioncard._on_land_row_selected()                                   # the player picks LAND
	# The next snapshot: Main asks MapView what is selected and replays it into the HUD.
	var sticky_payload: Dictionary = sticky_map.refresh_selection_payload()
	h._hud.reapply_selection(String(sticky_payload.get("kind", "none")), sticky_payload.get("data", {}))
	await h._settle()
	h._assert_hud("land row clears MapView's occupant selection (payload is not \"unit\")",
		String(sticky_payload.get("kind", "")) != "unit")
	h._assert_hud("land selection survives the next snapshot on a crowded hex",
		h._hud._selection._selected_subject == "land" and h._hud._selection._selected_unit.is_empty() and h._hud._selection._selected_herd.is_empty())
	await h._save("tile_panel_land_sticky")
	sticky_map.tile_selected.disconnect(h._hud.show_tile_selection)
	sticky_map.unit_selected.disconnect(h._hud.show_unit_selection)
	h._hud.roster_occupant_selected.disconnect(sticky_map.select_occupant)
	sticky_map.queue_free()
	await h.get_tree().process_frame

	# tile_panel_deselect_keeps_tile — THE BEHAVIOURAL ASSERTION for issue #405: clicking an EMPTY hex
	# while a herd is selected must leave that hex SELECTED, not selectionless. A PNG cannot carry this
	# claim — "no selection outline" and "an outline drawn under an overlay" look identical — so it is
	# asserted on state, through the real click path, the `tile_panel_land_sticky` idiom above: a real
	# MapView, Main's signal wiring, real `handle_hex_click` calls. `_handle_entity_selection`'s clear
	# branch only arms once an occupant is selected, so the first click (the herd) is what makes the
	# second click able to fail; asserting on the empty click alone would pass vacuously.
	var deselect_map: Node2D = h.MAP_VIEW_SCRIPT.new()
	deselect_map.visible = false   # data only — a visible map renders behind the HUD in every later frame
	h.add_child(deselect_map)
	# FoW OFF, stated explicitly (the harness rule): `_fow_enabled` fails closed to `true`, and a
	# fog-gated herd would never be selected by the first click, leaving nothing to deselect.
	deselect_map.set_fow_enabled(false)
	deselect_map.display_snapshot(_deselect_map_snapshot())
	# Main's wiring for this path, verbatim (Main._on_map_tile_selected / _on_map_herd_selected /
	# _on_map_selection_cleared).
	deselect_map.tile_selected.connect(h._hud.show_tile_selection)
	deselect_map.herd_selected.connect(h._hud.show_herd_selection)
	deselect_map.selection_cleared.connect(h._hud.clear_selection)
	deselect_map.handle_hex_click(DESELECT_HERD_TILE.x, DESELECT_HERD_TILE.y, MOUSE_BUTTON_LEFT)
	h._assert_hud("clicking a herd selects the herd AND its tile",
		deselect_map.selected_herd_id != "" and deselect_map.selected_tile == DESELECT_HERD_TILE)
	deselect_map.handle_hex_click(DESELECT_LAND_TILE.x, DESELECT_LAND_TILE.y, MOUSE_BUTTON_LEFT)
	var deselect_payload: Dictionary = deselect_map.refresh_selection_payload()
	await h._settle()
	h._assert_hud("deselecting a herd on an empty hex KEEPS that hex selected (#405)",
		deselect_map.selected_tile == DESELECT_LAND_TILE)
	h._assert_hud("deselecting a herd clears the OCCUPANT selection",
		deselect_map.selected_herd_id == "" and deselect_map.selected_unit_id == -1)
	h._assert_hud("the deselected hex falls back to its land card",
		String(deselect_payload.get("kind", "")) == "tile")
	await h._save("tile_panel_deselect_keeps_tile")
	deselect_map.tile_selected.disconnect(h._hud.show_tile_selection)
	deselect_map.herd_selected.disconnect(h._hud.show_herd_selection)
	deselect_map.selection_cleared.disconnect(h._hud.clear_selection)
	deselect_map.queue_free()
	await h.get_tree().process_frame

	# tile_panel_occupant_cycle — THE BEHAVIOURAL ASSERTION for issue #429: re-clicking a hex cycles
	# through ALL of its occupants, not just its bands. `_handle_entity_selection` used to take
	# `herds_here[0]` and only when the hex held no units at all, so a multi-herd hex always opened on
	# the same herd and a herd sharing a hex with ANY band was unreachable from the map at any number of
	# clicks. A PNG cannot carry that claim — the frames differ only in which name the card is showing —
	# so it is asserted on state through the real click path, the `tile_panel_land_sticky` idiom: a real
	# MapView, Main's signal wiring, real `handle_hex_click` calls.
	var cycle_map: Node2D = h.MAP_VIEW_SCRIPT.new()
	cycle_map.visible = false   # data only — a visible map renders behind the HUD in every later frame
	h.add_child(cycle_map)
	# FoW OFF, stated explicitly (the harness rule): `_fow_enabled` fails closed to `true` and
	# `_herds_on_tile` gates on `_is_tile_visible`, so a fogged hex presents a ZERO-occupant stack and
	# every assertion below would pass vacuously on a cycle with nothing in it.
	cycle_map.set_fow_enabled(false)
	cycle_map.display_snapshot(_cycle_map_snapshot())
	# Main's wiring, verbatim — INCLUDING the roster relay, because the HUD's fresh-hex auto-pick
	# re-enters `select_occupant` through it mid-click (tile_selected → show_tile_selection → render →
	# the auto-pick → roster_occupant_selected → here), rewriting `cycle_index` to the FIRST occupant.
	# Without this connection the harness would not exercise the re-entrancy the cycle has to survive.
	cycle_map.tile_selected.connect(h._hud.show_tile_selection)
	cycle_map.unit_selected.connect(h._hud.show_unit_selection)
	cycle_map.herd_selected.connect(h._hud.show_herd_selection)
	# The FOURTH map→HUD selection signal, and the one the land stop lives or dies on: without it the
	# land click reaches the HUD as nothing at all, the auto-pick sees two empty occupant dicts on a
	# hex it has no recorded choice for, and the first band is selected straight back.
	cycle_map.land_selected.connect(h._hud.show_land_selection)
	h._hud.roster_occupant_selected.connect(cycle_map.select_occupant)
	cycle_map.handle_hex_click(CYCLE_TILE.x, CYCLE_TILE.y, MOUSE_BUTTON_LEFT)
	h._assert_hud("click 1 of the occupant cycle lands on the band (bands still win the first click)",
		cycle_map.selected_unit_id == CYCLE_BAND_ENTITY and cycle_map.selected_herd_id == "")
	cycle_map.handle_hex_click(CYCLE_TILE.x, CYCLE_TILE.y, MOUSE_BUTTON_LEFT)
	h._assert_hud("click 2 advances PAST the band to the first herd (#429: unreachable before)",
		cycle_map.selected_herd_id == CYCLE_HERD_FIRST_ID and cycle_map.selected_unit_id == -1)
	cycle_map.handle_hex_click(CYCLE_TILE.x, CYCLE_TILE.y, MOUSE_BUTTON_LEFT)
	h._assert_hud("click 3 advances to the SECOND herd (a multi-herd hex is not stuck on herds[0])",
		cycle_map.selected_herd_id == CYCLE_HERD_SECOND_ID)
	# The cycled herd has to survive the next snapshot, which is where the HUD's sticky-choice guard
	# could undo it: Main asks MapView what is selected and replays whatever it answers.
	var cycle_payload: Dictionary = cycle_map.refresh_selection_payload()
	h._hud.reapply_selection(String(cycle_payload.get("kind", "none")), cycle_payload.get("data", {}))
	await h._settle()
	h._assert_hud("the cycled herd survives the next snapshot (the HUD auto-pick does not steal it back)",
		String(h._hud._selection._selected_herd.get("id", "")) == CYCLE_HERD_SECOND_ID)
	# Click 4 reaches the LAND — the cycle is everything the tile PANEL lists, not just the occupants,
	# and the land is its LAST stop so the first click on a fresh hex still opens on the top occupant.
	cycle_map.handle_hex_click(CYCLE_TILE.x, CYCLE_TILE.y, MOUSE_BUTTON_LEFT)
	await h._settle()
	h._assert_hud("click 4 advances past the last herd to the LAND (the cycle lists what the panel lists)",
		cycle_map.selected_unit_id == -1 and cycle_map.selected_herd_id == "" \
			and h._hud._selection._selected_subject == HudSelectionState.SUBJECT_LAND)
	# THE STICKY HALF. `_resolve_auto_selected_subject` auto-picks the first band whenever BOTH
	# occupant dicts are empty — which IS the land state — so a map-driven land pick that did not
	# record the choice tile would be undone by the very next snapshot, silently and invisibly. This
	# is the inverse of the herd case above, where a non-empty occupant dict suppresses the auto-pick
	# on its own. Same idiom: ask MapView what the next frame carries and replay whatever it answers.
	var cycle_land_payload: Dictionary = cycle_map.refresh_selection_payload()
	h._hud.reapply_selection(String(cycle_land_payload.get("kind", "none")), cycle_land_payload.get("data", {}))
	await h._settle()
	h._assert_hud("the cycled LAND survives the next snapshot (the HUD auto-pick does not steal the band back)",
		h._hud._selection._selected_subject == HudSelectionState.SUBJECT_LAND \
			and h._hud._selection._selected_unit.is_empty() and h._hud._selection._selected_herd.is_empty())
	await h._save("tile_panel_occupant_cycle")
	cycle_map.handle_hex_click(CYCLE_TILE.x, CYCLE_TILE.y, MOUSE_BUTTON_LEFT)
	h._assert_hud("click 5 WRAPS past the land to the top of the stack",
		cycle_map.selected_unit_id == CYCLE_BAND_ENTITY and cycle_map.selected_herd_id == "")
	# A PANEL roster-row click re-anchors the cycle: the next map click continues from THAT row, which
	# is what deriving the advance from the selected occupant's IDENTITY (rather than from the stored
	# index) buys. Picking the first herd from the list must make the next map click give the second.
	h._hud._selectioncard._on_roster_row_selected("herd", CYCLE_HERD_FIRST_ID)
	cycle_map.handle_hex_click(CYCLE_TILE.x, CYCLE_TILE.y, MOUSE_BUTTON_LEFT)
	h._assert_hud("a map re-click continues from the herd picked in the PANEL, not the stored index",
		cycle_map.selected_herd_id == CYCLE_HERD_SECOND_ID)
	cycle_map.tile_selected.disconnect(h._hud.show_tile_selection)
	cycle_map.unit_selected.disconnect(h._hud.show_unit_selection)
	cycle_map.herd_selected.disconnect(h._hud.show_herd_selection)
	cycle_map.land_selected.disconnect(h._hud.show_land_selection)
	h._hud.roster_occupant_selected.disconnect(cycle_map.select_occupant)
	cycle_map.queue_free()
	await h.get_tree().process_frame

	# The SMALLEST cycle with a land stop, and the one the change was asked for on: a hex with exactly
	# ONE animal and no band, where re-clicking has to TOGGLE herd ↔ land. It re-uses the deselect
	# fixture (one herd, no bands) because that is already that shape. No PNG — the frames it would
	# produce are the herd card and the land card, both already captured elsewhere; what is unproven
	# is the two-member cycle, which only state can carry. The roster relay is wired for the same
	# reason it is above: with no band on the hex the auto-pick reaches for the first HERD, so a land
	# stop that failed to record its choice tile would be pulled straight back to the animal.
	var toggle_map: Node2D = h.MAP_VIEW_SCRIPT.new()
	toggle_map.visible = false   # data only — a visible map renders behind the HUD in every later frame
	h.add_child(toggle_map)
	# FoW OFF, stated explicitly (the harness rule): `_fow_enabled` fails closed to `true`, and a
	# fog-gated herd leaves a ZERO-occupant hex whose cycle has no land stop to reach.
	toggle_map.set_fow_enabled(false)
	toggle_map.display_snapshot(_deselect_map_snapshot())
	toggle_map.tile_selected.connect(h._hud.show_tile_selection)
	toggle_map.herd_selected.connect(h._hud.show_herd_selection)
	toggle_map.land_selected.connect(h._hud.show_land_selection)
	h._hud.roster_occupant_selected.connect(toggle_map.select_occupant)
	toggle_map.handle_hex_click(DESELECT_HERD_TILE.x, DESELECT_HERD_TILE.y, MOUSE_BUTTON_LEFT)
	h._assert_hud("a lone herd still wins the FIRST click on its hex (land is the cycle's LAST stop)",
		toggle_map.selected_herd_id == DESELECT_HERD_ID)
	toggle_map.handle_hex_click(DESELECT_HERD_TILE.x, DESELECT_HERD_TILE.y, MOUSE_BUTTON_LEFT)
	await h._settle()
	h._assert_hud("re-clicking a ONE-animal hex toggles to the land",
		toggle_map.selected_herd_id == "" and toggle_map.selected_unit_id == -1 \
			and h._hud._selection._selected_subject == HudSelectionState.SUBJECT_LAND)
	toggle_map.handle_hex_click(DESELECT_HERD_TILE.x, DESELECT_HERD_TILE.y, MOUSE_BUTTON_LEFT)
	h._assert_hud("a third click toggles back to the animal (a two-member cycle wraps)",
		toggle_map.selected_herd_id == DESELECT_HERD_ID)
	toggle_map.tile_selected.disconnect(h._hud.show_tile_selection)
	toggle_map.herd_selected.disconnect(h._hud.show_herd_selection)
	toggle_map.land_selected.disconnect(h._hud.show_land_selection)
	h._hud.roster_occupant_selected.disconnect(toggle_map.select_occupant)
	toggle_map.queue_free()
	await h.get_tree().process_frame

	# tile_panel_unseen — a REMEMBERED hex. Chips + the land row render (geography is remembered
	# knowledge), the herd this fixture deliberately carries does NOT, and the drawer states that
	# the contents are unknown. An empty list would be a claim of emptiness we cannot back up.
	h._hud.clear_selection()
	h._show_tile(TileFx.sight_tile_fixture(TileFx.VIS_DISCOVERED))
	await h._settle()
	await h._save("tile_panel_unseen")

	# tile_panel_band — a PLAYER band lit while the dockable Band/City panel exists: its detail
	# renders there, so the drawer would otherwise be a blank gap. It must point at where the
	# detail went instead. (The panel is injected only for this frame and released after, so the
	# reserved edge does not follow the states below.)
	var tile_panel_band_panel: BandCityPanel = h.BAND_CITY_PANEL_SCENE.instantiate()
	h.add_child(tile_panel_band_panel)
	# Fan the panel's reservation onto the HUD as Main does, and dock it RIGHT — docked left it
	# reserves the very edge the selection card lives on and covers the frame under test.
	tile_panel_band_panel.reservation_changed.connect(func(edge: int, size: float):
		h._hud.set_reserved_inset(&"band_panel", edge, size))
	tile_panel_band_panel.set_dock(SIDE_RIGHT)
	# The panel's narrow shell shows ONE zone, and its prefs are a fresh profile (see the isolation
	# block in `_ready`), so it opens on `DEFAULT_TAB` = work. This frame is about where the band
	# DETAIL went, so ask for the band zone — the same rule `band_panel_preview` carries. It used to
	# come up on `band` only because a previous run had written that tab into the PLAYER's prefs file.
	tile_panel_band_panel.set_active_tab(BandCityPanel.ZONE_BAND)
	h._hud.set_band_city_panel(tile_panel_band_panel)
	# THREE player bands on this hex, and the faction default is the FIRST one — so "the band the
	# list has selected" and "the faction's default band" are DIFFERENT answers, which is the only
	# configuration in which the Move assertion below can fail (§18).
	var tile_panel_band_roster: Array = _crowded_bands_fixture()
	h._hud._band_labor._player_bands = tile_panel_band_roster
	h._hud._band_labor._player_band = tile_panel_band_roster[0]
	var tile_panel_band_subject: Dictionary = tile_panel_band_roster[0]
	tile_panel_band_subject["tile_info"] = _crowded_tile_fixture()
	h._hud.show_unit_selection(tile_panel_band_subject)
	# The player then picks the SECOND band, through the real subject-list selection path.
	h._hud._selectioncard.select_roster_occupant("unit", TILE_PANEL_MOVE_BAND_ENTITY)
	await h._settle()
	await h._save("tile_panel_band")

	# THE MOVE ASSERTION (§18). Driven through the drawer's REAL button — calling
	# `_targeting.begin_move_band` directly would assert the resolver, not the wiring — and the pending
	# move must name the band SELECTED IN THE LIST (302), never the faction default
	# (`_player_band`, 301), which is what a naive wiring resolves to on a crowded hex.
	var tile_panel_move_btn: Button = Q.find_button_by_text(h._hud.allocation_panel, MOVE_BUTTON_TEXT)
	h._assert_hud("the player-band drawer offers Move", tile_panel_move_btn != null)
	if tile_panel_move_btn != null:
		tile_panel_move_btn.emit_signal("pressed")
	await h._settle()
	h._assert_hud("Move enters move-band targeting", h._hud.is_targeting_active())
	h._assert_hud("…targeting the band SELECTED IN THE LIST, not the faction default",
		int(h._hud._targeting._pending_move_band.get("entity", -1)) == TILE_PANEL_MOVE_BAND_ENTITY)
	h._hud.cancel_active_targeting()
	await h._settle()
	h._hud.set_band_city_panel(null)
	h._hud.set_reserved_inset(&"band_panel", SIDE_RIGHT, 0.0)
	tile_panel_band_panel.queue_free()
	await h.get_tree().process_frame

	# (`tile_panel_feed_shown` is RETIRED with the command feed. It existed to prove the left dock's
	# TWO growing cards could share one height budget; there is one growing card there now.)

	# Restore the single-band compose context the states below assume.
	h._hud._band_labor._player_bands = []
	h._hud._band_labor._player_band = BandFx.band_fixture()
	h._hud._compose.reset_hunt_source()
	h._hud._compose.set_hunt_band(-1)
	h._hud._compose.set_hunt_floor(SourceForecast.FLOOR_FOOD_PEAK)
	h._hud._compose.reset_forage_source()
	h._hud._compose.set_forage_band(-1)

	# State 4 — targeting active: pressing "Move" on the band allocation panel enters
	# tile-targeting, raising the top-centre banner ("MOVE … click a destination tile").
	h._hud.show_unit_selection(BandFx.band_fixture())
	h._hud._targeting.begin_move_band()
	await h._settle()
	await h._save("targeting_banner")
	h._hud.cancel_active_targeting()

	# The old states 4a–4c — the pre-launch raid forecast hanging off the TARGETING BANNER — are
	# gone with the mechanism. They existed because the herd was only known at the targeting step; the
	# band-panel launch flow now picks the quarry FIRST, inside the compose sheet, so the forecast
	# lives in the form with the real party size and policy (band_panel_preview `band_panel_compose_hunt`).

	# State 5 — quick-hunt convenience (map double-click a herd): with idle workers it
	# assigns them to hunt; with none it posts a command-feed note instead of silently
	# no-opping. Seed a fully-staffed band (0 idle) so the note renders in the Command Feed.
	var staffed_band := BandFx.band_fixture()
	staffed_band["idle_workers"] = 0
	h._hud._band_labor._player_band = staffed_band
	h._show_tile(BaseFx.food_tile_fixture())
	h._hud.quick_assign_hunters("game_bison_02")
	await h._settle()
	await h._save("quick_hunt_note")

	# State 5a — PNG-LESS companion: **the shortcut must not blank the improvement axis** (issue #442).
	# `assign_labor` deliberately does not carry the second axis, so between the double-click and the
	# next snapshot the OPTIMISTIC PENDING overlay is the only thing holding it — and an emit that lets
	# it default to `IMPROVEMENT_NONE` flashes a running pen off the work board (and drops the herding
	# crew floor from the would-be count to the ownership-gated one) for the whole turn. No frame:
	# a board rendered from a blanked axis looks like a perfectly ordinary board, so only the overlay
	# can testify. The band hunts ONE herd and is already building its pen; the precondition assertion
	# is what stops the second one passing on a band that had nothing to keep.
	# `Hud._resolve_assign_band` prefers the SELECTED player unit over `player_band()`, and an earlier
	# state left one selected — so clear it, or the shortcut resolves to a band that is building nothing
	# and the assertion below judges the wrong band. (The next state clears it too; this is not restored.)
	h._hud.clear_selection()
	var quick_hunt_band := BandFx.band_fixture()
	quick_hunt_band["band_id"] = QUICK_HUNT_BAND_ID
	quick_hunt_band["idle_workers"] = QUICK_HUNT_IDLE_WORKERS
	quick_hunt_band["labor_assignments"] = [{
		"kind": "hunt", "workers": 2, "floor": 0.5,
		"improvement": SourceForecast.IMPROVEMENT_CORRAL,
		"fauna_id": QUICK_HUNT_HERD_ID, "target_x": 66, "target_y": 10,
	}]
	h._hud._band_labor._player_band = quick_hunt_band
	h._assert_hud("precondition: the quick-hunt band really is building a pen on that herd",
		h._hud._band_labor.improvement_for_hunt(quick_hunt_band, QUICK_HUNT_HERD_ID)
			== SourceForecast.IMPROVEMENT_CORRAL)
	h._hud.quick_assign_hunters(QUICK_HUNT_HERD_ID)
	var quick_hunt_pending: Dictionary = h._hud._band_labor.pending_assigns_for(
		int(quick_hunt_band.get("entity", -1))).get(
			h._hud._band_labor.pending_key(SourceForecast.LABOR_KIND_HUNT, -1, -1, QUICK_HUNT_HERD_ID), {})
	h._assert_hud("a quick-hunt keeps the pen the band is already building on that herd",
		String(quick_hunt_pending.get("improvement", "")) == SourceForecast.IMPROVEMENT_CORRAL)
	# Leave the overlay as it was found — a snapshot with a NEWER turn is what confirms a pending edit.
	h._hud._band_labor.reconcile_pending(h._hud._band_labor.current_turn() + 1)
	h._hud._band_labor._player_band = BandFx.band_fixture()
