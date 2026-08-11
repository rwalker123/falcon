extends Node

## Headless gate for the **client → server COMMAND path** — the direction `decode_guard` does not
## cover.
##
## ## The gap this closes
##
## `decode_guard` proves the client reads what the server sends. Nothing proved the client SENDS
## what the server reads, and the cost of that showed up in play: the sim was changed to resolve a
## band by its durable `BandId`, the client kept sending ECS `entity` bits, and **both are `u64`**.
## Nothing failed to compile, nothing failed to parse, nothing failed a test — the server looked up
## a band that did not exist and no-op'd. Every band-addressed order (`assign_labor`, `move_band`,
## `cancel_order`, `send_expedition`, `send_hunt_expedition`, `recall_expedition`) silently stopped
## working, and a human found it by playing.
##
## A grep would not have caught it: `int(band.get("entity", -1))` is perfectly valid GDScript that
## simply MEANT something different than the server thought. The only assertion that catches this
## class is a VALUE one — run the client's real emit path and check the number that comes out.
##
## ## How it works
##
## 1. Instance the real `HudLayer` + `BandCityPanel`, and push a canned snapshot roster through the
##    real ingest (`update_band_alerts` / `update_herds` / `set_grid_dimensions`) — plus a real
##    `MapView`, so the selected-unit MARKER path is exercised and not just the raw cohort dict.
## 2. Drive each band-addressed command through the code the player's click reaches, and capture the
##    payload off the HUD's own signal.
## 3. Format each captured payload with `Main`'s own `format_*` builders — the pure statics
##    `Main._on_hud_*` calls — so the recorded text is byte-for-byte what the client transmits.
## 4. Write those lines to `ui_preview_out/emitted_band_commands.json` — each as
##    `{kind, line, expected_kit}` — where **`cargo xtask command-guard` parses every one with the
##    REAL server-side parser** (`sim_runtime::command_text::parse_command_line`, the same function
##    the native `CommandBridge` runs) and asserts the band handle equals the fixture's `band_id`
##    AND the parsed kit equals `expected_kit`.
##
## **THE KIT TAIL IS ASSERTED FOR THE HANDLE'S OWN REASON.** `Main._kit_token` omits `kit <id>`
## whenever the selection equals the job default, so a line that merely PARSES says nothing about the
## kit: if `_kit_token` regressed to `""`, every drive below would still parse, `EXPECTED_KINDS` would
## still count, and this harness would report PASS while no kit ever left the client. `_record`
## therefore states, per line, which kit the parser must recover.
##
## **THE FIXTURE'S `entity` AND `band_id` ARE DELIBERATELY DIFFERENT VALUES.** If they agreed this
## harness would prove nothing at all — sending the wrong handle would produce the right number.
## That coincidence is exactly how the defect hid.
##
##   cargo xtask command-guard                                    # the whole gate
##   godot --headless --path . res://tools/command_guard.tscn     # this half alone
##
## Exits 0 on PASS, 1 on FAIL (CI-usable). No GPU or viewport needed — nothing here renders.

const HUD_SCENE := preload("res://src/ui/HudLayer.tscn")
const BAND_PANEL_SCENE := preload("res://src/ui/BandCityPanel.tscn")
const MAP_VIEW_SCRIPT := preload("res://src/scripts/MapView.gd")
## `Main`'s command-text builders are pure statics precisely so they can be reached without standing
## up the app scene — the `escape_claimant` precedent.
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")
## The world's kit roster, shared with both preview harnesses — one roster, one set of ids, so the
## `kit <id>` token this guard emits is the token those frames are read against.
const BandFx := preload("res://tools/ui_preview/fixtures_band.gd")

## Scratch prefs, never the player's real ones (the `band_panel_preview` rule).
const GUARD_PREFS_PATH := "user://command_guard_prefs.cfg"
const GUARD_DOCK_PREFS_PATH := "user://command_guard_dock.cfg"

const OUT_DIR := "res://ui_preview_out"
const OUT_PATH := "res://ui_preview_out/emitted_band_commands.json"

# ---- The fixture --------------------------------------------------------------------------------
#
# THE TWO HANDLES MUST NEVER BE EQUAL — see the header. They are also chosen so neither renders as a
# substring of the other, so even an eyeball of the emitted line can tell them apart.

