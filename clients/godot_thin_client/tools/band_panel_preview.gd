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
## Scratch DOCK prefs — never the player's `user://band_city_dock.cfg`. Without this the harness both
## read the tab a previous run left selected (so the early frames rendered whichever zone that was,
## not the band zone they exist to show) and wrote its own tab walk back over the player's.
const PREVIEW_DOCK_PREFS_PATH := "user://band_panel_preview_dock.cfg"
const BAND_PANEL_SCENE := preload("res://src/ui/BandCityPanel.tscn")
## The real MapView, for the map-selection path state (see `band_panel_people_map_path`).
const MAP_VIEW_SCRIPT := preload("res://src/scripts/MapView.gd")
## The hex `_band_fixture()` stands on — the tile the map-path state clicks.
const MAP_PATH_TILE := Vector2i(71, 18)
## A grid just large enough to hold MAP_PATH_TILE, and one flat terrain id to fill it with.
const MAP_PATH_GRID_W := 80
const MAP_PATH_GRID_H := 30
const MAP_PATH_TERRAIN_ID := 11
const OUT_DIR := "res://ui_preview_out"
# A left inspector strip width to prove co-edge stacking (bug 1).
const INSPECTOR_STRIP := 300.0
# The sim turn the arrival-schedule states render on, so the strip tooltips + the outlook "empty ~turn
# N" marker read as absolute turns rather than the pre-first-overlay relative form.
const ARRIVAL_PREVIEW_TURN := 40
# The paged-board states work a row of this many forage patches from this origin — far past one
# page in either shell, which is the whole point of the pager.
const MANY_SOURCE_COUNT := 34
const MANY_SOURCE_ORIGIN_X := 40
const MANY_SOURCE_ORIGIN_Y := 20
# Dependants per working-age adult in the big-band fixture, held near the base band's own shape
# (9 children + 5 elders to 16 workers) so its PEOPLE bar reads like a real band, not a scaled prop.
const MANY_SOURCE_CHILD_RATIO := 0.56
const MANY_SOURCE_ELDER_RATIO := 0.31
# Sub-pixel slack when comparing a zone's content rect against its host rect.
const ZONE_BOUNDS_TOLERANCE := 1.0
## One Wild Boar's worth of yield in provisions (`HerdTelemetryState.foodPerAnimal`) — the quarry
## fixture's delivered food is animals × this, so the sheet's forecast quotes a real food total.
const QUARRY_FOOD_PER_ANIMAL := 4.0
## The quarry fixtures straddle the band's hunt reach: the Wild Boar is a party's job, the Roe Deer
## one tile out is a local hunt the picker must refuse.
const QUARRY_BAND_HUNT_REACH := 2
const QUARRY_FAR_HERD_ID := "game_boar_04"
const QUARRY_FAR_X := 75
const QUARRY_FAR_Y := 18
const QUARRY_NEAR_HERD_ID := "game_deer_79"
const QUARRY_NEAR_X := 72
const QUARRY_NEAR_Y := 18
# The two disclosure keys of `_band_fixture()` (entity 904) — the `[url]` meta payload its Food /
# Morale rows carry, i.e. what `Hud._breakdown_key` builds for that band.
const BAND_FIXTURE_DISCLOSURE_FOOD := "food:904"
const BAND_FIXTURE_DISCLOSURE_MORALE := "morale:904"

## The work-inspector policy-picker states work TWO Hunt rows on one band, told apart by the rung they
## stand on: `corral` is an INVESTMENT rung (the picker offers only the four extractive ones, so it can
## highlight nothing) and `sustain` is the ordinary control.
const INVESTMENT_ROW_POLICY := "corral"
const INVESTMENT_ROW_HERD_ID := "game_aurochs_11"
const EXTRACTIVE_ROW_POLICY := "sustain"
const EXTRACTIVE_ROW_HERD_ID := "game_deer_07"
## The rung both assertions PRESS. Extractive, so on the investment row it is a genuine "discard the
## pen and take at Surplus instead", and on the control row an ordinary change of take.
const PICKED_RUNG_POLICY := "surplus"

# The two hunt-party fixtures the parties-inspector states open (entities from the fixtures below).
const HUNT_DELIVERING_ENTITY := 952
const HUNT_LEAN_ENTITY := 953
# A hunt party whose target herd has DROPPED OUT of `_world_herds` (lost/replaced), projecting 0.
const HUNT_LOST_ENTITY := 954
# A 21:9 monitor — comfortably past the wide shell's content cap, which is the whole point of the state.
const ULTRAWIDE_WIDTH := 3440
const ULTRAWIDE_HEIGHT := 900
# The two shell-threshold probe windows. The panel is bottom-docked in both, so the window width IS
# `_panel_extent().x`, the value `_shell_is_wide` tests — one pixel below the derived threshold (must
# pick the NARROW tabbed shell) and exactly at it (the narrowest legitimate WIDE shell). Derived from
# the panel's own const so they can never drift from the threshold they bracket.
const SHELL_THRESHOLD_WIDTH := int(BandCityPanel.WIDE_SHELL_MIN_WIDTH)
const SHELL_THRESHOLD_UNDERSHOOT := 1
const SHELL_THRESHOLD_HEIGHT := 900
# The window every state but the ultrawide one renders at.
const PREVIEW_SIZE := Vector2i(1500, 900)
# How many frames to keep re-asserting the window before giving up and warning.
const WINDOW_PIN_MAX_FRAMES := 30

## The size every state re-asserts before it renders — see `_pin_window`.
var _pinned_size := PREVIEW_SIZE
## The canvas size every state re-asserts, `ZERO` = leave the project's stretch alone — see `_pin_canvas`.
var _pinned_canvas := Vector2i.ZERO
var _hud: HudLayer
var _panel: BandCityPanel
## The last state `_save`d, so an assertion failure names the frame it fired on.
var _current_state := "<pre-render>"

