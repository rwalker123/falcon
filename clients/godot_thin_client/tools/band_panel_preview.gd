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
## Offset applied to a fixture cohort's `entity` to derive its `band_id` — see `_push_bands`.
const FIXTURE_BAND_ID_OFFSET := 4000
## One Wild Boar's worth of yield in provisions (`HerdTelemetryState.foodPerAnimal`) — the quarry
## fixture's delivered food is animals × this, so the sheet's forecast quotes a real food total.
const QUARRY_FOOD_PER_ANIMAL := 4.0
## One animal's worth of TRADE GOODS (issue #337) — a hunt pays a vector, so a raid cell carries this
## payload beside its food one. Small against the food quantum: an edible quarry is meat first.
const QUARRY_TRADE_PER_ANIMAL := 0.5
## The INEDIBLE quarry on the work board (issue #337): its hunt row pays trade goods and no food.
const TRADE_ONLY_HERD_ID := "game_wolf_03"
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
# Morale rows carry, i.e. what `DetailFormat.breakdown_key` builds for that band.
const BAND_FIXTURE_DISCLOSURE_FOOD := "food:904"
const BAND_FIXTURE_DISCLOSURE_MORALE := "morale:904"
const BAND_FIXTURE_DISCLOSURE_TRADE := "trade:904"

## The work-inspector policy-picker states work TWO Hunt rows on one band, told apart by the rung they
## stand on: `corral` is an INVESTMENT rung (the picker offers only the four extractive ones, so it can
## highlight nothing) and `sustain` is the ordinary control.
const INVESTMENT_ROW_POLICY := "corral"
const INVESTMENT_ROW_HERD_ID := "game_aurochs_11"
## The crew that mid-build pen owes. Set through `_set_managed_herders`, so BOTH herder counts carry it.
const INVESTMENT_ROW_HERDERS_NEEDED := 3
const EXTRACTIVE_ROW_POLICY := "sustain"
const EXTRACTIVE_ROW_HERD_ID := "game_deer_07"
## The rung both assertions PRESS. Extractive, so on the investment row it is a genuine "discard the
## pen and take at Surplus instead", and on the control row an ordinary change of take.
const PICKED_RUNG_POLICY := "surplus"

## The under-contained managed herd (fauna neglect-escape arc): a Corralled herd that needs 4 herders
## but is staffed with only 2, so it sheds animals — the work-board ⚠ / drifting-off note case.
const UNDER_HERDED_WORK_HERD_ID := "game_aurochs_uh"
## The crew that pen owes — the SAME number as the row's `workers_needed`, which is where the shed
## comes from (staffed 2 < needed 4), so the two read from one const rather than two loose literals.
const UNDER_HERDED_WORK_HERDERS_NEEDED := 4

## THE HERDER-FLOOR ROW (`band_panel_work_herder_floor`) — a MANAGED herd whose crew requirement is
## LARGER than what its take saturates, which is the only shape that can expose the bug: the row flags
## the herd under-herded and, without the floor, disables the very `+` that would staff the 3rd herder.
## The numbers are the playtest's Wild Fowl. `ceil(0.09 take ÷ 0.05 per worker) = 2` is the take-side
## max-useful; the crew is 3; the row is staffed at 2 with idle workers free, so the `+` is gated by
## the source and by nothing else. `food_per_animal` is deliberately ABSENT — a whole-animal quantum
## would re-derive the cap through the carry model and the frame would stop testing the floor.
const HERDER_FLOOR_HERD_ID := "game_fowl_hf"
const HERDER_FLOOR_HERDERS_NEEDED := 3
const HERDER_FLOOR_STAFFED := 2
const HERDER_FLOOR_PER_WORKER := 0.05
const HERDER_FLOOR_SUSTAIN_CEILING := 0.09
## What `max_useful_workers` answers for that pair, and what the cap would be WITHOUT the floor —
## named because both cap twins are asserted against it and against the crew that must outrank it.
const HERDER_FLOOR_TAKE_USEFUL := 2

## THE SOURCE-RUNG BOARD — one row per rung of both ladders, on ONE band, so the marks are judged
## against each other rather than one frame at a time. Wild carries NO mark (that is the design), so
## it is on the board as the control: without it the frame cannot show that absence reads as wild
## rather than as a missing glyph.
##   plants:  (70,20) wild · (71,20) 🌾 Tended Patch · (72,20) ▦ Field
##   animals: `game_boar_rp` ◎ pastoral (tamed, unpenned) · `game_aurochs_rp` 🐄 penned
## The two herds are the pair `DetailFormat` alone CANNOT tell apart — `husbandry_label` and
## `corral_label` both wear 🐄 — so a pastoral row that reads 🐄 here is the exact defect the mark
## exists to prevent.
const RUNG_WILD_TILE := Vector2i(70, 20)
const RUNG_TENDED_TILE := Vector2i(71, 20)
const RUNG_FIELD_TILE := Vector2i(72, 20)
## The committed crop each prepared patch carries — it rides the rung mark's TOOLTIP, which is the
## only place the board has room to name it.
const RUNG_TENDED_CROP := "Wild Emmer"
const RUNG_FIELD_CROP := "Einkorn"
const RUNG_PASTORAL_HERD_ID := "game_boar_rp"
const RUNG_PENNED_HERD_ID := "game_aurochs_rp"
## The penned herd's crew, staffed in full — this frame is about the RUNG, so it must not also trip
## the under-herded ⚠ and leave two explanations for one amber row.
const RUNG_PENNED_HERDERS := 2
## Every Nth many-source patch carries a rung, so the paged/threshold frames show rung marks mixed
## among wild rows at real board density. Coprime with each other and with the 3 the overstaffed
## rows cycle on, so no row lands on two conditions in lockstep.
const RUNG_MANY_TENDED_STRIDE := 4
const RUNG_MANY_FIELD_STRIDE := 7

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
const SHELL_THRESHOLD_UNDERSHOOT := 1
const SHELL_THRESHOLD_HEIGHT := 900
## The canvas the DOCK-ROW states render at (issue #324). 1080p with a bottom dock is the case the
## issue is about, and the canvas — not just the window — has to be pinned: `project.godot` stretches
## `canvas_items`, so a bare window pin renders at the 1920 base width whatever the window says.
const DOCKROW_CANVAS := Vector2i(1920, 1080)
## The map the dock-row states seed their minimap from — the DEFAULT size, resolved through the same
## registry the New Game pane and the inspector's Map tab use. The rail width the reflow declares is a
## function of the minimap's grid ASPECT (`MinimapPanel.resize_to_aspect`: `embedded_height × aspect`,
## clamped into the config's `[min_width, max_width]`), so it has to come from here and never from a
## literal — otherwise the frames render a nav cluster the game never has.
const DOCKROW_MAP := MapSizes.DEFAULT_KEY
## Flat fill for that stand-in minimap raster. `MinimapController._rebuild_image` paints one pixel per
## HEX from live terrain + fog, which needs a whole MapView snapshot; this harness only needs the
## thumbnail's SIZE to be honest, so it substitutes a flat 1px-per-hex image at the real grid
## dimensions. The aspect — the only thing that drives the rail width — is therefore the real one.
const DOCKROW_MINIMAP_FILL := Color(0.16, 0.24, 0.20, 1.0)
# The window every state but the ultrawide one renders at.
const PREVIEW_SIZE := Vector2i(1500, 900)
# How many frames to keep re-asserting the window before giving up and warning. Also the bound on
# `_capture`'s geometry retry, so a WM that refuses to honour the pin fails loudly instead of hanging.
const WINDOW_PIN_MAX_FRAMES := 30
## How many CONSECUTIVE frames the window must hold `PREVIEW_SIZE` in `_stabilize_canvas` before the
## first state renders, and the bound on how long it waits for that. The maximize is applied — and
## RE-applied — asynchronously, so "it is the right size once" is not the same as "it stays".
const CANVAS_STABLE_FRAMES := 30
const CANVAS_STABLE_MAX_FRAMES := 600
## Phase to seed the turn orb's calm breath at, as a fraction of `TurnOrb.PULSE_PERIOD`. The breath is
## `0.5 - 0.5 * cos(t)`, which is ZERO — its faintest, smallest instant — at phase 0, so freezing the
## clock there would render the pulse at the bottom of its range. A quarter period puts `cos` at 0,
## i.e. the breath's MIDPOINT, which is what an unfrozen frame averaged.
const TURN_ORB_PULSE_MIDPOINT_FRACTION := 0.25

## The size every state re-asserts before it renders — see `_pin_window`.
var _pinned_size := PREVIEW_SIZE
## The canvas size every state re-asserts, `ZERO` = leave the project's stretch alone — see `_pin_canvas`.
var _pinned_canvas := Vector2i.ZERO
var _hud: HudLayer
var _panel: BandCityPanel
## The last state `_save`d, so an assertion failure names the frame it fired on.
var _current_state := "<pre-render>"

