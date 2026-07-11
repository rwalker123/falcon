extends Node2D

## Dev-only MapView preview harness (companion to tools/ui_preview.gd, which is HUD-only).
##
## Instances the real MapView, feeds a canned snapshot via display_snapshot(), selects a
## player band, renders each state, and saves a PNG to ui_preview_out/. Lets us visually
## verify the selected-band labor highlights (work-range ring / worked forage tiles /
## hunted-herd ring + link) without a server. Run windowed (NOT headless —
## the dummy renderer can't read back the viewport):
##
##   godot --path . res://tools/map_preview.tscn
##
## then read ui_preview_out/map_*.png.

const MAP_VIEW := preload("res://src/scripts/MapView.gd")
const OUT_DIR := "res://ui_preview_out"

const GRID_W := 16
const GRID_H := 12
const BAND_ENTITY := 9001
const BAND_X := 8
const BAND_Y := 6
const TERRAIN_ID := 5  # arbitrary land biome for a legible backdrop
const STACK_ENTITY_BASE := 9100   # co-located band entities are STACK_ENTITY_BASE + i
const HERD_ON_TILE_ID := "game_boar_03"   # herd id used by the selected-hex herd fixture
# Canned settlement-stage tokens (the native bridge doesn't run here, so preview band dicts must
# carry settlement_stage_* directly). Icons are opaque sim strings — the emoji here just mirror the
# current config so the map token glyphs render. EMPTY exercises the fallback faction disc.
const STAGE_NOMADIC := {"id": "nomadic", "label": "Nomadic band", "icon": "⛺"}
const STAGE_CAMP := {"id": "camp", "label": "Seasonal camp", "icon": "🛖"}
const STAGE_VILLAGE := {"id": "village", "label": "Settled village", "icon": "🏘️"}
const STAGE_NONE := {"id": "", "label": "", "icon": ""}   # pre-stage / missing → fallback disc
# Stage cycle used to fan mixed glyphs across a co-located band stack.
const STACK_STAGE_CYCLE := [STAGE_NOMADIC, STAGE_CAMP, STAGE_VILLAGE, STAGE_NONE]
# Far-zoom LOD grid: large enough that fitted hexes fall under ICON_MIN_DETAIL_RADIUS.
const FAR_GRID_W := 72
const FAR_GRID_H := 52

var _map: Node2D

