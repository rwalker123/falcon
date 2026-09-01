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
## **THE SHIPMENT'S MANIFEST IS ASSERTED AGAINST THE PILES IT WAS DRAWN FROM**, and the piles are
## FRACTIONAL on purpose (`TRADE_FOOD_HELD_TICKS`). The cargo row's `+` clamps a press to the pile,
## so loading one to the end leaves the EXACT held amount on the row — and the server compares
## strictly, refusing a manifest naming a tick more than the band holds. An amount ROUNDED for
## legibility is therefore a refused shipment, and nothing else here could see it: the line parses,
## names the right band and carries the right kit. Each shipment entry records `cargo_held_ticks`,
## and the Rust half quantises what the real parser recovered and compares the two.
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
## The band's children; its elders are `BAND_SIZE − BAND_WORKING_AGE − BAND_CHILDREN`, since the
## three whole brackets partition the head count.
const BAND_CHILDREN := 9
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

# ---- The SHIPMENT's fixture (arc #527, issue #517) ----------------------------------------------

## The band the shipment is bound for, and the tie that lets it be. A THIRD durable id, unlike either
## of the two above so a destination confused with a sender is visible in the emitted line.
const DESTINATION_ID := 71309
const DESTINATION_LAST_SEEN_X := 46
const DESTINATION_LAST_SEEN_Y := 21
const DESTINATION_LAST_SEEN_TURN := 11
## Any strength above `TIE_STRENGTH_NONE` will do — the sheet's gate is `> 0`, and what this harness
## asserts is the LINE, not the ledger.
const TIE_STRENGTH_LIVE := 0.6

## **THE TWO PILES, AUTHORED IN THE SIM'S OWN FIXED-POINT TICKS** (`Scalar`, 10^6 to the unit) rather
## than as decimals, because the whole assertion is about the last digits: the Rust half compares what
## the server parser reconstructs against these exact integers.
##
## **21.050001 IS ADVERSARIAL ON PURPOSE, twice over.** A tenth rounds it UP to 21.1, which the server
## refuses outright; and flooring it onto the fixed-point grid alone still emits `21.050001`, which the
## parser's `f32` round-trip lands one tick ABOVE the pile. Only an amount that also backs off the
## 32-bit wire's own rounding survives, which is what `Main.cargo_wire_amount` does.
const TRADE_FOOD_HELD_TICKS := 21_050_001
## The material pile, fractional for the first of those two reasons. A batch is one pile of one
## material AT ONE RATING, so this one carries its reading like a real store row.
const TRADE_HIDE_HELD_TICKS := 4_567_891
const TRADE_HIDE_MATERIAL := "hide"
const TRADE_HIDE_AXIS := "tough"
const TRADE_HIDE_AXIS_VALUE := 0.9
const TRADE_HIDE_AXIS_BAND := "excellent"

## The shipment pack levers, sized so both piles fit at the drive's party — the meter gates the send,
## and a cargo this harness cannot send emits no line to parse.
const TRADE_PER_WORKER_CARRY := 120.0
const TRADE_MATERIAL_CARRY_WEIGHT := 1.0