func _ready() -> void:
	# FREEZE ANIMATION TIME — the treatment `ui_preview`, `map_preview` and `blend_probe` all carry, and
	# taken for the same reason: a frame that varies run-to-run cannot be pixel-diffed to prove a panel
	# refactor changed nothing. Measured before the freeze, two runs of IDENTICAL code differed byte-wise
	# in `band_panel_no_idle` — 51 px inside the turn orb's 71×70 ring box, the calm breath.
	#
	# What survives phase 0 was CHECKED against the draw code, not assumed:
	#   • the turn orb's breath is `0.5 - 0.5 * cos(t)`, which DEGENERATES to its faintest, smallest
	#     instant at phase 0, so its phase is seeded to the midpoint below rather than left at 0. It is
	#     drawn only while the orb has no attention entries (`_draw_pulse` vs `_draw_badge`), which is
	#     why just one frame moved;
	#   • MapView's awaiting-expedition / targeting pulses are not in any frame — both MapViews this
	#     harness builds are `visible = false`, data only;
	#   • the ONE tween in the whole client is `TellingPanel`'s page turn, and this harness pushes no
	#     narrative beats, so no tween is ever created here. `ui_preview` has to flush tweens in its
	#     settle; there is deliberately nothing to flush here. RE-CHECK THAT if a state ever drives the
	#     Telling panel: a Tween at `time_scale = 0` never advances AT ALL, so it would pin at its
	#     starting frame rather than merely render at a fixed phase.
	# `Hud._process` only hides a tooltip and `MapView._process` is input-driven, so neither carries a
	# time term; `Main` / `LogsPanel` / `ScriptHostManager` are not instanced. `_settle` waits on
	# `process_frame`, which still fires at `time_scale` 0.
	Engine.time_scale = 0.0
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
	# Hold the canvas until the WM stops fighting it — before the first state, so no LATER settle has
	# to spend a frame on it. See `_stabilize_canvas`.
	await _stabilize_canvas()

	# Seed the turn orb's calm breath at its MIDPOINT. `_pulse_time` only ever advances by `delta`,
	# which is 0 with the clock frozen, so whatever is set here is the phase every frame renders at —
	# and phase 0 is the breath's trough (alpha 0.30 / radius 44 of a 0.30..0.85 / 44..47 range), i.e.
	# a deterministic frame whose subject has faded to its faintest. Set once; nothing resets it.
	_hud.turn_orb._pulse_time = TurnOrb.PULSE_PERIOD * TURN_ORB_PULSE_MIDPOINT_FRACTION

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
	# THE DOCK-ROW REFLOW WIRING (issue #324), exactly as `Main._connect_band_city_panel` does it: a
	# SECOND listener on `reservation_changed` plus a one-shot seed. This harness does not instance
	# `Main`, so without it the reflow would only ever be exercised by poking the controller — and the
	# `band_panel_dockrow_*` states below are meant to drive the real path.
	if _hud.has_method("reflow_dock_row"):
		_panel.reservation_changed.connect(Callable(_hud, "reflow_dock_row"))
		_hud.reflow_dock_row(_panel.get_dock(), _panel.current_reservation_size())
	# The world's herds (Main pushes snapshot["herds"]): the Current-actions Hunt row names the herd
	# from here and, on click, jumps to its LIVE tile — the herd has MIGRATED away from the
	# assignment's launch target (70, 17) to (68, 15), which is exactly what the row must resolve.
	_hud.update_herds(_herd_fixtures())
	# The world's food modules (Main pushes snapshot["food_modules"]): the Forage row leads with the
	# module's map glyph (savanna grassland → 🌾 on (71, 18)).
	_hud.update_food_modules([
		{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"},
	])
	_push_bands([_scout_expedition_fixture(), _band_fixture(), _hunt_expedition_fixture()])
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
	_push_bands([_band_fixture()])
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

	# (b2) THE TRADE ROW (issue #381) — what THIS band earns per turn in the second product. The row is
	# **purely band-scoped**: it carries a rate and no stock, because the only trade-goods stock the sim
	# publishes is faction-global and every band would print the same total. So the states below pin the
	# rate's two ends plus the tier gate — there is no stock axis left to vary.
	#
	# (i) EARNING — the fixture's forage patch pays ⇄ 0.04 through the `realized == 0` fallback and its
	# deer pays ⇄ 0.04 outright, so the headline reads +0.08 over a TWO-row breakdown. Disclosure OPEN,
	# because **the Gathered row is the regression guard**: reading `realized_trade_yield` alone drops
	# the forage half, which is exactly how a cash-crop band came to read `+0.00` in playtest.
	# LEFT dock only; see (iii) for why the row is not in a T/B frame.
	_push_bands([_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	_click_disclosure(BAND_FIXTURE_DISCLOSURE_TRADE)
	await _settle()
	await _save("band_panel_trade_expanded_left")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_forage_trade_counted()
	_click_disclosure(BAND_FIXTURE_DISCLOSURE_TRADE)

	# (ii) ZERO — a band working no trade-paying source. **The row is STILL THERE**, reading `+0.00 /turn`
	# in neutral ink with no caret, and that is the whole point of the state: a row that vanished at zero
	# read in playtest as "this band cannot trade at all" rather than "it earns none right now". The caret
	# is absent because `register` declines an empty payload — an income-only breakdown has no rows when
	# there is no income — so a zero row is honestly inert rather than opening an empty popover.
	_push_bands([_no_trade_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	await _save("band_panel_trade_zero")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_trade_row_reads_zero()

	# (iii) THE SHORT-TIER DROP. The T/B dock's band zone is ~300px and CLIPS what it cannot hold, so the
	# Trade row is gated off there exactly as the food-outlook chart is — measured at 26px, against a zone
	# with nothing to spare. The SAME earning band as (i), in a TOP dock, must render Food/Morale/Growth
	# and NO Trade row. **Asserted, not just eyeballed**, because an absent row and a row clipped off the
	# bottom of a `clip_contents` zone are the same picture.
	_push_bands([_band_fixture()])
	_panel.set_dock(SIDE_TOP)
	await _settle()
	await _save("band_panel_trade_short_tier")
	_assert_zones_within_bounds()
	_assert_trade_row_absent_in_short_tier()

	# (c) CONCERNING food (net negative + low runway): the breakdown AUTO-shows (no click) under a red net.
	_push_bands([_concerning_food_band_fixture()])
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
	_push_bands([_band_fixture()] + _phase_expedition_fixtures())
	_hud._emit_assign_labor(_hud._band_labor._panel_band, "forage", 4, 72, 19, "", "surplus")
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	await _save("band_panel_status_glyphs")

	# Fit-to-content height (no clipping) — push a TALLER band: starving + full morale breakdown +
	# output row + the send-expedition section, so the summary column is much taller than the old fixed
	# T/B PANEL_HEIGHT would allow. Dock top/bottom and confirm every column's bottom row is visible and
	# the reserved strip grew to fit (map/HUD reflow is fanned onto the HUD as usual).
	_hud.show_tile_selection({})   # clear the foreign selection so the panel band is the subject again
	_push_bands([_starving_band_fixture(), _scout_expedition_fixture(), _hunt_expedition_fixture()])
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
	_push_bands([_cap_demo_band_fixture()])
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
	_push_bands([_arrivals_band_fixture()])
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
	_push_bands([_arrivals_starving_band_fixture()])
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
	# zeroes every remainder, leaving `HudFormat.apportion_people` nothing to redistribute: 9 + 16 + 4 = 29 in
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
	_push_bands([_band_fixture()])

	# The paged WORK BOARD at 34 sources — far past one page in the narrow (L dock) shell, so the
	# pager must appear and NOTHING may scroll. Its patches carry RUNG marks on a stride, so the
	# board is also where the marks are judged at real density — and, because the shell-threshold
	# probes below re-render this same band, where they are judged at the narrowest legal column.
	_hud.update_food_modules(_many_forage_modules())
	_hud.update_forage_patches(_many_source_patch_fixtures())
	_push_bands([_many_sources_band_fixture()])
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
	_hud._bandpanel._toggle_work_inspector(_hud._bandpanel._work_source_models(_hud._band_labor._panel_band, 0)[0]["key"])
	await _settle()
	await _save("band_panel_inspector")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._bandpanel._toggle_work_inspector(_hud._bandpanel._work_open_key)

	# The Work menu's destructive action asks first, and the confirm names what is SPARED.
	_hud._bandpanel._on_work_unassign_all_pressed(_hud._band_labor._panel_band, 34)
	await _settle()
	await _save("band_panel_clear_confirm")
	_dismiss_dialogs()

	# THE TWO PRODUCTS ON THE WORK BOARD (issue #337). The concerning-food band works three sources —
	# a forage patch (food only), a deer hunt (food AND trade, food leading) and a WOLF hunt whose food
	# fields are honestly 0. Its row must headline `⇄ +0.22` ALONE: before this arc the client read only
	# food, so the wolf row said `+0.00 /turn` and the pack looked worthless. The inspector strip is
	# opened on that row so its one-sentence readout is judged too — it states the same components the
	# row does. The Food line above is the control: it still counts FOOD only, so a trade-only hunt must
	# not move it (trade goods credit the faction stockpile, never the larder).
	_hud.update_food_modules([{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"}])
	_push_bands([_concerning_food_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_work_trade_rows")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_open_work_inspector_for_herd(TRADE_ONLY_HERD_ID)
	await _settle()
	await _save("band_panel_work_trade_inspector")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_hud._bandpanel._toggle_work_inspector(_hud._bandpanel._work_open_key)

	# THE AGGREGATES (issue #337, phase 2). Same board with the deer removed, so the band's ONLY hunt
	# pays trade: the head must read `2 sources +0.15 /turn ⇄ +0.22` — a SIBLING trade total, never
	# folded into the food one — and the hunt chip `🦌 1 · ⇄ 0.22`, with the food component suppressed
	# rather than printed as a `0.00` that says the wolf pack yields nothing. This is the frame the
	# fix is judged on: the previous state's header excluded the wolf's `+0.22` while its row sat
	# directly underneath, so the arithmetic visibly did not add up.
	_push_bands([_trade_only_hunt_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_work_trade_totals")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# THE WORK INSPECTOR'S POLICY PICKER — the one control on the board with no frame coverage at all
	# until now (`_work_policy_open` was never set true in either harness). Two rows, two behaviours:
	# a source standing on an INVESTMENT rung (Corral) highlights none of the four extractive rungs,
	# so it must SAY the standing rung and CONFIRM before a pick discards it; a source standing on an
	# extractive rung (Sustain) must behave exactly as it always has — one lit rung, immediate emit.
	_hud.update_food_modules([{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"}])
	_hud.update_herds(_investment_policy_herd_fixtures())
	_push_bands([_investment_policy_band_fixture()])
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
	_hud._bandpanel._work_policy_open = false
	_hud._bandpanel._toggle_work_inspector(_hud._bandpanel._work_open_key)

	# UNDER-CONTAINED managed herd in the WORK board (fauna neglect-escape arc): a Corralled herd that
	# needs 4 herders but is staffed with only 2 sheds animals to the wild. It must read as trouble
	# WHEREVER it is listed — here, on its work row — with the established overhunt ⚠ (amber marks +
	# amber severity stripe) and the "Too few herders — animals are drifting off." note in the
	# inspector, not only in its own drawer.
	_hud.update_herds(_under_herded_work_herd_fixtures())
	_push_bands([_under_herded_work_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_under_herded")

	# THE RUNG-READY MARK ON THE WORK BOARD (issue #412) — the panel twin of the map badge. Three rows,
	# and the CONTRAST is what the frame is for: a tended patch on willing ground offers `⌃▦`, a fully
	# tamed "pen"-ceiling herd offers `⌃🐄`, and a wild-ceiling herd offers nothing however much the
	# faction knows. A chevron on every row would prove nothing.
	#
	# Knowledge is pushed FIRST: the mark reads `RungGates` against the top bar's row, so without it
	# every source is honestly "not ready" and the board renders a frame with nothing to look at.
	_hud.update_intensification([{"faction": 0,
		"cultivation": 1.0, "seed_selection": 1.0, "herding": 1.0, "penning": 1.0}])
	_hud.update_food_modules([{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"}])
	_hud.update_forage_patches(_ready_patch_fixtures())
	_hud.update_herds(_ready_herd_fixtures())
	_push_bands([_ready_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_rung_ready")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_ready_marks()

	# The READY FILTER chip narrows the board to exactly those rows — its own count beside the
	# attention chip, never folded into it.
	_hud._bandpanel._set_work_filter(HudWorkVocab.WORK_FILTER_READY)
	await _settle()
	await _save("band_panel_rung_ready_filter")
	_assert_ready_filter_narrows()
	_hud._bandpanel._set_work_filter(HudWorkVocab.WORK_FILTER_ALL)
	_hud.update_intensification([])

	# THE FORAGE JUMP NAMES THE LAND (issue #412, a pre-existing defect the marks made reachable-looking).
	# A hunt row always named its herd; a forage row focused the tile and left the hex's AUTO-PICK to
	# choose, so on a hex that also holds a band or a herd it opened THAT instead of the patch. The mark
	# is what makes it matter: a row that says "this patch can be sown" must land on the patch.
	#
	# Asserted, not pictured — the wrong subject and the right one render the same card shape.
	_assert_forage_jump_names_land()
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_under_herded_work_row(UNDER_HERDED_WORK_HERD_ID)

	# THE HERDER FLOOR — the board must not flag a problem and then disable its own remedy. A managed
	# Wild Fowl herd grew to owe 3 keepers while its take saturates at 2 workers, and the row is staffed
	# at 2 with idle workers free. The take-side max-useful alone would gate the `+` dead at 2, directly
	# under the ⚠ that says a 3rd herder is needed (the playtest report). Both cap twins now floor on
	# `SourceForecast.herd_crew_floor`, so the row's `+` reaches the crew the sim is asking for — and the
	# assertion states that as the twin invariant, which a PNG structurally cannot carry.
	_hud.update_herds(_herder_floor_herd_fixtures())
	_push_bands([_herder_floor_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_work_herder_floor")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_herder_floor_row(HERDER_FLOOR_HERD_ID)

	# THE SOURCE-RUNG BOARD — five rows, one per rung of the two ladders, on ONE band so the marks are
	# read against each other: wild forage (NO mark, the control) · 🌾 Tended Patch · ▦ Field · ◎
	# pastoral herd · 🐄 penned herd. The mark is orthogonal to the policy glyph, which reads ♻ Sustain
	# on every row here precisely so the frame cannot be passed by the verb: before this, a Tended Patch
	# under Sustain and plain wild ground under Sustain were indistinguishable on the board. The narrow
	# (L) shell puts all five in one column at `WORK_COLUMN_MIN_WIDTH`, which is also where the label's
	# remaining width is judged.
	_hud.update_food_modules(_rung_forage_modules())
	_hud.update_forage_patches(_rung_patch_fixtures())
	_hud.update_herds(_rung_herd_fixtures())
	_push_bands([_rung_band_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"work")
	await _settle()
	await _save("band_panel_work_rungs")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_work_row_rungs()
	_assert_rung_labels_are_hoverable()

	# The same five rows in the WIDE (bottom) shell, where the rung slot competes with the multi-column
	# split for the label's width.
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("band_panel_work_rungs_wide")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# Back to the LEFT dock before moving on: the states after this one inherit the dock rather than
	# setting their own, so leaving the panel bottom-docked would silently re-render `band_panel_no_idle`
	# and `band_panel_compose_hunt` in the wide shell.
	_panel.set_dock(SIDE_LEFT)

	# Restore the reference band so later states start from their usual subject — and the paged board's
	# patch set with it, because `update_forage_patches` REPLACES the lookup: the ultrawide, dock-row
	# and shell-threshold states below re-render `_many_sources_band_fixture`, so leaving the five rung
	# patches installed would strip the marks back off exactly the frames that judge them at the
	# narrowest legal column.
	_hud.update_herds(_herd_fixtures())
	_hud.update_forage_patches(_many_source_patch_fixtures())
	_push_bands([_band_fixture()])

	# The parties COMPOSE sheet, QUARRY-FIRST. With a quarry picked the whole hunt form resolves: the
	# policy rungs carry their ascending per-policy metric, the party stepper caps at the raid's
	# max-useful plateau, the trip forecast reads, and the Send button takes its verdict.
	_hud.update_food_modules([{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"}])
	_hud.update_herds(_quarry_herd_fixtures())
	_push_bands([_scout_expedition_fixture(), _band_fixture(), _hunt_expedition_fixture()])
	_assert_quarry_eligibility()
	_panel.set_active_tab(&"parties")
	_hud._bandpanel._party_compose_open = true
	_hud._bandpanel._party_compose_mission = "hunt"
	_hud._compose.set_party_quarry(QUARRY_FAR_HERD_ID)
	# Picking a quarry fills the party to its max-useful cap (the one-shot `TargetingController._try_pick_quarry` sets);
	# seed it here too so the frame shows the shipped default (the party at the cap, not a stray 1).
	_hud._compose.arm_party_autofill()
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_hunt")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# The same sheet on ERADICATE — the frame the EXPEDITION rung's hint is judged on (issue #337). The
	# launch picker is the ONE surface that renders `SEND_HUNT_POLICY_HINTS` verbatim, and Eradicate's
	# line must describe the whole-stock haul, the currency the SPECIES pays (meat, ⇄ trade goods, or
	# both — the raid banks its trade half too now) and the permanent end state, never "delivers no food".
	_hud._bandpanel._send_hunt_policy = "eradicate"
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_hunt_eradicate")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._bandpanel._send_hunt_policy = SourceForecast.DEFAULT_HUNT_POLICY

	# The same sheet with NO quarry yet: the "Choose…" row, the hint, a disabled Send — and nothing
	# below it, since policy/party/forecast are all unanswerable without a herd.
	_hud._compose.clear_party_quarry()
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_hunt_no_quarry")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

	# Same sheet under Scout: scouting title, NO quarry row, NO policy picker, "Send scouting party…".
	_hud._bandpanel._party_compose_mission = "scout"
	_hud._bandpanel.rerender()
	await _settle()
	await _save("band_panel_compose_scout")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._bandpanel._party_compose_open = false
	_hud._bandpanel._party_compose_mission = ""
	_hud._compose.clear_party_quarry()

	# Zero idle workers: BOTH mission buttons (Scout / Hunt) stay VISIBLE and DISABLED, with the
	# shared reason line beneath them.
	_push_bands([_no_idle_band_fixture()])
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
	_push_bands([_many_sources_band_fixture(), _hunt_expedition_fixture()])
	_panel.set_dock(SIDE_BOTTOM)
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_DELIVERING_ENTITY))
	await _settle()
	await _save("band_panel_parties_inspector_wide")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_DELIVERING_ENTITY))   # close before the next state

	# (b) NARROW shell (left dock, Parties tab): the tall L/R parties zone holds both parties + the strip
	# with room to spare. Inspect the NO-SURPLUS party → the invisible-line bug the strip fixes:
	# "Next delivery: none — the herd has no surplus to raid" must be VISIBLE, not hidden.
	_push_bands([_band_fixture(), _hunt_expedition_fixture(), _lean_hunt_expedition_fixture()])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"parties")
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_LEAN_ENTITY))
	await _settle()
	await _save("band_panel_parties_inspector_narrow")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_LEAN_ENTITY))

	# (b2) NEXT-DELIVERY DISAMBIGUATION on a projected-0 forecast. A hunt party is bound to ONE herd
	# (its `expedition_target_herd`) that MIGRATES and is often NOT the herd on the tile the player is
	# looking at, so a projected 0 means one of two things and the party's target tells them apart:
	# still in `_world_herds` → at/below its policy floor (no surplus); absent → lost/replaced (returning
	# home). The Target row also carries the target's live position so the player can SEE which herd the
	# party is bound to. Render all three parties + assert every line. `_world_herds` = _herd_fixtures():
	# game_deer_07 (@68,15) + game_deer_79 (@64,11); the LOST party targets an absent id.
	_hud.update_herds(_herd_fixtures())
	_push_bands([
		_band_fixture(), _hunt_expedition_fixture(), _lean_hunt_expedition_fixture(),
		_lost_hunt_expedition_fixture(),
	])
	_panel.set_dock(SIDE_LEFT)
	_panel.set_active_tab(&"parties")
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_LOST_ENTITY))
	await _settle()
	await _save("band_panel_next_delivery_disambiguation")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_next_delivery_disambiguation()
	_hud._bandpanel._toggle_parties_inspector(str(HUNT_LOST_ENTITY))

	# (c) DETAIL-PANEL via the MARKER path — the FIX-4 regression. The Occupants-card drawer reads
	# `BandDetailLines.expedition_summary_lines(_selected_unit)`, and `_selected_unit` is the MapView unit MARKER, not
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
	_push_bands([_many_sources_band_fixture()])
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
	# The bottom-bar chrome now SHARES a horizontal dock's row (issue #324), and the shell test reads
	# the panel's width MINUS the trailing chrome rail — so the probe widths must add the live rail width
	# back on, or they would bracket a threshold the panel no longer applies to the raw window width. The
	# width is canvas-independent (`max` of a fixed 260px turn cluster and a grid-aspect minimap), and the
	# panel is already bottom-docked + reflowed from the ultrawide state above, so it can be read here.
	# `_rail_span()`, not `_rail_width()`: the rail also costs a `RAIL_SEPARATOR_SPAN` gutter, and probing
	# against the bare width would bracket the threshold 25px off.
	var rail_span: float = _panel._rail_span()
	var shell_threshold_width := int(ceil(BandCityPanel.WIDE_SHELL_MIN_WIDTH + rail_span))
	print("band_panel_preview: shell threshold probes at %d / %d (threshold %.0f + rail span %.0f)" % [
		shell_threshold_width - SHELL_THRESHOLD_UNDERSHOOT, shell_threshold_width,
		BandCityPanel.WIDE_SHELL_MIN_WIDTH, rail_span])
	# One pixel BELOW: the wide shell could not give the board a readable column, so the panel must
	# choose the NARROW tabbed shell — which hands the board the panel's WHOLE interior.
	await _pin_canvas(Vector2i(shell_threshold_width - SHELL_THRESHOLD_UNDERSHOOT, SHELL_THRESHOLD_HEIGHT))
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
	await _pin_canvas(Vector2i(shell_threshold_width, SHELL_THRESHOLD_HEIGHT))
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("band_panel_shell_at_threshold")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_shell_is_wide(true, "band_panel_shell_at_threshold")

	await _render_dock_row_states()

	_assert_herd_field_pairs()
	get_tree().quit()

# ---- THE DOCK-ROW REFLOW (issue #324) ---------------------------------------------------------
#
# On a HORIZONTAL dock the HUD's bottom-bar chrome shares the panel's reserved row — nav cluster at
# the leading end, turn orb at the trailing one — and `BottomBar` drops out of layout so `ContentRow`
# reclaims its height. A VERTICAL dock must be bit-identical to before. Rendered at 1080p, which is
# the window the issue is about, and driven through the REAL `reservation_changed → reflow_dock_row`
# path wired at the top of `_ready` (never by poking the controller).
func _render_dock_row_states() -> void:
	await _pin_canvas(DOCKROW_CANVAS)
	_seed_embedded_minimap()
	_push_bands([_many_sources_band_fixture()])

	# BOTTOM: the chrome in ONE column at the row's TRAILING end — minimap + zoom rail directly above the
	# turn orb — nothing in the row's leading gutter (the band zone is flush to the left edge), and
	# `BottomBar` gone.
	_panel.set_collapsed(false)
	_panel.set_dock(SIDE_BOTTOM)
	await _settle()
	await _save("band_panel_dockrow_bottom")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_chrome_parked(true, "band_panel_dockrow_bottom")
	_assert_parked_chrome_fits("band_panel_dockrow_bottom")
	_assert_shell_is_wide(true, "band_panel_dockrow_bottom")
	print("band_panel_preview: dockrow bottom — rail %.0fpx + %.0f gutter = %.0f span (nav %.0f, turn %.0f), stack needs %.0f of a %.0f strip, work zone %.0fpx" % [
		_panel._rail_width(), BandCityPanel.RAIL_SEPARATOR_SPAN, _panel._rail_span(),
		_hud.nav_backing.get_combined_minimum_size().x, _hud.turn_orb.get_combined_minimum_size().x,
		_hud._dockrow._required_height(), _panel.current_reservation_size(),
		_panel.work_zone_size().x])

	# TOP: the same column at the other horizontal edge, minimap still on top so the stack reads the same
	# either way. The nav cluster relocating from bottom-left to the TOP row is INTENDED — the chrome
	# follows the dock.
	_panel.set_dock(SIDE_TOP)
	await _settle()
	await _save("band_panel_dockrow_top")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_chrome_parked(true, "band_panel_dockrow_top")
	_assert_parked_chrome_fits("band_panel_dockrow_top")
	_assert_shell_is_wide(true, "band_panel_dockrow_top")

	# LEFT — THE CONTROL. A vertical dock keeps today's behaviour exactly: the chrome is back in
	# `BottomBar` and the rails contribute nothing. The work-zone baseline captured here is what the
	# round-trip state below compares against.
	_panel.set_dock(SIDE_LEFT)
	await _settle()
	await _save("band_panel_dockrow_left")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_assert_chrome_parked(false, "band_panel_dockrow_left")
	_assert_no_rail_width("band_panel_dockrow_left")
	var vertical_work_zone := _panel.work_zone_size()

	# COLLAPSED BOTTOM — the frame that proves collapse does not slice the minimap. The reserved strip
	# is `COLLAPSED_SIZE` (46px), far under the taller cluster's minimum, so the fit gate must DECLINE
	# and the chrome must stay in `BottomBar`.
	_panel.set_dock(SIDE_BOTTOM)
	_panel.set_collapsed(true)
	await _settle()
	await _save("band_panel_dockrow_collapsed_bottom")
	_assert_chrome_parked(false, "band_panel_dockrow_collapsed_bottom")
	_panel.set_collapsed(false)

	# THE ROUND TRIP. Reparenting round-trips are where this class of change rots, so walk
	# bottom → left → bottom → left and assert the clusters came home EXACTLY: authored parent AND
	# child index, the anchors/size flags captured at construction, `BottomBar`'s authored minimum
	# height, and a work zone identical to the never-reflowed baseline above.
	for edge in [SIDE_BOTTOM, SIDE_LEFT, SIDE_BOTTOM, SIDE_LEFT]:
		_panel.set_dock(edge)
		await _settle()
	await _save("band_panel_dockrow_reflow_round_trip")
	_assert_zones_within_bounds()
	_assert_zone_content_fits()
	_assert_chrome_parked(false, "band_panel_dockrow_reflow_round_trip")
	_assert_no_rail_width("band_panel_dockrow_reflow_round_trip")
	_assert_chrome_home_exact("band_panel_dockrow_reflow_round_trip")
	var round_trip_work_zone := _panel.work_zone_size()
	if not round_trip_work_zone.is_equal_approx(vertical_work_zone):
		push_error("band_panel_preview: round trip left the work zone at %s, baseline was %s" % [
			round_trip_work_zone, vertical_work_zone])
	else:
		print("band_panel_preview: assert OK — round trip restored work_zone_size() to %s" % round_trip_work_zone)

## Put a REAL embedded minimap in the HUD's `MinimapContainer` before the dock-row states render.
## Without it those frames judge the reflow against an EMPTY container — the left rail collapses to the
## zoom rail's ~80px instead of the ~290px the game actually has, so both the measured rail span and the
## frames would be honest about nothing. Driven exactly as `MinimapController._setup` drives it
## (`setup_embedded` into `Hud.get_minimap_container()`, then `set_grid_size`, which calls
## `resize_to_aspect`), with the grid resolved from `MapSizes` and the raster a documented flat stand-in
## for `_rebuild_image`'s per-hex paint — see `DOCKROW_MINIMAP_FILL`.
func _seed_embedded_minimap() -> void:
	var container: Control = _hud.get_minimap_container()
	if container == null:
		push_warning("band_panel_preview: no MinimapContainer — dock-row rail widths will be unrealistic")
		return
	var option: Dictionary = MapSizes.option_for(DOCKROW_MAP)
	var grid := Vector2i(int(option["width"]), int(option["height"]))
	var minimap := MinimapPanel.new()
	add_child(minimap)
	minimap.setup_embedded(container)
	var image := Image.create(grid.x, grid.y, false, Image.FORMAT_RGBA8)
	image.fill(DOCKROW_MINIMAP_FILL)
	minimap.set_texture(ImageTexture.create_from_image(image))
	minimap.set_grid_size(grid.x, grid.y)
	print("band_panel_preview: dockrow minimap — %s map %dx%d (aspect %.3f) → panel min %s" % [
		option["label"], grid.x, grid.y, float(grid.x) / float(grid.y),
		minimap.panel.custom_minimum_size])

## GUARD: is the bottom-bar chrome parked in the panel's rail slots, or home in `BottomBar`? Asserts
## BOTH halves of the swap — `BottomBar`'s visibility and each cluster's PARENT — because either one
## alone can be right while the other is wrong (a hidden bar with the chrome still inside it erases
## the chrome; a parked chrome under a visible bar double-books the row's height).
func _assert_chrome_parked(parked: bool, state_name: String) -> void:
	var failures: Array[String] = []
	if _hud.bottom_bar.visible == parked:
		failures.append("bottom_bar.visible is %s but the chrome should be %s" % [
			_hud.bottom_bar.visible, "parked" if parked else "home"])
	for pair in _parked_chrome_pairs():
		var cluster: Control = pair[0]
		var want: Node = pair[1] if parked else _hud.bottom_bar
		if cluster.get_parent() != want:
			failures.append("%s sits under %s, expected %s" % [
				cluster.name, cluster.get_parent().name, want.name])
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s chrome %s" % [state_name, "parked in the row" if parked else "home in BottomBar"])
		return
	for failure in failures:
		push_error("band_panel_preview: %s — %s" % [state_name, failure])

## The two parked-chrome clusters paired with the rail slot each belongs in — nav on TOP, turn cluster
## BELOW. One definition, so the parent assertion and the containment assertion cannot disagree about
## which cluster goes where.
func _parked_chrome_pairs() -> Array:
	return [
		[_hud.nav_backing, _panel.rail_slot_host(BandCityPanel.RAIL_SLOT_TOP)],
		[_hud.turn_orb, _panel.rail_slot_host(BandCityPanel.RAIL_SLOT_BOTTOM)],
	]

## GUARD: the parked chrome must FIT the rail and the rail must fit the strip, and the STACK must sit
## CENTRED in the column.
## **Fit** is the same claim `_assert_zone_content_fits` makes for the zones, and for the same reason:
## the rail CLIPS, so a cluster too wide or too tall for it is silently sliced rather than visibly
## broken. It is what catches a rail whose declared width lags the minimap's (the width is DECLARED,
## never measured from the content, so nothing else would notice) — and it is why these states seed a
## REAL minimap; against an empty `MinimapContainer` the rail collapses to the zoom rail's ~80px and the
## check is vacuous. Both levels are checked: each cluster inside the rail, and the rail inside the card's
## interior strip.
## **Centred** is the other half, and fitting does not imply it: a stack pinned to the rail's mid-line and
## grown DOWNWARD still sits entirely inside a 340px column while rendering ~64px low. That is exactly
## what `set_anchors_and_offsets_preset` does to a plain `Control` (see `BandCityPanel._build_rail`'s note
## 3), so the centre-vs-centre test is the guard on that trap.
func _assert_parked_chrome_fits(state_name: String) -> void:
	var failures: Array[String] = []
	var rail: Control = _panel._rail
	var rail_rect := rail.get_global_rect()
	var stack_top := INF
	var stack_bottom := -INF
	for pair in _parked_chrome_pairs():
		var cluster: Control = pair[0]
		var rect := cluster.get_global_rect()
		stack_top = minf(stack_top, rect.position.y)
		stack_bottom = maxf(stack_bottom, rect.end.y)
		var over := _rect_overflow(rect, rail_rect)
		if over.x > ZONE_BOUNDS_TOLERANCE or over.y > ZONE_BOUNDS_TOLERANCE:
			failures.append("%s %s spills the rail %s by (%.1f, %.1f)" % [
				cluster.name, rect, rail_rect, maxf(over.x, 0.0), maxf(over.y, 0.0)])
	# The rail itself must stay inside the card's interior — the strip the panel actually reserved.
	var strip := _panel._panel.get_global_rect()
	var rail_over := _rect_overflow(rail_rect, strip)
	if rail_over.x > ZONE_BOUNDS_TOLERANCE or rail_over.y > ZONE_BOUNDS_TOLERANCE:
		failures.append("the chrome rail %s spills the card %s by (%.1f, %.1f)" % [
			rail_rect, strip, maxf(rail_over.x, 0.0), maxf(rail_over.y, 0.0)])
	var drift: float = absf(0.5 * (stack_top + stack_bottom) - rail_rect.get_center().y)
	if drift > ZONE_BOUNDS_TOLERANCE:
		failures.append("the chrome stack sits %.0fpx off the rail's vertical centre (stack %.0f, rail %.0f)" % [
			drift, 0.5 * (stack_top + stack_bottom), rail_rect.get_center().y])
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s the chrome stack fits its rail, the rail fits the strip, and the stack is centred" % state_name)
		return
	for failure in failures:
		push_error("band_panel_preview: %s — %s" % [state_name, failure])

## How far `rect` pokes outside `bounds` on each axis (negative = comfortably inside).
func _rect_overflow(rect: Rect2, bounds: Rect2) -> Vector2:
	return Vector2(
		maxf(rect.end.x - bounds.end.x, bounds.position.x - rect.position.x),
		maxf(rect.end.y - bounds.end.y, bounds.position.y - rect.position.y))

## GUARD: a VERTICAL dock must spend NOTHING on the rail — neither its column nor its separator gutter —
## whatever width the HUD last declared; the panel forces it to 0 by EDGE, so the whole strip is the
## zones'. **Both halves are asserted**: `_rail_span()` covers the 25px gutter as well as the column, and
## the separator's own `visible` is checked because a stray hairline down the middle of a left dock is
## exactly the regression the shown-with-the-rail rule exists to prevent — and a `BoxContainer` only skips
## separation around a HIDDEN child, so the visibility IS what makes the span's zero honest.
func _assert_no_rail_width(state_name: String) -> void:
	var failures: Array[String] = []
	var span := _panel._rail_span()
	if not is_zero_approx(span):
		failures.append("still spends %.0fpx on the chrome rail" % span)
	if _panel._rail_separator.visible:
		failures.append("the rail separator hairline is still visible")
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s vertical dock spends nothing on the chrome rail and draws no hairline" % state_name)
		return
	for failure in failures:
		push_error("band_panel_preview: %s vertical dock — %s" % [state_name, failure])

## GUARD: the clusters came home to the EXACT authored parent, child index, anchors and size flags the
## controller captured before the first reflow. A preset applied on park must not leak into the
## un-reflowed layout, and an off-by-one index would silently swap the chrome with the bar's spacer.
func _assert_chrome_home_exact(state_name: String) -> void:
	var failures: Array[String] = []
	for entry_variant in _hud._dockrow._home:
		var entry: Dictionary = entry_variant
		var cluster: Control = entry["node"]
		if cluster.get_parent() != entry["parent"]:
			failures.append("%s parent is %s, authored %s" % [
				cluster.name, cluster.get_parent().name, entry["parent"].name])
		if cluster.get_index() != int(entry["index"]):
			failures.append("%s child index is %d, authored %d" % [
				cluster.name, cluster.get_index(), int(entry["index"])])
		var anchors: Array = [cluster.anchor_left, cluster.anchor_top, cluster.anchor_right, cluster.anchor_bottom]
		if anchors != entry["anchors"]:
			failures.append("%s anchors are %s, authored %s" % [cluster.name, anchors, entry["anchors"]])
		var flags: Array = [cluster.size_flags_horizontal, cluster.size_flags_vertical]
		if flags != entry["flags"]:
			failures.append("%s size flags are %s, authored %s" % [cluster.name, flags, entry["flags"]])
	var authored_min: float = _hud._dockrow._bottom_bar_min_height
	if not is_equal_approx(_hud.bottom_bar.custom_minimum_size.y, authored_min):
		failures.append("BottomBar minimum height is %.0f, authored %.0f" % [
			_hud.bottom_bar.custom_minimum_size.y, authored_min])
	if failures.is_empty():
		print("band_panel_preview: assert OK — %s chrome restored exactly (parent/index/anchors/flags/bar minimum)" % state_name)
		return
	for failure in failures:
		push_error("band_panel_preview: %s — %s" % [state_name, failure])

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
		"populations": _stamp_band_ids([party]),
	})
	view.unit_selected.connect(_hud.show_unit_selection)
	view.handle_hex_click(tile.x, tile.y, MOUSE_BUTTON_LEFT)
	view.unit_selected.disconnect(_hud.show_unit_selection)
	var lines: Array = _hud._banddetail.expedition_summary_lines(_hud._selection._selected_unit)
	var want := "Next delivery: ~15 food in 6 turns"
	if lines.has(want):
		print("band_panel_preview: assert OK — detail panel (marker path) renders '%s'" % want)
	else:
		push_error("band_panel_preview: detail panel MISSING '%s' — marker path dropped the field. Got: %s" % [
			want, str(lines)])
	view.queue_free()

## GUARD: a projected-0 next-delivery forecast must disambiguate on the party's TARGET herd, and the
## Target row must carry the target's live position. Requires `_world_herds` already set to
## `_herd_fixtures()`. Drives the shared `DetailFormat.expedition_next_delivery_line` /
## `BandDetailLines.expedition_summary_lines`
## helpers directly (the same ones the strip, the drawer and the row tooltip use) and prints every
## rendered line. Verified to FAIL before the target-based branch (a lost target reading "no surplus").
func _assert_next_delivery_disambiguation() -> void:
	# (1) target FOUND in telemetry, projects 0 → "no surplus", Target row shows the herd's position.
	var lean := _lean_hunt_expedition_fixture()
	var lean_delivery := DetailFormat.expedition_next_delivery_line(
		lean, _hud._band_labor.expedition_target_herd(lean))
	var lean_target := _summary_target_line(lean)
	_check_line("no-surplus delivery", lean_delivery, DetailFormat.EXPEDITION_NEXT_DELIVERY_NO_SURPLUS)
	_check_line("no-surplus target", lean_target, "Target: Red Deer (68, 15)")
	# (2) target ABSENT from telemetry, projects 0 → "target herd lost".
	var lost := _lost_hunt_expedition_fixture()
	var lost_delivery := DetailFormat.expedition_next_delivery_line(
		lost, _hud._band_labor.expedition_target_herd(lost))
	_check_line("lost delivery", lost_delivery, DetailFormat.EXPEDITION_NEXT_DELIVERY_TARGET_LOST)
	# (3) projecting party (delivery > 0) → the ETA line, Target row shows the herd's position.
	var live := _hunt_expedition_fixture()
	var live_delivery := DetailFormat.expedition_next_delivery_line(
		live, _hud._band_labor.expedition_target_herd(live))
	var live_target := _summary_target_line(live)
	_check_line("projecting delivery", live_delivery, "Next delivery: ~14 food in 6 turns")
	_check_line("projecting target", live_target, "Target: Roe Deer (64, 11)")

## The `Target: …` line `BandDetailLines.expedition_summary_lines` emits for a party ("" if none).
func _summary_target_line(party: Dictionary) -> String:
	for line in _hud._banddetail.expedition_summary_lines(party):
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
	var row: HBoxContainer = _hud._bandpanel._build_party_row(_hunt_expedition_fixture())
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
## fractional (Scalar), so `HudFormat.apportion_people` distributes the remainders by largest remainder —
## which only works if the remainders survive the trip. A marker that narrowed them with `int()`
## truncates every one to zero, and the header then undercounts against the band's own size.
func _assert_people_sum_matches_size(band: Dictionary, state_name: String) -> void:
	var raw: Array[float] = [
		float(band.get("age_children", 0.0)),
		float(band.get("age_working", 0.0)),
		float(band.get("age_elders", 0.0)),
	]
	var whole := HudFormat.apportion_people(raw)
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
## `Control` wrappers (`HudWidgets.wrap_zone`) that report NO minimum size, so the measurable thing is the
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
## The SHORT band-zone tier must drop the Trade row (`BandPanelController._build_vitals_label` passes
## `compact`). Asserted rather than eyeballed: a dropped row and a row clipped off the bottom of a
## `clip_contents` zone are the SAME PICTURE, so only a text read can tell them apart. It reads the
## rendered vitals BBCode back out of the live label, which is also what makes it fail if the gate is
## removed — the row would be present in the text while still invisible in the PNG.
##
## **MATCH BARE KEYS, NOT `"Trade:"`.** `DetailFormat._split_kv` splits each `Key: value` line into a
## BBCode TABLE row and drops the `": "` separator, so the colon is never in the rendered text.
func _assert_trade_row_absent_in_short_tier() -> void:
	var vitals := _find_vitals_label(_panel)
	if vitals == null:
		push_error("band_panel_preview: short-tier trade assert found no vitals label")
		return
	var text: String = vitals.get_parsed_text()
	# The Food row proves the vitals label is actually populated — without it, "no Trade row" would
	# pass vacuously on an empty label.
	if not text.contains("Food"):
		push_error("band_panel_preview: short-tier trade assert — vitals label has no Food row (vacuous)")
		return
	if text.contains("Trade"):
		push_error("band_panel_preview: SHORT tier still renders the Trade row — the compact gate is off")
		return
	print("band_panel_preview: assert OK — SHORT tier drops the Trade row (Food row still present)")

## **THE FORAGE-TRADE REGRESSION.** A forage source ships `realized_trade_yield == 0` (the documented
## not-yet-projected sentinel) beside a real `trade_yield`, and the decoder always inserts the key — so
## a fallback spelled `has("realized_trade_yield")` silently drops every cash crop and the row reads
## `+0.00` on a band visibly selling flax. The fixture's patch pays 0.04 and its deer pays 0.04, so the
## headline must read +0.08 and the breakdown must carry BOTH categories. A PNG cannot carry this — the
## broken and the fixed frame differ by two characters — so it is asserted on both halves: the total
## proves the forage contribution landed, the Gathered row proves it landed on the right category.
func _assert_forage_trade_counted() -> void:
	var vitals := _find_vitals_label(_panel)
	if vitals == null:
		push_error("band_panel_preview: forage-trade assert found no vitals label")
		return
	var text: String = vitals.get_parsed_text()
	if not text.contains("+0.08"):
		push_error("band_panel_preview: Trade must read +0.08 (forage 0.04 + hunt 0.04) — got: %s" % text)
		return
	# The band-local STOCK, read off `stores.trade_goods` the way the Food row reads the larder.
	# Matched as the VALUE cell's own run (`12 · +0.08`) rather than `Trade 12`: the KV formatter splits
	# the row into table cells and the key cell carries the disclosure caret, so the two are never
	# adjacent in the parsed text.
	if not text.contains("12 · +0.08"):
		push_error("band_panel_preview: Trade row does not carry the band's stock of 12 — got: %s" % text)
		return
	var rows := _disclosure_rows(BAND_FIXTURE_DISCLOSURE_TRADE)
	var joined := "\n".join(rows)
	if not joined.contains(DetailFormat.FOOD_LABEL_GATHERED):
		push_error("band_panel_preview: the Trade breakdown has no Gathered row — the forage source's trade was dropped (rows: %s)" % joined)
		return
	if not joined.contains(DetailFormat.FOOD_LABEL_HUNTED):
		push_error("band_panel_preview: the Trade breakdown has no Hunted row (rows: %s)" % joined)
		return
	print("band_panel_preview: assert OK — a forage source's trade counts (Trade +0.08, Gathered + Hunted)")

## The breakdown rows stashed for a disclosure key, read back the way the popover reads them.
func _disclosure_rows(key: String) -> Array[String]:
	var payloads: Dictionary = _hud._disclosures._breakdown_payloads
	var rows: Array[String] = []
	var stashed: Variant = payloads.get(key, [])
	if stashed is Array:
		for row in (stashed as Array):
			rows.append(String(row))
	return rows

## The zero case: the Trade row must be PRESENT and read a zero rate. Asserted because "absent" and
## "present but zero" are one glance apart in a PNG and the difference is the whole playtest report.
func _assert_trade_row_reads_zero() -> void:
	var vitals := _find_vitals_label(_panel)
	if vitals == null:
		push_error("band_panel_preview: zero-trade assert found no vitals label")
		return
	var text: String = vitals.get_parsed_text()
	if not text.contains("Trade"):
		push_error("band_panel_preview: a band earning no trade dropped its Trade row — it must read zero")
		return
	# `format_yield` writes a signed magnitude, so a zero rate renders "+0.00". Matching the NUMBER
	# rather than the row keeps this from passing on an earning band that merely has a Trade row.
	if not text.contains("+0.00"):
		push_error("band_panel_preview: zero-trade band's Trade row does not read +0.00 — got: %s" % text)
		return
	print("band_panel_preview: assert OK — a band earning no trade still shows Trade, reading +0.00")

func _find_vitals_label(node: Node) -> RichTextLabel:
	if node is RichTextLabel and (node as RichTextLabel).get_parsed_text().contains("Morale"):
		return node as RichTextLabel
	for child in node.get_children():
		var found := _find_vitals_label(child)
		if found != null:
			return found
	return null

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
## The forage jump must leave the LAND as the lit subject, even on a hex whose roster also holds a
## band (the auto-pick's preference, and what it used to hand back instead).
func _assert_forage_jump_names_land() -> void:
	var subjects: Array = []
	_hud._bandpanel.roster_occupant_selected.connect(
		func(kind: String, _id: Variant) -> void: subjects.append(kind), CONNECT_ONE_SHOT)
	_hud._bandpanel.focus_labor_source(71, 18)
	_assert_band_panel("forage jump — the row names the LAND, not the hex's auto-picked occupant",
		subjects == [HudSelectionState.SUBJECT_LAND])
	_assert_band_panel("forage jump — the land is the lit subject afterwards",
		_hud._selection.subject() == HudSelectionState.SUBJECT_LAND)

## Pass/fail reporting for the rung-ready assertions, in this harness's `push_error` idiom so a
## regression fails loudly in the run log rather than waiting to be noticed in a thumbnail.
func _assert_band_panel(label: String, ok: bool) -> void:
	if ok:
		print("band_panel_preview: PASS — ", label)
	else:
		push_error("band_panel_preview: FAIL — %s" % label)

## The rung-ready board fixture: three sources, exactly one of each answer the mark can give.
func _ready_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 940
	band["id"] = "Band 12"
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 3, "workers_needed": 3, "policy": "sustain",
			"target_x": 71, "target_y": 18, "actual_yield": 0.48, "sustainable_yield": 0.48},
		{"kind": "hunt", "workers": 2, "workers_needed": 2, "policy": "sustain",
			"fauna_id": "ready_tamed", "target_x": 70, "target_y": 17,
			"actual_yield": 0.30, "sustainable_yield": 0.30},
		{"kind": "hunt", "workers": 2, "workers_needed": 2, "policy": "sustain",
			"fauna_id": "ready_never", "target_x": 69, "target_y": 19,
			"actual_yield": 0.20, "sustainable_yield": 0.20},
	]
	return band

## A TENDED patch on willing ground → its next rung is Sow.
func _ready_patch_fixtures() -> Array:
	return [{
		"x": 71, "y": 18, "ecology_phase": "thriving",
		"is_cultivated": true, "is_field": false, "sow_site_refusal": "",
		"composition": [{"species": "wild_wheat", "display_name": "Wild Wheat",
			"share": 1.0, "can_cultivate": true, "can_sow": true}],
	}]

## One fully tamed "pen"-ceiling herd (→ Corral) and one "wild"-ceiling herd that can never climb —
## the control that proves the mark is selective rather than decorative.
func _ready_herd_fixtures() -> Array:
	return [
		{"id": "ready_tamed", "species": "Aurochs", "x": 70, "y": 17,
			"population": 210, "ecology_phase": "thriving", "huntable": true,
			"domestication": 1.0, "husbandry_ceiling": "pen", "per_worker_yield": 0.15,
			"hunt_policy_ceilings": {"sustain": 0.30, "surplus": 0.90, "deplete": 1.40,
				"eradicate": 2.00, "corral": 0.70}},
		{"id": "ready_never", "species": "Roe Deer", "x": 69, "y": 19,
			"population": 90, "ecology_phase": "thriving", "huntable": true,
			"domestication": 0.0, "husbandry_ceiling": "wild", "per_worker_yield": 0.10,
			"hunt_policy_ceilings": {"sustain": 0.20, "surplus": 0.60, "deplete": 0.90,
				"eradicate": 1.40}},
	]

## The mark is SELECTIVE — two of the three rows offer a rung, the wild-ceiling herd none. Asserted
## rather than eyeballed: three chevrons and one chevron look similar in a thumbnail, and "the mark
## renders" is a much weaker claim than "the mark renders where it should and nowhere else".
func _assert_ready_marks() -> void:
	var models: Array = _hud._bandpanel._work_source_models(_hud._band_labor.panel_band(), 0)
	var ready: Array = models.filter(func(m): return String(m["ready_policy"]) != "")
	_assert_band_panel("ready — exactly two of the three worked sources offer a rung", ready.size() == 2)
	var by_policy: Array = ready.map(func(m): return String(m["ready_policy"]))
	by_policy.sort()
	_assert_band_panel("ready — the tended patch offers Sow and the tamed herd Corral",
		by_policy == ["corral", "sow"])
	_assert_band_panel("ready — the wild-ceiling herd offers nothing",
		models.filter(func(m): return String(m["herd_id"]) == "ready_never" \
			and String(m["ready_policy"]) == "").size() == 1)

## The ready chip narrows the board to the offering rows and nothing else.
func _assert_ready_filter_narrows() -> void:
	var models: Array = _hud._bandpanel._work_source_models(_hud._band_labor.panel_band(), 0)
	var shown: Array = _hud._bandpanel._filter_work_models(models)
	_assert_band_panel("ready filter — the board narrows to the two offering rows", shown.size() == 2)
	_assert_band_panel("ready filter — every shown row actually offers a rung",
		shown.filter(func(m): return String(m["ready_policy"]) == "").is_empty())

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
	var penned := {
		"id": INVESTMENT_ROW_HERD_ID, "species": "Aurochs", "x": 70, "y": 17,
		"population": 210, "ecology_phase": "thriving", "huntable": true,
		"domestication": 1.0, "corral_progress": 0.4,
		"per_worker_yield": 0.25,
		"hunt_policy_ceilings": {
			"sustain": 0.40, "surplus": 1.10, "deplete": 1.60, "eradicate": 2.40,
			"tame": 0.20, INVESTMENT_ROW_POLICY: 0.75,
		},
	}
	_set_managed_herders(penned, INVESTMENT_ROW_HERDERS_NEEDED)
	return [
		penned,
		{
			"id": EXTRACTIVE_ROW_HERD_ID, "species": "Red Deer", "x": 69, "y": 19,
			"population": 90, "ecology_phase": "thriving", "huntable": true,
			"per_worker_yield": 0.10,
			"hunt_policy_ceilings": {
				"sustain": 0.20, "surplus": 0.60, "deplete": 0.90, "eradicate": 1.40,
			},
		},
	]

## A band keeping an UNDER-CONTAINED pen: one keeper works the Corralled herd, but it needs 4 herders.
## The work board must flag its Hunt row (fauna neglect-escape arc). `herded_fraction` is left STALE at
## 1.0 to prove the flag derives from the ACTUAL staffed count (2 < needed 4), not the lagging fraction.
func _under_herded_work_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 918
	band["id"] = "Band 18"
	band["labor_assignments"] = [
		{"kind": "hunt", "workers": 2, "workers_needed": UNDER_HERDED_WORK_HERDERS_NEEDED,
			"policy": "corral",
			"fauna_id": UNDER_HERDED_WORK_HERD_ID, "target_x": 70, "target_y": 17,
			"actual_yield": 5.40, "sustainable_yield": 5.40, "overdraws": false},
		{"kind": "scout", "workers": 1},
	]
	return band

## The Corralled herd that row works: needs 4 herders, `herded_fraction` a stale 1.0 (the OLD code
## would have read it "fully herded"), so only the actual staffed count exposes the shed.
func _under_herded_work_herd_fixtures() -> Array:
	var penned := {
		"id": UNDER_HERDED_WORK_HERD_ID, "species": "Aurochs", "x": 70, "y": 17,
		"population": 210, "ecology_phase": "thriving", "huntable": true,
		"domestication": 1.0, "corralled": true, "herded_fraction": 1.0,
		"per_worker_yield": 5.40,
		"hunt_policy_ceilings": {
			"sustain": 5.40, "surplus": 6.0, "deplete": 7.0, "eradicate": 8.0,
			"tame": 5.40, "corral": 5.40,
		},
	}
	_set_managed_herders(penned, UNDER_HERDED_WORK_HERDERS_NEEDED)
	return [penned]

## The band working that Wild Fowl: 2 herders on it (below the crew of 3) and idle workers free, on an
## EXTRACTIVE rung so `herd_crew_floor` reads the ownership-gated `herders_needed` — the field the row's
## own under-herded ⚠ gates on, which is the whole point of the frame.
func _herder_floor_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 919
	band["id"] = "Band 19"
	band["labor_assignments"] = [
		{"kind": "hunt", "workers": HERDER_FLOOR_STAFFED,
			"workers_needed": HERDER_FLOOR_HERDERS_NEEDED, "policy": "sustain",
			"fauna_id": HERDER_FLOOR_HERD_ID, "target_x": 70, "target_y": 17,
			"actual_yield": HERDER_FLOOR_SUSTAIN_CEILING,
			"sustainable_yield": HERDER_FLOOR_SUSTAIN_CEILING, "overdraws": false},
	]
	return band

## The herd itself — TAMED but unpenned (the ◎ pastoral rung), so it is owned and really does owe the
## keepers its `herders_needed` names, while its take stays small enough that the take-side max-useful
## (2) lands BELOW that crew (3).
func _herder_floor_herd_fixtures() -> Array:
	var fowl := {
		"id": HERDER_FLOOR_HERD_ID, "species": "Wild Fowl", "x": 70, "y": 17,
		"population": 60, "ecology_phase": "thriving", "huntable": true,
		"domestication": 1.0, "corralled": false,
		"per_worker_yield": HERDER_FLOOR_PER_WORKER,
		"hunt_policy_ceilings": {
			"sustain": HERDER_FLOOR_SUSTAIN_CEILING, "surplus": 0.14, "deplete": 0.20,
			"eradicate": 0.30, "tame": 0.05, "corral": 0.05,
		},
	}
	_set_managed_herders(fowl, HERDER_FLOOR_HERDERS_NEEDED)
	return [fowl]

## THE INVARIANT AS A TEST: one row cannot flag a problem and disable its own remedy, and the two cap
## twins cannot gate differently.
##
## Three claims, and the middle one is what makes the other two non-vacuous:
##   1. the row still carries the under-herded ⚠ — the board KNOWS the herd is short a keeper;
##   2. its `+` is ENABLED at the staffed 2, so the remedy the ⚠ demands is reachable;
##   3. `source_worker_cap_state` (the worked row) and `_forecast_worker_cap` (the compose stepper)
##      answer with the SAME ceiling — the crew of 3, not the take-side 2 — which is the promise the
##      two twins make by sitting beside each other.
func _assert_herder_floor_row(herd_id: String) -> void:
	var band: Dictionary = _hud._band_labor._panel_band
	var idle := _hud._band_labor.effective_idle(band)
	if idle <= 0:
		push_error("band_panel_preview: herder-floor frame needs idle workers to gate on the source")
		return
	var found := false
	for model in _hud._bandpanel._work_source_models(band, idle):
		var m: Dictionary = model
		if String(m.get("herd_id", "")) != herd_id:
			continue
		found = true
		if not bool(m.get("under_herded", false)):
			push_error("band_panel_preview: expected under_herded on the Hunt row for %s" % herd_id)
		elif not bool(m.get("can_add", false)):
			push_error(("band_panel_preview: the under-herded row for %s disables its own `+` at %d "
				+ "workers with %d idle — the board flags the shed and refuses the fix")
				% [herd_id, int(m.get("workers", 0)), idle])
		else:
			print("band_panel_preview: assert OK — the under-herded row keeps its `+` live (crew %d > take-useful %d)"
				% [HERDER_FLOOR_HERDERS_NEEDED, HERDER_FLOOR_TAKE_USEFUL])
	if not found:
		push_error("band_panel_preview: no Hunt work row for %s" % herd_id)
		return
	# The twins, asked the same question about the same herd+policy. `_forecast_worker_cap` is given an
	# assignable count above both candidate ceilings so its answer IS the usefulness ceiling and not a
	# labor bound; `source_worker_cap_state` is probed on either side of that ceiling.
	var herd := _hud._band_labor.find_world_herd(herd_id)
	var forecast := SourceForecast.forecast_inputs(herd, SourceForecast.SOURCE_KIND_HERD,
		HudComposeVocab.BARE_FORECAST_PREFIX, "sustain")
	var floor_workers := SourceForecast.herd_crew_floor(herd, forecast)
	var compose_cap := int(_hud._drawercompose._forecast_worker_cap(
		forecast, HERDER_FLOOR_HERDERS_NEEDED + 1, floor_workers)["cap"])
	var row_below: bool = bool(SourceForecast.source_worker_cap_state(
		forecast, HERDER_FLOOR_HERDERS_NEEDED - 1, 1, floor_workers)["can_add"])
	var row_at: bool = bool(SourceForecast.source_worker_cap_state(
		forecast, HERDER_FLOOR_HERDERS_NEEDED, 1, floor_workers)["can_add"])
	if compose_cap != HERDER_FLOOR_HERDERS_NEEDED:
		push_error("band_panel_preview: the compose stepper caps at %d, not the crew of %d"
			% [compose_cap, HERDER_FLOOR_HERDERS_NEEDED])
	elif not (row_below and not row_at):
		push_error(("band_panel_preview: the worked row does not gate at the crew of %d "
			+ "(can_add below=%s, at=%s)") % [HERDER_FLOOR_HERDERS_NEEDED, row_below, row_at])
	else:
		print("band_panel_preview: assert OK — both cap twins gate at the crew of %d, above the take-useful %d"
			% [HERDER_FLOOR_HERDERS_NEEDED, HERDER_FLOOR_TAKE_USEFUL])

## The under-contained Hunt row must carry the shed flag: the ⚠ mark, the drifting-off note, and the
## `under_herded` model flag the row + inspector tint from.
func _assert_under_herded_work_row(herd_id: String) -> void:
	var band: Dictionary = _hud._band_labor._panel_band
	var found := false
	for model in _hud._bandpanel._work_source_models(band, 0):
		var m: Dictionary = model
		if String(m.get("herd_id", "")) != herd_id:
			continue
		found = true
		if not bool(m.get("under_herded", false)):
			push_error("band_panel_preview: expected under_herded on the Hunt row for %s" % herd_id)
		elif not String(m.get("marks", "")).contains(HudComposeVocab.OVERHUNT_FLAG):
			push_error("band_panel_preview: expected the ⚠ mark on the under-herded row for %s" % herd_id)
		elif not String(m.get("note", "")).contains("drifting off"):
			push_error("band_panel_preview: expected the drifting-off note on the under-herded row for %s" % herd_id)
		else:
			print("band_panel_preview: assert OK — under-herded Hunt row flags the shed (⚠ + note)")
	if not found:
		push_error("band_panel_preview: no Hunt work row for %s" % herd_id)

# ---- THE SOURCE-RUNG BOARD ------------------------------------------------------------------------
#
# `update_forage_patches` was called EXACTLY ONCE in this whole harness (the per-source-cap state), so
# `forage_patch_lookup()` was empty for every Work-tab frame and no rung could ever have rendered here.
# These fixtures close that: the rung frame below, and rung-marked patches under the paged board so the
# marks are also seen at real density and in the narrow-shell threshold frames.

## A band working one source per rung — three forage rows (wild / Tended / Field) and two hunt rows
## (pastoral / penned). Every row is staffed and unremarkable otherwise, so the ONLY thing that differs
## down the board is the rung mark.
func _rung_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 922
	band["id"] = "Band 22"
	band["idle_workers"] = 6
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 2, "workers_needed": 2, "policy": "sustain",
			"target_x": RUNG_WILD_TILE.x, "target_y": RUNG_WILD_TILE.y,
			"actual_yield": 0.61, "sustainable_yield": 0.61},
		{"kind": "forage", "workers": 2, "workers_needed": 2, "policy": "sustain",
			"target_x": RUNG_TENDED_TILE.x, "target_y": RUNG_TENDED_TILE.y,
			"actual_yield": 0.97, "sustainable_yield": 0.97},
		{"kind": "forage", "workers": 2, "workers_needed": 2, "policy": "sustain",
			"target_x": RUNG_FIELD_TILE.x, "target_y": RUNG_FIELD_TILE.y,
			"actual_yield": 1.94, "sustainable_yield": 1.94},
		{"kind": "hunt", "workers": 2, "workers_needed": 2, "policy": "sustain",
			"fauna_id": RUNG_PASTORAL_HERD_ID, "target_x": 70, "target_y": 19,
			"actual_yield": 1.20, "sustainable_yield": 1.20},
		{"kind": "hunt", "workers": RUNG_PENNED_HERDERS, "workers_needed": RUNG_PENNED_HERDERS,
			"policy": "sustain",
			"fauna_id": RUNG_PENNED_HERD_ID, "target_x": 69, "target_y": 20,
			"actual_yield": 5.40, "sustainable_yield": 5.40},
	]
	return band

## The three patches those forage rows work. Deliberately RUNG FIELDS ONLY — no `per_worker_yield` /
## `ceiling_*` — so `SourceForecast.max_useful_workers` stays UNBOUNDED and the steppers gate exactly as
## they did before patches were pushed here at all. This frame is about the mark, not the cap.
func _rung_patch_fixtures() -> Array:
	return [
		{"x": RUNG_WILD_TILE.x, "y": RUNG_WILD_TILE.y, "is_cultivated": false, "is_field": false},
		{"x": RUNG_TENDED_TILE.x, "y": RUNG_TENDED_TILE.y, "is_cultivated": true, "is_field": false,
			"committed_display_name": RUNG_TENDED_CROP},
		# A Field is ALSO cultivated — that is why the row builder tests `is_field` FIRST, and why this
		# fixture sets both rather than the field flag alone.
		{"x": RUNG_FIELD_TILE.x, "y": RUNG_FIELD_TILE.y, "is_cultivated": true, "is_field": true,
			"committed_display_name": RUNG_FIELD_CROP},
	]

## The two herds those hunt rows work: one TAMED but unpenned (pastoral), one CORRALLED. The penned one
## is fully staffed so the frame carries no ⚠ competing with the rung mark for the eye.
func _rung_herd_fixtures() -> Array:
	var penned := {
		"id": RUNG_PENNED_HERD_ID, "species": "Aurochs", "x": 69, "y": 20,
		"population": 180, "ecology_phase": "thriving", "huntable": true,
		"domestication": 1.0, "corralled": true,
		"hunt_policy_ceilings": {"sustain": 5.40},
	}
	_set_managed_herders(penned, RUNG_PENNED_HERDERS)
	return [
		{
			"id": RUNG_PASTORAL_HERD_ID, "species": "Wild Boar", "x": 70, "y": 19,
			"population": 140, "ecology_phase": "thriving", "huntable": true,
			# Tamed but NOT corralled — the rung the animal ladder had no glyph of its own for.
			"domestication": 1.0, "corralled": false,
			"hunt_policy_ceilings": {"sustain": 1.20},
		},
		penned,
	]

## Forage modules for the rung tiles, so each Forage row still resolves its map glyph and the rung mark
## is read BESIDE a source glyph rather than in isolation.
func _rung_forage_modules() -> Array:
	var modules: Array = []
	for tile in [RUNG_WILD_TILE, RUNG_TENDED_TILE, RUNG_FIELD_TILE]:
		modules.append({"x": tile.x, "y": tile.y, "module": "savanna_grassland", "kind": "gather"})
	return modules

## Patches for the PAGED board, so the rung marks are also seen at real board density and in the
## narrow-shell threshold frames. Carries `_cap_demo_patch_fixtures()` forward because
## `update_forage_patches` CLEARS the lookup: dropping (71,18) would re-enable a `+` the
## `band_panel_work_trade_*` frames render disabled, moving a frame this change has nothing to do with.
## Rung fields only, for the same cap-neutrality reason as `_rung_patch_fixtures`.
func _many_source_patch_fixtures() -> Array:
	var patches := _cap_demo_patch_fixtures()
	for i in range(MANY_SOURCE_COUNT):
		var patch := {"x": MANY_SOURCE_ORIGIN_X + i, "y": MANY_SOURCE_ORIGIN_Y}
		if i % RUNG_MANY_FIELD_STRIDE == 3:
			patch["is_cultivated"] = true
			patch["is_field"] = true
			patch["committed_display_name"] = RUNG_FIELD_CROP
		elif i % RUNG_MANY_TENDED_STRIDE == 1:
			patch["is_cultivated"] = true
			patch["committed_display_name"] = RUNG_TENDED_CROP
		patches.append(patch)
	return patches

## Every row on the rung board must carry the mark its rung wears — and, decisively, the WILD row must
## carry NONE. Asserting only the marked rows would pass a build that stamped a glyph on everything.
func _assert_work_row_rungs() -> void:
	var expected := {
		"forage:%d,%d" % [RUNG_WILD_TILE.x, RUNG_WILD_TILE.y]: "",
		"forage:%d,%d" % [RUNG_TENDED_TILE.x, RUNG_TENDED_TILE.y]: DetailFormat.CULTIVATION_GLYPH,
		"forage:%d,%d" % [RUNG_FIELD_TILE.x, RUNG_FIELD_TILE.y]: DetailFormat.field_glyph(),
		"hunt:%s" % RUNG_PASTORAL_HERD_ID: DetailFormat.pastoral_glyph(),
		"hunt:%s" % RUNG_PENNED_HERD_ID: DetailFormat.CORRAL_GLYPH,
	}
	var seen := {}
	for model in _hud._bandpanel._work_source_models(_hud._band_labor._panel_band, 0):
		var m: Dictionary = model
		var key := String(m.get("key", ""))
		if not expected.has(key):
			continue
		seen[key] = true
		var glyph := String(m.get("rung_glyph", ""))
		if glyph != String(expected[key]):
			push_error("band_panel_preview: %s expected rung glyph '%s' but got '%s'" % [
				key, expected[key], glyph])
		elif glyph != "" and String(m.get("rung_tooltip", "")) == "":
			push_error("band_panel_preview: %s wears a rung glyph with no tooltip naming the rung" % key)
	for key in expected:
		if not seen.has(key):
			push_error("band_panel_preview: no work row for %s on the rung board" % key)
	if seen.size() == expected.size():
		print("band_panel_preview: assert OK — %d work rows wear their standing rung (wild bare)" % seen.size())

## The rung mark's TOOLTIP has to actually be reachable, and its slot must not eat the row's click —
## two SILENT failures a rendered frame cannot show. A `Label` defaults to `MOUSE_FILTER_IGNORE`, which
## makes `tooltip_text` a no-op (this HUD has shipped six such tooltips nobody ever saw), while the
## obvious fix, `HudWidgets.set_label_tooltip`, sets `STOP` — which would swallow the press that opens
## the inspector strip. Only `PASS` satisfies both, so that is what is asserted.
##
## The marks are found by `HudWorkVocab.WORK_ROW_RUNG_META`, NEVER by their glyph: `savanna_grassland`'s
## SITE icon is also 🌾, so a text match walks straight into the row's source-icon Label — which this
## assertion did, and failed on, before the meta existed.
func _assert_rung_labels_are_hoverable() -> void:
	var labels: Array = []
	_collect_rung_labels(_panel, labels)
	var marked := 0
	for label_variant in labels:
		var label: Label = label_variant
		if String(label.get_meta(HudWorkVocab.WORK_ROW_RUNG_META)) == "":
			continue   # a WILD row's reserved-but-empty slot — nothing to hover
		marked += 1
		if label.tooltip_text == "":
			push_error("band_panel_preview: rung mark '%s' carries no tooltip" % label.text)
			return
		if label.mouse_filter != Control.MOUSE_FILTER_PASS:
			push_error("band_panel_preview: rung mark '%s' has mouse_filter %d — PASS is the only value that both shows the tooltip and lets the row's click through" % [
				label.text, label.mouse_filter])
			return
	if marked == 0:
		push_error("band_panel_preview: no rung mark rendered in the panel (%d slots) — the mark is missing" % labels.size())
	else:
		print("band_panel_preview: assert OK — %d rung marks are hoverable (tooltip + PASS), %d wild slots bare" % [
			marked, labels.size() - marked])

func _collect_rung_labels(node: Node, out: Array) -> void:
	if node is Label and (node as Label).has_meta(HudWorkVocab.WORK_ROW_RUNG_META):
		out.append(node)
	for child in node.get_children():
		_collect_rung_labels(child, out)

## Open the work inspector on the row standing on `policy`, with its policy picker EXPANDED, and
## repage so the picker actually renders. `_work_policy_open` is otherwise never true in either
## harness, which is why this control had zero frame coverage.
## Open the work inspector on the row working a NAMED herd — the trade-row frames need a specific
## source (the wolf), not "the first row", which is the forage patch.
func _open_work_inspector_for_herd(herd_id: String) -> void:
	var band: Dictionary = _hud._band_labor._panel_band
	for model_variant in _hud._bandpanel._work_source_models(band, 0):
		var model: Dictionary = model_variant
		if String(model.get("herd_id", "")) != herd_id:
			continue
		_hud._bandpanel._toggle_work_inspector(String(model.get("key", "")))
		return
	push_error("band_panel_preview: no work row hunting '%s' — fixture drifted?" % herd_id)

func _open_work_policy_picker(policy: String) -> void:
	var band: Dictionary = _hud._band_labor._panel_band
	for model_variant in _hud._bandpanel._work_source_models(band, 0):
		var model: Dictionary = model_variant
		if String(model.get("policy", "")) != policy:
			continue
		_hud._bandpanel._work_open_key = String(model.get("key", ""))
		_hud._bandpanel._work_policy_open = true
		_hud._bandpanel._repage_work_zone()
		return
	push_error("band_panel_preview: no work row standing on '%s' — fixture drifted?" % policy)

## The open inspector strip: the work zone host's PanelContainer (the board and chips are boxes).
func _work_inspector_strip() -> PanelContainer:
	var host: VBoxContainer = _hud._bandpanel._work_zone_host
	if host == null or not is_instance_valid(host):
		return null
	for child in host.get_children():
		if child is PanelContainer:
			return child
	return null

## The inspector picker's rung buttons, keyed by policy — found by the `HudWidgets.POLICY_RUNG_META`
## the picker stamps on each one, NEVER by matching its face. The face is presentation and has already
## changed twice (glyph + metric → glyph + name over metric → that pair as child Labels at two sizes,
## which left the Button's own `text` empty), and each time a text match here would have quietly
## returned nothing and passed every assertion vacuously. It also has to RECURSE now: a rung is a cell
## (a MarginContainer holding the button and the label stack), so the grid's children are no longer the
## buttons themselves.
func _picker_rung_buttons() -> Dictionary:
	var buttons := {}
	var strip := _work_inspector_strip()
	if strip == null:
		return buttons
	var grid := _find_first_grid(strip)
	if grid == null:
		return buttons
	_collect_rung_buttons(grid, buttons)
	return buttons

func _collect_rung_buttons(node: Node, out: Dictionary) -> void:
	if node is Button and (node as Button).has_meta(HudWidgets.POLICY_RUNG_META):
		out[String((node as Button).get_meta(HudWidgets.POLICY_RUNG_META))] = node
	for child in node.get_children():
		_collect_rung_buttons(child, out)

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
	var want := HudWorkVocab.WORK_INSPECT_STANDING_INVESTMENT_FORMAT % HudFormat.policy_face(policy)
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

## Settle the window ONCE, in `_ready`, before any state renders — and take the maximize DELIBERATELY
## on the way, which is what closes the last of the drift.
##
## `project.godot` opens the window MAXIMIZED and macOS applies that asynchronously, so whether a run
## ever passed through the monitor-sized window was a COIN FLIP — and it is a coin flip the pixels
## remember: `window/stretch` is `canvas_items` with an `expand` aspect, so the stretch scale swings
## across a maximize and the rasterized-glyph coverage state does not come back bit-identical. It is
## also a LAYOUT flip, not merely a pixel one — a run that loses the race renders the "bottom dock"
## states at the monitor's width, i.e. against the ultrawide content cap rather than the wide shell
## the state exists to judge (one measured run drew `band_panel_left` at 5120×1410). Dodging the
## maximize is not available — `ui_preview` measured a late one landing mid-run after 30 stable frames
## — so ASK for it, then undo it: every run then takes the same path.
func _stabilize_canvas() -> void:
	get_window().mode = Window.MODE_MAXIMIZED
	for _i in range(CANVAS_STABLE_MAX_FRAMES):
		if get_window().size != PREVIEW_SIZE:
			break
		await get_tree().process_frame
	# Restore and HOLD: the maximize is re-applied asynchronously, so "the right size once" is not the
	# same as "it stays" — wait for CANVAS_STABLE_FRAMES consecutive good frames. After this every
	# `_pin_window` at the same size returns without awaiting, so each state gets the same number of
	# layout passes in every run.
	var stable := 0
	for _i in range(CANVAS_STABLE_MAX_FRAMES):
		if get_window().size == PREVIEW_SIZE and get_window().mode == Window.MODE_WINDOWED:
			stable += 1
			if stable >= CANVAS_STABLE_FRAMES:
				return
		else:
			stable = 0
			await _pin_window(PREVIEW_SIZE)
		await get_tree().process_frame
	push_error("band_panel_preview: the window never held the pinned %s canvas — frames will drift" % PREVIEW_SIZE)

## The viewport image, GUARANTEED to be at the size this state pinned (or an integer HiDPI multiple of
## it). The WM's deferred maximize can resize the render target between a settle and a capture, so
## re-pin and re-draw until the geometry is the pinned one, then give up loudly rather than save a
## frame that silently renders the panel at a width the state never asked for.
func _capture(name: String) -> Image:
	for _i in range(WINDOW_PIN_MAX_FRAMES):
		var image := get_viewport().get_texture().get_image()
		if image == null:
			# No image to read back — the dummy renderer (i.e. someone ran this with `--headless`,
			# which selects it on Godot 4.5+). Capture is impossible, but the compile/scene gate and
			# every assertion still ran. Run WITHOUT `--headless` for PNGs.
			push_warning("band_panel_preview: null image (dummy renderer?) — skipping %s.png; run without --headless" % name)
			return null
		var w := image.get_width()
		var h := image.get_height()
		if w % _pinned_size.x == 0 and h % _pinned_size.y == 0 \
				and w / _pinned_size.x == h / _pinned_size.y:
			return image
		await _pin_window(_pinned_size)
		await get_tree().process_frame
		RenderingServer.force_draw()
		await get_tree().process_frame
	push_error("band_panel_preview: viewport never came back to the pinned %s canvas for %s" % [_pinned_size, name])
	return null

func _settle() -> void:
	# Re-assert the window EVERY state: the WM's maximize lands asynchronously and can arrive between
	# two states, rendering them at different resolutions (blend_probe hit the same thing).
	await _pin_window(_pinned_size)
	await get_tree().process_frame
	RenderingServer.force_draw()
	await get_tree().process_frame

func _save(name: String) -> void:
	_current_state = name
	# Check the herd fixtures RENDERING IN THIS FRAME, so a half-set field pair fails against the state
	# it silently mis-renders rather than against nothing at all.
	_guard_frame_herd_fields(name)
	var image: Image = await _capture(name)
	if image == null:
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
	var meta := HudDisclosureVocab.BREAKDOWN_TOGGLE_META_PREFIX + key
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


# ---- the herd herders_needed FIELD-PAIR guard ---------------------------------------------------
# The sim exports TWO herder counts per herd and the client reads DIFFERENT ones by rung, so a fixture
# that sets only one is a silent lie rather than an error:
#   • `herders_needed` — OWNERSHIP-GATED (`fauna::herd_herders_needed`): 0 unless the herd is
#     corralled or owned. The extractive rungs' field, and what the drawer's "Herders A / N" row reads.
#   • `herders_needed_if_managed` — ownership-INDEPENDENT (`fauna::would_be_herders_needed`): the crew
#     the herd WOULD owe, 0 only for a species that can never be tamed. `DrawerComposeController`'s
#     `_forecast_worker_cap` floor reads THIS one for the INVESTMENT rungs (Tame / Corral).
# Both this harness's managed herds set only the first, so any state that opened a compose sheet on
# them would floor the investment cap at 0 — no error, just a wrong number on a frame whose whole job
# is to be read. Half-setting the pair is not catchable by eye, so it is caught here.
#
# THE INVARIANT, from the sim, not from guesswork: `would_be_herders_needed` is identical to
# `herd_herders_needed` except its gate, so the two agree on every herd EXCEPT a not-yet-owned tameable
# one (gated 0, would-be crew real). A herd whose gated count is `> 0` is by definition managed
# (corralled or owned) and therefore tameable, so the ungated field takes the same branch:
#     herders_needed > 0  ⇒  herders_needed_if_managed == herders_needed
# and, in general, `herders_needed_if_managed >= herders_needed`.
const HERDERS_NEEDED_KEY := "herders_needed"
const HERDERS_NEEDED_IF_MANAGED_KEY := "herders_needed_if_managed"
## Deep-scan bound. Fixtures are trees, but a bound turns a future self-referencing one into a stop
## rather than an infinite walk.
const HERD_SCAN_MAX_DEPTH := 8

var _herd_pair_scans := 0
var _herd_pair_violations := 0

## Set BOTH herder counts on a MANAGED herd fixture. The sim exports them EQUAL there (see the
## invariant above), and setting them one at a time is precisely the mistake the guard exists to
## catch — so managed fixtures set them together, through this. A still-WILD but tameable herd is the
## one case where they differ; this harness has none, and one added later writes them by hand.
func _set_managed_herders(fixture: Dictionary, needed: int) -> void:
	fixture[HERDERS_NEEDED_KEY] = needed
	fixture[HERDERS_NEEDED_IF_MANAGED_KEY] = needed

## Walk everything reachable from `subject` and check the pair on every dict that carries either half.
## Deliberately a SCAN and not a per-fixture assertion: a guard you have to remember to call for each
## new fixture is the same failure mode as remembering to set the second field.
func _guard_herd_fields(subject: Variant, where: String, depth: int = 0) -> void:
	if depth > HERD_SCAN_MAX_DEPTH:
		return
	if subject is Array:
		for item in (subject as Array):
			_guard_herd_fields(item, where, depth + 1)
		return
	if not (subject is Dictionary):
		return
	var dict: Dictionary = subject
	if dict.has(HERDERS_NEEDED_KEY) or dict.has(HERDERS_NEEDED_IF_MANAGED_KEY):
		_herd_pair_scans += 1
		var needed := int(dict.get(HERDERS_NEEDED_KEY, 0))
		var if_managed := int(dict.get(HERDERS_NEEDED_IF_MANAGED_KEY, 0))
		if if_managed < needed:
			_herd_pair_violations += 1
			push_error(("band_panel_preview: %s — herd \"%s\" declares %s %d but %s %d. The would-be "
				+ "crew can never be SMALLER than the ownership-gated one, and on a herd with herders "
				+ "(i.e. a managed one) the sim exports them EQUAL — the investment rungs' worker cap "
				+ "floors on the second field, so half-setting the pair silently caps the crew at the "
				+ "take-side count. Set both through _set_managed_herders.") % [where,
				String(dict.get("id", "?")), HERDERS_NEEDED_KEY, needed,
				HERDERS_NEEDED_IF_MANAGED_KEY, if_managed])
		elif needed > 0 and if_managed != needed:
			# The OTHER half of the invariant, and the one a `>=` test lets through. The gate is the
			# ONLY difference between the two sim functions, so a NON-ZERO gated count already says the
			# herd passed the gate — it is corralled or owned — and the would-be crew is then computed
			# from the same species and headcount by the same arithmetic. A bigger would-be crew is not
			# a conservative fixture, it is an impossible herd: it claims managing this herd would cost
			# MORE than managing it already does.
			_herd_pair_violations += 1
			push_error(("band_panel_preview: %s — herd \"%s\" declares %s %d and %s %d. Once %s is "
				+ "above zero the herd IS managed, and the would-be crew is the SAME crew — the sim's "
				+ "two functions differ only by the ownership gate this herd has already passed, so "
				+ "they must be EQUAL here. Set both through _set_managed_herders; only a still-WILD "
				+ "tameable herd may carry a larger would-be crew, and its gated count is 0.")
				% [where, String(dict.get("id", "?")), HERDERS_NEEDED_KEY, needed,
				HERDERS_NEEDED_IF_MANAGED_KEY, if_managed, HERDERS_NEEDED_KEY])

	for value in dict.values():
		_guard_herd_fields(value, where, depth + 1)

## Every herd dictionary the HUD is holding as this frame renders — the world list, the panel's band
## and the roster around it, plus the selection state (whose `tile_info` carries herds too).
func _guard_frame_herd_fields(state: String) -> void:
	_guard_herd_fields(_hud._band_labor._world_herds, state)
	_guard_herd_fields(_hud._band_labor._player_band, state)
	_guard_herd_fields(_hud._band_labor._player_bands, state)
	_guard_herd_fields(_hud._band_labor._panel_band, state)
	_guard_herd_fields(_hud._selection._selected_herd, state)
	_guard_herd_fields(_hud._selection._roster_herds, state)
	_guard_herd_fields(_hud._selection._selected_tile_info, state)

## The field-pair guard's verdict, ONE line for the whole run (each violation has already been
## push_error'd against the frame it rendered in). The scanned count is part of the claim: a guard that
## walked nothing would pass vacuously, and "0 herd dicts scanned" says so out loud.
func _assert_herd_field_pairs() -> void:
	if _herd_pair_violations > 0:
		push_error("band_panel_preview: %d herd dict(s) of %d scanned half-set the herders_needed pair"
			% [_herd_pair_violations, _herd_pair_scans])
		return
	print("band_panel_preview: assert OK — every herd fixture keeps the herders_needed pair consistent (%d herd dicts scanned)"
		% _herd_pair_scans)

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
			"sustain": 0.30, "surplus": 1.20, "deplete": 0.60, "eradicate": 0.0,
		},
		# The TRADE half of the vector (issue #337) — a boar's hide sells beside its meat.
		"per_worker_trade": 0.12, "trade_per_animal": QUARRY_TRADE_PER_ANIMAL,
		"hunt_policy_trade_ceilings": {
			"sustain": 0.05, "surplus": 0.18, "deplete": 0.09, "eradicate": 0.0,
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
		# The deeper policies raid to a lower floor and so take MORE (Surplus < Deplete), which is the
		# ASCENDING per-policy metric the picker buttons must read.
		# EVERY rung DELIVERS, Eradicate included. `delivers_food` was REDEFINED by issue #337 — it now
		# says the QUARRY IS EDIBLE, not "this rung is a denial mission" — and an Eradicate raid banks
		# the whole-stock windfall. (This fixture used to assert the opposite, which was correct before
		# that arc.) Each cell carries the trade payload too: a hunt pays a vector, not a food scalar.
		for entry in [["sustain", 0], ["surplus", 2], ["deplete", 3], ["eradicate", 5]]:
			var animals: int = base + int(entry[1])
			table["%s:%d" % [String(entry[0]), w]] = {
				"turns_to_fill": turns, "delivers_food": true, "delivers_trade": true,
				"animals_taken": animals,
				"delivered_food": float(animals) * QUARRY_FOOD_PER_ANIMAL,
				"delivered_trade": float(animals) * QUARRY_TRADE_PER_ANIMAL,
				"wasted_food": 0.0}
	herd["hunt_trip_estimates"] = table
	# A second huntable herd INSIDE the band's hunt reach. It is not a party's job (the band can work
	# it from home), so the picker must refuse it — the near half of the eligibility assertion.
	var near := {
		"id": QUARRY_NEAR_HERD_ID, "species": "Roe Deer", "x": QUARRY_NEAR_X, "y": QUARRY_NEAR_Y,
		"population": 90, "ecology_phase": "thriving", "huntable": true,
		"per_worker_yield": 0.8,
		"hunt_policy_ceilings": {"sustain": 0.20, "surplus": 0.80, "deplete": 0.40, "eradicate": 0.0},
		"per_worker_trade": 0.12, "trade_per_animal": QUARRY_TRADE_PER_ANIMAL,
		"hunt_policy_trade_ceilings": {"sustain": 0.03, "surplus": 0.12, "deplete": 0.06, "eradicate": 0.0},
		"hunt_trip_estimates": table.duplicate(true),
	}
	return [herd, near]

## The tile_info a map click on a herd's hex delivers (`TargetingController._huntable_herd_on_tile` reads `herds`).
func _quarry_tile_info(herd: Dictionary) -> Dictionary:
	return {"x": int(herd["x"]), "y": int(herd["y"]), "herds": [herd]}

## A hunting PARTY is for game the band cannot work from home, so the quarry picker must refuse a herd
## inside the band's `hunt_reach` (`TargetingController.is_expedition_quarry`) — the near herd is a LOCAL hunt. This
## is behavioural, not pictorial: the refusal happens at the click, which no frame can show. Verified
## to FAIL (the near herd is accepted, `_compose.party_quarry_id()` = the near id) with the eligibility test
## removed from `TargetingController._try_pick_quarry`.
func _assert_quarry_eligibility() -> void:
	var herds := _quarry_herd_fixtures()
	var far: Dictionary = herds[0]
	var near: Dictionary = herds[1]
	_hud.update_herds(herds)
	# NEAR — inside hunt reach: refused, and targeting stays armed so the player can pick again.
	_hud._compose.clear_party_quarry()
	_hud._targeting._pending_pick_quarry = {"band": _band_fixture()}
	_hud._targeting._try_pick_quarry(_quarry_tile_info(near))
	assert(_hud._compose.party_quarry_id() == "",
		"band_panel_preview: a herd INSIDE hunt reach was accepted as a quarry (%s)" \
		% _hud._compose.party_quarry_id())
	assert(not _hud._targeting._pending_pick_quarry.is_empty(),
		"band_panel_preview: the refused pick dropped out of targeting instead of staying armed")
	# FAR — beyond hunt reach: accepted, and the pick ends targeting.
	_hud._targeting._try_pick_quarry(_quarry_tile_info(far))
	assert(_hud._compose.party_quarry_id() == QUARRY_FAR_HERD_ID,
		"band_panel_preview: a herd BEYOND hunt reach was refused as a quarry (%s)" \
		% _hud._compose.party_quarry_id())
	_hud._targeting._pending_pick_quarry = {}
	_hud._compose.clear_party_quarry()
	print("band_panel_preview: assert OK — quarry picker takes the far herd, refuses the near one")

## Herds for the per-source-cap verify state: game_deer_07 carries the pre-commit forecast fields the
## Current-actions Hunt row reads via `HudBandLaborState.find_world_herd` + `SourceForecast.forecast_inputs` — `per_worker_yield`
## plus the herd's ONLY ceiling representation, the `hunt_policy_ceilings` table (a herd has no flat
## `ceiling_*` scalars; the forage patches below still do).
## max-useful = ceil(0.20 / 0.10) = 2, so a Hunt row staffed at 2 is AT its cap.
func _cap_demo_herd_fixtures() -> Array:
	return [
		{"id": "game_deer_07", "species": "Red Deer", "x": 68, "y": 15, "population": 120,
			"ecology_phase": "thriving", "per_worker_yield": 0.10,
			"hunt_policy_ceilings": {"sustain": 0.20}},
	]

## Give a RAW wire patch the per-policy ROWS the decoder now builds — the six policy-keyed dicts that
## are a patch's only ceiling representation (#426). Every rung gets the same ceiling and per-worker
## term, which is all these cap fixtures need; the two non-food accounts stay absent, so the
## render-only-when-non-zero rule leaves every frame unchanged. The ui_preview twin is
## `_seed_forage_rows`, which derives its numbers from `patch_`-prefixed tile_info keys instead.
func _wire_patch_rows(patch: Dictionary, ceiling: float) -> Dictionary:
	var ceilings := {}
	var per_worker := {}
	for policy in ["sustain", "surplus", "deplete", "eradicate", "cultivate", "sow"]:
		ceilings[policy] = ceiling
		per_worker[policy] = float(patch.get("per_worker_yield", 0.0))
	patch["forage_policy_ceilings"] = ceilings
	patch["forage_policy_per_worker"] = per_worker
	return patch

## Forage patches for the per-source-cap verify state (shape `update_forage_patches` consumes — the RAW
## wire dict with BARE forecast keys). (71,18): max-useful = ceil(0.30 / 0.10) = 3. (60,20): max-useful
## = ceil(0.50 / 0.10) = 5.
func _cap_demo_patch_fixtures() -> Array:
	return [
		# The per-policy ROW, not the retired flat `ceiling_sustain` scalar (#426): these are RAW wire
		# patches (bare keys, no `patch_` prefix), and the row is the only ceiling representation the
		# wire carries now — a flat scalar here would leave the work rows' `+` uncapped.
		_wire_patch_rows({"x": 71, "y": 18, "per_worker_yield": 0.10}, 0.30),
		_wire_patch_rows({"x": 60, "y": 20, "per_worker_yield": 0.10}, 0.50),
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
## builds carries exactly the age structure the panel is judged on. **Fog cannot redact it, and not
## because fog is off** — a fresh MapView now defaults to fog ON. `_rebuild_unit_markers` builds the
## marker list unfiltered (the fog gate is `_unit_hidden_by_fog` at DRAW time, and it exempts your
## OWN bands), and this fixture's band is faction 0. So this state reads the marker, never a
## fog-gated `tile_info` — unlike `ui_preview`'s `tile_panel_land_sticky`, which must disable FoW
## explicitly. Verified by A/B: flipping the default moves no frame here.
func _map_path_snapshot() -> Dictionary:
	var terrain: Array = []
	terrain.resize(MAP_PATH_GRID_W * MAP_PATH_GRID_H)
	terrain.fill(MAP_PATH_TERRAIN_ID)
	return {
		"grid": {"width": MAP_PATH_GRID_W, "height": MAP_PATH_GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": _stamp_band_ids([_band_fixture()]),
	}

## Stamp a fixture cohort with the `band_id` the real wire carries, DELIBERATELY DIFFERENT from its
## `entity`. `band_id` is the durable handle every band-addressed command names
## (`HudConst.NO_BAND_ID`); `entity` is client-local ECS allocation state. Both are plain ints, so a
## fixture where the two agree cannot tell a correct emit from one that sent the entity — which is
## exactly how that defect shipped. The offset keeps ids readable (band 904 -> 4904) while
## guaranteeing they differ. Stamped at PUSH time, not at construction, because several fixtures
## override `entity` after the builder returns.
static func _stamp_band_ids(cohorts: Array) -> Array:
	var stamped: Array = []
	for cohort_variant in cohorts:
		var cohort: Dictionary = (cohort_variant as Dictionary).duplicate(true)
		cohort["band_id"] = int(cohort.get("entity", 0)) + FIXTURE_BAND_ID_OFFSET
		stamped.append(cohort)
	return stamped

## Push a cohort roster through the real snapshot path (`update_band_alerts`), band ids stamped.
func _push_bands(cohorts: Array) -> void:
	_hud.update_band_alerts(_stamp_band_ids(cohorts))

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
		# Thriving growth: fed (neutral hunger, row omitted), saturated larder, net-positive food →
		# 1.0 × 1.5 × 1.25 = 188% of normal, neutral ink, collapsed ▸ disclosure.
		"fertility_hunger": 1.0,
		"fertility_reserve": 1.5,
		"fertility_trend": 1.25,
		# Trade goods are the THIRD key on the band's own `stores` since issue #381 — the sim moved them
		# off the faction stockpile, so this is what the Trade row's total reads.
		"stores": {"provisions": 84.0, "trade_goods": 12.0},
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
		# tests. Only the herd drawer and `TargetingController.is_expedition_quarry` read it, so no other state moves.
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
			# **THE LIVE FORAGE SHAPE, AND IT IS THE REGRESSION THIS FIXTURE EXISTS FOR.** A cash crop
			# really does sell (`labor.rs`), so `trade_yield` is non-zero — but its `realized_trade_yield`
			# is the documented `PLANT_TRADE_FORECAST_NOT_YET_PROJECTED` **0.0**, and the decoder inserts
			# that key UNCONDITIONALLY. Both keys present, one of them zero, is exactly what the wire sends
			# and exactly what a `has("realized_trade_yield")` test reads as "projected: nothing".
			{"kind": "forage", "workers": 5, "workers_needed": 2, "policy": "sustain", "target_x": 71, "target_y": 18, "actual_yield": 0.48, "sustainable_yield": 0.48, "trade_yield": 0.04, "realized_trade_yield": 0.0},
			# BOTH PRODUCTS on the worked row (issue #337): a deer pays meat AND hide, so the row
			# headline must read `+0.20 /turn · ⇄ +0.04` — food leading, trade only because it is
			# non-zero. `trade_yield` is NOT food income: the Food line's Gathered/Hunted breakdown
			# still sums `actual_yield` alone, which is what keeps the larder identity closed.
			{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": 70, "target_y": 17, "actual_yield": 0.46, "sustainable_yield": 0.20, "trade_yield": 0.04, "realized_trade_yield": 0.04},
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
		# THE TRADE-ONLY ROW (issue #337): a wolf pack pays pelts and NO meat, so every food field on
		# this assignment is honestly 0. The row must headline `⇄ +0.22` ALONE — no "+0.00 /turn",
		# which is the false reading that said the hunt was worth nothing — and it must NOT appear in
		# the Food line's Hunted total, because trade goods never enter the larder.
		{"kind": "hunt", "workers": 2, "fauna_id": TRADE_ONLY_HERD_ID, "policy": "deplete", "target_x": 72, "target_y": 19, "actual_yield": 0.0, "sustainable_yield": 0.0, "trade_yield": 0.22, "realized_trade_yield": 0.22},
		{"kind": "scout", "workers": 2},
	]
	return band

## `_band_fixture` with every TRADE component stripped off its assignments — the band that earns no
## trade at all, which is what the zero-rate Trade row is judged on. Strips rather than hand-writing a
## fixture so it cannot drift from `_band_fixture`'s chrome (and so the ONLY difference between this
## band and the earning one is the thing under test).
func _no_trade_band_fixture() -> Dictionary:
	var band := _band_fixture()
	var stripped: Array = []
	for a in (band["labor_assignments"] as Array):
		var d := (a as Dictionary).duplicate(true)
		d.erase("trade_yield")
		d.erase("realized_trade_yield")
		stripped.append(d)
	band["labor_assignments"] = stripped
	return band

## The trade-only-HUNT variant of the band above: the deer is unassigned, so every hunt this band works
## pays trade and no food. It exists to exercise the AGGREGATE suppression path — the per-kind hunt chip
## has no food component to state at all — which the mixed board cannot reach, since one food-paying
## hunt there keeps the chip's food term alive.
func _trade_only_hunt_band_fixture() -> Dictionary:
	var band := _concerning_food_band_fixture()
	band["labor_assignments"] = (band["labor_assignments"] as Array).filter(
		func(a): return String((a as Dictionary).get("fauna_id", "")) != EXTRACTIVE_ROW_HERD_ID)
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
	# ...and its growth has collapsed with its larder: eating short off a draining store with income
	# gone → 0.55 × 1.05 × 0.25 = 14% of normal, a red Growth row above a WARN caret. It is the extra
	# row + disclosure this variant exists to push past the old fixed panel height.
	band["fertility_hunger"] = 0.55
	band["fertility_reserve"] = 1.05
	band["fertility_trend"] = 0.25
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
## that is NOT "no surplus": `find_world_herd` returns {} for the target id, so the delivery line must
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