func _ready() -> void:
	get_window().size = Vector2i(1000, 800)
	DirAccess.make_dir_absolute(OUT_DIR)
	_map = MAP_VIEW.new()
	add_child(_map)
	await get_tree().process_frame
	await get_tree().process_frame

	# State A — a band working two forage tiles + hunting a distant herd. Shows the
	# work-range ring (Chebyshev square), two strong-green worked forage tiles, and the
	# red herd ring + band→herd link (the herd sits OUTSIDE the ring: hunt reach = range + leash).
	_map.display_snapshot(_snapshot_work())
	_map.selected_unit_id = BAND_ENTITY
	_map._fit_map_to_view()
	await _settle()
	await _save("map_band_work")

	# State B — the same band with scouts staffed: scouting no longer draws a map highlight
	# (its effect is the extended sight visible in the fog; `scout_reveal_radius` is a
	# sight-range bonus, not a reveal disc). This state is a regression guard that NO blue
	# scouted disc appears — only the work-range ring + the single worked forage tile.
	_map.display_snapshot(_snapshot_scout())
	_map.selected_unit_id = BAND_ENTITY
	_map._fit_map_to_view()
	await _settle()
	await _save("map_band_scout")

	# State C — optimistic pending overlay: a just-issued forage assign (new tile) + a pending
	# move destination show in a distinct dashed-amber style, over the confirmed highlights.
	_map.display_snapshot(_snapshot_work())
	_map.selected_unit_id = BAND_ENTITY
	_map.set_labor_pending({
		BAND_ENTITY: {
			"turn": 0,
			"assign": {"forage:6,7": {"kind": "forage", "x": 6, "y": 7, "herd_id": ""}},
			"move": {"x": 8, "y": 9},
		}
	})
	_map._fit_map_to_view()
	await _settle()
	await _save("map_band_pending")

	# State D — Wondrous Sites: a landmark (⛰) and a settle-site (⛲) glyph marker, plus one
	# placed on the herd tile to exercise the overlap nudge (offset up so both stay legible).
	_map.set_labor_pending({})  # clear State C's pending overlay so this frame reads clean
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_sites())
	_map.selected_unit_id = BAND_ENTITY
	_map._fit_map_to_view()
	await _settle()
	await _save("map_sites")

	# State E — persistence under fog: FoW on, every tile only Discovered (remembered) except the
	# band's own hex (Active). A discovered site is permanent knowledge, so all three glyph markers
	# must STILL render on the fogged/remembered tiles (unlike the Active-only herd/food markers).
	_map.set_fow_enabled(true)
	_map.display_snapshot(_snapshot_sites_fogged())
	_map.selected_unit_id = BAND_ENTITY
	_map._fit_map_to_view()
	await _settle()
	await _save("map_sites_fogged")

	# State F — scouting expeditions (docs/plan_exploration_and_sites.md §2): alongside the
	# resident band (solid faction dot, unchanged) two detached parties render as hollow
	# flag discs — one Outbound, one Awaiting-orders (pulsing amber ring). Verifies the distinct
	# marker + idle indicator without disturbing resident-band rendering.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_expeditions())
	_map.selected_unit_id = -1
	_map._fit_map_to_view()
	await _settle()
	await _save("map_expeditions")

	# State G — multi-band card stack (hex-icon-stack UX): 4 player bands on one hex render as an
	# up-right offset stack (top card + 2 dimmed cards) plus a `×4` count badge, the tile carries the
	# white selection outline, and the active (selected) band shows the white top-card ring.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_stack(4))
	_map.selected_tile = Vector2i(BAND_X, BAND_Y)
	_map.selected_unit_id = STACK_ENTITY_BASE + 1   # not the first band → verifies active reordering
	_map._fit_map_to_view()
	await _settle()
	await _save("map_band_stack")

	# State H — mixed hex: a band (center token) sharing a hex with 1 herd + 1 food site + 3 wonders.
	# Exercises the fixed edge slots (3 visible icons) AND the `+N` overflow chip (2 spill over), on a
	# selected hex (white outline). Priority fill is wonder → food → herd.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_mixed())
	_map.selected_tile = Vector2i(BAND_X, BAND_Y)
	_map.selected_unit_id = BAND_ENTITY
	_map._fit_map_to_view()
	await _settle()
	await _save("map_mixed_hex")

	# State I — far-zoom level-of-detail: a large grid makes fitted hexes tiny (radius <
	# ICON_MIN_DETAIL_RADIUS), so secondary edge icons + count/overflow chips are suppressed — only
	# the primary band tokens draw. Regression guard that far zoom stays legible, not a glyph soup.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_far_zoom())
	_map.selected_unit_id = -1
	_map.selected_tile = Vector2i(-1, -1)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_far_zoom")

	# State J — selected hex containing a herd: the white hex outline is the SOLE selection cue;
	# the herd glyph gets NO ring (fixes the redundant/confusing circle and the split-state where a
	# migrating herd's ring diverged from the outline). selected_herd_id targets the herd on the tile.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_herd_on_tile())
	_map.selected_tile = Vector2i(BAND_X, BAND_Y)
	_map.selected_herd_id = HERD_ON_TILE_ID
	_map.selected_unit_id = -1
	_map._fit_map_to_view()
	await _settle()
	await _save("map_herd_selected")

	# State K — split-state guard: the selected band (selected_unit_id) stands on a DIFFERENT hex than
	# selected_tile, simulating a band that migrated off the clicked hex on turn-advance. The outline
	# stays on selected_tile; NO active-ring may draw on the band's actual hex (group_tile !=
	# selected_tile). Confirms the ring can never diverge from the outline into a split selection.
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_stack(1))       # one band on (BAND_X, BAND_Y)
	_map.selected_unit_id = STACK_ENTITY_BASE       # that band
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(BAND_X - 3, BAND_Y - 2)   # a different, empty hex
	_map._fit_map_to_view()
	await _settle()
	await _save("map_ring_divergence")

	# State L — settlement-stage glyph tokens: four bands (three stages + one empty-stage fallback),
	# side by side with DIFFERENT factions, so the ⛺→🛖→🏘️ progression + distinct faction-colored
	# nameplate banners read at a glance (no selection chrome).
	_map.set_fow_enabled(false)
	_map.display_snapshot(_snapshot_stages_row())
	_map.selected_unit_id = -1
	_map.selected_herd_id = ""
	_map.selected_tile = Vector2i(-1, -1)
	_map._fit_map_to_view()
	await _settle()
	await _save("map_stage_glyphs")

	get_tree().quit()

func _settle() -> void:
	await get_tree().process_frame
	RenderingServer.force_draw()
	await get_tree().process_frame

func _save(name: String) -> void:
	var image := get_viewport().get_texture().get_image()
	if image == null:
		push_warning("map_preview: null image (dummy renderer?) — run without --headless")
		return
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("map_preview: failed to save %s (err %d)" % [name, err])
	else:
		print("map_preview: saved ", name, ".png")