## The band's ECS entity bits: client-local identity (selection, marker keys, the pending overlay).
## It must appear in NO emitted command.
const BAND_ENTITY := 904
## The band's durable `PopulationCohortState.bandId`: the ONE handle a command may name.
const BAND_ID := 71204

## The same pair for a detached hunting party — `recall_expedition` takes its band id too.
const PARTY_ENTITY := 952
const PARTY_ID := 71252

const FACTION := 0
const BAND_X := 40
const BAND_Y := 20
## Where `move_band` / `send_expedition` are told to go — inside the grid, away from the band.
const TARGET_X := 44
const TARGET_Y := 23
const GRID_W := 80
const GRID_H := 52

## The band can work a source this far out, and hunt this far out. The quarry sits BEYOND the hunt
## reach, which is what makes it an expedition's job rather than a local hunt.
const BAND_WORK_RANGE := 2
const BAND_HUNT_REACH := 3
const BAND_IDLE_WORKERS := 6
const BAND_WORKING_AGE := 16
const BAND_SIZE := 30
const PARTY_WORKERS := 2

const NEAR_HERD_ID := "game_deer_07"
const FAR_HERD_ID := "game_boar_04"
const FAR_HERD_X := 52
const FAR_HERD_Y := 20
## One Wild Boar's worth of food — the quantum the raid table is built from.
const FOOD_PER_ANIMAL := 4.0
## The raid table's fixed shape: every party size takes this many animals over this many turns. Flat
## on purpose — this harness asserts a HANDLE, not a forecast, so the numbers only have to be
## coherent enough that the compose sheet renders a viable raid with an enabled Send.
const RAID_ANIMALS := 8
const RAID_TURNS := 6
const RAID_MAX_PARTY := 8

## Frames to let the HUD/panel rebuild between drives. Nothing renders, so this is layout settling
## only — the controls have to exist before a button can be pressed.
const SETTLE_FRAMES := 3
## The worker count the split drive leaves the stepper at. Any value clear of the floors will do —
## this guard asserts the LINE the client builds, not whether the sim would accept it.
const SPLIT_WORKERS := 5

# ---- Recording ----------------------------------------------------------------------------------

var _hud: Node = null
var _panel: Node = null
var _emitted: Array = []
var _failures: Array = []

func _ready() -> void:
	NarrativeForkPanel.config_path_override = GUARD_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(GUARD_PREFS_PATH))
	BandCityPanel.config_path_override = GUARD_DOCK_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(GUARD_DOCK_PREFS_PATH))

	_hud = HUD_SCENE.instantiate()
	add_child(_hud)
	_panel = BAND_PANEL_SCENE.instantiate()
	add_child(_panel)
	_hud.set_band_city_panel(_panel)

	_connect_recorders()

	await _settle()
	_hud.set_grid_dimensions({"width": GRID_W, "height": GRID_H, "wrap_horizontal": false})
	_hud.update_herds(_herd_fixtures())
	_hud.update_band_alerts([_band_fixture(), _party_fixture()])
	# The kit roster, so every compose sheet below resolves a real selection and the `kit <id>` tail
	# is a token the REAL parser has to accept rather than one the client never emits.
	_hud.update_kit_roster(BandFx.kit_roster_fixture(),
		BandFx.KIT_DEFAULT_HUNT, BandFx.KIT_DEFAULT_FORAGE,
		BandFx.KIT_DEFAULT_SCOUT, BandFx.KIT_DEFAULT_WARRIOR)
	await _settle()

	await _drive_assign_labor()
	await _drive_cancel_order()
	await _drive_move_band()
	await _drive_send_expedition()
	await _drive_recall_expedition()
	await _drive_split_band()
	await _drive_send_hunt_expedition_from_band_panel()
	await _drive_send_hunt_expedition_from_herd_drawer()
	await _drive_send_denial_raid()
	await _drive_assign_labor_kits()

	_assert_every_command_emitted()
	_write_emitted()
	_finish()

# ---- Drivers ------------------------------------------------------------------------------------
#
# Each drives the path a player's click actually takes, as far up as the harness can reach without a
# real mouse. Where a payload is built inside an inline `pressed` lambda (both hunting-expedition
# sites) the REAL button is pressed, found by its `HudWidgets.SEND_HUNT_CONFIRM_META` — its face is
# the raid verdict, so text is the one thing that cannot identify it.

