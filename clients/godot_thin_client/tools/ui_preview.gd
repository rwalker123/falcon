extends Node

## Dev-only UI preview harness.
##
## Instances the real HudLayer with canned selection data, renders each state,
## and saves a PNG to `ui_preview_out/` in the project. Lets us iterate on HUD /
## selection-panel / targeting styling without a running server or manual
## screenshots. Not part of the game — run explicitly:
##
##   godot --path . res://tools/ui_preview.tscn
##
## then read ui_preview_out/*.png.

const HUD_SCENE := preload("res://src/ui/HudLayer.tscn")
# Force-compile MapView here so the harness also acts as a full-context compile
# check for it (autoloads are registered when the harness runs as a scene, which
# --check-only cannot do).
const MAP_VIEW_SCRIPT := preload("res://src/scripts/MapView.gd")
const OUT_DIR := "res://ui_preview_out"

var _hud: HudLayer

func _ready() -> void:
	get_window().size = Vector2i(1500, 900)
	DirAccess.make_dir_absolute(OUT_DIR)

	# A mid-tone terrain-ish backdrop so the translucent card reads correctly.
	var bg_layer := CanvasLayer.new()
	bg_layer.layer = -10
	add_child(bg_layer)
	var bg := ColorRect.new()
	bg.color = Color(0.10, 0.15, 0.16)
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	bg_layer.add_child(bg)

	_hud = HUD_SCENE.instantiate()
	add_child(_hud)
	await get_tree().process_frame
	await get_tree().process_frame

	# Top-bar Sedentarization meter (faction 0, soft band) — visible across all frames.
	_hud.update_sedentarization([{"faction": 0, "score": 62.0, "stage": "soft"}])

	# Top-bar demographics readout (faction 0 age structure + dependency ratio).
	_hud.update_demographics([{"faction": 0, "children": 34, "working": 51, "elders": 15}])

	# State 1 — a single band selected: the Occupants roster + drawer (primary Scout).
	# The band fixture carries a warn-tier larder, so the Food line reads amber.
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("band")

	# State 1a — a well-fed but demoralized band: healthy food (∞) yet morale 0.22
	# (< critical), so the drawer's Morale line reads a red 22%.
	_hud.show_unit_selection(_low_morale_band_fixture())
	await _settle()
	await _save("band_low_morale")

	# State 1b — band alerts: seed previous sizes, then a snapshot that raises all
	# three alert kinds (starving red / losing-population amber / idle quiet).
	_hud.update_band_alerts(_band_alert_baseline())
	_hud.update_band_alerts(_band_alert_fixture())
	await _settle()
	await _save("band_alerts")

	# State 2 — a food tile selected (primary Harvest button).
	_hud.show_tile_selection(_food_tile_fixture())
	await _settle()
	await _save("food_tile")

	# State 3 — a herd selected on a food tile: Harvest + Hunt + Follow verbs plus
	# the Sustain/Surplus/Eradicate policy picker all surface together. A Thriving
	# herd shows a neutral ecology readout.
	_hud.show_herd_selection(_herd_fixture())
	await _settle()
	await _save("herd_verbs")

	# State 3b — an overhunted herd: the ecology readout warns "⚠ Collapsing" in red.
	_hud.show_herd_selection(_collapsing_herd_fixture())
	await _settle()
	await _save("herd_collapsing")

	# State 3c — a domesticated herd: the husbandry readout shows "🐄 Domesticated".
	_hud.show_herd_selection(_domesticated_herd_fixture())
	await _settle()
	await _save("herd_domesticated")

	# State 3d — a populated hex: the Tile card + the Occupants roster split. Three
	# player bands (days_of_food 15 / 7 / 2 → green / amber / red vitality dots, with
	# harvest / scout / idle activity glyphs) under Bands (3), and one stressed herd
	# (amber ecology dot) under Wildlife (1). Auto-selects the first band, so the
	# drawer shows its Rations and the Scout verb.
	_hud.show_tile_selection(_occupied_tile_fixture())
	await _settle()
	await _save("occupants_band")

	# State 3e — the same hex with the wildlife row selected: the drawer swaps to the
	# herd's Species / Biomass and the Hunt / Follow + policy verbs.
	_hud.show_herd_selection(_occupied_herd_fixture())
	await _settle()
	await _save("occupants_herd")

	# State 4 — targeting active: pending harvest raises the top-centre banner
	# and flips the button to the armed "Cancel Harvest" treatment.
	_hud.show_tile_selection(_food_tile_fixture())
	_hud._begin_pending_forage(66, 10, "savanna_grassland", "forage")
	await _settle()
	await _save("targeting_banner")

	# Icon probe last, on a top layer with its own backdrop (rendering is warm by
	# now), so every food glyph is captured via the map's draw path.
	var probe_layer := CanvasLayer.new()
	probe_layer.layer = 100
	add_child(probe_layer)
	var probe_bg := ColorRect.new()
	probe_bg.color = Color(0.06, 0.09, 0.10)
	probe_bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	probe_layer.add_child(probe_bg)
	var probe := preload("res://tools/icon_probe.gd").new()
	probe_layer.add_child(probe)
	await _settle()
	await _save("food_icons")

	get_tree().quit()

