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
# Slice 1 reserved-dock probe: left-edge reservation width used to verify the HUD insets.
const RESERVED_PROBE_WIDTH := 300.0

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

	# Top-bar Wondrous-Sites discoveries readout (faction 0): a landmark + a settle-site, so
	# the count reads `◈ Discoveries 2  ⛰ ⛲` and the distinct glyphs show.
	_hud.update_discoveries([{
		"faction": 0,
		"sites": [
			{"x": 12, "y": 8, "site_id": "great_peak", "category": "landmark", "display_name": "Great Peak", "glyph": "⛰"},
			{"x": 20, "y": 14, "site_id": "verdant_basin", "category": "settle_site", "display_name": "Verdant Basin", "glyph": "⛲"},
		],
	}])

	# The labor-allocation UI (Early-Game Labor slice 3b) targets the single player band;
	# seed it so the herd/tile "assign" controls resolve a band to staff.
	_hud._player_band = _band_fixture()

	# State 1 — a single band selected (GOOD state): the Occupants roster + the labor allocation panel.
	# Food + Morale are healthy, so BOTH summary rows read collapsed with a ▸ disclosure caret
	# (`Food ▸ …` / `Morale 82% ▸`) — click-to-expand, nothing auto-shown.
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("band")

	# State 1-food-a — GOOD food, breakdown force-EXPANDED. The good band's breakdown is hidden by
	# default (net positive, long runway); the static harness can't click the Food disclosure, so we
	# force the per-band expand override to confirm the click-expanded layout renders (indented
	# `Gathered · Hunted · Eaten` sub-line under Food) without clipping.
	_hud._breakdown_expanded = {"food:904": true}
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("band_food_expanded")
	_hud._breakdown_expanded = {}

	# State 1-morale-a — GOOD morale, breakdown force-EXPANDED (same disclosure as Food): forcing the
	# per-band morale override opens the collapsed-by-default morale contribution sub-lines.
	_hud._breakdown_expanded = {"morale:904": true}
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("band_morale_expanded")
	_hud._breakdown_expanded = {}

	# State 1-food-b — CONCERNING food (net negative + low runway): the Food line net reads red and
	# the category breakdown is AUTO-shown (no click needed), mirroring the morale breakdown.
	_hud.show_unit_selection(_concerning_food_band_fixture())
	await _settle()
	await _save("band_food_concerning")

	# State 1b — an all-idle band: no assignments, every worker idle. The allocation panel
	# shows just the Scout + Warrior rows (both at 0) under the Working/Idle header.
	var idle_band := _band_fixture()
	idle_band["activity"] = "idle"
	idle_band["idle_workers"] = 16
	idle_band["labor_assignments"] = []
	_hud.show_unit_selection(idle_band)
	await _settle()
	await _save("band_idle")

	# State 1p — optimistic pending feedback: a fresh forage assignment (6 workers to a new
	# tile) is in flight before the snapshot confirms. The panel shows an amber "· pending"
	# Forage row and the Idle count reflects it immediately (16 − [5+4+2+2+6=19] clamps to 0).
	# (Seeds the HUD-local pending map directly to mimic a just-issued assign_labor.)
	_hud._pending_labor = {
		904: {
			"turn": 0,
			"assign": {"forage:64,20": {"kind": "forage", "workers": 6, "x": 64, "y": 20, "herd_id": "", "policy": ""}},
		}
	}
	_hud.show_unit_selection(_band_fixture())
	await _settle()
	await _save("band_pending")
	_hud._pending_labor = {}

	# State 1e — a scouting expedition selected in its awaiting-orders phase: the drawer shows the
	# dedicated expedition readout (Mission / Phase "Awaiting orders" / Party / Provisions) and the
	# Recall + Move panel with the amber awaiting callout, instead of the labor-allocation UI.
	_hud.show_unit_selection(_expedition_fixture())
	await _settle()
	await _save("expedition_panel")

	# State 1f — the same expedition after Recall, now in its returning phase: the panel's button
	# reads "Returning" (disabled) instead of a grayed-out "Recall", and the awaiting callout is
	# gone. The drawer Phase row reads "Returning".
	var returning_expedition := _expedition_fixture()
	returning_expedition["expedition_phase"] = "returning"
	_hud.show_unit_selection(returning_expedition)
	await _settle()
	await _save("expedition_returning")

	# State 1g — outfit party cap: a resident band with 16 idle workers but a server party cap of 8.
	# The "Send scouting expedition" Party stepper maxes at min(idle 16, cap 8) = 8 — dialed to 8, the
	# + is disabled, confirming the stepper clamps to the CAP, not to idle.
	var cap_band := _band_fixture()
	cap_band["idle_workers"] = 16
	cap_band["max_expedition_party_size"] = 8
	cap_band["labor_assignments"] = []   # all 16 working-age workers read idle
	_hud._send_expedition_count = 8
	_hud.show_unit_selection(cap_band)
	await _settle()
	await _save("expedition_outfit_cap")
	_hud._send_expedition_count = 1   # reset so later states render a fresh party stepper

	# State 1h — a hunting expedition (PR 2, §2b) selected in its Hunting phase: the panel shows the
	# hunt readout (Mission "Hunting expedition", Target herd, Policy, Carried 8 / 16, Party) +
	# Recall/Move.
	_hud.show_unit_selection(_hunt_expedition_fixture())
	await _settle()
	await _save("expedition_hunt_panel")

	# State 1i — a FULL hunt party (carried at the carry ceiling): the Carried row reads "16 / 16 …
	# · FULL" and the Phase is Delivering (it heads home when full).
	var full_hunt := _hunt_expedition_fixture()
	full_hunt["expedition_phase"] = "delivering"
	full_hunt["stores"] = {"provisions": 16.0}
	full_hunt["days_of_food"] = 8.0
	_hud.show_unit_selection(full_hunt)
	await _settle()
	await _save("expedition_hunt_full")

	# State 1j — a recalled hunt party in its Returning phase: the Phase reads "Returning" and the
	# panel's Recall button flips to a disabled "Returning" (same treatment as the scout panel).
	var returning_hunt := _hunt_expedition_fixture()
	returning_hunt["expedition_phase"] = "returning"
	returning_hunt["stores"] = {"provisions": 12.0}
	returning_hunt["days_of_food"] = 6.0
	_hud.show_unit_selection(returning_hunt)
	await _settle()
	await _save("expedition_hunt_returning")

	# State 1k — the hunt launch policy picker: an idle band (short allocation panel) showing the
	# "Send expedition" outfit block — the party stepper, the scout + hunt send buttons, and the hunt
	# POLICY radio (Market selected here) with its one-line behaviour hint.
	var launch_band := _band_fixture()
	launch_band["idle_workers"] = 12
	launch_band["labor_assignments"] = []
	_hud._send_hunt_policy = "market"
	_hud.show_unit_selection(launch_band)
	await _settle()
	await _save("expedition_launch_policy")
	_hud._send_hunt_policy = "sustain"

	# State 1a — a well-fed but demoralized band: healthy food (∞) yet morale 0.22
	# (< critical), so the drawer's Morale line reads a red 22%. Discontent drags
	# Output to 56% (red) and the itemized morale breakdown + recovery guidance show.
	_hud.show_unit_selection(_low_morale_band_fixture())
	await _settle()
	await _save("band_low_morale")

	# State 1b — band alerts: seed previous sizes, then a snapshot that raises all
	# three alert kinds (starving red / losing-population amber / idle quiet).
	_hud.update_band_alerts(_band_alert_baseline())
	_hud.update_band_alerts(_band_alert_fixture())
	await _settle()
	await _save("band_alerts")

	# State 1c — Wondrous Sites: the top-bar `◈ Discoveries` readout plus a `SiteDiscovered`
	# command-feed entry (server-provided kind/label render generically). Confirms both surfaces.
	_hud.ingest_command_events([
		{"tick": 42, "kind": "site_discovered", "label": "Discovered Verdant Basin", "detail": "A settle-site revealed at (20, 14)."},
	])
	_hud.clear_selection()
	await _settle()
	await _save("discoveries")

	# band_alerts (above) left _player_band as an alert-fixture band (no work_range, far from the food
	# tile); seed a NEAR band so the forage controls resolve an in-range actor.
	_hud._player_band = _forage_range_bands()[0]
	_hud._player_bands = []
	_hud._forage_assign_key = ""
	_hud._forage_assign_band = -1

	# State 2 — a food tile selected, band WITHIN forage range: the Tile card's "Assign foragers"
	# controls (a "Band:" dropdown naming the actor band + a Foragers −/+ count + an enabled **Forage**
	# button). With one player band the dropdown is a single item ("Band 1").
	_hud.show_tile_selection(_food_tile_fixture())
	await _settle()
	await _save("food_tile")

	# State 2b — the same food tile, single FAR band (~21 tiles away, beyond work_range 2): foraging is
	# stationary gathering with NO expedition fallback, so the Forage button is DISABLED and an
	# out-of-range hint shows ("(66,10) is 21 tiles away — beyond this band's forage range (2)").
	_hud._player_band = _forage_range_bands()[1]
	_hud._player_bands = []
	_hud._forage_assign_key = ""
	_hud._forage_assign_band = -1
	_hud.show_tile_selection(_food_tile_fixture())
	await _settle()
	await _save("food_forage_out_of_range")

	# State 2c — TWO bands at DIFFERENT distances from ONE food tile, NEAR band selected (821, 1 tile
	# away ≤ range 2): enabled **Forage**. The band-picker selection — not the tile — drives it.
	_hud._player_bands = _forage_range_bands()
	_hud._player_band = _hud._player_bands[0]
	_hud._forage_assign_key = ""
	_hud._forage_assign_band = -1
	_hud.show_tile_selection(_food_tile_fixture())
	await _settle()
	await _save("food_forage_band_near")

	# State 2d — same two bands, FAR band selected via the picker (822, ~21 tiles away): the SAME tile
	# now DISABLES Forage + shows the out-of-range hint, proving WHICH band is selected drives the
	# enabled-vs-disabled state (the case single-band playtest can't cover).
	_hud._forage_assign_band = int(_forage_range_bands()[1]["entity"])
	_hud._build_forage_assign_controls(_food_tile_fixture())
	await _settle()
	await _save("food_forage_band_far")
	# Reset so later states resolve their usual band.
	_hud._player_bands = []
	_hud._forage_assign_key = ""
	_hud._forage_assign_band = -1

	# band_alerts (above) overwrote _player_band with alert-fixture bands (which carry no hunt_reach);
	# re-seed the reference band so the herd assign controls resolve a proper band with a hunt reach.
	_hud._player_band = _band_fixture()
	_hud._player_bands = []
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1

	# State 3 — a huntable herd selected on a food tile, WITHIN the band's hunt reach: the "Assign
	# hunters" controls (a "Band:" dropdown naming the actor band, a Hunters −/+ count, the
	# sustain/surplus/market/eradicate policy picker, and the local "Assign Local Hunt" button). A
	# Thriving herd shows a neutral ecology readout in the drawer.
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

	# State 3f — TWO player bands: the "Assign hunters" controls' "Band:" dropdown lists both
	# (positional "Band 1" / "Band 2"). Default selection is the resolved band (Band 1, 12 idle);
	# the Hunters count is dialed up to 8 (< cap 12, so + stays enabled).
	_hud._player_bands = _two_player_bands()
	_hud._player_band = _hud._player_bands[0]
	_hud._hunt_assign_key = ""   # force a fresh seed so the default selection = resolved band
	_hud.show_herd_selection(_herd_fixture())
	_hud._hunt_assign_count = 8
	_hud._build_herd_assign_controls(_herd_fixture())
	await _settle()
	await _save("herd_band_picker")

	# State 3g — same, after switching the dropdown to Band 2 (only 2 idle): the picker path
	# re-caps the Hunters count to the newly-selected band's assignable workers (8 → 2, + now
	# disabled), demonstrating selection → actor band → stepper re-cap.
	var second_band: Dictionary = _two_player_bands()[1]
	_hud._hunt_assign_band = int(second_band["entity"])
	_hud._hunt_assign_count = clampi(
		_hud._hunt_assign_count, 0, _hud._assignable_hunt_workers(second_band, _herd_fixture()["id"]))
	_hud._build_herd_assign_controls(_herd_fixture())
	await _settle()
	await _save("herd_band_picker_b")
	# Reset so later states render their usual single-band dropdown.
	_hud._player_bands = []
	_hud._hunt_assign_key = ""

	# State 3h — distance-aware herd-hunt, SINGLE far band: a lone band ~27 tiles from the herd (beyond
	# its hunt_reach 7). The affordance fully replaces the local option — the button reads "Send Hunting
	# Expedition", a distance hint shows, the stepper reads "Party", and Assign emits
	# send_hunt_expedition (party = the stepper), NOT assign_labor.
	_hud._player_bands = [_hunt_distance_bands()[1]]   # only the FAR band
	_hud._player_band = _hud._player_bands[0]
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1
	_hud.show_herd_selection(_hunt_distance_herd())
	await _settle()
	await _save("herd_hunt_expedition")

	# State 3i — TWO bands at DIFFERENT distances from ONE herd, NEAR band selected: band 811 sits ON
	# the herd (distance 0 ≤ reach 7) → "Assign Local Hunt" + assign_labor. The band-picker selection —
	# not the herd — drives it (the resolved/default band is the near one here).
	_hud._player_bands = _hunt_distance_bands()
	_hud._player_band = _hud._player_bands[0]
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1
	_hud.show_herd_selection(_hunt_distance_herd())
	await _settle()
	await _save("herd_hunt_band_near")

	# State 3j — same two bands, FAR band selected via the picker (entity 812, ~27 tiles away): the SAME
	# herd now offers "Send Hunting Expedition" (party cap = min(idle 6, max party 8) = 6), proving that
	# WHICH band is selected flips the label + command + band-entity target, not the herd.
	_hud._hunt_assign_band = int(_hunt_distance_bands()[1]["entity"])   # FAR band
	_hud._build_herd_assign_controls(_hunt_distance_herd())
	await _settle()
	await _save("herd_hunt_band_far")
	# Reset so later states render their usual single-band dropdown + default band.
	_hud._player_bands = []
	_hud._player_band = _band_fixture()
	_hud._hunt_assign_key = ""
	_hud._hunt_assign_band = -1

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

	# State 4 — targeting active: pressing "Move" on the band allocation panel enters
	# tile-targeting, raising the top-centre banner ("MOVE … click a destination tile").
	_hud.show_unit_selection(_band_fixture())
	_hud._on_move_band_pressed()
	await _settle()
	await _save("targeting_banner")

	# State 5 — quick-hunt convenience (map double-click a herd): with idle workers it
	# assigns them to hunt; with none it posts a command-feed note instead of silently
	# no-opping. Seed a fully-staffed band (0 idle) so the note renders in the Command Feed.
	var staffed_band := _band_fixture()
	staffed_band["idle_workers"] = 0
	_hud._player_band = staffed_band
	_hud.show_tile_selection(_food_tile_fixture())
	_hud.quick_assign_hunters("game_bison_02")
	await _settle()
	await _save("quick_hunt_note")

	# State 6 — turn orb, ALL-CLEAR: a player band with zero idle workers → empty
	# attention registry → the orb calm-pulses (dashed cyan arc), the caption reads
	# "Turn 42 · ▸ all clear", and no badge shows.
	_hud.clear_selection()
	_hud.update_overlay(42, {})
	_hud.update_band_alerts([
		{"faction": 0, "entity": 501, "size": 40, "days_of_food": 999.0, "activity": "forage",
			"current_x": 30, "current_y": 20, "idle_workers": 0},
	])
	await _settle()
	await _save("turn_orb_clear")

	# State 6b — turn orb, EMPTY registry, orb-face CLICK: advancing must always be possible
	# from the orb, so with nothing to triage the click ADVANCES the turn directly and opens NO
	# popover (the old bug opened a tall blank box whose Advance affordance was pushed off-screen,
	# trapping the player). Assert the emitted advance signal (the harness can't run a real turn)
	# and that no popover opened; the saved frame must show the calm pulse with no blank box.
	var advance_hits := [0]
	var advance_cb := func() -> void: advance_hits[0] += 1
	_hud.turn_orb.advance_requested.connect(advance_cb)
	_hud.turn_orb._on_face_pressed()
	await _settle()
	_assert_turn_orb("empty click advances", advance_hits[0] == 1 and not _hud.turn_orb._popover_open)
	await _save("turn_orb_clear_click_advances")

	# State 6c — turn orb, NON-EMPTY registry: the click opens the reasons popover, and the
	# popover's `Advance ▸` footer button emits advance_requested (unchanged behavior). Seed one
	# attention entry, open via the face click, then fire the footer button and assert the emit.
	advance_hits[0] = 0
	_hud.update_band_alerts([
		{"faction": 0, "entity": 511, "size": 40, "days_of_food": 999.0, "activity": "forage",
			"current_x": 30, "current_y": 20, "idle_workers": 5},
	])
	_hud.turn_orb._on_face_pressed()
	await _settle()
	var opened := _hud.turn_orb._popover_open
	var footer_btn := _turn_orb_advance_button()
	var had_footer := footer_btn != null
	if had_footer:
		footer_btn.pressed.emit()   # frees the popover (advance closes it)
	await _settle()
	_assert_turn_orb("non-empty popover + footer advances",
		opened and had_footer and advance_hits[0] == 1 and not _hud.turn_orb._popover_open)
	_hud.turn_orb.advance_requested.disconnect(advance_cb)

	# State 7 — turn orb, ALL THREE ATTENTION KINDS (the folded-in Alerts panel): a first
	# snapshot seeds prior band sizes so "losing population" has a baseline, then the live
	# snapshot fires one of each producer — Band 1 starving (days 3 < critical → critical/red),
	# Band 2 shrank 90→78 with emigrants (losing population → warn/amber), Band 3 has idle
	# workers (warn/amber). The badge reads "3", the pulse stops, and the popover (opened here)
	# lists all three with the starving/critical row sorted to the TOP, each with a Jump row.
	# A starving EXPEDITION is interleaved between the bands to verify the bands-only numbering:
	# it produces NO attention entry (never "Band N starving") and does not shift Band 2/Band 3's
	# positional numbers — the idle-workers row still reads "Band 3", matching the picker/header.
	_hud.update_band_alerts([
		{"faction": 0, "entity": 601, "size": 120, "days_of_food": 12.0, "activity": "forage",
			"current_x": 21, "current_y": 15},
		{"faction": 0, "entity": 602, "size": 90, "days_of_food": 999.0, "activity": "hunt",
			"current_x": 31, "current_y": 21},
		{"faction": 0, "entity": 603, "size": 60, "days_of_food": 999.0, "activity": "forage",
			"current_x": 12, "current_y": 9},
	])
	_hud.update_band_alerts([
		# Band 1 — starving (3 days of food, below critical).
		{"faction": 0, "entity": 601, "size": 120, "days_of_food": 3.0, "activity": "forage",
			"current_x": 21, "current_y": 15},
		# A detached hunt expedition, also starving — must NOT emit a "Band N starving" entry and
		# must NOT consume a band number (Band 2/Band 3 below stay 2 and 3).
		{"faction": 0, "entity": 650, "size": 6, "days_of_food": 2.0, "is_expedition": true,
			"expedition_mission": "hunt", "expedition_phase": "hunting", "home_band_entity": 601,
			"current_x": 25, "current_y": 18},
		# Band 2 — losing population: 90 → 78, well-fed but 12 emigrated last turn → "people leaving".
		{"faction": 0, "entity": 602, "size": 78, "days_of_food": 999.0, "morale": 0.30,
			"morale_cause": 1, "last_emigrated": 12, "activity": "hunt", "current_x": 31, "current_y": 21},
		# Band 3 — idle labor: 4 working-age workers unassigned.
		{"faction": 0, "entity": 603, "size": 60, "days_of_food": 999.0, "activity": "forage",
			"current_x": 12, "current_y": 9, "idle_workers": 4},
	])
	_hud.turn_orb.open_popover()
	await _settle()
	await _save("turn_orb_attention")

	# State 8 — reserved-space docking (Slice 1 refactor): a left-edge reservation of
	# RESERVED_PROBE_WIDTH px insets the whole HUD (LayoutRoot.offset_left), so the top/bottom
	# bars start that much further right — mirroring how the docked Inspector shrinks the play
	# space. Save the inset frame, then release it (size 0) and save the restored frame.
	_hud.clear_selection()
	_hud.set_reserved_inset(&"inspector", SIDE_LEFT, RESERVED_PROBE_WIDTH)
	await _settle()
	await _save("reserved_dock")
	_hud.set_reserved_inset(&"inspector", SIDE_LEFT, 0.0)
	await _settle()
	await _save("reserved_dock_cleared")

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
	# Force a synchronous frame rather than awaiting `RenderingServer.frame_post_draw`.
	# Under the dummy rendering backend (which `--headless` selects on Godot 4.5) no
	# real draw ever posts, so that await never returns and the harness hangs. force_draw
	# just no-ops there, so a stray headless run fails fast in `_save` instead of hanging.
	RenderingServer.force_draw()
	await get_tree().process_frame