## `assign_labor` — the map's double-click quick-hunt, which is fully public and resolves the band
## itself, so this is the whole chain: snapshot roster → `_resolve_assign_band` → `_emit_assign_labor`.
func _drive_assign_labor() -> void:
	_hud.quick_assign_hunters(NEAR_HERD_ID)
	await _settle()

## `cancel_order` — the Work zone's "Unassign all work". The HUD relays the BAND DICT itself here
## (there is no HUD-side payload build), so `Main.format_cancel_order` is what reads the handle, and
## the band handed to it is the one the panel holds.
func _drive_cancel_order() -> void:
	_hud.cancel_order_requested.emit(_hud._band_labor.panel_band(), "work")
	await _settle()

## `move_band` — SELECT THE BAND ON THE MAP FIRST, so `_resolve_assign_band` returns the MapView
## MARKER rather than the raw cohort. That is the harder half of the contract: the marker is a
## rebuilt copy, so a `band_id` missing from `MapView._rebuild_unit_markers` emits band 0 while every
## snapshot-path drive still passes.
func _drive_move_band() -> void:
	_select_band_marker_from_map()
	await _settle()
	_hud._targeting.begin_move_band()
	_hud._targeting.try_dispatch({"x": TARGET_X, "y": TARGET_Y})
	await _settle()

## `send_expedition` — outfit a party off the selected band, then click the destination tile.
func _drive_send_expedition() -> void:
	_hud._targeting.begin_send_expedition(_hud._band_labor.panel_band(), PARTY_WORKERS)
	_hud._targeting.try_dispatch({"x": TARGET_X, "y": TARGET_Y})
	await _settle()

## `recall_expedition` — the parties zone's row `✕` (its confirm dialog wraps this same call).
func _drive_recall_expedition() -> void:
	_hud._bandpanel._on_recall_expedition_pressed(_party_fixture())
	await _settle()

## `split_band` — the parties zone's Form-a-new-band sheet, driven against the BAND rather than a
## party: fission divides the band where it stands, so the payload names the band the sheet was
## opened on and the worker count the stepper was left at.
func _drive_split_band() -> void:
	_hud._bandpanel._on_split_band_pressed(_band_fixture(), SPLIT_WORKERS)
	await _settle()

## `send_hunt_expedition`, site 1 of 2 — the Band panel's parties compose sheet.
func _drive_send_hunt_expedition_from_band_panel() -> void:
	_hud._selection.clear()
	_panel.set_active_tab(&"parties")
	_hud._bandpanel._party_compose_open = true
	_hud._bandpanel._party_compose_mission = "hunt"
	_hud._compose.set_party_quarry(FAR_HERD_ID)
	# **A NON-DEFAULT KIT, so the line carries the tail rather than omitting it.** `Main._kit_token`
	# omits `kit <id>` when the selection equals the job default — which is the shipped case and is
	# byte-identical to the pre-roster line — so composing the default here would test nothing new.
	_hud._compose.set_party_kit_id(BandFx.KIT_ID_NONE)
	_hud._bandpanel.rerender()
	await _settle()
	_press_send_hunt_confirm(_panel, "band panel parties compose")
	await _settle()
	_hud._bandpanel._party_compose_open = false
	_hud._bandpanel._party_compose_mission = ""
	_hud._compose.clear_party_quarry()

## `send_denial_raid` (`docs/plan_denial_raid.md`) — the parties compose sheet's THIRD mission. Its
## own driver and its own confirm meta, because it is its own command: the grammar is CLOSED at four
## tokens (`send_denial_raid <faction> <band> <party> <fauna_id>`) and a fifth is a hard parse error,
## so a payload that picked up a floor or a fill target would be REJECTED by the real parser this
## gate runs — which is exactly the assertion worth having, and one no client-side test can make.
func _drive_send_denial_raid() -> void:
	_hud._selection.clear()
	_panel.set_active_tab(&"parties")
	_hud._bandpanel._party_compose_open = true
	_hud._bandpanel._party_compose_mission = HudComposeVocab.COMPOSE_MISSION_DENY
	_hud._compose.set_party_quarry(FAR_HERD_ID)
	# The one order the closed four-token grammar still admits, and the reason this drive matters
	# most: a `kit <id>` pair the parser refuses would be a hard parse error here.
	_hud._compose.set_party_kit_id(BandFx.KIT_ID_NONE)
	_hud._bandpanel.rerender()
	await _settle()
	_press_meta_button(_panel, HudWidgets.SEND_DENIAL_CONFIRM_META, "band panel denial compose")
	await _settle()
	_hud._bandpanel._party_compose_open = false
	_hud._bandpanel._party_compose_mission = ""
	_hud._compose.clear_party_quarry()