func _terrain_array() -> Array:
	var arr: Array = []
	arr.resize(GRID_W * GRID_H)
	arr.fill(TERRAIN_ID)
	return arr

func _base_snapshot(band: Dictionary, herds: Array) -> Dictionary:
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": _terrain_array()},
		"populations": [band],
		"herds": herds,
	}

## Merge a stage's presentation tokens into a band dict (in place) and return it.
func _with_stage(band: Dictionary, stage: Dictionary) -> Dictionary:
	band["settlement_stage_id"] = String(stage.get("id", ""))
	band["settlement_stage_label"] = String(stage.get("label", ""))
	band["settlement_stage_icon"] = String(stage.get("icon", ""))
	return band

func _band(assignments: Array, work_range: int, scout_radius: int) -> Dictionary:
	return _with_stage({
		"entity": BAND_ENTITY,
		"faction": 0,
		"current_x": BAND_X,
		"current_y": BAND_Y,
		"size": 30,
		"id": "Band 1",
		"work_range": work_range,
		"scout_reveal_radius": scout_radius,
		"labor_assignments": assignments,
	}, STAGE_NOMADIC)

func _deer_herd() -> Dictionary:
	# Well outside the work-range ring (Chebyshev distance 5 from the band).
	return {"id": "game_deer_07", "label": "Red Deer (game_deer_07)", "x": 13, "y": 6, "biomass": 800.0, "huntable": true}

func _snapshot_work() -> Dictionary:
	var assignments := [
		{"kind": "forage", "workers": 5, "target_x": 7, "target_y": 6},
		{"kind": "forage", "workers": 3, "target_x": 9, "target_y": 8},
		{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": 13, "target_y": 6},
		{"kind": "warrior", "workers": 2},
	]
	return _base_snapshot(_band(assignments, 2, 2), [_deer_herd()])

func _snapshot_scout() -> Dictionary:
	var assignments := [
		{"kind": "scout", "workers": 5},
		{"kind": "forage", "workers": 3, "target_x": 7, "target_y": 6},
	]
	return _base_snapshot(_band(assignments, 2, 2), [_deer_herd()])

func _sites_state() -> Array:
	return [{
		"faction": 0,
		"sites": [
			{"x": 6, "y": 5, "site_id": "great_peak", "category": "landmark", "display_name": "Great Peak", "glyph": "⛰"},
			{"x": 10, "y": 7, "site_id": "verdant_basin", "category": "settle_site", "display_name": "Verdant Basin", "glyph": "⛲"},
			# On the deer-herd tile → exercises the overlap nudge (marker offset up).
			{"x": 13, "y": 6, "site_id": "sky_arch", "category": "landmark", "display_name": "Sky Arch", "glyph": "⛰"},
		],
	}]

func _snapshot_sites() -> Dictionary:
	var snap := _base_snapshot(_band([], 2, 2), [_deer_herd()])
	snap["discovered_sites"] = _sites_state()
	return snap

## A detached scouting party (docs/plan_exploration_and_sites.md §2): a cohort tagged Expedition
## flowing through the same populations[] array as a band. `awaiting` drives the pulsing idle ring.
func _expedition(entity: int, x: int, y: int, phase: String) -> Dictionary:
	return {
		"entity": entity,
		"faction": 0,
		"current_x": x,
		"current_y": y,
		"size": 6,
		"id": "Scouts",
		"is_expedition": true,
		"expedition_mission": "scout",
		"expedition_phase": phase,
		"is_traveling": phase != "awaiting",
	}

func _snapshot_expeditions() -> Dictionary:
	var snap := _base_snapshot(_band([], 2, 2), [])
	# Two expeditions alongside the resident band: one outbound, one awaiting (pulsing ring).
	snap["populations"].append(_expedition(9101, 11, 3, "outbound"))
	snap["populations"].append(_expedition(9102, 5, 9, "awaiting"))
	return snap

func _band_at(entity: int, x: int, y: int, stage: Dictionary = STAGE_NOMADIC, faction: int = 0) -> Dictionary:
	return _with_stage({
		"entity": entity,
		"faction": faction,
		"current_x": x,
		"current_y": y,
		"size": 30,
		"id": "Band %d" % entity,
		"work_range": 2,
		"scout_reveal_radius": 0,
		"labor_assignments": [],
	}, stage)

## N player bands co-located on (BAND_X, BAND_Y) → exercises the offset card stack + `×N` badge
## folded onto the banner's right end. Fans DIFFERENT stage glyphs (and DIFFERENT factions) across
## the cards so the active (top) card's faction banner shows a distinct color; only the active card
## draws a banner, so the back cards are bare dimmed glyphs behind it.
func _snapshot_stack(n: int) -> Dictionary:
	var bands: Array = []
	for i in range(n):
		var stage: Dictionary = STACK_STAGE_CYCLE[i % STACK_STAGE_CYCLE.size()]
		bands.append(_band_at(STACK_ENTITY_BASE + i, BAND_X, BAND_Y, stage, i % 3))
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": _terrain_array()},
		"populations": bands,
		"herds": [],
	}

