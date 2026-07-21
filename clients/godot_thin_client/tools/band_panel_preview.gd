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
## Scratch prefs file — never the player's `user://narrative.cfg`.
const PREVIEW_PREFS_PATH := "user://band_panel_preview_prefs.cfg"
const BAND_PANEL_SCENE := preload("res://src/ui/BandCityPanel.tscn")
const OUT_DIR := "res://ui_preview_out"
# A left inspector strip width to prove co-edge stacking (bug 1).
const INSPECTOR_STRIP := 300.0
# The sim turn the arrival-schedule states render on, so the strip tooltips + the outlook "empty ~turn
# N" marker read as absolute turns rather than the pre-first-overlay relative form.
const ARRIVAL_PREVIEW_TURN := 40

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

	# Isolate the narrative/HUD-panel preferences from the player's real profile before the HUD
	# reads them — otherwise a developer who has pressed `L` renders different frames than one who
	# has not. Same rule as ui_preview; see its prefs-isolation block.
	NarrativeForkPanel.config_path_override = PREVIEW_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(PREVIEW_PREFS_PATH))

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
	# The world's herds (Main pushes snapshot["herds"]): the Current-actions Hunt row names the herd
	# from here and, on click, jumps to its LIVE tile — the herd has MIGRATED away from the
	# assignment's launch target (70, 17) to (68, 15), which is exactly what the row must resolve.
	_hud.update_herds(_herd_fixtures())
	# The world's food modules (Main pushes snapshot["food_modules"]): the Forage row leads with the
	# module's map glyph (savanna grassland → 🌾 on (71, 18)).
	_hud.update_food_modules([
		{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"},
	])
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

	# Clickable Current-actions rows: each Forage/Hunt row's LABEL is an inline link that jumps the
	# map to that source (Scout/Warrior are band-wide roles — plain labels, not links). A static frame
	# can't hover, so synthesize a mouse-motion over the Hunt row's label and render the HOVER skin
	# (tinted fill + cyan border/text) — the affordance proof.
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	var hunt_link := _find_button_containing(_panel, "Hunt ")
	if hunt_link != null:
		# The harness window has no real pointer, so drive the button's hover state directly (the
		# same notification the engine sends on mouse-enter) — BaseButton then draws its hover skin.
		hunt_link.notification(Control.NOTIFICATION_MOUSE_ENTER)
	else:
		push_warning("band_panel_preview: no Hunt link button found — Current-actions row not clickable?")
	await _settle()
	await _save("band_panel_source_row_hover")

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

	# Food + Morale summary-line disclosures — force-expanded in BOTH dock layouts (tall LEFT / wide
	# TOP). The static harness can't click, so the per-band override opens each collapsed-by-default
	# breakdown to confirm the click-expanded layout renders without clipping.
	# (a) Food breakdown open (indented Gathered/Hunted/Eaten under the Food line).
	_hud.update_band_alerts([_band_fixture()])
	_hud._breakdown_expanded = {"food:904": true}
	_hud._refresh_panel_band()
	for state in [{"edge": SIDE_LEFT, "name": "band_panel_food_expanded_left"},
			{"edge": SIDE_TOP, "name": "band_panel_food_expanded_top"}]:
		_panel.set_dock(state["edge"])
		await _settle()
		await _save(state["name"])

	# (b) Morale breakdown open (same disclosure mechanism, indented contributions under Morale).
	_hud._breakdown_expanded = {"morale:904": true}
	_hud._refresh_panel_band()
	for state in [{"edge": SIDE_LEFT, "name": "band_panel_morale_expanded_left"},
			{"edge": SIDE_TOP, "name": "band_panel_morale_expanded_top"}]:
		_panel.set_dock(state["edge"])
		await _settle()
		await _save(state["name"])
	_hud._breakdown_expanded = {}

	# (c) CONCERNING food (net negative + low runway): the breakdown AUTO-shows (no click) under a red net.
	_hud.update_band_alerts([_concerning_food_band_fixture()])
	for state in [{"edge": SIDE_LEFT, "name": "band_panel_food_concerning_left"},
			{"edge": SIDE_TOP, "name": "band_panel_food_concerning_top"}]:
		_panel.set_dock(state["edge"])
		await _settle()
		await _save(state["name"])

	# ROW STATUS GLYPHS — the vocabulary frame. One band whose Current actions carry a CONFIRMED
	# forage row (● working, overstaffed → "· only 2 of 5 working") + a CONFIRMED hunt row (● working,
	# overdrawing → ⚠), plus a PENDING forage row on a DIFFERENT tile (◌, amber) so pending and working
	# read side by side and the ⚠/overstaffing notes prove they still compose. Active expeditions cover
	# every phase glyph: outbound ➤ / hunting ● / delivering ◄ / returning ◄ / awaiting ▮▮ + words.
	_hud.show_tile_selection({})   # clear the foreign selection so the panel band is the subject
	# Drop the earlier bug-2 pending assign (it targets the same tile as the confirmed forage row and
	# would mask it) so this frame shows a CONFIRMED row and a PENDING row side by side.
	_hud._pending_labor.clear()
	_hud.update_band_alerts([_band_fixture()] + _phase_expedition_fixtures())
	_hud._emit_assign_labor(_hud._panel_band, "forage", 4, 72, 19, "", "surplus")
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	await _save("band_panel_status_glyphs")

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

	# PER-SOURCE MAX-USEFUL CAP on the Current-actions rows. Push a band with idle workers to spare and
	# three staffed sources: a Forage row staffed AT its patch's max-useful (3), a Forage row BELOW its
	# patch's max-useful (1 of 5), and a Hunt row staffed AT its herd's max-useful (2). With idle still
	# available the two AT-cap rows' `+` must be DISABLED (capped per source), the below-cap row's `+`
	# ENABLED, and Scout's `+` still tracks idle. The forecast fields ride the pushed herds/patches.
	_hud.show_tile_selection({})
	_hud._pending_labor.clear()
	_hud.update_herds(_cap_demo_herd_fixtures())
	_hud.update_forage_patches(_cap_demo_patch_fixtures())
	_hud.update_band_alerts([_cap_demo_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	await _save("band_panel_source_cap")

	# ARRIVAL SCHEDULE — the per-source tick strip + the merged Food-outlook chart. Seed a current turn
	# so the strip's cell tooltips + the chart's "empty ~turn N" marker read as absolute turns.
	_hud.update_overlay(ARRIVAL_PREVIEW_TURN, {})
	_hud.show_tile_selection({})
	_hud._pending_labor.clear()

	# (a) A LUMPY hunt (gaps) beside a CONTINUOUS forage (every slot positive). The hunt row must gain a
	# tick strip with visible gaps; the forage row must gain NONE (the gap rule); the merged projection
	# must sawtooth upward (hauls > flat drain).
	_hud.update_band_alerts([_arrivals_band_fixture()])
	for state in [{"edge": SIDE_LEFT, "name": "band_panel_arrivals_left"},
			{"edge": SIDE_TOP, "name": "band_panel_arrivals_top"}]:
		_panel.set_dock(state["edge"])
		await _settle()
		await _settle()   # let the deferred fit_content re-pack settle before capture
		await _save(state["name"])

	# (b) A band whose larder EMPTIES inside the horizon: sparse lumpy hauls under a heavy drain, so the
	# walk hits 0 and the chart draws the dashed DANGER "empty ~turn N" marker.
	_hud.update_band_alerts([_arrivals_starving_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	await _save("band_panel_arrivals_empty")

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

## Depth-first search for the first Button whose text CONTAINS `needle` (used to locate a
## Current-actions link row in the live panel tree — the row text leads with a resource glyph,
## so this deliberately does not anchor at the start).
func _find_button_containing(node: Node, needle: String) -> Button:
	if node is Button and (node as Button).text.contains(needle):
		return node as Button
	for child in node.get_children():
		var found := _find_button_containing(child, needle)
		if found != null:
			return found
	return null

## The snapshot's herd list (shape `Hud.update_herds` / `MapView._rebuild_herd_markers` consume).
## The hunted herd sits at (68, 15) — NOT the (70, 17) its hunt assignment was launched at — so the
## Hunt row's jump proves it resolves the herd's current position, not the stale target.
func _herd_fixtures() -> Array:
	return [
		{"id": "game_deer_07", "species": "Red Deer", "x": 68, "y": 15, "population": 120, "ecology_phase": "stressed"},
		{"id": "game_deer_79", "species": "Roe Deer", "x": 64, "y": 11, "population": 90, "ecology_phase": "thriving"},
	]

## Herds for the per-source-cap verify state: game_deer_07 carries the BARE pre-commit forecast fields
## (per_worker_yield / ceiling_sustain) the Current-actions Hunt row reads via `_find_world_herd` +
## `_forecast_inputs`. max-useful = ceil(0.20 / 0.10) = 2, so a Hunt row staffed at 2 is AT its cap.
func _cap_demo_herd_fixtures() -> Array:
	return [
		{"id": "game_deer_07", "species": "Red Deer", "x": 68, "y": 15, "population": 120,
			"ecology_phase": "thriving", "per_worker_yield": 0.10, "ceiling_sustain": 0.20},
	]

## Forage patches for the per-source-cap verify state (shape `update_forage_patches` consumes — the RAW
## wire dict with BARE forecast keys). (71,18): max-useful = ceil(0.30 / 0.10) = 3. (60,20): max-useful
## = ceil(0.50 / 0.10) = 5.
func _cap_demo_patch_fixtures() -> Array:
	return [
		{"x": 71, "y": 18, "per_worker_yield": 0.10, "ceiling_sustain": 0.30},
		{"x": 60, "y": 20, "per_worker_yield": 0.10, "ceiling_sustain": 0.50},
	]

## The per-source-cap verify band: idle workers to spare (4), one Forage row AT its patch max-useful
## (3 at (71,18)), one Forage row BELOW its patch max-useful (1 of 5 at (60,20)), one Hunt row AT its
## herd max-useful (2 on game_deer_07), plus a Scout role. The two AT-cap `+`s must go dead with idle
## still available; the below-cap Forage `+` and the band-wide Scout `+` must stay enabled.
func _cap_demo_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 910
	band["id"] = "Band 8"
	band["idle_workers"] = 4
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 3, "policy": "sustain", "target_x": 71, "target_y": 18, "actual_yield": 0.30, "sustainable_yield": 0.30},
		{"kind": "forage", "workers": 1, "policy": "sustain", "target_x": 60, "target_y": 20, "actual_yield": 0.10, "sustainable_yield": 0.10},
		{"kind": "hunt", "workers": 2, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": 68, "target_y": 15, "actual_yield": 0.20, "sustainable_yield": 0.20},
		{"kind": "scout", "workers": 1},
	]
	return band

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
		# Good food state: long larder runway (≥ warn) + positive net (0.94 − 0.68 = +0.26) → the Food
		# line reads "… · +0.26 /turn" (green) with the category breakdown collapsed (clickable open).
		"days_of_food": 22.0,
		# Good morale (collapsed ▸ disclosure); the signed Layer-1 contributions give the morale
		# breakdown real content when expanded.
		"morale": 0.82,
		"morale_settling": 0.012,
		"morale_terrain": -0.010,
		"morale_climate": -0.006,
		"stores": {"provisions": 84.0},
		"working_age": 16,
		"idle_workers": 3,
		"max_expedition_party_size": 8,
		"work_range": 2,
		"hunt_reach": 16,
		# `settlement_stage_id` is the panel header's SPRITE key (the icon is only the emoji
		# fallback for a stage with no bundled art) — see `StageSprites`.
		"settlement_stage_id": "camp",
		"settlement_stage_icon": "🛖",
		"settlement_stage_label": "Camp",
		"activity": "forage",
		# Band food flow on the Food summary line: net income vs consumption + the Gathered/Hunted
		# breakdown (summed from the assignment actual_yields by kind).
		"food_income": 0.94,
		"food_consumption": 0.68,
		# The hunt overdraws (actual 0.46 > sustainable 0.20) so the ⚠ overhunting flag renders on its
		# allocation row; the forage is renewable (actual == sustainable) so it never flags. The forage
		# is also OVERSTAFFED (5 assigned, 2 needed) → the "· only 2 of 5 working" note, and carries a
		# `policy` so its row shows the ♻ policy glyph — both must survive beside the ● status glyph.
		"labor_assignments": [
			{"kind": "forage", "workers": 5, "workers_needed": 2, "policy": "sustain", "target_x": 71, "target_y": 18, "actual_yield": 0.48, "sustainable_yield": 0.48},
			{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": 70, "target_y": 17, "actual_yield": 0.46, "sustainable_yield": 0.20},
			{"kind": "scout", "workers": 2},
			{"kind": "warrior", "workers": 2},
		],
	}

## A CONCERNING food state: net-negative flow (income 0.30 < consumption 0.95 → net −0.65) + a low
## larder runway (4 days). Both trip the concerning gate, so the category breakdown auto-shows under
## a red net figure. Reuses band 904's chrome fields but a distinct entity so the cycler stays 1/1.
func _concerning_food_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 906
	band["id"] = "Band 4"
	band["days_of_food"] = 4.0
	band["food_income"] = 0.30
	band["food_consumption"] = 0.95
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 3, "target_x": 71, "target_y": 18, "actual_yield": 0.15, "sustainable_yield": 0.15},
		{"kind": "hunt", "workers": 2, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": 70, "target_y": 17, "actual_yield": 0.15, "sustainable_yield": 0.20},
		{"kind": "scout", "workers": 2},
	]
	return band

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

## One expedition per PHASE, all homed on band 904 — the fixture set behind `band_panel_status_glyphs`:
## the Active-expeditions rows must render a distinct, legible glyph for each (➤ outbound / ● hunting /
## ◄ delivering / ◄ returning) and spell `awaiting` out in WARN amber (▮▮ Awaiting orders), since a
## parked party is a demand on the player, not a status.
func _phase_expedition_fixtures() -> Array:
	var scout_outbound := _scout_expedition_fixture()
	var scout_awaiting := _scout_expedition_fixture()
	scout_awaiting["entity"] = 953
	scout_awaiting["id"] = "Scouts 2"
	scout_awaiting["expedition_phase"] = "awaiting"
	var scout_returning := _scout_expedition_fixture()
	scout_returning["entity"] = 954
	scout_returning["id"] = "Scouts 3"
	scout_returning["expedition_phase"] = "returning"
	var hunt_hunting := _hunt_expedition_fixture()
	var hunt_delivering := _hunt_expedition_fixture()
	hunt_delivering["entity"] = 955
	hunt_delivering["id"] = "Hunters 2"
	hunt_delivering["expedition_phase"] = "delivering"
	return [scout_outbound, scout_awaiting, scout_returning, hunt_hunting, hunt_delivering]

## A LUMPY big-game hunt schedule: ~6-food hauls on scattered turns, zeros between them (the cadence a
## whole-animal hunt actually delivers). Length = arrivals_horizon_turns (20). Realized ≈ 2.7/turn.
func _lumpy_hunt_schedule() -> Array:
	var haul_turns := {1: true, 3: true, 4: true, 6: true, 9: true, 11: true, 14: true, 16: true, 19: true}
	var schedule: Array = []
	for i in range(20):
		schedule.append(6.0 if haul_turns.has(i) else 0.0)
	return schedule

## A CONTINUOUS forage schedule at `rate` every turn — no gap, so its row draws NO tick strip (the gap
## rule). Length 20; `rate` matches the fixture's shown realized yield so the merged chart is honest.
func _continuous_forage_schedule(rate: float = 0.9) -> Array:
	var schedule: Array = []
	for i in range(20):
		schedule.append(rate)
	return schedule

## A SPARSE hunt schedule (two hauls, deep gaps) for the emptying-larder state: the drain outpaces the
## trickle and the second haul lands too late, so the larder walk hits 0 mid-horizon.
func _sparse_hunt_schedule() -> Array:
	var haul_turns := {2: true, 9: true}
	var schedule: Array = []
	for i in range(20):
		schedule.append(5.0 if haul_turns.has(i) else 0.0)
	return schedule

## A player band whose sources carry projected arrivals: a LUMPY hunt (gaps → strip) beside a
## CONTINUOUS forage (no gap → no strip). Positive net (hauls + trickle > flat drain), so the merged
## Food-outlook chart sawtooths UPWARD.
func _arrivals_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 920
	band["id"] = "Band 9"
	# NET-POSITIVE (income 3.6 vs drain 2.0), so the runway is the not-food-limited sentinel and the
	# Food line reads ∞ — the sim reports 999 whenever net drain <= 0. A finite countdown here would
	# contradict the upward-sawtoothing chart directly beneath it.
	band["days_of_food"] = BandFoodStatus.UNLIMITED_DAYS
	band["stores"] = {"provisions": 30.0}
	band["food_income"] = 3.6
	band["food_consumption"] = 2.0
	band["labor_assignments"] = [
		{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain",
			"target_x": 70, "target_y": 17, "actual_yield": 2.7, "sustainable_yield": 2.7,
			"realized_yield": 2.7, "arrival_schedule": _lumpy_hunt_schedule()},
		{"kind": "forage", "workers": 3, "policy": "sustain", "target_x": 71, "target_y": 18,
			"actual_yield": 0.9, "sustainable_yield": 0.9, "realized_yield": 0.9,
			"arrival_schedule": _continuous_forage_schedule()},
		{"kind": "scout", "workers": 2},
	]
	return band

## A player band whose larder EMPTIES inside the horizon: a heavy drain over a sparse hunt + a thin
## forage trickle, so the Food-outlook walk reaches 0 and the chart draws the dashed "empty ~turn N".
func _arrivals_starving_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 921
	band["id"] = "Band 10"
	# The runway is the HONEST one — larder walked with income counted (12 food, net drain ~1.6/turn),
	# so it lands on the same turn the chart's dashed "empty ~turn N" marker does. The old
	# larder/consumption reading would have said 4 here and visibly contradicted the chart below it.
	band["days_of_food"] = 9.0
	band["stores"] = {"provisions": 12.0}
	band["food_income"] = 0.9
	band["food_consumption"] = 2.5
	band["labor_assignments"] = [
		{"kind": "hunt", "workers": 3, "fauna_id": "game_deer_07", "policy": "sustain",
			"target_x": 70, "target_y": 17, "actual_yield": 0.5, "sustainable_yield": 0.5,
			"realized_yield": 0.5, "arrival_schedule": _sparse_hunt_schedule()},
		{"kind": "forage", "workers": 2, "policy": "sustain", "target_x": 71, "target_y": 18,
			"actual_yield": 0.4, "sustainable_yield": 0.4, "realized_yield": 0.4,
			"arrival_schedule": _continuous_forage_schedule(0.4)},
		{"kind": "scout", "workers": 1},
	]
	return band

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