## The most `+` presses one cargo row may take before the drive gives up. A whole-unit step over a
## ~21-unit pile needs 22, so this is generous rather than tight; it exists so a row whose `+` stops
## disabling fails HERE instead of spinning the harness forever.
const CARGO_LOAD_MAX_PRESSES := 64

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
	# The band's TIES, which are what the shipment form draws its destinations from — a sheet with no
	# live tie renders a sentence instead of a send, and the trade drive would have nothing to press.
	_hud.update_connections(_connection_fixtures())
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
	await _drive_build_kit()
	await _drive_build_order()
	await _drive_send_trade_expedition()
	_drive_road_verbs()

	_assert_every_command_emitted()
	_assert_every_role_is_emittable()
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
	# **THE THIRD GRAMMAR — A BAND-WIDE ROLE, AND EVERY ROLE, NOT A REPRESENTATIVE ONE.**
	# `assign_labor <faction> <band> <role> <workers>` takes no tile, no herd, no floor and no
	# species, so its tail is CLOSED but for the kit token.
	#
	# ⛔ **IT DROVE `scout` ALONE, AND THE COMMENT SAID WHY: *"Warrior parses through the identical
	# arm of `handle_assign_labor`, so a second emit would buy a duplicate rather than a second
	# claim."* THAT REASONING IS WRONG, AND IT COST A WHOLE SLICE.** The roles share an arm in the
	# SIM, and the sim is not the only gate: `sim_runtime::command_text::parse_command_line`
	# enumerates them SEPARATELY, in another crate, and the client's native bridge runs that parser
	# BEFORE it sends. `builders` was missing from that enumeration from
	# `docs/plan_standing_upkeep.md` §2.5 onward, so every Builders staffing line was refused
	# locally — the pool has never been staffable through the UI, no build queue could ever move, and
	# the dock blamed the network. This guard was green throughout, because the one role it did not
	# drive was the one role that did not parse.
	#
	# **SO THE LIST IS THE CLAIM.** Every role `Main.format_assign_labor` can emit is driven, and a
	# role added there without being added here fails `_assert_every_role_is_emittable` below rather
	# than failing in play.
	for role in ASSIGN_LABOR_ROLES:
		# ⛔ **THE `builders` ROW TAKES NO `kit` TOKEN ANY MORE** (`docs/plan_standing_upkeep.md`
		# §4.7a ②). The builders' kit is a property of the queue ENTRY, and `handle_assign_labor`
		# REFUSES a `kit` token on this role by name — it parses and is then rejected, which is a
		# state this parser-level guard cannot see. So the role is swept BARE, and the per-entry
		# override is driven by `_drive_build_kit` below, which is where it now lives.
		var kit := KitRoster.NO_KIT_ID if role == HudConst.LABOR_KIND_BUILDERS \
			else BandFx.KIT_ID_NONE
		_hud._emit_assign_labor(band, String(role), PARTY_WORKERS, -1, -1, "",
			SourceForecast.DEFAULT_HARVEST_FLOOR, "", SourceForecast.IMPROVEMENT_NONE, kit)
		await _settle()
	# **AND THE BARE FORM OF THE ONE THAT BROKE.** `_kit_token` omits an empty selection, so this is
	# `assign_labor <f> <b> builders 3` with nothing after the count — the exact line the Builders
	# pool's `+` emits for a player who has never opened a kit picker, and therefore the exact line
	# the parser refused. The tailed form above and this one are two different parses.
	_hud._emit_assign_labor(band, HudConst.LABOR_KIND_BUILDERS, PARTY_WORKERS, -1, -1, "",
		SourceForecast.DEFAULT_HARVEST_FLOOR, "", SourceForecast.IMPROVEMENT_NONE,
		KitRoster.NO_KIT_ID)
	await _settle()

## **`build_kit` — THE PER-ENTRY BUILDERS KIT** (`docs/plan_standing_upkeep.md` §4.7a ②), driven on
## BOTH source forms, because a tile and a herd are two different grammars of one verb and a builder
## that gets the pair backwards passes either alone.
##
## **IT IS THE VERB THAT REPLACED A `kit` TOKEN ON THE `builders` ROW**, and it is band-addressed only
## in the sense that the SOURCE is: the line names no band at all, because every band holding that
## source holds the same entry. That is exactly the kind of thing this guard exists to pin — the
## grammar is the one place a client can be well-formed and mean something else.
##
## **A NON-DEFAULT KIT, DELIBERATELY.** Picking the DERIVED entry emits no `kit` token (that is how
## the override is cleared), and `_record` treats an expectation equal to the default as a fixture
## error — rightly, since the assertion could never fail there. The clearing case is asserted where it
## can be: `band_panel_preview` reads it off `Main.format_build_kit` on the live picker.
func _drive_build_kit() -> void:
	var band: Dictionary = _hud._band_labor.panel_band()
	# **`BUILD_RUNG_ANY` IS STATED, NOT DEFAULTED** — no plant or animal kit binds a rung, so the
	# unqualified ask is the right one here; the lookup takes no default so that a caller which
	# cannot name a rung has to say so rather than arrive at the refusal by omission.
	var derived := KitRoster.build_kit_for_branch(_hud._band_labor.kits(),
		KitRoster.BUILD_BRANCH_PLANT, KitRoster.BUILD_RUNG_ANY)
	_hud._bandpanel._emit_build_kit(band, {
		"kind": SourceForecast.LABOR_KIND_FORAGE, "x": TARGET_X, "y": TARGET_Y, "herd_id": "",
	}, BandFx.KIT_ID_NONE, derived)
	await _settle()
	_hud._bandpanel._emit_build_kit(band, {
		"kind": SourceForecast.LABOR_KIND_HUNT, "x": -1, "y": -1, "herd_id": NEAR_HERD_ID,
	}, BandFx.KIT_ID_NONE, KitRoster.build_kit_for_branch(_hud._band_labor.kits(),
		KitRoster.BUILD_BRANCH_ANIMAL, KitRoster.BUILD_RUNG_ANY))
	await _settle()