func _save(name: String) -> void:
	var image := get_viewport().get_texture().get_image()
	if image == null:
		# No image to read back — the dummy renderer (i.e. someone ran this with
		# `--headless`, which selects it on Godot 4.5). Capture is impossible, but
		# the compile/scene gate still passed. Run WITHOUT `--headless` for PNGs.
		push_warning("ui_preview: null image (dummy renderer?) — skipping %s.png; run without --headless to capture" % name)
		return
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		push_error("ui_preview: failed to save %s (err %d)" % [name, err])
	else:
		print("ui_preview: saved ", name, ".png")

## Walk the open reasons popover to its `Advance ▸` footer button (last body row's child).
func _turn_orb_advance_button() -> Button:
	var pop := _hud.turn_orb._popover
	if pop == null or pop.get_child_count() == 0:
		return null
	var body := pop.get_child(0)
	if body.get_child_count() == 0:
		return null
	var footer := body.get_child(body.get_child_count() - 1)
	if footer.get_child_count() == 0:
		return null
	var btn := footer.get_child(0)
	return btn as Button

func _assert_turn_orb(label: String, ok: bool) -> void:
	if ok:
		print("ui_preview: PASS turn-orb — ", label)
	else:
		push_error("ui_preview: FAIL turn-orb — %s" % label)