func _ready() -> void:
	# PIN THE WINDOW. `project.godot` opens MAXIMIZED and macOS applies — and re-applies — that
	# asynchronously, so a bare `size =` is a race the harness does not stay winning: every frame then
	# renders at monitor size instead of PREVIEW_SIZE, silently changing what each state proves (a
	# 3440-wide "bottom dock" frame is testing the ultrawide cap, not the ordinary wide shell). Same
	# hazard `blend_probe._pin_canvas` exists for.
	await _pin_window(PREVIEW_SIZE)
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

	BandCityPanel.config_path_override = PREVIEW_DOCK_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(PREVIEW_DOCK_PREFS_PATH))

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
		_hud._band_labor._player_bands.size(), _hud._band_labor._player_expeditions.size()])

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
	print("band_panel_preview: bug2 — _panel_band empty after foreign select? ", _hud._band_labor._panel_band.is_empty())
	_hud._emit_assign_labor(_hud._band_labor._panel_band, "forage", 6, 71, 18, "", "")
	await _settle()
	await _save("band_panel_stepper_foreign")

	# Food + Morale summary-line disclosures, in BOTH dock layouts (tall LEFT / wide TOP). The
	# breakdown opens in a POPOVER, never inline — so these frames prove two things at once: the
	# popover renders its rows, and the band zone behind it is UNCHANGED (WORKFORCE + both role cards
	# still whole). Driven through the REAL path: `meta_clicked` on the live vitals label, i.e. the
	# exact signal a click emits and the exact handler it runs — a debug back door could pass here
	# while the live path was broken.
	# (a) Food breakdown (Gathered/Hunted/Eaten).
	_hud.update_band_alerts([_band_fixture()])
	_panel.set_active_tab(&"band")   # the narrow shell shows ONE zone; these frames judge the band one
	for state in [{"edge": SIDE_LEFT, "name": "band_panel_food_expanded_left"},
			{"edge": SIDE_TOP, "name": "band_panel_food_expanded_top"}]:
		_panel.set_dock(state["edge"])
		await _settle()
		_click_disclosure(BAND_FIXTURE_DISCLOSURE_FOOD)
		await _settle()
		await _save(state["name"])
		_assert_zones_within_bounds()
		_assert_work_zone_readable()
		_assert_zone_content_fits()
		_click_disclosure(BAND_FIXTURE_DISCLOSURE_FOOD)   # toggle shut before the next dock

	# (b) Morale breakdown (same disclosure mechanism, same popover, indented contributions).
	for state in [{"edge": SIDE_LEFT, "name": "band_panel_morale_expanded_left"},
			{"edge": SIDE_TOP, "name": "band_panel_morale_expanded_top"}]:
		_panel.set_dock(state["edge"])
		await _settle()
		_click_disclosure(BAND_FIXTURE_DISCLOSURE_MORALE)
		await _settle()
		await _save(state["name"])
		_assert_zones_within_bounds()
		_assert_work_zone_readable()
		_assert_zone_content_fits()
		_click_disclosure(BAND_FIXTURE_DISCLOSURE_MORALE)

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
	_hud._band_labor._pending_labor.clear()
	_hud.update_band_alerts([_band_fixture()] + _phase_expedition_fixtures())
	_hud._emit_assign_labor(_hud._band_labor._panel_band, "forage", 4, 72, 19, "", "surplus")
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
	_hud._band_labor._pending_labor.clear()
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
	_hud._band_labor._pending_labor.clear()

	# (a) A LUMPY hunt (gaps) beside a CONTINUOUS forage (every slot positive). The hunt row must gain a
	# tick strip with visible gaps; the forage row must gain NONE (the gap rule); the merged projection
	# must sawtooth upward (hauls > flat drain).
	# `_arrivals_band_fixture` is the fixture that actually RENDERS the FOOD OUTLOOK chart (it carries
	# `arrival_schedule`s; the plain `_band_fixture` does not, so its band zone has no chart at all).
	# The TALL (L) shell shows the full chart; the height-capped T/B shells (top + bottom) land the band
	# zone in the SHORT tier, where the chart is DROPPED and the role cards go hint-less. The
	# content-fits assertion on the T/B frames is what proves that drop keeps the zone inside its box:
	# ungated (the chart rendered at full height in the SHORT tier) it overruns the ~300px T/B cap by
	# 115px, which is exactly the overflow the tier gating exists to prevent — and which the work-heavy
	# `band_panel_work_wide` / `band_panel_parties_inspector_wide` states cannot catch (their big band's
	# vitals carry no chart either).
	_hud.update_band_alerts([_arrivals_band_fixture()])
	_panel.set_active_tab(&"band")   # the narrow (L) shell shows ONE zone; these frames judge the band one
	for state in [{"edge": SIDE_LEFT, "name": "band_panel_arrivals_left"},
			{"edge": SIDE_TOP, "name": "band_panel_arrivals_top"},
			{"edge": SIDE_BOTTOM, "name": "band_panel_arrivals_bottom"}]:
		_panel.set_dock(state["edge"])
		await _settle()
		await _settle()   # let the deferred fit_content re-pack settle before capture
		await _save(state["name"])
		_assert_zones_within_bounds()
		_assert_work_zone_readable()
		_assert_zone_content_fits()

	# (b) A band whose larder EMPTIES inside the horizon: sparse lumpy hauls under a heavy drain, so the
	# walk hits 0 and the chart draws the dashed DANGER "empty ~turn N" marker.
	_hud.update_band_alerts([_arrivals_starving_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	await _save("band_panel_arrivals_empty")

	# ---- Zone content (docs/band_panel_ux_proposal.html) ----------------------
	# PEOPLE + WORKFORCE bars and the two role CARDS, in the TALL (L dock) shell where the band zone
	# gets its full height: both bars, their keys, the dependency ratio, and the hinted cards.
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"band")
	await _settle()
	await _save("band_panel_people")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# band_panel_people_map_path — THE SAME PEOPLE BLOCK, reached the OTHER way: by clicking the band
	# ON THE MAP. `band_panel_people` above drives the SNAPSHOT path (`update_band_alerts` re-resolves
	# the band from the raw `populations` floats), which is exactly the path that SELF-HEALS the marker
	# truncation bug — so it could never have caught it. The map path feeds the panel MapView's unit
	# MARKER instead (`_rebuild_unit_markers` → `refresh_selection_payload` → `show_unit_selection` →
	# `_render_band_into_panel`), and a marker that narrowed the fractional age brackets with `int()`
	# zeroes every remainder, leaving `_apportion_people` nothing to redistribute: 9 + 16 + 4 = 29 in
	# the PEOPLE header against a band of 30. Driven through the REAL MapView, never a hand-built dict.
	var map_path_view: Node2D = MAP_VIEW_SCRIPT.new()
	map_path_view.visible = false   # data only — a visible map would render behind every later frame
	add_child(map_path_view)
	map_path_view.display_snapshot(_map_path_snapshot())
	map_path_view.unit_selected.connect(_hud.show_unit_selection)
	map_path_view.handle_hex_click(MAP_PATH_TILE.x, MAP_PATH_TILE.y, MOUSE_BUTTON_LEFT)
	# The HUD already holds its own copy of the payload, so the map goes away BEFORE the capture:
	# MapView's minimap is its own CanvasLayer and is NOT hidden by `visible = false`, so a surviving
	# instance paints a stray thumbnail into this frame and every later one (map_preview's gotcha).
	map_path_view.unit_selected.disconnect(_hud.show_unit_selection)
	map_path_view.queue_free()
	await get_tree().process_frame
	await _settle()
	_assert_people_sum_matches_size(_hud._selection._selected_unit, "band_panel_people_map_path")
	await _save("band_panel_people_map_path")
	# Restore the snapshot-path band so the later states start from the same subject they always did.
	_hud.update_band_alerts([_band_fixture()])

	# The paged WORK BOARD at 34 sources — far past one page in the narrow (L dock) shell, so the
	# pager must appear and NOTHING may scroll.
	_hud.update_food_modules(_many_forage_modules())
	_hud.update_band_alerts([_many_sources_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_work_page")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# The same 34 sources in the WIDE (bottom dock) shell: multi-column, column-major, hairlines.
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("band_panel_work_wide")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# A row OPEN in the inspector strip: the board loses rows to it, and still no scrollbar.
	_panel.set_dock(SIDE_LEFT)
	_hud._toggle_work_inspector(_hud._work_source_models(_hud._band_labor._panel_band, 0)[0]["key"])
	await _settle()
	await _save("band_panel_inspector")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._toggle_work_inspector(_hud._work_open_key)

	# The Work menu's destructive action asks first, and the confirm names what is SPARED.
	_hud._on_work_unassign_all_pressed(_hud._band_labor._panel_band, 34)
	await _settle()
	await _save("band_panel_clear_confirm")
	_dismiss_dialogs()

	# THE WORK INSPECTOR'S POLICY PICKER — the one control on the board with no frame coverage at all
	# until now (`_work_policy_open` was never set true in either harness). Two rows, two behaviours:
	# a source standing on an INVESTMENT rung (Corral) highlights none of the four extractive rungs,
	# so it must SAY the standing rung and CONFIRM before a pick discards it; a source standing on an
	# extractive rung (Sustain) must behave exactly as it always has — one lit rung, immediate emit.
	_hud.update_food_modules([{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"}])
	_hud.update_herds(_investment_policy_herd_fixtures())
	_hud.update_band_alerts([_investment_policy_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	_open_work_policy_picker(INVESTMENT_ROW_POLICY)
	await _settle()
	await _save("band_panel_work_policy_investment")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_standing_investment_line(INVESTMENT_ROW_POLICY)
	_assert_policy_pick_confirms(INVESTMENT_ROW_POLICY, true)

	# The CONTROL: the very same picker on the extractive row beside it. Both assertions here must
	# pass BEFORE and AFTER the investment fix — they are what proves it cannot fire on the normal path.
	_open_work_policy_picker(EXTRACTIVE_ROW_POLICY)
	await _settle()
	await _save("band_panel_work_policy_extractive")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_lit_rung(EXTRACTIVE_ROW_POLICY)
	_assert_policy_pick_confirms(EXTRACTIVE_ROW_POLICY, false)
	_hud._work_policy_open = false
	_hud._toggle_work_inspector(_hud._work_open_key)

	# The parties COMPOSE sheet, QUARRY-FIRST. With a quarry picked the whole hunt form resolves: the
	# policy rungs carry their ascending per-policy metric, the party stepper caps at the raid's
	# max-useful plateau, the trip forecast reads, and the Send button takes its verdict.
	_hud.update_food_modules([{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"}])
	_hud.update_herds(_quarry_herd_fixtures())
	_hud.update_band_alerts([_scout_expedition_fixture(), _band_fixture(), _hunt_expedition_fixture()])
	_assert_quarry_eligibility()
	_panel.set_active_tab(&"parties")
	_hud._party_compose_open = true
	_hud._party_compose_mission = "hunt"
	_hud._compose.set_party_quarry(QUARRY_FAR_HERD_ID)
	# Picking a quarry fills the party to its max-useful cap (the one-shot `_try_pick_quarry` sets);
	# seed it here too so the frame shows the shipped default (the party at the cap, not a stray 1).
	_hud._compose.arm_party_autofill()
	_hud._rerender_panel_allocation()
	await _settle()
	await _save("band_panel_compose_hunt")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# The same sheet with NO quarry yet: the "Choose…" row, the hint, a disabled Send — and nothing
	# below it, since policy/party/forecast are all unanswerable without a herd.
	_hud._compose.clear_party_quarry()
	_hud._rerender_panel_allocation()
	await _settle()
	await _save("band_panel_compose_hunt_no_quarry")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# Same sheet under Scout: scouting title, NO quarry row, NO policy picker, "Send scouting party…".
	_hud._party_compose_mission = "scout"
	_hud._rerender_panel_allocation()
	await _settle()
	await _save("band_panel_compose_scout")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._party_compose_open = false
	_hud._party_compose_mission = ""
	_hud._compose.clear_party_quarry()

	# Zero idle workers: BOTH mission buttons (Scout / Hunt) stay VISIBLE and DISABLED, with the
	# shared reason line beneath them.
	_hud.update_band_alerts([_no_idle_band_fixture()])
	await _settle()
	await _save("band_panel_no_idle")

	_assert_no_scroll_containers()
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# PARTIES INSPECTOR STRIP — a row click opens the full Mission/Target/Policy/Phase/Carried/
	# Next-delivery detail, mirroring the work board's row → inspector.
	_hud.show_tile_selection({})
	_hud._band_labor._pending_labor.clear()
	_hud.update_herds(_herd_fixtures())

	# (a) WIDE shell (bottom dock): the strip renders in the height-capped T/B shell too → the
	# DELIVERING party's "Next delivery: ~14 food in 6 turns". Reuses the work-heavy band fixture (the
	# `band_panel_work_wide` config) so the board is populated; its band zone fits the ~300px T/B cap
	# for the same reason `_band_fixture`'s does — the SHORT tier drops the FOOD OUTLOOK chart (that
	# gating is what `band_panel_arrivals_top`/`_bottom` guard with a chart-bearing fixture). The strip
	# + a party row + footer fit because the strip replaces the bottom spacer (`_build_parties_zone_content`).
	_hud.update_food_modules(_many_forage_modules())
	_hud.update_band_alerts([_many_sources_band_fixture(), _hunt_expedition_fixture()])
	_panel.set_dock(SIDE_BOTTOM)
	_hud._toggle_parties_inspector(str(HUNT_DELIVERING_ENTITY))
	await _settle()
	await _save("band_panel_parties_inspector_wide")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._toggle_parties_inspector(str(HUNT_DELIVERING_ENTITY))   # close before the next state

	# (b) NARROW shell (left dock, Parties tab): the tall L/R parties zone holds both parties + the strip
	# with room to spare. Inspect the NO-SURPLUS party → the invisible-line bug the strip fixes:
	# "Next delivery: none — the herd has no surplus to raid" must be VISIBLE, not hidden.
	_hud.update_band_alerts([_band_fixture(), _hunt_expedition_fixture(), _lean_hunt_expedition_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"parties")
	_hud._toggle_parties_inspector(str(HUNT_LEAN_ENTITY))
	await _settle()
	await _save("band_panel_parties_inspector_narrow")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._toggle_parties_inspector(str(HUNT_LEAN_ENTITY))

	# (b2) NEXT-DELIVERY DISAMBIGUATION on a projected-0 forecast. A hunt party is bound to ONE herd
	# (its `expedition_target_herd`) that MIGRATES and is often NOT the herd on the tile the player is
	# looking at, so a projected 0 means one of two things and the party's target tells them apart:
	# still in `_world_herds` → at/below its policy floor (no surplus); absent → lost/replaced (returning
	# home). The Target row also carries the target's live position so the player can SEE which herd the
	# party is bound to. Render all three parties + assert every line. `_world_herds` = _herd_fixtures():
	# game_deer_07 (@68,15) + game_deer_79 (@64,11); the LOST party targets an absent id.
	_hud.update_herds(_herd_fixtures())
	_hud.update_band_alerts([
		_band_fixture(), _hunt_expedition_fixture(), _lean_hunt_expedition_fixture(),
		_lost_hunt_expedition_fixture(),
	])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"parties")
	_hud._toggle_parties_inspector(str(HUNT_LOST_ENTITY))
	await _settle()
	await _save("band_panel_next_delivery_disambiguation")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_next_delivery_disambiguation()
	_hud._toggle_parties_inspector(str(HUNT_LOST_ENTITY))

	# (c) DETAIL-PANEL via the MARKER path — the FIX-4 regression. The Occupants-card drawer reads
	# `_expedition_summary_lines(_selected_unit)`, and `_selected_unit` is the MapView unit MARKER, not
	# a raw `_player_expeditions` dict. Drive the REAL marker path (display_snapshot →
	# _rebuild_unit_markers → handle_hex_click → show_unit_selection → _selected_unit) with a hunt party
	# projecting 14.5 food in 6t, and ASSERT the Next-delivery line reaches the panel (rounds to 15).
	_assert_detail_panel_delivery()

	# (d) The row ✕ recall must CONFIRM first (like "Recall all"), not emit immediately.
	_assert_row_recall_confirms()

	# ULTRAWIDE: past the width the three zones can USE, the wide shell CENTRES at its content cap
	# instead of stretching, leaving equal margins either side. Without it a single work row is strung
	# across the whole monitor and the band zone sits a screen away from the parties zone. The frame to
	# read is the equality of the two black margins — and that the board itself is unchanged.
	await _pin_window(Vector2i(ULTRAWIDE_WIDTH, ULTRAWIDE_HEIGHT))
	_panel.set_dock(SIDE_BOTTOM)
	_hud.update_band_alerts([_many_sources_band_fixture()])
	await _settle()
	await _save("band_panel_wide_ultrawide")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	print("band_panel_preview: ultrawide — work zone %.0fpx of a %dpx panel (capped + centred)" % [
		_panel.work_zone_size().x, ULTRAWIDE_WIDTH])

	# THE SHELL THRESHOLD, bracketed. `WIDE_SHELL_MIN_WIDTH` is DERIVED from what the wide shell needs
	# (both flanks + one readable work column + the separators + the card chrome), and nothing else in
	# this harness renders anywhere near it — 1500 and 3440 are both comfortably past it, so a
	# too-low threshold was invisible here. These two frames are the before/after of the flip.
	# One pixel BELOW: the wide shell could not give the board a readable column, so the panel must
	# choose the NARROW tabbed shell — which hands the board the panel's WHOLE interior.
	await _pin_canvas(Vector2i(SHELL_THRESHOLD_WIDTH - SHELL_THRESHOLD_UNDERSHOOT, SHELL_THRESHOLD_HEIGHT))
	_panel.set_dock(SIDE_BOTTOM)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_shell_below_threshold")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_shell_is_wide(false, "band_panel_shell_below_threshold")

	# Exactly AT it: the narrowest legitimate wide shell — three columns, the work zone at exactly
	# `ZONE_WORK_MIN_WIDTH`, its rows still legible with un-clipped labels.
	await _pin_canvas(Vector2i(SHELL_THRESHOLD_WIDTH, SHELL_THRESHOLD_HEIGHT))
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("band_panel_shell_at_threshold")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_shell_is_wide(true, "band_panel_shell_at_threshold")

	get_tree().quit()

## GUARD (FIX 4): the Next-delivery line must reach the DETAIL PANEL through the MARKER, not only the
## raw `_player_expeditions` dict. Push a hunt party through a REAL MapView (display_snapshot →
## _rebuild_unit_markers), click its hex to set `_hud._selection._selected_unit`, and assert the marker-sourced
## drawer line reads "Next delivery: ~15 food in 6 turns" (14.5 → 15). Verified to FAIL before the
## marker copy carried the three fields.
func _assert_detail_panel_delivery() -> void:
	var view: Node2D = MAP_VIEW_SCRIPT.new()
	view.visible = false   # data only — a visible map paints behind later frames (minimap gotcha)
	add_child(view)
	var tile := Vector2i(64, 11)
	var terrain: Array = []
	terrain.resize(MAP_PATH_GRID_W * MAP_PATH_GRID_H)
	terrain.fill(MAP_PATH_TERRAIN_ID)
	var party := _hunt_expedition_fixture()
	party["current_x"] = tile.x
	party["current_y"] = tile.y
	party["expedition_projected_delivery"] = 14.5
	party["expedition_eta_turns"] = 6
	view.display_snapshot({
		"grid": {"width": MAP_PATH_GRID_W, "height": MAP_PATH_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [party],
	})
	view.unit_selected.connect(_hud.show_unit_selection)
	view.handle_hex_click(tile.x, tile.y, MOUSE_BUTTON_LEFT)
	view.unit_selected.disconnect(_hud.show_unit_selection)
	var lines: Array = _hud._expedition_summary_lines(_hud._selection._selected_unit)
	var want := "Next delivery: ~15 food in 6 turns"
	if lines.has(want):
		print("band_panel_preview: assert OK — detail panel (marker path) renders '%s'" % want)
	else:
		push_error("band_panel_preview: detail panel MISSING '%s' — marker path dropped the field. Got: %s" % [
			want, str(lines)])
	view.queue_free()

## GUARD: a projected-0 next-delivery forecast must disambiguate on the party's TARGET herd, and the
## Target row must carry the target's live position. Requires `_world_herds` already set to
## `_herd_fixtures()`. Drives the shared `_expedition_next_delivery_line` / `_expedition_summary_lines`
## helpers directly (the same ones the strip, the drawer and the row tooltip use) and prints every
## rendered line. Verified to FAIL before the target-based branch (a lost target reading "no surplus").
func _assert_next_delivery_disambiguation() -> void:
	# (1) target FOUND in telemetry, projects 0 → "no surplus", Target row shows the herd's position.
	var lean := _lean_hunt_expedition_fixture()
	var lean_delivery := _hud._expedition_next_delivery_line(lean)
	var lean_target := _summary_target_line(lean)
	_check_line("no-surplus delivery", lean_delivery, _hud.EXPEDITION_NEXT_DELIVERY_NO_SURPLUS)
	_check_line("no-surplus target", lean_target, "Target: Red Deer (68, 15)")
	# (2) target ABSENT from telemetry, projects 0 → "target herd lost".
	var lost := _lost_hunt_expedition_fixture()
	var lost_delivery := _hud._expedition_next_delivery_line(lost)
	_check_line("lost delivery", lost_delivery, _hud.EXPEDITION_NEXT_DELIVERY_TARGET_LOST)
	# (3) projecting party (delivery > 0) → the ETA line, Target row shows the herd's position.
	var live := _hunt_expedition_fixture()
	var live_delivery := _hud._expedition_next_delivery_line(live)
	var live_target := _summary_target_line(live)
	_check_line("projecting delivery", live_delivery, "Next delivery: ~14 food in 6 turns")
	_check_line("projecting target", live_target, "Target: Roe Deer (64, 11)")

## The `Target: …` line `_expedition_summary_lines` emits for a party ("" if none).
func _summary_target_line(party: Dictionary) -> String:
	for line in _hud._expedition_summary_lines(party):
		if String(line).begins_with("Target:"):
			return String(line)
	return ""

## Assert a rendered line equals what we want, printing the exact string either way.
func _check_line(label: String, got: String, want: String) -> void:
	if got == want:
		print("band_panel_preview: assert OK — %s renders '%s'" % [label, got])
	else:
		push_error("band_panel_preview: %s expected '%s' but got '%s'" % [label, want, got])

## GUARD: the row ✕ (single-party recall) must route through the CONFIRM dialog, not fire the recall
## emit immediately — mirroring "Recall all". Build a real party row, press its recall Button, and
## assert a ConfirmationDialog appeared on the HUD while `recall_expedition_requested` did NOT fire.
## Verified to FAIL with the ✕ wired straight to `_on_recall_expedition_pressed`.
func _assert_row_recall_confirms() -> void:
	var fired := [false]
	var sink := func(_payload: Dictionary) -> void: fired[0] = true
	_hud.recall_expedition_requested.connect(sink)
	var row: HBoxContainer = _hud._build_party_row(_hunt_expedition_fixture())
	var recall: Button = row.get_child(row.get_child_count() - 1)   # ✕ is the row's last child
	recall.pressed.emit()
	var dialog_shown := false
	for child in _hud.get_children():
		if child is ConfirmationDialog:
			dialog_shown = true
	_hud.recall_expedition_requested.disconnect(sink)
	if dialog_shown and not fired[0]:
		print("band_panel_preview: assert OK — row ✕ recall confirms first (no immediate emit)")
	else:
		push_error("band_panel_preview: row ✕ recall did NOT confirm (dialog=%s, emitted=%s)" % [
			dialog_shown, fired[0]])
	_dismiss_dialogs()
	row.queue_free()

## GUARD: whenever the WIDE shell is active, the work zone must be at least one readable board column
## (`ZONE_WORK_MIN_WIDTH`) — otherwise Hud's `_work_board_capacity` clamps to a single column too
## narrow for its own row labels, and the NARROW shell would have given the board strictly MORE room.
## That is the invariant a hand-picked `WIDE_SHELL_MIN_WIDTH` violated across a whole band of widths,
## and the recursive zone-bounds assertion cannot catch it: a CLIPPED label still sits inside its rect.
func _assert_work_zone_readable() -> void:
	if not _panel._shell_is_wide():
		return
	var work_width := _panel.work_zone_size().x
	if work_width + ZONE_BOUNDS_TOLERANCE < BandCityPanel.ZONE_WORK_MIN_WIDTH:
		push_error("band_panel_preview: wide shell with a %.0fpx work zone — under ZONE_WORK_MIN_WIDTH (%.0f)" % [
			work_width, BandCityPanel.ZONE_WORK_MIN_WIDTH])
	else:
		print("band_panel_preview: assert OK — wide shell work zone %.0fpx >= %.0f" % [
			work_width, BandCityPanel.ZONE_WORK_MIN_WIDTH])

## GUARD: the two threshold-probe states exist to pin WHICH shell is chosen, so state it outright —
## a frame that silently rendered the other shell would still pass every other assertion here.
func _assert_shell_is_wide(expected: bool, state_name: String) -> void:
	var actual := _panel._shell_is_wide()
	if actual != expected:
		push_error("band_panel_preview: %s expected shell wide=%s but got %s" % [
			state_name, expected, actual])
	else:
		print("band_panel_preview: assert OK — %s shell wide=%s" % [state_name, actual])

## GUARD: the PEOPLE block's three brackets must account for EVERY person in the band. They arrive
## fractional (Scalar), so `Hud._apportion_people` distributes the remainders by largest remainder —
## which only works if the remainders survive the trip. A marker that narrowed them with `int()`
## truncates every one to zero, and the header then undercounts against the band's own size.
func _assert_people_sum_matches_size(band: Dictionary, state_name: String) -> void:
	var raw: Array[float] = [
		float(band.get("age_children", 0.0)),
		float(band.get("age_working", 0.0)),
		float(band.get("age_elders", 0.0)),
	]
	var whole := _hud._apportion_people(raw)
	var total := 0
	for part in whole:
		total += part
	var size := int(band.get("size", 0))
	if total != size:
		push_error("band_panel_preview: %s PEOPLE brackets sum to %d but the band holds %d (raw %s — narrowed?)" % [
			state_name, total, size, str(raw)])
	else:
		print("band_panel_preview: assert OK — %s PEOPLE brackets sum to the band's %d people" % [state_name, size])

## GUARD: the zone model is NO-SCROLL by construction — a ScrollContainer anywhere in the panel would
## silently reintroduce the content-dependent sizing the rework removed.
func _assert_no_scroll_containers() -> void:
	var found := _find_scroll_container(_panel)
	if found != null:
		push_error("band_panel_preview: ScrollContainer in the panel at %s — the zones must not scroll" % found.get_path())
	else:
		print("band_panel_preview: assert OK — no ScrollContainer in the panel")

func _find_scroll_container(node: Node) -> Node:
	if node is ScrollContainer:
		return node
	for child in node.get_children():
		var found := _find_scroll_container(child)
		if found != null:
			return found
	return null

## GUARD: a zone's content must FIT — not merely sit inside its host's rect. The zone hosts clip, so
## content the box cannot hold still reports a rect within bounds and passes `_assert_zones_within_bounds`
## while being silently sliced off the frame (the WORKFORCE key row cut mid-glyph, the role cards gone).
## Containment is not completeness: the invariant that matters is that the zone box is at least as tall
## as the content's own combined minimum size.
func _assert_zone_content_fits() -> void:
	var failures: Array[String] = []
	for host_variant in _find_zone_hosts(_panel):
		var host: Control = host_variant
		_collect_zone_content_shortfall(host, host, failures)
	if failures.is_empty():
		print("band_panel_preview: assert OK — every zone's content fits its zone box (%s)" % _current_state)
		return
	for failure in failures:
		push_error("band_panel_preview: %s — %s" % [_current_state, failure])

## Walk a zone host looking for content the BOX cannot hold. The zone content roots are plain
## `Control` wrappers (`Hud._wrap_zone`) that report NO minimum size, so the measurable thing is the
## column inside them — hence the recursion past every zero-minimum wrapper. A control that DOES
## report a minimum height is measured from where it sits (its top, relative to the zone) and then
## not descended into: its own minimum already accounts for its children.
func _collect_zone_content_shortfall(node: Node, host: Control, failures: Array[String]) -> void:
	for child in node.get_children():
		if not (child is Control):
			continue
		var content: Control = child
		if not content.visible:
			continue
		var needed := content.get_combined_minimum_size().y
		if needed <= 0.0:
			_collect_zone_content_shortfall(content, host, failures)
			continue
		var top := content.global_position.y - host.global_position.y
		var box := host.size.y
		if top + needed > box + ZONE_BOUNDS_TOLERANCE:
			failures.append("zone %s: %s (%s) needs %.0fpx from y=%.0f but the box is only %.0fpx (short by %.0f)" % [
				host.name, content.name, content.get_class(), needed, top, box, top + needed - box])

## GUARD: nothing a zone renders may fall outside the zone rect it was given. Checked RECURSIVELY —
## the top-level content is anchored full-rect and so always "fits", while the thing that actually
## overflows is a board row off the bottom of the column. The hosts clip, so an overflow is invisible
## in the frame; this is the only thing that catches it.
func _assert_zones_within_bounds() -> void:
	var failures: Array[String] = []
	for host_variant in _find_zone_hosts(_panel):
		var host: Control = host_variant
		_collect_zone_overflow(host, host.get_global_rect(), failures)
	if failures.is_empty():
		print("band_panel_preview: assert OK — every zone renders inside its zone rect")
		return
	for failure in failures:
		push_error("band_panel_preview: %s" % failure)

func _collect_zone_overflow(node: Node, bounds: Rect2, failures: Array[String]) -> void:
	for child in node.get_children():
		if not (child is Control):
			continue
		var content: Control = child
		if not content.visible:
			continue
		var rect := content.get_global_rect()
		# Zero-sized spacers/separators report a degenerate rect; only real content can overflow.
		if rect.size.x > 0.0 and rect.size.y > 0.0:
			var over_x: float = rect.end.x - bounds.end.x
			var over_y: float = rect.end.y - bounds.end.y
			if over_x > ZONE_BOUNDS_TOLERANCE or over_y > ZONE_BOUNDS_TOLERANCE:
				failures.append("%s (%s) overflows its zone by (%.1f, %.1f)" % [
					content.name, content.get_class(), maxf(over_x, 0.0), maxf(over_y, 0.0)])
				continue   # one report per subtree — its children overflow by construction
		_collect_zone_overflow(content, bounds, failures)

## The panel's fixed-size zone hosts (BandCityPanel names them `Zone_<key>` / `NarrowZoneHost`).
func _find_zone_hosts(node: Node) -> Array:
	var hosts: Array = []
	if String(node.name).begins_with("Zone_") or node.name == "NarrowZoneHost":
		hosts.append(node)
	for child in node.get_children():
		hosts.append_array(_find_zone_hosts(child))
	return hosts

## Two Hunt rows on one band, told apart by the rung they STAND on: a part-built pen (an INVESTMENT
## rung, which the work inspector's four-extractive-rung picker cannot highlight) and an ordinary
## Sustain take (the control). Same band, same zone, so the two frames differ in exactly the rung.
func _investment_policy_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 912
	band["id"] = "Band 9"
	band["labor_assignments"] = [
		{"kind": "hunt", "workers": 3, "workers_needed": 3, "policy": INVESTMENT_ROW_POLICY,
			"fauna_id": INVESTMENT_ROW_HERD_ID, "target_x": 70, "target_y": 17,
			"actual_yield": 0.75, "sustainable_yield": 0.75},
		{"kind": "hunt", "workers": 2, "workers_needed": 2, "policy": EXTRACTIVE_ROW_POLICY,
			"fauna_id": EXTRACTIVE_ROW_HERD_ID, "target_x": 69, "target_y": 19,
			"actual_yield": 0.20, "sustainable_yield": 0.20},
		{"kind": "scout", "workers": 1},
	]
	return band

## The two herds those rows work. The pen is mid-build (`corral_progress`), which is exactly the
## ~25-turn investment a pick in the work inspector would throw away.
func _investment_policy_herd_fixtures() -> Array:
	return [
		{
			"id": INVESTMENT_ROW_HERD_ID, "species": "Aurochs", "x": 70, "y": 17,
			"population": 210, "ecology_phase": "thriving", "huntable": true,
			"domestication": 1.0, "corral_progress": 0.4, "herders_needed": 3,
			"per_worker_yield": 0.25,
			"hunt_policy_ceilings": {
				"sustain": 0.40, "surplus": 1.10, "market": 1.60, "eradicate": 2.40,
				"tame": 0.20, INVESTMENT_ROW_POLICY: 0.75,
			},
		},
		{
			"id": EXTRACTIVE_ROW_HERD_ID, "species": "Red Deer", "x": 69, "y": 19,
			"population": 90, "ecology_phase": "thriving", "huntable": true,
			"per_worker_yield": 0.10,
			"hunt_policy_ceilings": {
				"sustain": 0.20, "surplus": 0.60, "market": 0.90, "eradicate": 1.40,
			},
		},
	]

## Open the work inspector on the row standing on `policy`, with its policy picker EXPANDED, and
## repage so the picker actually renders. `_work_policy_open` is otherwise never true in either
## harness, which is why this control had zero frame coverage.
func _open_work_policy_picker(policy: String) -> void:
	var band: Dictionary = _hud._band_labor._panel_band
	for model_variant in _hud._work_source_models(band, 0):
		var model: Dictionary = model_variant
		if String(model.get("policy", "")) != policy:
			continue
		_hud._work_open_key = String(model.get("key", ""))
		_hud._work_policy_open = true
		_hud._repage_work_zone()
		return
	push_error("band_panel_preview: no work row standing on '%s' — fixture drifted?" % policy)

## The open inspector strip: the work zone host's PanelContainer (the board and chips are boxes).
func _work_inspector_strip() -> PanelContainer:
	var host: VBoxContainer = _hud._work_zone_host
	if host == null or not is_instance_valid(host):
		return null
	for child in host.get_children():
		if child is PanelContainer:
			return child
	return null

## The inspector picker's rung buttons, keyed by policy. The work inspector passes NO `takes`, so a
## button's face is exactly `Hud._policy_face(policy)` — the same vocabulary the standing line uses.
func _picker_rung_buttons() -> Dictionary:
	var buttons := {}
	var strip := _work_inspector_strip()
	if strip == null:
		return buttons
	var grid := _find_first_grid(strip)
	if grid == null:
		return buttons
	for child in grid.get_children():
		if not (child is Button):
			continue
		for policy in HudLayer.LABOR_HUNT_POLICIES:
			if (child as Button).text == _hud._policy_face(String(policy)):
				buttons[String(policy)] = child
	return buttons

func _find_first_grid(node: Node) -> GridContainer:
	if node is GridContainer:
		return node
	for child in node.get_children():
		var found := _find_first_grid(child)
		if found != null:
			return found
	return null

## RED 1: a source standing on an INVESTMENT rung must SAY so. Without it the picker highlights none
## of its four rungs and reads as an unset control on a very-much-set assignment.
func _assert_standing_investment_line(policy: String) -> void:
	var want := HudLayer.WORK_INSPECT_STANDING_INVESTMENT_FORMAT % _hud._policy_face(policy)
	var strip := _work_inspector_strip()
	if strip != null and _find_label_with_text(strip, want) != null:
		print("band_panel_preview: assert OK — inspector states the standing rung ('%s')" % want)
	else:
		push_error("band_panel_preview: inspector never rendered the standing-investment line '%s'" % want)

func _find_label_with_text(node: Node, text: String) -> Label:
	if node is Label and (node as Label).text == text:
		return node
	for child in node.get_children():
		var found := _find_label_with_text(child, text)
		if found != null:
			return found
	return null

## RED 2 (the important one) / CONTROL (i): press a real rung button and watch what happens.
## `want_confirm` true  — the standing rung is an INVESTMENT: a ConfirmationDialog must appear and
##                        `assign_labor_requested` must NOT fire yet (the ~25-turn build is at stake).
## `want_confirm` false — the ordinary EXTRACTIVE path: the emit must land immediately, no dialog.
func _assert_policy_pick_confirms(standing: String, want_confirm: bool) -> void:
	var buttons := _picker_rung_buttons()
	if not buttons.has(PICKED_RUNG_POLICY):
		push_error("band_panel_preview: no '%s' rung in the work inspector's picker" % PICKED_RUNG_POLICY)
		return
	var fired := [false]
	var sink := func(_payload: Dictionary) -> void: fired[0] = true
	_hud.assign_labor_requested.connect(sink)
	(buttons[PICKED_RUNG_POLICY] as Button).pressed.emit()
	var dialog_shown := false
	for child in _hud.get_children():
		if child is ConfirmationDialog:
			dialog_shown = true
	_hud.assign_labor_requested.disconnect(sink)
	if dialog_shown == want_confirm and fired[0] == (not want_confirm):
		print("band_panel_preview: assert OK — a '%s' row's pick %s" % [
			standing, "confirms before discarding" if want_confirm else "emits immediately"])
	else:
		push_error("band_panel_preview: '%s' row pick expected (confirm=%s, emit=%s) but got (confirm=%s, emit=%s)" % [
			standing, want_confirm, not want_confirm, dialog_shown, fired[0]])
	_dismiss_dialogs()

## CONTROL (ii): on an EXTRACTIVE row exactly ONE rung wears the `primary` variant. There is no other
## marker of "this is the standing rung" than the button's own resting fill, so read it back.
func _assert_lit_rung(standing: String) -> void:
	var lit: Array[String] = []
	var buttons := _picker_rung_buttons()
	for policy in buttons:
		var box := (buttons[policy] as Button).get_theme_stylebox("normal")
		if box is StyleBoxFlat and (box as StyleBoxFlat).bg_color.is_equal_approx(HudStyle.BUTTON_PRIMARY_BG):
			lit.append(String(policy))
	if lit.size() == 1 and lit[0] == standing:
		print("band_panel_preview: assert OK — exactly one rung lit, and it is '%s'" % standing)
	else:
		push_error("band_panel_preview: expected only '%s' lit in the picker but got %s" % [standing, str(lit)])

## Close any modal the preview opened, so the next state renders unobstructed.
func _dismiss_dialogs() -> void:
	for child in _hud.get_children():
		if child is AcceptDialog:
			(child as AcceptDialog).hide()
			child.queue_free()

## 34 gather modules on a row of tiles, so every Forage row resolves a real map glyph.
func _many_forage_modules() -> Array:
	var modules: Array = []
	for i in range(MANY_SOURCE_COUNT):
		modules.append({"x": MANY_SOURCE_ORIGIN_X + i, "y": MANY_SOURCE_ORIGIN_Y,
			"module": "savanna_grassland", "kind": "gather"})
	return modules

## A band working MANY_SOURCE_COUNT forage patches — the case the paged board exists for (34 rows
## would be ~950px of unbroken list in the old stack).
func _many_sources_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["working_age"] = MANY_SOURCE_COUNT * 2
	band["idle_workers"] = MANY_SOURCE_COUNT
	# Keep the age split in step with the enlarged workforce — `age_working` IS `working_age`, and the
	# three sum to `size` (see `_band_fixture`). Derived, not retyped, so raising MANY_SOURCE_COUNT
	# cannot silently desync the PEOPLE bar from the WORKFORCE bar beneath it.
	var workers: int = int(band["working_age"])
	band["age_working"] = workers
	band["age_children"] = int(round(workers * MANY_SOURCE_CHILD_RATIO))
	band["age_elders"] = int(round(workers * MANY_SOURCE_ELDER_RATIO))
	band["size"] = workers + int(band["age_children"]) + int(band["age_elders"])
	var assignments: Array = []
	for i in range(MANY_SOURCE_COUNT):
		assignments.append({
			"kind": "forage", "workers": 1,
			# Every third patch is overstaffed, so the ⚠ attention chip + the WARN stripe have content.
			"workers_needed": 1 if i % 3 != 0 else 0,
			"policy": "sustain",
			"target_x": MANY_SOURCE_ORIGIN_X + i, "target_y": MANY_SOURCE_ORIGIN_Y,
			"actual_yield": 0.10 + 0.01 * float(i), "sustainable_yield": 0.10 + 0.01 * float(i),
		})
	band["labor_assignments"] = assignments
	return band

## Every worker committed: the parties footer must still SHOW its button, disabled, with the reason.
func _no_idle_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["idle_workers"] = 0
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 16, "workers_needed": 16, "policy": "sustain",
			"target_x": 71, "target_y": 18, "actual_yield": 0.48, "sustainable_yield": 0.48},
	]
	return band

## Pin the CANVAS (`content_scale_size`) as well as the window, and keep the two equal so the stretch
## factor is exactly 1 and the panel's canvas-space width IS `size.x`.
##
## Needed because `project.godot` stretches `canvas_items` with an `expand` aspect: the canvas is
## never SMALLER than the project's base resolution on either axis, so `get_visible_rect().size.x`
## floors at 1920 however narrow the window is — a plain `_pin_window(1055, 900)` still renders a
## 1920-wide panel and silently proves nothing about a sub-1920 threshold.
func _pin_canvas(size: Vector2i) -> void:
	_pinned_canvas = size
	await _pin_window(size)

## Force the window WINDOWED at `size` and wait for the WM to actually honour it, so a maximize
## cannot land between two states and render them at different resolutions.
func _pin_window(size: Vector2i) -> void:
	_pinned_size = size
	var window := get_window()
	window.mode = Window.MODE_WINDOWED
	window.size = size
	if _pinned_canvas != Vector2i.ZERO:
		window.content_scale_size = _pinned_canvas
	for _i in range(WINDOW_PIN_MAX_FRAMES):
		if window.size == size and window.mode == Window.MODE_WINDOWED:
			break
		window.mode = Window.MODE_WINDOWED
		window.size = size
		await get_tree().process_frame
	if window.size != size:
		push_warning("band_panel_preview: window pinned to %s but reports %s" % [size, window.size])

func _settle() -> void:
	# Re-assert the window EVERY state: the WM's maximize lands asynchronously and can arrive between
	# two states, rendering them at different resolutions (blend_probe hit the same thing).
	await _pin_window(_pinned_size)
	await get_tree().process_frame
	RenderingServer.force_draw()
	await get_tree().process_frame

func _save(name: String) -> void:
	_current_state = name
	var image := get_viewport().get_texture().get_image()
	if image == null:
		push_warning("band_panel_preview: null image (dummy renderer?) — skipping %s.png; run without --headless" % name)
		return
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("band_panel_preview: failed to save %s (err %d)" % [name, err])
	else:
		print("band_panel_preview: saved ", name, ".png")

## Drive a Food/Morale disclosure the way a CLICK does: emit `meta_clicked` on the live vitals
## RichTextLabel with the very `[url]` meta its own text carries, so the bound handler + anchor run
## exactly as they do in the game. A debug back door (poking Hud state directly) would pass even with
## the click path broken, which is the whole reason this goes through the signal.
func _click_disclosure(key: String) -> void:
	var meta := HudLayer.BREAKDOWN_TOGGLE_META_PREFIX + key
	var label := _find_meta_label(_panel, meta)
	if label == null:
		push_warning("band_panel_preview: no vitals label offering '%s' — disclosure not rendered?" % meta)
		return
	label.meta_clicked.emit(meta)

func _find_meta_label(node: Node, meta: String) -> RichTextLabel:
	if node is RichTextLabel and (node as RichTextLabel).text.contains("[url=%s]" % meta):
		return node
	for child in node.get_children():
		var found := _find_meta_label(child, meta)
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

## The QUARRY herd for the party compose sheet: a Wild Boar carrying BOTH sim-exported tables — the
## band FLOW ceilings and, decisively, the forward-simulated `hunt_trip_estimates` the sheet's policy
## metrics / max-useful party cap / trip forecast are all pure lookups into. Without the trip table the
## sheet renders bare rungs and no forecast, i.e. exactly the state the quarry-first flow exists to fix.
## It sits 4 tiles from the band at (71,18), so the round-trip travel term is exercised too.
func _quarry_herd_fixtures() -> Array:
	var herd := {
		"id": QUARRY_FAR_HERD_ID, "species": "Wild Boar", "x": QUARRY_FAR_X, "y": QUARRY_FAR_Y,
		"population": 140, "ecology_phase": "thriving", "huntable": true,
		"per_worker_yield": 0.8, "food_per_animal": QUARRY_FOOD_PER_ANIMAL,
		"hunt_policy_ceilings": {
			"sustain": 0.30, "surplus": 1.20, "market": 0.60, "eradicate": 0.0,
		},
	}
	# The server's measured boar raid: 1 hunter → 5 animals / 7 turns, 2 → 8 / 8, 3+ → 8 / 4. Delivered
	# food plateaus at party 2, so the sheet's stepper must cap there with its "max 2 useful" note.
	var turns_row := [7, 8, 4, 4, 4, 4, 4, 4]
	var animals_row := [5, 8, 8, 8, 8, 8, 8, 8]
	var table := {}
	for i in animals_row.size():
		var w := i + 1
		var turns := int(turns_row[i])
		var base := int(animals_row[i])
		# A CLEAN raid — the party hauls its whole kill home, so delivered = animals × fpa, waste 0.
		# The deeper policies raid to a lower floor and so take MORE (Surplus < Market), which is the
		# ASCENDING per-policy metric the picker buttons must read.
		table["sustain:%d" % w] = {"turns_to_fill": turns, "delivers_food": true,
			"animals_taken": base, "delivered_food": float(base) * QUARRY_FOOD_PER_ANIMAL,
			"wasted_food": 0.0}
		table["surplus:%d" % w] = {"turns_to_fill": turns, "delivers_food": true,
			"animals_taken": base + 2, "delivered_food": float(base + 2) * QUARRY_FOOD_PER_ANIMAL,
			"wasted_food": 0.0}
		table["market:%d" % w] = {"turns_to_fill": turns, "delivers_food": true,
			"animals_taken": base + 3, "delivered_food": float(base + 3) * QUARRY_FOOD_PER_ANIMAL,
			"wasted_food": 0.0}
		# Eradicate is a DENIAL rung: the SIM says so via `delivers_food`, never the policy string.
		table["eradicate:%d" % w] = {"turns_to_fill": turns, "delivers_food": false,
			"animals_taken": base + 5, "delivered_food": 0.0, "wasted_food": 0.0}
	herd["hunt_trip_estimates"] = table
	# A second huntable herd INSIDE the band's hunt reach. It is not a party's job (the band can work
	# it from home), so the picker must refuse it — the near half of the eligibility assertion.
	var near := {
		"id": QUARRY_NEAR_HERD_ID, "species": "Roe Deer", "x": QUARRY_NEAR_X, "y": QUARRY_NEAR_Y,
		"population": 90, "ecology_phase": "thriving", "huntable": true,
		"per_worker_yield": 0.8,
		"hunt_policy_ceilings": {"sustain": 0.20, "surplus": 0.80, "market": 0.40, "eradicate": 0.0},
		"hunt_trip_estimates": table.duplicate(true),
	}
	return [herd, near]

## The tile_info a map click on a herd's hex delivers (`Hud._huntable_herd_on_tile` reads `herds`).
func _quarry_tile_info(herd: Dictionary) -> Dictionary:
	return {"x": int(herd["x"]), "y": int(herd["y"]), "herds": [herd]}

## A hunting PARTY is for game the band cannot work from home, so the quarry picker must refuse a herd
## inside the band's `hunt_reach` (`Hud._is_expedition_quarry`) — the near herd is a LOCAL hunt. This
## is behavioural, not pictorial: the refusal happens at the click, which no frame can show. Verified
## to FAIL (the near herd is accepted, `_compose.party_quarry_id()` = the near id) with the eligibility test
## removed from `_try_pick_quarry`.
func _assert_quarry_eligibility() -> void:
	var herds := _quarry_herd_fixtures()
	var far: Dictionary = herds[0]
	var near: Dictionary = herds[1]
	_hud.update_herds(herds)
	# NEAR — inside hunt reach: refused, and targeting stays armed so the player can pick again.
	_hud._compose.clear_party_quarry()
	_hud._pending_pick_quarry = {"band": _band_fixture()}
	_hud._try_pick_quarry(_quarry_tile_info(near))
	assert(_hud._compose.party_quarry_id() == "",
		"band_panel_preview: a herd INSIDE hunt reach was accepted as a quarry (%s)" \
		% _hud._compose.party_quarry_id())
	assert(not _hud._pending_pick_quarry.is_empty(),
		"band_panel_preview: the refused pick dropped out of targeting instead of staying armed")
	# FAR — beyond hunt reach: accepted, and the pick ends targeting.
	_hud._try_pick_quarry(_quarry_tile_info(far))
	assert(_hud._compose.party_quarry_id() == QUARRY_FAR_HERD_ID,
		"band_panel_preview: a herd BEYOND hunt reach was refused as a quarry (%s)" \
		% _hud._compose.party_quarry_id())
	_hud._pending_pick_quarry = {}
	_hud._compose.clear_party_quarry()
	print("band_panel_preview: assert OK — quarry picker takes the far herd, refuses the near one")

## Herds for the per-source-cap verify state: game_deer_07 carries the pre-commit forecast fields the
## Current-actions Hunt row reads via `_find_world_herd` + `_forecast_inputs` — `per_worker_yield`
## plus the herd's ONLY ceiling representation, the `hunt_policy_ceilings` table (a herd has no flat
## `ceiling_*` scalars; the forage patches below still do).
## max-useful = ceil(0.20 / 0.10) = 2, so a Hunt row staffed at 2 is AT its cap.
func _cap_demo_herd_fixtures() -> Array:
	return [
		{"id": "game_deer_07", "species": "Red Deer", "x": 68, "y": 15, "population": 120,
			"ecology_phase": "thriving", "per_worker_yield": 0.10,
			"hunt_policy_ceilings": {"sustain": 0.20}},
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

## The MapView snapshot behind `band_panel_people_map_path` — the SAME `_band_fixture()` cohort the
## snapshot-path state uses, on a flat grid just big enough to hold its hex, so the marker MapView
## builds carries exactly the age structure the panel is judged on. FoW is off in a fresh MapView.
func _map_path_snapshot() -> Dictionary:
	var terrain: Array = []
	terrain.resize(MAP_PATH_GRID_W * MAP_PATH_GRID_H)
	terrain.fill(MAP_PATH_TERRAIN_ID)
	return {
		"grid": {"width": MAP_PATH_GRID_W, "height": MAP_PATH_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [_band_fixture()],
	}

## A player-faction Camp-stage band (population-snapshot shape update_band_alerts consumes):
## working-age labor with idle workers + a couple of active assignments + the settlement stage
## header fields, so the relocated panel shows a full detail + allocation report.
func _band_fixture() -> Dictionary:
	return {
		"id": "Band 2",
		"entity": 904,
		"faction": 0,
		"size": 30,
		"pos": [71, 18],
		"current_x": 71,
		"current_y": 18,
		# Good food state: long larder runway (≥ warn) + positive net (0.94 − 0.68 = +0.26) → the Food
		# line reads "… · +0.26 /turn" (green) with the category breakdown collapsed (clickable open).
		"turns_of_food": 22.0,
		# Good morale (collapsed ▸ disclosure); the signed Layer-1 contributions give the morale
		# breakdown real content when expanded.
		"morale": 0.82,
		"morale_settling": 0.012,
		"morale_terrain": -0.010,
		"morale_climate": -0.006,
		"stores": {"provisions": 84.0},
		"working_age": 16,
		"idle_workers": 3,
		# Age structure (PopulationCohortState children/working/elders) — the band zone's PEOPLE bar.
		# **`age_working` MUST equal `working_age`, and the three MUST sum to `size`.** They are one
		# band counted two ways, and the sim keeps them in step; a fixture that disagrees renders a
		# PEOPLE bar of 99 working-age adults above a WORKFORCE bar of 16 workers, which reads as a
		# bug in the very frame the two-bar design is judged on. These are the live game's own
		# numbers (`Pop 30 👶9 🛠16 🧓5`), so dep = round((9 + 5) / 16 * 100) = 88 per 100 workers.
		# FRACTIONAL, as the wire actually carries them (Scalar) — the panel apportions them to whole
		# people. Rounding each on its own gives 9 + 17 + 5 = 31 for a band of 30, which is the
		# off-by-one this fixture now guards: the frame must read 9 · 16 · 5.
		"age_children": 9.2925,
		"age_working": 16.5375,
		"age_elders": 4.6425,
		"max_expedition_party_size": 8,
		# The raid-forecast levers the sim echoes on every cohort: the slow-raid warn line and the
		# move rate the client adds round-trip travel from. Without them the compose sheet's forecast
		# degrades to hunting turns only and can never read "slow" — i.e. it would prove less.
		"expedition_viability_warn_turns": 20,
		"band_move_tiles_per_turn": 2.0,
		"work_range": 2,
		# Deliberately SHORT: the quarry fixtures straddle it (Wild Boar 4 tiles out = a party's job,
		# Roe Deer 1 tile out = a local hunt), which is what the quarry-eligibility assertion below
		# tests. Only the herd drawer and `_is_expedition_quarry` read it, so no other state moves.
		"hunt_reach": QUARRY_BAND_HUNT_REACH,
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
	band["turns_of_food"] = 4.0
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
	band["turns_of_food"] = 1.5
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
		"turns_of_food": 9.0,
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
	band["turns_of_food"] = BandFoodStatus.UNLIMITED_TURNS
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
	band["turns_of_food"] = 9.0
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
		"turns_of_food": 5.0,
		"is_expedition": true,
		"expedition_mission": "hunt",
		"expedition_phase": "hunting",
		"expedition_target_herd": "game_deer_79",
		"expedition_hunt_policy": "surplus",
		"home_band_entity": 904,
		# In-flight next delivery → the parties inspector's "Next delivery: ~14 food in 6 turns" line.
		"expedition_eta_turns": 6,
		"expedition_projected_delivery": 14.0,
		"expedition_recurring": false,
	}

## A hunt party whose forecast projects ZERO delivery — the herd is at/below its policy floor, so the
## raid returns empty. The field is PRESENT and 0 (a real no-surplus answer), which the parties
## inspector must render as "Next delivery: none — the herd has no surplus to raid", never hide.
func _lean_hunt_expedition_fixture() -> Dictionary:
	return {
		"id": "Hunters 2",
		"entity": 953,
		"faction": 0,
		"size": 4,
		"current_x": 64,
		"current_y": 11,
		"turns_of_food": 4.0,
		"is_expedition": true,
		"expedition_mission": "hunt",
		"expedition_phase": "hunting",
		"expedition_target_herd": "game_deer_07",
		"expedition_hunt_policy": "sustain",
		"home_band_entity": 904,
		"expedition_eta_turns": 0,
		"expedition_projected_delivery": 0.0,
		"expedition_recurring": false,
	}

## A hunt party whose target herd is GONE from `_world_herds` (lost/replaced) — a projected-0 forecast
## that is NOT "no surplus": `_find_world_herd` returns {} for the target id, so the delivery line must
## read "target herd lost — the party is returning home", distinct from the at-floor no-surplus case.
func _lost_hunt_expedition_fixture() -> Dictionary:
	return {
		"id": "Hunters 3",
		"entity": HUNT_LOST_ENTITY,
		"faction": 0,
		"size": 5,
		"current_x": 62,
		"current_y": 9,
		"turns_of_food": 6.0,
		"is_expedition": true,
		"expedition_mission": "hunt",
		"expedition_phase": "returning",
		# NOT in `_herd_fixtures()` — the target the party launched at is no longer in the telemetry.
		"expedition_target_herd": "game_deer_gone",
		"expedition_hunt_policy": "sustain",
		"home_band_entity": 904,
		"expedition_eta_turns": 0,
		"expedition_projected_delivery": 0.0,
		"expedition_recurring": false,
	}