## One band sharing (BAND_X, BAND_Y) with 1 herd + 1 food site + 3 wonders → 3 edge slots + `+2` chip.
func _snapshot_mixed() -> Dictionary:
	var snap := {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": _terrain_array()},
		"populations": [_band_at(BAND_ENTITY, BAND_X, BAND_Y, STAGE_VILLAGE)],
		"herds": [{"id": "game_boar_03", "label": "Wild Boar (game_boar_03)", "x": BAND_X, "y": BAND_Y, "biomass": 400.0, "huntable": true}],
		"food_modules": [{"x": BAND_X, "y": BAND_Y, "module": "berry_patch", "kind": "forage"}],
		"discovered_sites": [{
			"faction": 0,
			"sites": [
				{"x": BAND_X, "y": BAND_Y, "site_id": "peak_a", "category": "landmark", "display_name": "Peak A", "glyph": "⛰"},
				{"x": BAND_X, "y": BAND_Y, "site_id": "spring_b", "category": "settle_site", "display_name": "Spring B", "glyph": "⛲"},
				{"x": BAND_X, "y": BAND_Y, "site_id": "grove_c", "category": "landmark", "display_name": "Grove C", "glyph": "🌋"},
			],
		}],
	}
	return snap

## Four separate bands on adjacent hexes → the ⛺ / 🛖 / 🏘️ glyph tokens side by side for a direct
## progression read, each over its faction-colored nameplate banner. Bands are assigned DIFFERENT
## factions (blue / orange / green / orange) so distinct banner colors read at a glance. The fourth
## band is empty-stage → exercises the neutral non-circular fallback marker (with a banner, no disc).
func _snapshot_stages_row() -> Dictionary:
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": _terrain_array()},
		"populations": [
			_band_at(STACK_ENTITY_BASE, BAND_X - 3, BAND_Y, STAGE_NOMADIC, 0),
			_band_at(STACK_ENTITY_BASE + 1, BAND_X - 1, BAND_Y, STAGE_CAMP, 1),
			_band_at(STACK_ENTITY_BASE + 2, BAND_X + 1, BAND_Y, STAGE_VILLAGE, 2),
			_band_at(STACK_ENTITY_BASE + 3, BAND_X + 3, BAND_Y, STAGE_NONE, 1),
		],
		"herds": [],
	}

## A single herd alone on the band's hex → selecting that hex must show only the outline, no ring.
func _snapshot_herd_on_tile() -> Dictionary:
	return {
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": _terrain_array()},
		"populations": [],
		"herds": [{"id": HERD_ON_TILE_ID, "label": "Wild Boar (game_boar_03)", "x": BAND_X, "y": BAND_Y, "biomass": 400.0, "huntable": true}],
	}

## Large grid so fitted hexes are tiny (< ICON_MIN_DETAIL_RADIUS): bands + secondaries present, but
## only the primary tokens should draw (secondary icons + chips suppressed by LOD).
func _snapshot_far_zoom() -> Dictionary:
	var terrain: Array = []
	terrain.resize(FAR_GRID_W * FAR_GRID_H)
	terrain.fill(TERRAIN_ID)
	var cx := FAR_GRID_W / 2
	var cy := FAR_GRID_H / 2
	return {
		"grid": {"width": FAR_GRID_W, "height": FAR_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [
			_band_at(BAND_ENTITY, cx, cy),
			_band_at(STACK_ENTITY_BASE, cx + 3, cy + 2),
		],
		"herds": [{"id": "game_deer_09", "label": "Red Deer (game_deer_09)", "x": cx + 1, "y": cy, "biomass": 600.0, "huntable": true}],
		"food_modules": [{"x": cx - 1, "y": cy + 1, "module": "berry_patch", "kind": "forage"}],
	}

func _snapshot_sites_fogged() -> Dictionary:
	var snap := _snapshot_sites()
	# Visibility raster (raw encoding: 0.0 unexplored / 0.5 discovered / 1.0 active). All tiles
	# Discovered except the band's own hex Active, so the site markers sit on remembered tiles.
	var vis := PackedFloat32Array()
	vis.resize(GRID_W * GRID_H)
	vis.fill(0.5)
	vis[BAND_Y * GRID_W + BAND_X] = 1.0
	snap["overlays"] = {
		"terrain": _terrain_array(),
		"channels": {"visibility": {"raw": vis, "normalized": vis, "label": "Visibility"}},
	}
	return snap