func _band_fixture() -> Dictionary:
	return {
		"id": "Band 2",
		"size": 148,
		"entity": 904,
		"faction": 0,
		"pos": [71, 18],
		# Good food state: a long larder runway (≥ warn) + positive net (0.94 − 0.68 = +0.26) → the
		# Food line reads "… · +0.26 /turn" and the category breakdown is collapsed (clickable open).
		"days_of_food": 22.0,
		# Good morale (≥ warn, not falling) → the Morale row is collapsed with a ▸ caret. The signed
		# Layer-1 contributions (above the breakdown epsilon) give the disclosure real content on expand.
		"morale": 0.82,
		"morale_settling": 0.012,
		"morale_terrain": -0.010,
		"morale_climate": -0.006,
		"stores": {"provisions": 84.0},
		# Early-Game Labor (slice 3b): 16 working-age workers, 3 idle, split across a
		# Forage tile, a Hunt herd, and the Scout + Warrior band-wide roles.
		"working_age": 16,
		"idle_workers": 3,
		# Server's hard party-size cap (expedition config, default 8) — the outfit stepper maxes at
		# min(idle, this).
		"max_expedition_party_size": 8,
		"work_range": 2,
		# Hunt reach (work_range + hunt leash) — large enough here that BOTH the reference herd_fixture
		# (9 tiles from this band's pos) and the occupied-hex herd (16 tiles) stay WITHIN reach, so those
		# herd states render the LOCAL "Assign Local Hunt" controls (the far-herd expedition path has its
		# own dedicated fixtures, _hunt_distance_bands).
		"hunt_reach": 16,
		"scout_reveal_radius": 2,
		"activity": "forage",
		# Band food flow (Food summary line): total income across the worked sources vs the cohort's
		# consumption. Net = 0.94 − 0.68 = +0.26 (positive → larder growing), shown green on the Food
		# line. Per-source actual/sustainable yields live on the assignments below; the hunt overdraws
		# (0.46 > 0.20) so its allocation row shows the ⚠ flag; forage (actual == sustainable) never does.
		# The Gathered/Hunted breakdown sums the assignment actual_yields (0.48 / 0.46) by kind.
		"food_income": 0.94,
		"food_consumption": 0.68,
		"labor_assignments": [
			{"kind": "forage", "workers": 5, "target_x": 71, "target_y": 18, "actual_yield": 0.48, "sustainable_yield": 0.48},
			{"kind": "hunt", "workers": 4, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": 70, "target_y": 17, "actual_yield": 0.46, "sustainable_yield": 0.20},
			{"kind": "scout", "workers": 2},
			{"kind": "warrior", "workers": 2},
		],
		"tile_info": {
			"x": 71, "y": 18,
			"terrain_label": "Freshwater Marsh",
			"tags_text": "Freshwater, Wetland",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	}

## A CONCERNING food state: net-negative flow (income 0.30 < consumption 0.95 → net −0.65) and a
## low larder runway (4 days). Both trip `_food_is_concerning`, so the category breakdown auto-shows
## under a red net figure without any click.
func _concerning_food_band_fixture() -> Dictionary:
	var band := _band_fixture()
	band["entity"] = 905
	band["id"] = "Band 3"
	band["days_of_food"] = 4.0
	band["food_income"] = 0.30
	band["food_consumption"] = 0.95
	band["labor_assignments"] = [
		{"kind": "forage", "workers": 3, "target_x": 71, "target_y": 18, "actual_yield": 0.15, "sustainable_yield": 0.15},
		{"kind": "hunt", "workers": 2, "fauna_id": "game_deer_07", "policy": "sustain", "target_x": 70, "target_y": 17, "actual_yield": 0.15, "sustainable_yield": 0.20},
		{"kind": "scout", "workers": 2},
	]
	return band

## A scouting expedition (docs/plan_exploration_and_sites.md §2) in its awaiting-orders phase:
## a detached party (is_expedition) carrying a mission/phase + party size + provisions. The drawer
## renders the dedicated expedition readout + Recall/Move panel, not the labor-allocation UI.
func _expedition_fixture() -> Dictionary:
	return {
		"id": "Scouts 1",
		"size": 6,
		"entity": 7001,
		"faction": 0,
		"pos": [80, 30],
		"days_of_food": 9.0,
		"stores": {"provisions": 48.0},
		"is_expedition": true,
		"expedition_mission": "scout",
		"expedition_phase": "awaiting",
		"tile_info": {
			"x": 80, "y": 30,
			"terrain_label": "Highland Tundra",
			"tags_text": "Cold, Exposed",
			"visibility_state": "active",
			"food_module": "",
			"food_module_label": "None",
		},
	}

## A hunting expedition (PR 2, docs/plan_exploration_and_sites.md §2b): a detached party following a
## migratory herd. mission "hunt" + a target herd + carried food (its own kills). The drawer renders
## the hunt readout (target herd + carried food + phase) + Recall/Move.
func _hunt_expedition_fixture() -> Dictionary:
	return {
		"id": "Hunters 1",
		"size": 5,
		"entity": 7101,
		"faction": 0,
		"pos": [64, 22],
		"days_of_food": 4.0,
		# Carried 8 of a 16 carry cap → "Carried 8 / 16".
		"stores": {"provisions": 8.0},
		"is_expedition": true,
		"expedition_mission": "hunt",
		"expedition_phase": "hunting",
		"expedition_target_herd": "game_deer_07",
		"expedition_hunt_policy": "surplus",
		"expedition_carry_cap": 16.0,
		"tile_info": {
			"x": 64, "y": 22,
			"terrain_label": "Prairie Steppe",
			"tags_text": "Fertile",
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
	fixture["morale_delta"] = -0.010
	fixture["morale_cause"] = 1  # Terrain
	# Civilization Wellbeing (docs/plan_civ_wellbeing.md): discontent drags Output to 56%
	# (< critical → red), and the four signed Layer-1 contributions (sum = morale_delta)
	# drive the itemized breakdown. People are relocating (last_emigrated > 0).
	fixture["output_multiplier"] = 0.56
	fixture["discontent_fraction"] = 0.44
	fixture["last_emigrated"] = 6
	fixture["morale_settling"] = 0.010   # +1.0%  settling (positive base growth)
	fixture["morale_terrain"] = -0.012   # −1.2%  harsh terrain
	fixture["morale_climate"] = -0.008   # −0.8%  harsh climate
	fixture["morale_unrest"] = 0.0       # below epsilon → row omitted
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
		# Losing population to relocation: size 90 → 78, well-fed (∞) but discontented and
		# 12 people emigrated last turn → amber alert "losing population — people leaving".
		{"faction": 0, "entity": 102, "size": 78, "days_of_food": 999.0, "morale": 0.30, "morale_cause": 1, "last_emigrated": 12, "activity": "hunt", "current_x": 40, "current_y": 22,
			"harvest": {"band_label": "Band Ash"}},
		# Idle labor: quiet low-priority alert.
		{"faction": 0, "entity": 103, "size": 45, "days_of_food": 999.0, "activity": "idle", "current_x": 12, "current_y": 9},
	]

## Two player bands (multi-band split is deferred, but the assign controls' band-picker must
## handle N). Different idle_workers so switching the dropdown visibly re-caps the worker
## stepper; neither hunts the deer herd, so the cap for a fresh source == idle_workers.
func _two_player_bands() -> Array:
	# hunt_reach 6 keeps both bands WITHIN local reach of the (66,10) herd (distances 0 and 3), so the
	# band-picker states test the LOCAL-hunt re-cap (the distance-aware expedition path is exercised by
	# _hunt_distance_bands below).
	return [
		{"entity": 801, "faction": 0, "size": 120, "current_x": 66, "current_y": 10,
			"working_age": 14, "idle_workers": 12, "hunt_reach": 6, "activity": "forage", "labor_assignments": []},
		{"entity": 802, "faction": 0, "size": 40, "current_x": 68, "current_y": 12,
			"working_age": 6, "idle_workers": 2, "hunt_reach": 6, "activity": "hunt", "labor_assignments": []},
	]

## Distance-aware herd-hunt (docs/plan_exploration_and_sites.md §2b): two player bands at DIFFERENT
## distances from ONE herd — a NEAR band ON the herd tile (within hunt_reach → LOCAL hunt) and a FAR
## band ~27 tiles away (beyond reach → hunting EXPEDITION). Proves the SELECTED band (band-picker)
## drives the local-vs-expedition label + command + band-entity target — the case single-band
## playtest can't surface. Both carry idle workers + a party cap so either verb is dialable.
func _hunt_distance_bands() -> Array:
	return [
		{"entity": 811, "faction": 0, "size": 120, "current_x": 66, "current_y": 10,
			"working_age": 14, "idle_workers": 10, "hunt_reach": 7, "max_expedition_party_size": 8,
			"activity": "forage", "labor_assignments": []},
		{"entity": 812, "faction": 0, "size": 80, "current_x": 86, "current_y": 24,
			"working_age": 10, "idle_workers": 6, "hunt_reach": 7, "max_expedition_party_size": 8,
			"activity": "hunt", "labor_assignments": []},
	]

## Range-aware forage: two player bands at DIFFERENT distances from the (66,10) food tile — a NEAR band
## 1 tile away (within work_range 2 → forage ENABLED) and a FAR band ~21 tiles away (beyond range →
## forage DISABLED + out-of-range hint). Foraging is stationary gathering, so out-of-range has NO
## expedition fallback — just a disabled button. Proves the SELECTED band (band-picker) drives the
## enabled-vs-disabled state — the case single-band playtest can't surface.
func _forage_range_bands() -> Array:
	return [
		{"entity": 821, "faction": 0, "size": 120, "current_x": 67, "current_y": 10,
			"working_age": 14, "idle_workers": 10, "work_range": 2, "activity": "forage", "labor_assignments": []},
		{"entity": 822, "faction": 0, "size": 80, "current_x": 80, "current_y": 24,
			"working_age": 10, "idle_workers": 6, "work_range": 2, "activity": "forage", "labor_assignments": []},
	]

## The herd the distance-aware states select — the same (66,10) herd but a NON-food tile_info, so the
## Tile card drops its "Assign foragers" block and the hunt button + distance hint sit in-frame.
func _hunt_distance_herd() -> Dictionary:
	var herd := _herd_fixture()
	herd["tile_info"] = {
		"x": 66, "y": 10,
		"terrain_label": "Prairie Steppe",
		"tags_text": "Fertile",
		"visibility_state": "active",
		"food_module": "",
		"food_module_label": "None",
	}
	return herd

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
		# A discovered Wondrous Site on this tile → the Tile card shows a "Site: …" line.
		"site_name": "Verdant Basin",
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