## **`build_order` — THE QUEUE'S REORDER** (`docs/plan_standing_upkeep.md` §4.7b ③), both source
## forms again.
##
## **THIS ONE DOES NAME A BAND, where `build_kit` and `unqueue` do not** — a queue belongs to a band —
## so it is squarely what this guard's handle assertion is for: the fixture's `entity` and `band_id`
## are deliberately different numbers, and a client sending entity bits down the reorder would produce
## a line that parses and moves someone else's queue.
func _drive_build_order() -> void:
	var band: Dictionary = _hud._band_labor.panel_band()
	_hud._bandpanel._emit_build_order(band, {
		"kind": SourceForecast.LABOR_KIND_FORAGE, "key": "forage:%d,%d" % [TARGET_X, TARGET_Y],
		"x": TARGET_X, "y": TARGET_Y, "herd_id": "",
	}, BUILD_ORDER_POSITION)
	await _settle()
	_hud._bandpanel._emit_build_order(band, {
		"kind": SourceForecast.LABOR_KIND_HUNT, "key": "hunt:%s" % NEAR_HERD_ID,
		"x": -1, "y": -1, "herd_id": NEAR_HERD_ID,
	}, SourceForecast.BUILD_QUEUE_HEAD)
	await _settle()

## The position the plant drive moves its entry to. **Not the head**, because 0 is what an
## uninitialised int and a dropped field both look like.
const BUILD_ORDER_POSITION := 2

## `send_trade_expedition` (arc #527, issue #517) — the parties compose sheet's FIFTH mission, and the
## one drive whose subject is an AMOUNT rather than a handle.
##
## **THE CARGO IS LOADED THROUGH THE ROWS' OWN `+`, TO THE END OF EACH PILE**, which is the whole
## point: `_set_cargo_amount` clamps a press to what the band holds, so the last press leaves the
## exact fractional held amount on the row — the documented one-press way to load a pile the stepper's
## whole-unit step cannot reach. What is under test is the AMOUNT the client then spells, so a drive
## that wrote the manifest directly would test its own arithmetic instead of the sheet's.
##
## The destination is seated directly rather than picked through the popup: an `OptionButton`'s popup
## is an embedded subwindow and this half runs `--headless`, and WHICH tie is chosen is asserted by
## `ui_preview`'s `trade_picker_destination`, where the pick is a real pointer gesture.
func _drive_send_trade_expedition() -> void:
	_hud._selection.clear()
	# ⛔ **DROP THE ROLE SWEEP'S OPTIMISTIC OVERLAY FIRST, or this drive is testing the wrong band.**
	# `_drive_assign_labor_kits` above puts `PARTY_WORKERS` on EVERY band-wide role, and each of those
	# is recorded as a pending assignment the instant it is emitted — so by the time the sweep ends,
	# the fixture band's whole workforce is optimistically spoken for and `effective_idle` is 0. The
	# parties compose sheet does not render at a compose pool of 0 (there is nobody to send), so this
	# drive found no destination row, no cargo rows and no confirm button, and reported four failures
	# about a shipment form that had simply not been drawn.
	#
	# **It bit when the `roadwork` role landed** (arc #532), which is the sweep's sixth entry and the
	# one that took the pool past zero — but the coupling was there all along and the next role would
	# have found it again. `reconcile_pending` at a later turn is exactly what a fresh snapshot does
	# to these entries, so the drive starts from the band the wire describes rather than from the
	# previous drive's optimism.
	_hud._band_labor.reconcile_pending(_hud._band_labor.current_turn() + 1)
	_panel.set_active_tab(&"parties")
	_hud._bandpanel._party_compose_open = true
	_hud._bandpanel._party_compose_mission = HudComposeVocab.COMPOSE_MISSION_TRADE
	_hud._bandpanel._trade_destination_band = DESTINATION_ID
	_hud._bandpanel.rerender()
	await _settle()
	await _load_whole_pile(HudComposeVocab.COMPOSE_CARGO_FOOD_LABEL)
	await _load_whole_pile(TRADE_HIDE_MATERIAL)
	_press_meta_button(_panel, HudWidgets.SEND_TRADE_CONFIRM_META, "band panel trade compose")
	await _settle()