## `send_hunt_expedition`, site 2 of 2 — the herd drawer's assign control, which flips to the
## expedition branch because the quarry lies beyond the band's `hunt_reach`.
func _drive_send_hunt_expedition_from_herd_drawer() -> void:
	var herd := _far_herd_fixture()
	_hud.show_herd_selection(herd)
	await _settle()
	# `open_herd_compose` takes the herd it is composing for — it gates on `_herd_compose_available`
	# and keys the compose state off `herd.id`, so the drawer's own selection is not enough.
	#
	# **TWO OPENS, and the order is forced from both ends.** The FIRST open is the source change, which
	# drops the composed kit (`ComposeState.reset_hunt_kit`) so the sheet takes THIS herd's own default
	# — so a selection written before it does not survive to be composed. The SECOND names the same
	# herd, so it is not a source change and the pick stands; it also re-renders, which matters because
	# the commit button's payload is captured in a `pressed` closure built during the render, so a
	# selection written after the LAST render is not the one the button carries. Either half alone
	# emits the untailed line and the drive silently asserts nothing about the kit — which is exactly
	# what `_record`'s job-default check refuses.
	_hud._drawercompose.open_herd_compose(herd)
	await _settle()
	_hud._compose.set_hunt_kit_id(BandFx.KIT_ID_NONE)
	_hud._drawercompose.open_herd_compose(herd)
	await _settle()
	_press_send_hunt_confirm(_hud, "herd drawer compose")
	await _settle()

## `assign_labor` with the KIT TAIL, on ALL THREE grammars (`docs/plan_denial_raid.md`). The
## quick-hunt drive above emits the untailed line (it names no kit, so the job default stands and the
## token is omitted); this one emits the tailed twin of each, which is what puts `kit <id>` in front
## of the real parser on the forage grammar's two optional positionals, on the hunt grammar, and on a
## BAND-WIDE role's otherwise closed four-token tail.
##
## It reaches `HudLayer._emit_assign_labor` DIRECTLY rather than through a compose sheet, and that is
## deliberate: what is under test here is the LINE, and the two compose sheets' own kit plumbing is
## asserted in the preview harnesses, where the picker can be read back. Standing up a tile card here
## would buy a second copy of that coverage and a forage fixture this file has no other use for.
func _drive_assign_labor_kits() -> void:
	var band: Dictionary = _hud._band_labor.panel_band()
	_hud._emit_assign_labor(band, SourceForecast.LABOR_KIND_HUNT, PARTY_WORKERS,
		int(band.get("current_x", 0)), int(band.get("current_y", 0)), NEAR_HERD_ID,
		SourceForecast.DEFAULT_HARVEST_FLOOR, "", SourceForecast.IMPROVEMENT_NONE,
		BandFx.KIT_ID_NONE)
	await _settle()
	_hud._emit_assign_labor(band, SourceForecast.LABOR_KIND_FORAGE, PARTY_WORKERS,
		TARGET_X, TARGET_Y, "", SourceForecast.DEFAULT_HARVEST_FLOOR, "",
		SourceForecast.IMPROVEMENT_NONE, BandFx.KIT_ID_NONE)
	await _settle()
	# **THE THIRD GRAMMAR — A BAND-WIDE ROLE.** `assign_labor <faction> <band> scout <workers>` takes
	# no tile, no herd, no floor and no species, so its tail was CLOSED and the kit token had never
	# once been put in front of the real parser on it. The role cards mount a picker now, so the line
	# is emittable from the UI; this is the drive that proves the server accepts it. `scout` stands
	# for the pair — Warrior parses through the identical arm of `handle_assign_labor`, so a second
	# emit would buy a duplicate rather than a second claim.
	_hud._emit_assign_labor(band, HudConst.LABOR_KIND_SCOUT, PARTY_WORKERS, -1, -1, "",
		SourceForecast.DEFAULT_HARVEST_FLOOR, "", SourceForecast.IMPROVEMENT_NONE,
		BandFx.KIT_ID_NONE)
	await _settle()

