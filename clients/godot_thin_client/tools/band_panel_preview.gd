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
# The two disclosure keys of `_band_fixture()` (entity 904) — the `[url]` meta payload its Food /
# Morale rows carry, i.e. what `Hud._breakdown_key` builds for that band.
const BAND_FIXTURE_DISCLOSURE_FOOD := "food:904"
const BAND_FIXTURE_DISCLOSURE_MORALE := "morale:904"
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
	_hud._toggle_work_inspector(_hud._work_source_models(_hud._panel_band, 0)[0]["key"])
	await _settle()
	await _save("band_panel_inspector")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._toggle_work_inspector(_hud._work_open_key)

	# The Work menu's destructive action asks first, and the confirm names what is SPARED.
	_hud._on_work_unassign_all_pressed(_hud._panel_band, 34)
	await _settle()
	await _save("band_panel_clear_confirm")
	_dismiss_dialogs()

	# The parties COMPOSE sheet, mission-first: Hunt picked → party stepper, policy picker, forecast.
	_hud.update_food_modules([{"x": 71, "y": 18, "module": "savanna_grassland", "kind": "gather"}])
	_hud.update_band_alerts([_scout_expedition_fixture(), _band_fixture(), _hunt_expedition_fixture()])
	_panel.set_active_tab(&"parties")
	_hud._party_compose_open = true
	_hud._party_compose_mission = "hunt"
	_hud._rerender_panel_allocation()
	await _settle()
	await _save("band_panel_compose_hunt")
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()
	_hud._party_compose_open = false

	# Zero idle workers: "Send a party…" stays VISIBLE and DISABLED, with its reason.
	_hud.update_band_alerts([_no_idle_band_fixture()])
	await _settle()
	await _save("band_panel_no_idle")

	_assert_no_scroll_containers()
	_assert_zones_within_bounds()
	_assert_work_zone_readable()
	_assert_zone_content_fits()

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
	}