## Press one cargo row's `+` until the pile is loaded whole — the button DISABLES at the ceiling, which
## is the harness's signal that the clamp has left the exact held amount on the row.
func _load_whole_pile(needle: String) -> void:
	var presses := 0
	while presses < CARGO_LOAD_MAX_PRESSES:
		var plus := _cargo_plus_button(_panel, needle)
		if plus == null:
			_fail("no cargo row for `%s` on the shipment sheet" % needle)
			return
		if plus.disabled:
			break
		plus.pressed.emit()
		presses += 1
		await _settle()
	if presses == 0 or presses >= CARGO_LOAD_MAX_PRESSES:
		_fail("the cargo row for `%s` never filled (%d presses) — the pile must be loadable and its `+` must stop"
			% [needle, presses])

## The `+` of the cargo row whose name label contains `needle`. A cargo row is a name label followed by
## the shared stepper faces, so the `+` is the row's LAST child — found structurally, since a text match
## would find every stepper on the sheet.
func _cargo_plus_button(root: Node, needle: String) -> Button:
	if root is HBoxContainer:
		var row := root as HBoxContainer
		var count := row.get_child_count()
		if count > 0 and row.get_child(0) is Label \
				and (row.get_child(0) as Label).text.contains(needle):
			var last := row.get_child(count - 1)
			if last is Button and (last as Button).text == HudWorkVocab.STEPPER_PLUS_FACE:
				return last as Button
	for child in root.get_children():
		var found := _cargo_plus_button(child, needle)
		if found != null:
			return found
	return null

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
	_hud.build_kit_requested.connect(func(p: Dictionary) -> void:
		_record("build_kit", p, MAIN_SCRIPT.format_build_kit(p)))
	_hud.build_order_requested.connect(func(p: Dictionary) -> void:
		_record("build_order", p, MAIN_SCRIPT.format_build_order(p)))
	_hud.cancel_order_requested.connect(func(band: Dictionary, scope: String) -> void:
		_record("cancel_order", band, MAIN_SCRIPT.format_cancel_order(band, scope)))
	# **THE SHIPMENT RECORDS ITS PILES BESIDE ITS LINE.** They are the only thing the emitted amounts
	# mean anything against, and they are stated in the sim's own TICKS so the comparison is exact —
	# see the header. The cargo ids are the sender's own store keys, which is what the parser rebuilds.
	_hud.send_trade_expedition_requested.connect(func(p: Dictionary) -> void:
		_record("send_trade_expedition", p, MAIN_SCRIPT.format_send_trade_expedition(p), {
			HudConst.STORE_ITEM_PROVISIONS: TRADE_FOOD_HELD_TICKS,
			TRADE_HIDE_MATERIAL: TRADE_HIDE_HELD_TICKS,
		}))