## Push the band through a REAL MapView and click its hex, so the HUD's selected unit is the marker
## `_rebuild_unit_markers` built — not the cohort dict the snapshot path holds.
func _select_band_marker_from_map() -> void:
	var view: Node2D = Node2D.new()
	view.set_script(MAP_VIEW_SCRIPT)
	add_child(view)
	var terrain: Array = []
	terrain.resize(GRID_W * GRID_H)
	terrain.fill(0)
	view.display_snapshot({
		"grid": {"width": GRID_W, "height": GRID_H, "wrap_horizontal": false},
		"overlays": {"terrain": terrain},
		"populations": [_band_fixture()],
	})
	view.unit_selected.connect(_hud.show_unit_selection)
	view.handle_hex_click(BAND_X, BAND_Y, MOUSE_BUTTON_LEFT)
	view.unit_selected.disconnect(_hud.show_unit_selection)
	view.queue_free()

## Press the meta-tagged "send hunting expedition" confirm somewhere under `root`.
func _press_send_hunt_confirm(root: Node, where: String) -> void:
	_press_meta_button(root, HudWidgets.SEND_HUNT_CONFIRM_META, where)

## Press a confirm found BY META, never by face — every launch button in this client wears its own
## verdict as its text. **Each mission has its OWN meta**, and that is not tidiness: a search for
## "the send button" on a parties compose sheet could not tell which MISSION it had just launched,
## and the two emit different signals with non-interchangeable payloads.
func _press_meta_button(root: Node, meta: String, where: String) -> void:
	var button := _find_meta_button(root, meta)
	if button == null:
		_fail("no `%s` confirm button found in the %s" % [meta, where])
		return
	if button.disabled:
		_fail("the %s's confirm is disabled — the fixture order must be launchable" % where)
		return
	button.pressed.emit()

func _find_meta_button(node: Node, meta: String) -> Button:
	if node is Button and node.has_meta(meta):
		return node
	for child in node.get_children():
		var found := _find_meta_button(child, meta)
		if found != null:
			return found
	return null

# ---- Recording ----------------------------------------------------------------------------------

func _connect_recorders() -> void:
	_hud.assign_labor_requested.connect(func(p: Dictionary) -> void:
		_record("assign_labor", p, MAIN_SCRIPT.format_assign_labor(p)))
	_hud.move_band_requested.connect(func(p: Dictionary) -> void:
		_record("move_band", p, MAIN_SCRIPT.format_move_band(p)))
	_hud.send_expedition_requested.connect(func(p: Dictionary) -> void:
		_record("send_expedition", p, MAIN_SCRIPT.format_send_expedition(p)))
	_hud.send_hunt_expedition_requested.connect(func(p: Dictionary) -> void:
		_record("send_hunt_expedition", p, MAIN_SCRIPT.format_send_hunt_expedition(p)))
	_hud.send_denial_raid_requested.connect(func(p: Dictionary) -> void:
		_record("send_denial_raid", p, MAIN_SCRIPT.format_send_denial_raid(p)))
	_hud.recall_expedition_requested.connect(func(p: Dictionary) -> void:
		_record("recall_expedition", p, MAIN_SCRIPT.format_recall_expedition(p)))
	_hud.split_band_requested.connect(func(p: Dictionary) -> void:
		_record("split_band", p, MAIN_SCRIPT.format_split_band(p)))
	_hud.cancel_order_requested.connect(func(band: Dictionary, scope: String) -> void:
		_record("cancel_order", band, MAIN_SCRIPT.format_cancel_order(band, scope)))

## The commands whose grammar carries a `kit <id>` tail. Every OTHER kind records `expected_kit` as
## `""` — a command with no kit axis names no kit, which is what the Rust half's `NotKitBearing`
## answer means.
const KIT_BEARING_KINDS := {
	"assign_labor": true,
	"send_hunt_expedition": true,
	"send_denial_raid": true,
}

