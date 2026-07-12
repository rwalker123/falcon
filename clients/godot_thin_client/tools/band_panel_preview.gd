extends Node

## Dev-only preview harness for the dockable Band / City panel (slice 2 scaffold).
##
## Instances the real BandCityPanel alongside a real HudLayer, wires the panel's
## reservation onto the HUD (mirroring Main's `_apply_reservation` fan-out for the
## `hud` surface), then docks the panel to each edge (+ collapsed) and dumps one
## PNG per state so the chrome + the HUD reflow can be eyeballed without a server.
## The full MAP reflow/clip is only exercised in the running client.
##
##   godot --path . res://tools/band_panel_preview.tscn
##
## then read ui_preview_out/band_panel_*.png.

const HUD_SCENE := preload("res://src/ui/HudLayer.tscn")
const BAND_PANEL_SCENE := preload("res://src/ui/BandCityPanel.tscn")
const OUT_DIR := "res://ui_preview_out"
# A left inspector strip width to prove co-edge stacking (bug 1).
const INSPECTOR_STRIP := 300.0

var _hud: HudLayer
var _panel: BandCityPanel

func _ready() -> void:
	get_window().size = Vector2i(1500, 900)
	DirAccess.make_dir_absolute(OUT_DIR)

	var bg_layer := CanvasLayer.new()
	bg_layer.layer = -10
	add_child(bg_layer)
	var bg := ColorRect.new()
	bg.color = Color(0.10, 0.15, 0.16)
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	bg_layer.add_child(bg)

	_hud = HUD_SCENE.instantiate()
	add_child(_hud)

	_panel = BAND_PANEL_SCENE.instantiate()
	add_child(_panel)
	# Fan the panel's reservation onto the HUD, as Main does for both surfaces.
	_panel.reservation_changed.connect(func(edge: int, size: float):
		if _hud.has_method("set_reserved_inset"):
			_hud.set_reserved_inset(&"band_panel", edge, size))

	await get_tree().process_frame
	await get_tree().process_frame

	# Seed the top bar so the HUD reflow reads against real content.
	_hud.update_sedentarization([{"faction": 0, "score": 62.0, "stage": "soft"}])
	_hud.update_demographics([{"faction": 0, "children": 34, "working": 51, "elders": 15}])

	# Slice 3: inject the panel into the HUD and push a player band through the real snapshot
	# path (update_band_alerts → _refresh_panel_band), so the FULL band detail relocates into the
	# panel — summary lines + labor allocation + the settlement stage header/cycler.
	# Push the band PLUS two detached expeditions (home_band_entity = the band's entity): the cycler
	# must read 1/1 (expeditions excluded), and the panel's "Active expeditions" section must list
	# both. Order interleaves an expedition first to prove the split (not just "first cohort = band").
	_hud.set_band_city_panel(_panel)
	_hud.update_band_alerts([_scout_expedition_fixture(), _band_fixture(), _hunt_expedition_fixture()])
	print("band_panel_preview: cycler split — player_bands=%d (expect 1), player_expeditions=%d (expect 2)" % [
		_hud._player_bands.size(), _hud._player_expeditions.size()])

	# Dock to each edge and render.
	_panel.set_collapsed(false)
	for state in [
		{"edge": SIDE_LEFT, "name": "band_panel_left"},
		{"edge": SIDE_RIGHT, "name": "band_panel_right"},
		{"edge": SIDE_TOP, "name": "band_panel_top"},
		{"edge": SIDE_BOTTOM, "name": "band_panel_bottom"},
	]:
		_panel.set_dock(state["edge"])
		await _settle()
		await _save(state["name"])

	# Collapsed rail (docked left).
	_panel.set_dock(SIDE_LEFT)
	_panel.set_collapsed(true)
	await _settle()
	await _save("band_panel_collapsed")
	_panel.set_collapsed(false)

	# Bug 1 — co-edge stacking with the Inspector. Reserve a left inspector strip (as Main does)
	# and push the band panel's matching leading offset, docked left: the panel must render to the
	# RIGHT of the strip (no overlap at x=0). The strip region is left empty here (no inspector in
	# this harness) — what matters is the panel starts at INSPECTOR_STRIP, not the screen edge.
	_panel.set_dock(SIDE_LEFT)
	_hud.set_reserved_inset(&"inspector", SIDE_LEFT, INSPECTOR_STRIP)
	_panel.set_edge_offset(INSPECTOR_STRIP)
	await _settle()
	await _save("band_panel_stacked_left")
	_hud.set_reserved_inset(&"inspector", SIDE_LEFT, 0.0)
	_panel.set_edge_offset(0.0)

	# Bug 2 — panel stays populated on a stepper edit while a FOREIGN hex is selected. Selecting a
	# tile calls `_selected_unit.clear()`; `_panel_band` must NOT alias it. Then drive a worker
	# assign on the panel band (the worker-stepper path → `_after_pending_change`): the panel must
	# stay populated (never blank) and show the optimistic "· pending".
	_hud.show_tile_selection({"x": 5, "y": 5, "terrain_label": "Prairie Steppe", "visibility_state": "active"})
	print("band_panel_preview: bug2 — _panel_band empty after foreign select? ", _hud._panel_band.is_empty())
	_hud._emit_assign_labor(_hud._panel_band, "forage", 6, 71, 18, "", "")
	await _settle()
	await _save("band_panel_stepper_foreign")

	# Fit-to-content height (no clipping) — push a TALLER band: starving + full morale breakdown +
	# output row + the send-expedition section, so the summary column is much taller than the old fixed
	# T/B PANEL_HEIGHT would allow. Dock top/bottom and confirm every column's bottom row is visible and
	# the reserved strip grew to fit (map/HUD reflow is fanned onto the HUD as usual).
	_hud.show_tile_selection({})   # clear the foreign selection so the panel band is the subject again
	_hud.update_band_alerts([_starving_band_fixture(), _scout_expedition_fixture(), _hunt_expedition_fixture()])
	for state in [
		{"edge": SIDE_TOP, "name": "band_panel_top_tall"},
		{"edge": SIDE_BOTTOM, "name": "band_panel_bottom_tall"},
	]:
		_panel.set_dock(state["edge"])
		await _settle()
		await _settle()   # extra frame: let the deferred fit_content re-pack + reservation settle
		await _save(state["name"])

	get_tree().quit()