## ⛔ **THE ROUTE BRANCH'S TILE VERBS — the only tile commands that NAME A BAND.**
##
## `grade <faction> <band> <x> <y>` and `pave` likewise: `cultivate`/`sow`'s grammar plus a band
## token, because a patch's keeper is whoever is already foraging it and **a road has no work row at
## all**, so who will keep the tile has to be said out loud. Issuing one declares the job and names
## the keeper in the same act.
##
## **THE EXTRA TOKEN IS EXACTLY WHAT THIS GUARD EXISTS FOR.** Both handles here are `u64`-ish
## integers in a positional grammar, so a builder that emitted `cultivate`'s four-token form would
## still parse — the sim would read the BAND as the x coordinate and grade a tile nobody asked for,
## with nothing failing anywhere. The Rust half asserts the parsed band equals the fixture's
## `band_id`, and the fixture's `entity` and `band_id` are deliberately different numbers.
##
## Driven straight through `Main.format_improvement`, the pure static the compose sheet's press
## reaches: there is no road control on the shipped HUD yet, so a click path would be a fiction and
## the builder is the whole of the client's side of this verb.
func _drive_road_verbs() -> void:
	for improvement in SourceForecast.ROUTE_IMPROVEMENTS:
		var payload := {
			"improvement": improvement,
			"faction": HudConst.PLAYER_FACTION_ID,
			"band_id": BAND_ID,
			"x": TARGET_X,
			"y": TARGET_Y,
		}
		_record(String(improvement), payload, MAIN_SCRIPT.format_improvement(payload))
	# **AND A ROAD VERB WITH NO BAND BUILDS NOTHING**, which is a refusal rather than a default: the
	# token IS the keeper, and guessing one would commit somebody else's people to a standing bill.
	# Asserted here rather than left to the parser, because a line missing its band token parses as
	# the tile form of some OTHER verb rather than failing.
	var bandless := {
		"improvement": SourceForecast.IMPROVEMENT_GRADE,
		"faction": HudConst.PLAYER_FACTION_ID,
		"x": TARGET_X,
		"y": TARGET_Y,
	}
	if not MAIN_SCRIPT.format_improvement(bandless).is_empty():
		_fail("grade: a road verb with no band built a line — the keeper token is not optional")