func _settle() -> void:
	await get_tree().process_frame
	await RenderingServer.frame_post_draw
	await get_tree().process_frame

func _save(name: String) -> void:
	var image := get_viewport().get_texture().get_image()
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("ui_preview: failed to save %s (err %d)" % [name, err])
	else:
		print("ui_preview: saved ", name, ".png")

func _band_fixture() -> Dictionary:
	return {
		"id": "Band 2",
		"size": 148,
		"entity": 904,
		"pos": [71, 18],
		"days_of_food": 7.0,
		"morale": 0.82,
		"stores": {"provisions": 84.0},
		"tile_info": {
			"x": 71, "y": 18,
			"terrain_label": "Freshwater Marsh",
			"tags_text": "Freshwater, Wetland",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	}

## A well-fed band whose morale has collapsed on a harsh tile: food is not limited
## (∞) but morale 0.22 sits below the critical threshold, so the Morale row reads red.
func _low_morale_band_fixture() -> Dictionary:
	var fixture := _band_fixture()
	fixture["id"] = "Band 5"
	fixture["entity"] = 905
	fixture["days_of_food"] = 999.0
	fixture["stores"] = {"provisions": 260.0}
	fixture["morale"] = 0.22
	# Falling morale driven by the harsh cavern terrain: the drawer shows
	# "Morale: 22% ▼ — harsh terrain (Karst Cavern Mouth)".
	fixture["morale_delta"] = -0.006
	fixture["morale_cause"] = 1  # Terrain
	fixture["tile_info"] = {
		"x": 44, "y": 61,
		"terrain_label": "Karst Cavern Mouth",
		"tags_text": "Subsurface, Harsh",
		"visibility_state": "active",
		# Cavern habitability (~0.0825) lands in the Harsh band → amber Tile-card row.
		"habitability": 0.0825,
		# High-latitude cold ~-2° → "Polar" climate band (neutral Tile-card row).
		"temperature": -2.0,
		"food_module": "",
		"food_module_label": "None",
	}
	return fixture

## Prior-snapshot band sizes so the "losing population" alert has a baseline to
## compare against (Band Ash drops 90 → 78 in the live fixture below).
func _band_alert_baseline() -> Array:
	return [
		{"faction": 0, "entity": 101, "size": 60, "days_of_food": 12.0, "activity": "harvest", "current_x": 71, "current_y": 18},
		{"faction": 0, "entity": 102, "size": 90, "days_of_food": 999.0, "activity": "hunt", "current_x": 40, "current_y": 22},
		{"faction": 0, "entity": 103, "size": 45, "days_of_food": 999.0, "activity": "harvest", "current_x": 12, "current_y": 9},
	]

func _band_alert_fixture() -> Array:
	return [
		# Starving: 3 days of food (< critical) → red alert.
		{"faction": 0, "entity": 101, "size": 60, "days_of_food": 3.0, "activity": "harvest", "current_x": 71, "current_y": 18,
			"harvest": {"band_label": "Band Fen"}},
		# Losing population from harsh terrain: size 90 → 78, well-fed (∞) but the
		# dominant morale cause is Terrain → amber alert "losing population — harsh terrain".
		{"faction": 0, "entity": 102, "size": 78, "days_of_food": 999.0, "morale": 0.30, "morale_cause": 1, "activity": "hunt", "current_x": 40, "current_y": 22,
			"harvest": {"band_label": "Band Ash"}},
		# Idle labor: quiet low-priority alert.
		{"faction": 0, "entity": 103, "size": 45, "days_of_food": 999.0, "activity": "idle", "current_x": 12, "current_y": 9},
	]

func _food_tile_fixture() -> Dictionary:
	return {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		# Fertile steppe: low drain → "Hospitable" (green Tile-card row).
		"habitability": 0.01,
		# Mid-latitude ~18° → "Temperate" climate band (neutral Tile-card row).
		"temperature": 18.0,
		"food_module": "savanna_grassland",
		"food_module_label": "Savanna Grassland",
		"food_module_weight": 1.0,
		"food_kind": "savanna_track",
	}

func _herd_fixture() -> Dictionary:
	return {
		"id": "game_deer_07",
		"label": "Red Deer (game_deer_07)",
		"species": "Red Deer",
		"size_class": "big",
		"huntable": true,
		"ecology_phase": "thriving",
		"domestication": 0.4,
		"x": 66, "y": 10,
		"biomass": 820.0,
		"route_length": 3,
		"tile_info": _food_tile_fixture(),
	}

## A hex with an occupant stack: 3 player bands + 1 herd, for the Occupants roster.
func _occupied_tile_fixture() -> Dictionary:
	return {
		"x": 58, "y": 24,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "savanna_grassland",
		"food_module_label": "Savanna Grassland",
		"food_module_weight": 1.0,
		"food_kind": "savanna_track",
		"units": _occupied_units_fixture(),
		"herds": [_occupied_herd_only()],
	}

## Three player bands sharing the hex, spanning the food-status tiers (green /
## amber / red) and distinct activities (harvest / scout / idle glyphs).
func _occupied_units_fixture() -> Array:
	return [
		{"id": "Band Fen", "entity": 301, "faction": 0, "size": 120, "pos": [58, 24],
			"days_of_food": 15.0, "activity": "harvest", "stores": {"provisions": 180.0}},
		{"id": "Band Ash", "entity": 302, "faction": 0, "size": 86, "pos": [58, 24],
			"days_of_food": 7.0, "activity": "scout", "stores": {"provisions": 40.0}},
		{"id": "Band Bryn", "entity": 303, "faction": 0, "size": 54, "pos": [58, 24],
			"days_of_food": 2.0, "activity": "idle", "stores": {"provisions": 8.0}},
	]

## The stressed herd sharing the occupied hex (amber ecology dot).
func _occupied_herd_only() -> Dictionary:
	return {
		"id": "game_bison_02",
		"label": "Steppe Bison (game_bison_02)",
		"species": "Steppe Bison",
		"size_class": "big",
		"huntable": true,
		"ecology_phase": "stressed",
		"domestication": 0.0,
		"biomass": 240.0,
		"x": 58, "y": 24,
	}

## The occupied hex's herd carrying its tile_info, so show_herd_selection renders
## the full roster with the wildlife row selected.
func _occupied_herd_fixture() -> Dictionary:
	var herd := _occupied_herd_only()
	herd["tile_info"] = _occupied_tile_fixture()
	return herd

func _collapsing_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["biomass"] = 96.0
	fixture["ecology_phase"] = "collapsing"
	fixture["domestication"] = 0.0
	return fixture

func _domesticated_herd_fixture() -> Dictionary:
	var fixture := _herd_fixture()
	fixture["domestication"] = 1.0
	return fixture