func _settle() -> void:
	await get_tree().process_frame
	RenderingServer.force_draw()
	await get_tree().process_frame

func _save(name: String) -> void:
	var image := get_viewport().get_texture().get_image()
	if image == null:
		push_warning("band_panel_preview: null image (dummy renderer?) — skipping %s.png; run without --headless" % name)
		return
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("band_panel_preview: failed to save %s (err %d)" % [name, err])
	else:
		print("band_panel_preview: saved ", name, ".png")

## A player-faction Camp-stage band (population-snapshot shape update_band_alerts consumes):
## working-age labor with idle workers + a couple of active assignments + the settlement stage
## header fields, so the relocated panel shows a full detail + allocation report.
func _band_fixture() -> Dictionary:
	return {
		"id": "Band 2",
		"entity": 904,
		"faction": 0,
		"size": 148,
		"pos": [71, 18],
		"current_x": 71,
		"current_y": 18,
		"days_of_food": 7.0,
		"morale": 0.82,
		"stores": {"provisions": 84.0},
		"working_age": 16,
		"idle_workers": 3,
		"max_expedition_party_size": 8,
		"work_range": 2,
		"hunt_reach": 16,
		"settlement_stage_icon": "🛖",
		"settlement_stage_label": "Camp",
		"activity": "forage",
		"labor_assignments": [
			{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18},
			{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": 70, "target_y": 17},
			{"kind": "scout", "workers": 2},
			{"kind": "warrior", "workers": 2},
		],
	}

## A TALLER band variant (same entity 904, so the expeditions still attach): starving + declining
## morale with the full itemized breakdown + an Output row + the send-expedition section, so the
## summary column runs well past the old fixed T/B PANEL_HEIGHT — the case that used to clip.
func _starving_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["days_of_food"] = 1.5
	band["morale"] = 0.22
	band["morale_delta"] = -0.055
	band["morale_cause"] = 1   # Terrain
	band["morale_settling"] = 0.010
	band["morale_terrain"] = -0.030
	band["morale_climate"] = -0.020
	band["morale_unrest"] = -0.015
	band["output_multiplier"] = 0.62
	band["last_emigrated"] = 4
	return band

## A detached SCOUT expedition outfitted by band 904 (home_band_entity), outbound to (39,26).
func _scout_expedition_fixture() -> Dictionary:
	return {
		"id": "Scouts 1",
		"entity": 951,
		"faction": 0,
		"size": 4,
		"current_x": 39,
		"current_y": 26,
		"days_of_food": 9.0,
		"is_expedition": true,
		"expedition_mission": "scout",
		"expedition_phase": "outbound",
		"home_band_entity": 904,
	}

## A detached HUNT expedition outfitted by band 904, following game_deer_79 under a Surplus policy.
func _hunt_expedition_fixture() -> Dictionary:
	return {
		"id": "Hunters 1",
		"entity": 952,
		"faction": 0,
		"size": 6,
		"current_x": 66,
		"current_y": 12,
		"days_of_food": 5.0,
		"is_expedition": true,
		"expedition_mission": "hunt",
		"expedition_phase": "hunting",
		"expedition_target_herd": "game_deer_79",
		"expedition_hunt_policy": "surplus",
		"home_band_entity": 904,
	}