## Record one emitted command. A builder that DECLINES (empty dict) is itself a failure here: every
## drive below is a well-formed order, so "nothing to send" means a handle went missing.
##
## **It records the kit the line is EXPECTED to carry**, and `cargo xtask command-guard` asserts the
## real server parser recovers exactly that. Without it the four kit drives proved only that a line
## PARSES: `Main._kit_token` regressed to `""` would leave every line valid, every `EXPECTED_KINDS`
## count intact, and the gate green while no kit ever left the client.
##
## The expectation is the DRIVE'S OWN `kit_id`, taken off the payload it composed — not a second copy
## of `_kit_token`'s rule. The one case that would make it circular is a drive composing the job
## DEFAULT, where the token is legitimately omitted and the assertion could never fail; that is a
## fixture error and fails here rather than passing quietly.
func _record(kind: String, payload: Dictionary, formatted: Dictionary) -> void:
	if formatted.is_empty():
		_fail("%s: Main declined to build a line — a required field was missing from %s" % [kind, payload])
		return
	var expected_kit := ""
	if KIT_BEARING_KINDS.has(kind):
		expected_kit = String(payload.get("kit_id", "")).strip_edges()
		var job_default := String(payload.get("default_kit_id", "")).strip_edges()
		if expected_kit != "" and expected_kit == job_default:
			_fail("%s: the drive composed the JOB DEFAULT (%s), so `Main._kit_token` omits the tail and this line asserts nothing about the kit" % [kind, expected_kit])
			return
	_emitted.append({"kind": kind, "line": String(formatted["line"]), "expected_kit": expected_kit})

## The commands this guard must see. Missing one is a failure: a driver that quietly stopped
## reaching its emit site would otherwise turn this guard green by producing nothing to check.
const EXPECTED_KINDS := {
	# FOUR — the map's quick-hunt (which names no kit, so the line is the untailed one) plus the three
	# `_drive_assign_labor_kits` emits that put `kit <id>` on all three grammars: hunt, forage and a
	# BAND-WIDE role, whose otherwise closed tail had never been parsed with the token on it.
	"assign_labor": 4,
	"cancel_order": 1,
	"move_band": 1,
	"send_expedition": 1,
	"recall_expedition": 1,
	# Fission's own verb. It names a BAND rather than a party, so the handle assertion is what proves
	# the client does not send entity bits down the split either.
	"split_band": 1,
	# TWO — the Band panel's parties compose and the herd drawer's, which build their payloads
	# independently and so can drift apart.
	"send_hunt_expedition": 2,
	# ONE — the parties compose sheet is the denial raid's only launch site.
	"send_denial_raid": 1,
}

func _assert_every_command_emitted() -> void:
	var counts: Dictionary = {}
	for entry in _emitted:
		var kind := String(entry["kind"])
		counts[kind] = int(counts.get(kind, 0)) + 1
	for kind in EXPECTED_KINDS:
		var want := int(EXPECTED_KINDS[kind])
		var got := int(counts.get(kind, 0))
		if got != want:
			_fail("expected %d `%s` command(s), captured %d" % [want, kind, got])

func _write_emitted() -> void:
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(OUT_DIR))
	var doc := {
		# What the Rust half asserts against. Both halves are recorded so a failure report can say
		# "this is the entity, and it is what you sent" rather than only "wrong number".
		"band_id": BAND_ID,
		"band_entity": BAND_ENTITY,
		"expedition_band_id": PARTY_ID,
		"expedition_entity": PARTY_ENTITY,
		"faction": FACTION,
		"commands": _emitted,
	}
	var file := FileAccess.open(OUT_PATH, FileAccess.WRITE)
	if file == null:
		_fail("could not write %s" % OUT_PATH)
		return
	file.store_string(JSON.stringify(doc, "  "))
	file.close()
	print("command_guard: wrote %d command(s) to %s" % [_emitted.size(), OUT_PATH])
	for entry in _emitted:
		print("command_guard:   %s -> %s" % [entry["kind"], entry["line"]])

# ---- Fixtures -----------------------------------------------------------------------------------

## The player band, in the shape `population_to_dict` emits. `entity` and `band_id` differ — see the
## header; that difference is the whole point of the fixture.
func _band_fixture() -> Dictionary:
	return {
		"id": "Band 1",
		"entity": BAND_ENTITY,
		"band_id": BAND_ID,
		"faction": FACTION,
		"size": BAND_SIZE,
		"pos": [BAND_X, BAND_Y],
		"current_x": BAND_X,
		"current_y": BAND_Y,
		"working_age": BAND_WORKING_AGE,
		"idle_workers": BAND_IDLE_WORKERS,
		"age_children": 9.0,
		"age_working": float(BAND_WORKING_AGE),
		"age_elders": float(BAND_SIZE - BAND_WORKING_AGE - 9),
		"turns_of_food": 20.0,
		"morale": 0.8,
		"stores": {"provisions": 80.0},
		"output_multiplier": 1.0,
		"work_range": BAND_WORK_RANGE,
		"hunt_reach": BAND_HUNT_REACH,
		"max_expedition_party_size": RAID_MAX_PARTY,
		"band_move_tiles_per_turn": 1.0,
		"hunt_per_worker_provisions": 0.4,
		"labor_assignments": [],
	}