## The commands whose grammar carries a `kit <id>` tail. Every OTHER kind records `expected_kit` as
## `""` — a command with no kit axis names no kit, which is what the Rust half's `NotKitBearing`
## answer means.
const KIT_BEARING_KINDS := {
	"assign_labor": true,
	"send_hunt_expedition": true,
	"send_denial_raid": true,
	# The per-entry builders kit. It rides the SAME `_kit_token` rule as the other three, which is the
	# whole reason *"pick the default"* clears the override rather than needing a literal of its own.
	"build_kit": true,
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
##
## `held_ticks` is the SHIPMENT's own extra: `{cargo id: ticks}` for every pile the drive composed
## from, which the Rust half compares the parsed amounts against. Empty for every other kind — a
## command with no manifest names no pile — and its absence on a shipment is a hard error there rather
## than a skip, absence being the state that silently checks nothing.
func _record(kind: String, payload: Dictionary, formatted: Dictionary,
		held_ticks: Dictionary = {}) -> void:
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
	var entry := {"kind": kind, "line": String(formatted["line"]), "expected_kit": expected_kit}
	if not held_ticks.is_empty():
		entry["cargo_held_ticks"] = held_ticks
	_emitted.append(entry)

## The commands this guard must see. Missing one is a failure: a driver that quietly stopped
## reaching its emit site would otherwise turn this guard green by producing nothing to check.
## **EVERY BAND-WIDE ROLE `Main.format_assign_labor` CAN EMIT.** Its own `match` arm is a literal
## list, so this is a second spelling of it — and `_assert_every_role_is_emittable` is what stops the
## two drifting: a role dropped from here is a role this guard stops parsing, which is precisely the
## hole `builders` fell through for a slice.
##
## The two SOURCE kinds (`forage`, `hunt`) are not here: they carry targets and are driven by their
## own grammars above.
const ASSIGN_LABOR_ROLES := [
	HudConst.LABOR_KIND_SCOUT,
	HudConst.LABOR_KIND_WARRIOR,
	HudConst.LABOR_KIND_AGRICULTURE,
	HudConst.LABOR_KIND_HUSBANDRY,
	HudConst.LABOR_KIND_ROADWORK,
	HudConst.LABOR_KIND_BUILDERS,
]

## A role name no builder knows, for the negative below.
const ASSIGN_LABOR_UNKNOWN_ROLE := "stonemason"

## The three TARGETED/untailed drives `_drive_assign_labor_kits` makes before the role sweep: the
## map's quick-hunt, and hunt + forage with a `kit <id>` tail.
const ASSIGN_LABOR_GRAMMAR_DRIVES := 3

## …and the BARE `builders` line beside its tailed one — the exact line the pool's `+` emits.
const ASSIGN_LABOR_BARE_DRIVES := 1

## What `EXPECTED_KINDS` must say for `assign_labor`. Spelled here because a `const` initializer
## cannot call `Array.size()`, and re-derived at runtime so the two cannot drift.
const ASSIGN_LABOR_EXPECTED := 10

## **THE LIST ABOVE IS THE WHOLE OF WHAT THE CLIENT CAN SAY, ASSERTED RATHER THAN TRUSTED.**
##
## Two halves, and the negative is what makes the positive mean anything: every listed role must
## produce a LINE from the real builder (so the guard is driving a command the client can actually
## emit, not a string this file invented), and a role nobody knows must produce NOTHING (so the
## builder is enumerating rather than accepting anything handed to it — which would make the positive
## half vacuous).
##
## **IT CANNOT SEE A ROLE ADDED TO `format_assign_labor` AND NOT TO THIS LIST**, and nothing in
## GDScript can: the builder's arm is a `match` on literals with no reflectable set behind it. What
## it CAN do is fail the moment such a role is dropped from here, which is the direction the defect
## travelled — `builders` was in the builder and in the sim, and only the text grammar and this guard
## did not know it.
func _assert_every_role_is_emittable() -> void:
	var band := _band_fixture()
	var missing: Array[String] = []
	for role in ASSIGN_LABOR_ROLES:
		var line := String(MAIN_SCRIPT.format_assign_labor({
			"faction": HudConst.PLAYER_FACTION_ID,
			"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
			"kind": String(role), "workers": PARTY_WORKERS,
		}).get("line", ""))
		if not line.begins_with("assign_labor ") or not line.ends_with(
				" %s %d" % [String(role), PARTY_WORKERS]):
			missing.append("%s -> \"%s\"" % [String(role), line])
	if not missing.is_empty():
		_fail("band-wide roles that build no assign_labor line: %s" % "; ".join(missing))
	if String(MAIN_SCRIPT.format_assign_labor({
			"faction": HudConst.PLAYER_FACTION_ID,
			"band_id": int(band.get("band_id", HudConst.NO_BAND_ID)),
			"kind": ASSIGN_LABOR_UNKNOWN_ROLE, "workers": PARTY_WORKERS,
		}).get("line", "")) != "":
		_fail("`%s` built an assign_labor line, so the role list is not a list"
			% ASSIGN_LABOR_UNKNOWN_ROLE)
	# **AND THE EXPECTED COUNT IS RE-DERIVED FROM THE LIST**, because `EXPECTED_KINDS` has to spell it
	# as a literal: a role added to the sweep without bumping that number would leave the emit count
	# short and the failure would name the COUNT rather than the role, which is a worse error message
	# for the same mistake.
	var want := ASSIGN_LABOR_GRAMMAR_DRIVES + ASSIGN_LABOR_ROLES.size() + ASSIGN_LABOR_BARE_DRIVES
	if want != ASSIGN_LABOR_EXPECTED:
		_fail("ASSIGN_LABOR_EXPECTED is %d, but the drives add up to %d — bump it with the role list"
			% [ASSIGN_LABOR_EXPECTED, want])
	print("command_guard: %d band-wide role(s) build a line, and an unknown one builds none"
		% ASSIGN_LABOR_ROLES.size())

const EXPECTED_KINDS := {
	# The map's quick-hunt (which names no kit, so the line is the untailed one), the two TARGETED
	# grammars from `_drive_assign_labor_kits` (hunt and forage, both with `kit <id>` on a tail that
	# had never been parsed with the token), then EVERY band-wide role once with that token — and
	# `builders` a second time BARE, which is the exact line the pool's `+` emits and the exact line
	# the text grammar refused for a slice.
	# **A LITERAL, because a `const` initializer cannot read another script's `Array.size()`** — the
	# cross-class `const` hazard `hud-modules.md` records, and it is a hard parse error rather than a
	# silent zero. `_assert_every_role_is_emittable` re-derives the sum at RUNTIME and fails if this
	# number and the role list have come apart, so the literal cannot go stale unnoticed.
	"assign_labor": ASSIGN_LABOR_EXPECTED,
	"cancel_order": 1,
	"move_band": 1,
	"send_expedition": 1,
	"recall_expedition": 1,
	# Fission's own verb. It names a BAND rather than a party, so the handle assertion is what proves
	# the client does not send entity bits down the split either.
	"split_band": 1,
	# TWO — the Band panel's parties compose and the herd drawer's, which build their payloads
	# independently and so can drift apart.
	# TWO each — the tile form and the herd form, which are two grammars of one verb and the pair a
	# builder can get backwards (`docs/plan_standing_upkeep.md` §4.7a ②, §4.7b ③).
	"build_kit": 2,
	"build_order": 2,
	"send_hunt_expedition": 2,
	# ONE — the parties compose sheet is the denial raid's only launch site.
	"send_denial_raid": 1,
	# ONE — the shipment's only launch site is that same sheet, and one line carries both piles.
	"send_trade_expedition": 1,
	# ONE EACH — the route branch's two tile verbs, whose whole difference from `cultivate`/`sow` is
	# the BAND token in the middle. See `_drive_road_verbs`.
	"grade": 1,
	"pave": 1,
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
		# The age brackets, in WHOLE PEOPLE. `working_age` above IS the working one, so only the two
		# dependent brackets are stated and the elders are DERIVED — the three must sum to `size`,
		# which is what the sim guarantees on the wire.
		"children": BAND_CHILDREN,
		"elders": BAND_SIZE - BAND_WORKING_AGE - BAND_CHILDREN,
		"turns_of_food": 20.0,
		"morale": 0.8,
		# **THE LARDER IS A FRACTIONAL PILE, and that is the shipment drive's whole subject** — see
		# `TRADE_FOOD_HELD_TICKS`. Nothing else in this file reads the store.
		"stores": {HudConst.STORE_ITEM_PROVISIONS: _units(TRADE_FOOD_HELD_TICKS)},
		# One pile of one material AT ONE RATING, the shape the sim's own store keeps.
		HudCraftingVocab.BAND_MATERIAL_BATCHES_KEY: [_hide_batch_fixture()],
		# The two shipment pack levers, so the manifest the drive composes is one the meter passes and
		# the send button therefore exists to be pressed.
		"expedition_trade_per_worker_carry": TRADE_PER_WORKER_CARRY,
		"expedition_trade_material_carry_weight": TRADE_MATERIAL_CARRY_WEIGHT,
		"output_multiplier": 1.0,
		"work_range": BAND_WORK_RANGE,
		"hunt_reach": BAND_HUNT_REACH,
		"max_expedition_party_size": RAID_MAX_PARTY,
		"band_move_tiles_per_turn": 1.0,
		"hunt_per_worker_provisions": 0.4,
		"labor_assignments": [],
	}

## The band's one material pile. Its rating is what makes it a BATCH rather than a quantity of `hide`,
## and the sheet's row key is built from it.
func _hide_batch_fixture() -> Dictionary:
	return {
		HudCraftingVocab.BATCH_MATERIAL_ID_KEY: TRADE_HIDE_MATERIAL,
		HudCraftingVocab.BATCH_AMOUNT_KEY: _units(TRADE_HIDE_HELD_TICKS),
		HudCraftingVocab.BATCH_READINGS_KEY: [{
			HudCraftingVocab.READING_AXIS_KEY: TRADE_HIDE_AXIS,
			HudCraftingVocab.READING_VALUE_KEY: TRADE_HIDE_AXIS_VALUE,
			HudCraftingVocab.READING_BAND_NAME_KEY: TRADE_HIDE_AXIS_BAND,
		}],
	}

## **THE BAND'S ONE LIVE TIE**, which is what the shipment form draws its destinations from. The
## ledger's own shape: two durable ids, a strength, and where the subject was last seen.
func _connection_fixtures() -> Array:
	return [{
		"observer_band_id": BAND_ID,
		"subject_band_id": DESTINATION_ID,
		"strength": TIE_STRENGTH_LIVE,
		"last_seen_x": DESTINATION_LAST_SEEN_X,
		"last_seen_y": DESTINATION_LAST_SEEN_Y,
		"last_seen_turn": DESTINATION_LAST_SEEN_TURN,
		"last_contact_turn": DESTINATION_LAST_SEEN_TURN,
		"first_contact_turn": DESTINATION_LAST_SEEN_TURN,
	}]

## A pile stated in the sim's fixed-point TICKS, as the wire's own decode would hand it to this client
## — `Scalar` over its scale, with the digit count read off `Main` rather than typed here, so the
## fixture and the formatter under test cannot disagree about the sim's precision.
func _units(ticks: int) -> float:
	return float(ticks) / pow(10.0, MAIN_SCRIPT.SIM_SCALAR_DECIMALS)

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