## A detached hunting party homed on the band above — the `recall_expedition` subject.
func _party_fixture() -> Dictionary:
	return {
		"id": "Hunters 1",
		"entity": PARTY_ENTITY,
		"band_id": PARTY_ID,
		"faction": FACTION,
		"size": PARTY_WORKERS,
		"current_x": BAND_X + 1,
		"current_y": BAND_Y,
		"turns_of_food": 8.0,
		"is_expedition": true,
		"expedition_mission": "hunt",
		"expedition_phase": "hunting",
		"expedition_target_herd": FAR_HERD_ID,
		"expedition_floor": 0.5,
		"home_band_entity": BAND_ENTITY,
	}

func _herd_fixtures() -> Array:
	return [_near_herd_fixture(), _far_herd_fixture()]

## A herd INSIDE `hunt_reach` — the local-hunt subject the quick-hunt shortcut assigns to.
func _near_herd_fixture() -> Dictionary:
	return {
		"id": NEAR_HERD_ID, "species": "Red Deer",
		"x": BAND_X + 1, "y": BAND_Y,
		"population": 90, "ecology_phase": "thriving", "huntable": true,
		"per_worker_yield": 0.5, "food_per_animal": FOOD_PER_ANIMAL,
		# The TERMS the client composes a ceiling from: `max(0, B - floor*K) x rate` at any floor
		# (`docs/plan_harvest_floor.md` §5). The retired per-stance table this replaced could only
		# answer four of them.
		"biomass": 90.0, "carrying_capacity": 100.0, "provisions_per_biomass": 0.0125,
	}

## A herd BEYOND `hunt_reach` — so both hunting-expedition compose surfaces take their expedition
## branch and offer an enabled Send.
func _far_herd_fixture() -> Dictionary:
	var herd := {
		"id": FAR_HERD_ID, "species": "Wild Boar",
		"x": FAR_HERD_X, "y": FAR_HERD_Y,
		"population": 140, "ecology_phase": "thriving", "huntable": true,
		"per_worker_yield": 0.8, "food_per_animal": FOOD_PER_ANIMAL,
		"biomass": 90.0, "carrying_capacity": 100.0,
		"provisions_per_biomass": 0.0075,
	}
	herd["hunt_trip_estimates"] = _raid_table()
	return herd

## A flat raid table — one cell per SAMPLED FLOOR × party size, mirroring the sim's own
## `RAID_FORECAST_FLOOR_SAMPLES`. Every cell delivers, so the Send button takes its ordinary enabled
## treatment and the drive is never blocked by a verdict.
##
## The row carries `floor` and `party_workers` as FIELDS: the client scans the rows rather than
## rebuilding the `"<floor>:<party>"` key, because the real key renders the floor with Rust's float
## Display and a GDScript-side near-miss would find nothing at all — silently.
func _raid_table() -> Dictionary:
	var table := {}
	for floor_value in [0.0, 0.15, 0.30, 0.50, 0.80]:
		for workers in range(1, RAID_MAX_PARTY + 1):
			table["%s:%d" % [str(floor_value), workers]] = {
				"floor": floor_value,
				"party_workers": workers,
				"turns_to_fill": RAID_TURNS,
				"delivers_food": true,
				"animals_taken": RAID_ANIMALS,
				"delivered_food": float(RAID_ANIMALS) * FOOD_PER_ANIMAL,
				"wasted_food": 0.0,
			}
	return table

# ---- Plumbing -----------------------------------------------------------------------------------

func _settle() -> void:
	for _i in SETTLE_FRAMES:
		await get_tree().process_frame

func _fail(message: String) -> void:
	_failures.append(message)
	push_error("command_guard: FAIL — %s" % message)
	printerr("command_guard: FAIL — %s" % message)

func _finish() -> void:
	if _failures.is_empty():
		print("command_guard: PASS — %d command(s) captured" % _emitted.size())
		get_tree().quit(0)
		return
	printerr("command_guard: FAILED with %d problem(s)" % _failures.size())
	get_tree().quit(1)
