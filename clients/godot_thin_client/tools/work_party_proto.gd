extends Node

## **DEV-ONLY UX PROTOTYPE for the work party panel (issues #684 / #686) — a THROWAWAY, not a feature.**
##
## Nothing about a work party exists on the wire yet: every number and every line of copy below is a
## hand-written fixture literal, and this harness renders THREE CANDIDATE PANEL SHAPES for one
## scenario so the shape can be chosen before any sim code is written.
##
## **It is a sibling of `band_panel_preview`, never a chapter of `ui_preview`.** It renders MOCK zone
## content, so it must not enter that harness's state walk or shift any frame in it. It shares only
## `ui_preview_out/` and the hang guard.
##
## **IT TOUCHES NO SHIPPING SCRIPT.** The real `BandCityPanel` is instanced and driven through its
## public `set_zone_layout` / `set_zones` seam, which is exactly how `BandPanelController` drives it —
## so the chrome, the dock, the tab bar and every zone BOX are the real ones, and only the contents of
## the three zones are this file's mock. The zone contents are built out of the real visual language
## (`HudWidgets` leaves, `HudStyle` ink, `BandCityPanel.panel_card_stylebox()` via `BandComposeFloat`),
## read-only, so what is being judged is the SHAPE and not a wireframe.
##
## FROM THE REPO ROOT:
##
##   godot --headless --path clients/godot_thin_client --import
##   scripts/preview.sh res://tools/work_party_proto.tscn ; echo "EXIT=$?"
##
## then read the six `ui_preview_out/work_party_*.png`. **The exit status is the verdict** — see
## `.claude/rules/client/test-harnesses.md`.

const HUD_SCENE := preload("res://src/ui/HudLayer.tscn")
const BAND_PANEL_SCENE := preload("res://src/ui/BandCityPanel.tscn")
## `Main`, for its RULE and nothing else — `band_panel_preview`'s idiom. The harness never instances
## it; it borrows the two `static` predicates that say whether the HUD yields its strip to a docked
## panel and what lateral bounds go with that, so these frames reflow like the live client's.
const MAIN_SCRIPT := preload("res://src/scripts/Main.gd")

const HARNESS_NAME := "work_party_proto"
const OUT_DIR := "res://ui_preview_out"
const EXIT_OK := 0
const EXIT_FAILED := 1
## `DisplayServer.get_name()` under `--headless`, where there is no viewport texture to read back.
const HEADLESS_DISPLAY_DRIVER := "headless"

## The hang guard, a SIBLING node in `work_party_proto.tscn` (`tools/preview_watchdog.gd`). Same shape
## and same reason as the other render harnesses: the whole walk runs from one long `await`ing
## `_ready()`, so a runtime error in it aborts the run without ever reaching `_finish()`.
const WATCHDOG_NODE := "Watchdog"
const WATCHDOG_PROGRESS_METHOD := "note_progress"

## The canvas every frame is pinned to, per the spec for this prototype.
const PREVIEW_SIZE := Vector2i(1920, 1080)
const WINDOW_PIN_MAX_FRAMES := 180
const CANVAS_STABLE_FRAMES := 8
const CANVAS_STABLE_MAX_FRAMES := 600
## Sub-pixel slack between the canvas asked for and the projection Godot computes from it.
const CANVAS_PROJECTION_TOLERANCE := 1.5
## Floor on the scale divisor, so a zeroed `content_scale_factor` cannot make the expectation infinite.
const CONTENT_SCALE_MIN := 0.01

## Scratch prefs — never the player's `user://band_city_dock.cfg` / `user://narrative.cfg`.
const PREVIEW_DOCK_PREFS_PATH := "user://work_party_proto_dock.cfg"
const PREVIEW_PREFS_PATH := "user://work_party_proto_prefs.cfg"

## The map stand-in behind the panel. `MOUSE_FILTER_IGNORE` for `band_panel_preview`'s reason: in the
## live client the map is a `Node2D`, so a `Control` backdrop that swallowed presses would be the
## harness's own decoration changing what the frame proves.
const BACKDROP_LAYER := -10
const BACKDROP_COLOR := Color(0.10, 0.15, 0.16)

## Sub-pixel slack when comparing a zone's content against its box.
const ZONE_BOUNDS_TOLERANCE := 1.0

## The turn orb's calm breath is `0.5 - 0.5 * cos(t)`, which degenerates at phase 0; seeded to the
## midpoint so the frozen clock renders it at a readable, deterministic instant.
const TURN_ORB_PULSE_MIDPOINT_FRACTION := 0.5

## The top bar's one seeded readout, so the HUD behind the panel is not blank chrome.
const TOPBAR_SEDENTARIZATION_SCORE := 62.0
const TOPBAR_SEDENTARIZATION_STAGE := "soft"

# =====================================================================================
# THE SCENARIO — one band, two work parties, identical in all three shapes.
# Every literal here is a FIXTURE. Nothing is recomputed: `+2.5 to band` is `4.2 − 1.5` net of
# friction and is written out rather than derived, because there is no model behind it yet.
# =====================================================================================

const BAND_NAME := "Elk Hollow"
const BAND_PEOPLE := 11
const BAND_HEX_X := 24
const BAND_HEX_Y := 18
## The header's settlement stage — the panel's sprite key, its emoji fallback and its label.
const BAND_STAGE_ID := "camp"
const BAND_STAGE_GLYPH := "🛖"
const BAND_STAGE_LABEL := "Camp"
## The cycler reads "1 / 1": one player band, which is what this scenario has.
const BAND_CYCLER_INDEX := 0
const BAND_CYCLER_COUNT := 1

## **THE FIXED COPY.** The three shapes may differ ONLY in where the party lives and where the haul
## control sits — never in wording, ink or numbers — so every one of these strings is written once and
## rendered verbatim by all three builders. `_assert_copy` counts them across all three zones.
const NEAR_SOURCE_LINE := "Hazel patch · 2 tiles · pools with band"
const FAR_SOURCE_LINE := "Red Deer herd · 7 tiles · out of reach"
const NEAR_CREW := "3 gatherers"
const FAR_CREW := "4 hunters"
const NEAR_PACK := "Pack 4.2 / 9.0"
const FAR_PACK := "Pack 12.0 / 20.0"
const NEAR_ATE := "Party ate 1.5"
const FAR_ATE := "Party ate 2.0"
const NEAR_DELIVERY := "+2.5 to band"
const FAR_DELIVERY := "Hauling 10.0 → Elk Hollow · 3 turns"
## The far party's verb, and the one control whose PLACE is what the three shapes are arguing about.
const HAUL_VERB := "↥ Haul to …"

## Which ink a delivery line takes. Named keys rather than colours because a `const` Dictionary cannot
## hold a `HudStyle` value (the palette is a `static var`, not a constant expression).
const DELIVERY_INK_INCOME := "income"
const DELIVERY_INK_TRANSIT := "transit"

## The near party: inside `reach_tiles`, so its take pools with the band's larder.
const NEAR_PARTY := {
	"crew": NEAR_CREW,
	"source": NEAR_SOURCE_LINE,
	"pack": NEAR_PACK,
	"ate": NEAR_ATE,
	"delivery": NEAR_DELIVERY,
	"delivery_ink": DELIVERY_INK_INCOME,
	"haul": false,
}
## The far party: beyond `reach_tiles`, so it hauls — and it is the one carrying the `↥ Haul to …` verb.
const FAR_PARTY := {
	"crew": FAR_CREW,
	"source": FAR_SOURCE_LINE,
	"pack": FAR_PACK,
	"ate": FAR_ATE,
	"delivery": FAR_DELIVERY,
	"delivery_ink": DELIVERY_INK_TRANSIT,
	"haul": true,
}
const PARTIES := [NEAR_PARTY, FAR_PARTY]

## The destination picker, opened by `↥ Haul to …`.
const PICKER_HEAD := "Send 10.0 food to"
const PICKER_ROWS := [
	"Elk Hollow · 7 tiles",
	"Stonebrook · 4 tiles",
	"Reed Camp · 12 tiles",
]
## Which row is pre-selected when the picker opens — the party's own band, the default destination.
const PICKER_SELECTED_ROW := 0
const PICKER_SEND := "Send"
const PICKER_CANCEL := "Cancel"

# ---- the zones a shape does NOT change, mocked with representative content ------------------------
# Drawn in every frame so no shape reads as emptier than another for a reason that is not the shape.

## The band zone's vitals rows — the merged larder line and its neighbours.
const VITALS_ROWS := [
	["Food", "34.2  ▲ +3.6 /turn · 1.2 fodder"],
	["Morale", "62 · settled"],
	["Stores", "hide 1.4 · fibre 0.8"],
]
## Key column width for those rows, so the three values start at one x.
const VITALS_KEY_WIDTH := 58.0

## The PEOPLE bar: 11 people, and the working-age slice is the pool the two parties are drawn from.
const PEOPLE_CHILDREN := 2
const PEOPLE_WORKING := 8
const PEOPLE_ELDERS := 1

## The WORKFORCE bar: the eight working-age hands, seven of them on the two sources the parties work.
const WORKFORCE_FORAGE := 3
const WORKFORCE_HUNT := 4
const WORKFORCE_IDLE := 1

## The work board's rows. The first two are the sources the two parties are out on; the third is an
## ordinary staffed row with no party, so a shape that changes every row is visibly different from one
## that changes only the rows with parties on them.
const WORK_ROW_NEAR := {
	"name": "Harvest (25, 20)",
	"status": "+2.5 /turn · hazel",
	"crew": WORKFORCE_FORAGE,
	"party": 0,
}
const WORK_ROW_FAR := {
	"name": "Hunt Red Deer",
	"status": "0.0 /turn · in transit",
	"crew": WORKFORCE_HUNT,
	"party": 1,
}
const WORK_ROW_PLAIN := {
	"name": "Harvest (23, 17)",
	"status": "+1.1 /turn · berry thicket",
	"crew": 1,
	"party": -1,
}
const WORK_ROWS := [WORK_ROW_NEAR, WORK_ROW_FAR, WORK_ROW_PLAIN]

## The zone heads' readouts. Constant across the three shapes, deliberately: a readout that counted
## the rows in the zone the shape moved the party INTO would be a wording difference between shapes.
const WORK_HEAD_READOUT := "3 sources · +3.6 /turn"
const PARTIES_HEAD_READOUT := "1 out · 2 workers"
const BAND_HEAD_PEOPLE_READOUT := "11 people"
const WORKFORCE_HEAD_READOUT := "1 idle of 8"
const AWAY_HEAD_READOUT := "2 parties · 7 workers"

## The AWAY block's own head — shape C's whole claim in one word: a party is part of the band, so it
## is in the band's own people readout.
const AWAY_HEAD := "Away"

## The one real expedition in the parties zone, so that zone is never empty and the work parties in
## shape B are visibly sitting BESIDE something.
const EXPEDITION_ROW_FACE := "⚑ Scouting · (29, 12)"
const EXPEDITION_ROW_STATUS := "2 scouts · returns in 4 turns"

## The parties footer's existing verbs, in the shipped order.
const FOOTER_VERBS := [
	HudComposeVocab.COMPOSE_MISSION_LABEL_SCOUT,
	HudComposeVocab.COMPOSE_MISSION_LABEL_HUNT,
	HudComposeVocab.COMPOSE_MISSION_LABEL_DENY,
	HudComposeVocab.COMPOSE_MISSION_LABEL_TRADE,
	HudComposeVocab.COMPOSE_MISSION_LABEL_SPLIT,
]

# =====================================================================================
# THE THREE SHAPES
# =====================================================================================

## A — the party lives on its WORK ROW. Assignment and party state are one place.
const SHAPE_A := "A"
## B — the party lives in the PARTIES zone, beside expeditions.
const SHAPE_B := "B"
## C — the party lives in the BAND zone, under WORKFORCE.
const SHAPE_C := "C"

## Per shape: which zone the party block is mounted in (hence which tab the narrow left dock shows),
## and the frame names. **A LEFT DOCK IS THE NARROW SHELL**, which draws ONE zone at a time, so each
## shape is rendered on the tab that owns its change — that is what makes the three frames comparable.
const SHAPES := [
	{"id": SHAPE_A, "zone": BandCityPanel.ZONE_WORK,
		"frame": "work_party_A_work_row", "picker_frame": "work_party_A_haul_pick"},
	{"id": SHAPE_B, "zone": BandCityPanel.ZONE_PARTIES,
		"frame": "work_party_B_parties_zone", "picker_frame": "work_party_B_haul_pick"},
	{"id": SHAPE_C, "zone": BandCityPanel.ZONE_BAND,
		"frame": "work_party_C_band_zone", "picker_frame": "work_party_C_haul_pick"},
]

## **SHAPE B HIDES THE UNSELECTED PARTY'S NUMBERS, and that is the shape rather than a defect.** Its
## rows are a row → detail disclosure, so only the SELECTED party's pack / ate / delivery are on
## screen; the near party's three lines are legitimately absent from that frame. Every other string is
## present exactly once in every shape, which is what `_assert_copy` checks — and it checks the
## absences too, so a shape that leaked the party into a second zone fails.
const SHAPE_B_HIDDEN_LINES := [NEAR_PACK, NEAR_ATE, NEAR_DELIVERY]

# =====================================================================================
# Layout numbers this file owns (everything else comes off HudWidgets / HudWorkVocab / HudStyle)
# =====================================================================================

## A party block's own line spacing — the parties inspector strip's, since that is the surface these
## lines read like.
const PARTY_LINE_SEPARATION := HudComposeVocab.PARTIES_INSPECTOR_LINE_SEPARATION
## The gutter a party block is indented by inside a work row / band entry, so it reads as detail
## hanging off the row above rather than as a sibling row. The status line's own indent.
const PARTY_BLOCK_INDENT := int(HudWorkVocab.STATUS_LINE_INDENT)
## Gap between the two parts of a party line (`3 gatherers` · `Hazel patch …`).
const PARTY_PART_SEPARATION := HudWorkVocab.STATUS_LINE_SEPARATION
## The separator drawn between those parts — the client's one inline separator.
const PARTY_PART_SEPARATOR := "·"
## Vertical gap between two party entries in the AWAY block.
const AWAY_ENTRY_SEPARATION := HudWorkVocab.ZONE_BLOCK_SEPARATION

## The `↥ Haul to …` verb's own sizing, matched to the inline links the inspector strips carry.
const HAUL_BUTTON_FONT_SIZE := HudWorkVocab.ALLOC_SECTION_FONT_SIZE
const HAUL_BUTTON_PADDING_V := HudWorkVocab.WORK_STEPPER_PADDING_V
## The verb's stable handle, so an assertion reaches it by IDENTITY rather than by face.
const HAUL_BUTTON_META := &"work_party_proto_haul"

## The picker sheet's own spacing and its destination rows' height.
const PICKER_LINE_SEPARATION := HudWorkVocab.ZONE_BLOCK_SEPARATION
const PICKER_ROW_HEIGHT := HudWorkVocab.WORK_ROW_HEIGHT
const PICKER_BUTTON_SEPARATION := HudWorkVocab.WORKER_STEPPER_SEPARATION
## The picker card's stable handle.
const PICKER_META := &"work_party_proto_picker"

## Frames to wait after mounting the float: `BandComposeFloat.mount` fires `refit()` without awaiting
## it, and that coroutine spends one `process_frame` before it measures. Two, so the measurement has
## landed AND the placement it drives has been laid out.
const FLOAT_SETTLE_FRAMES := 2

# =====================================================================================

var _pinned_size := PREVIEW_SIZE
var _pinned_canvas := Vector2i.ZERO
var _hud: HudLayer
var _panel: BandCityPanel
var _reservation_listener: Callable
var _watchdog: Node = null
var _picker_float: BandComposeFloat = null
## The three zone CONTENT roots this shape handed the panel, by zone key. Held because the narrow
## shell keeps the inactive zones detached-but-owned: a copy sweep over the panel's own tree would
## only ever see the tab that is up, and "the party appears nowhere else" is a claim about all three.
var _zone_contents: Dictionary = {}
var _current_state := "<pre-render>"
var _failures := 0
var _asserts := 0


func _ready() -> void:
	_watchdog = _resolve_watchdog()
	# FREEZE ANIMATION TIME, the treatment every render harness takes: a frame that varies run to run
	# cannot be compared against the run before it. `_settle` waits on `process_frame`, which still
	# fires at `time_scale` 0.
	Engine.time_scale = 0.0
	await _pin_window(PREVIEW_SIZE)
	DirAccess.make_dir_absolute(OUT_DIR)

	var bg_layer := CanvasLayer.new()
	bg_layer.layer = BACKDROP_LAYER
	add_child(bg_layer)
	var bg := ColorRect.new()
	bg.color = BACKDROP_COLOR
	bg.set_anchors_preset(Control.PRESET_FULL_RECT)
	bg.mouse_filter = Control.MOUSE_FILTER_IGNORE
	bg_layer.add_child(bg)

	# Isolate the panel/narrative preferences from the developer's real profile before any UI reads
	# them, or a developer who has moved the dock renders different frames than one who has not.
	NarrativeForkPanel.config_path_override = PREVIEW_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(PREVIEW_PREFS_PATH))
	BandCityPanel.config_path_override = PREVIEW_DOCK_PREFS_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(PREVIEW_DOCK_PREFS_PATH))

	# PIN THE INTERFACE SCALE and the PALETTE out of that same real settings file: `ClientSettings` is
	# an autoload that has already read it, and both have already been applied. Assign the member
	# rather than the setter (which saves over the developer's file) and re-emit so `UiScaler` applies
	# the pin through its own path.
	ClientSettings.ui_scale = ClientSettings.UI_SCALE_DEFAULT
	ClientSettings.changed.emit()
	HudPalette.apply(HudPalette.DEFAULT_THEME)

	_hud = HUD_SCENE.instantiate()
	add_child(_hud)

	_panel = BAND_PANEL_SCENE.instantiate()
	add_child(_panel)
	# Fan the panel's reservation onto the HUD as `Main` does, INCLUDING the edges where the HUD does
	# not yield its strip. **THE RULE IS CALLED, NEVER RESTATED** — a harness that restated it would
	# keep passing after the rule moved.
	_reservation_listener = func(edge: int, size: float):
		var hud_overlaid: bool = MAIN_SCRIPT.band_dock_overlays_hud(edge, size, _hud, _panel)
		MAIN_SCRIPT.push_hud_strip(_hud, &"band_panel", edge, size, hud_overlaid)
		var keeps_bottom_strip: bool = hud_overlaid and edge == SIDE_BOTTOM
		if _hud.has_method("set_right_column_bottom_clearance"):
			_hud.set_right_column_bottom_clearance(size if keeps_bottom_strip else 0.0)
		var bounds: Vector2 = MAIN_SCRIPT.band_panel_lateral_bounds(edge, size, _hud) \
			if hud_overlaid else Vector2.ZERO
		_panel.set_lateral_bounds(bounds.x, bounds.y)
	_panel.reservation_changed.connect(_reservation_listener)
	if _hud.has_method("reflow_dock_row"):
		_panel.reservation_changed.connect(Callable(_hud, "reflow_dock_row"))
		_hud.reflow_dock_row(_panel.get_dock(), _panel.current_reservation_size())

	await get_tree().process_frame
	await get_tree().process_frame
	await _stabilize_canvas()

	if _hud.turn_orb != null:
		_hud.turn_orb._pulse_time = TurnOrb.PULSE_PERIOD * TURN_ORB_PULSE_MIDPOINT_FRACTION
	# One seeded top-bar readout so the HUD behind the panel is not blank chrome. `has_method`-probed:
	# these entry points are reached by name, so a retired one must fail here rather than at the call.
	if _hud.has_method("update_sedentarization"):
		_hud.update_sedentarization([{"faction": 0,
			"score": TOPBAR_SEDENTARIZATION_SCORE, "stage": TOPBAR_SEDENTARIZATION_STAGE}])

	# The header and the dock, once — they are chrome, identical in all six frames.
	_panel.set_header(BAND_STAGE_ID, BAND_STAGE_GLYPH, BAND_NAME, BAND_STAGE_LABEL,
		HudFormat.BAND_HEADER_POSITION_FORMAT % [BAND_HEX_X, BAND_HEX_Y])
	_panel.set_cycler(BAND_CYCLER_INDEX, BAND_CYCLER_COUNT)
	_panel.set_subject_jumpable(true)
	_panel.set_collapsed(false)
	_panel.set_shown(true)
	_panel.set_dock(SIDE_LEFT)
	# **DECLARED BEFORE THE CONTENTS ARE BUILT**, which is `set_zone_layout`'s own contract: the layout
	# is what the shell threshold sums over, and the contents are measured against the box that choice
	# fixes. Borrowed from the controller read-only — a second copy of a band's three zones here is a
	# second thing to keep in step.
	_panel.set_zone_layout(BandPanelController.BAND_ZONE_LAYOUT)

	for shape_variant in SHAPES:
		var shape: Dictionary = shape_variant
		await _render_shape(shape)

	_finish()


## One shape: build its three zones, show the tab that owns its change, render it, then open the
## destination picker on the far party and render that.
func _render_shape(shape: Dictionary) -> void:
	_note_progress()
	var id := String(shape["id"])
	var zone: StringName = shape["zone"]
	_build_zones(id)
	_panel.set_active_tab(zone)
	await _settle()
	# Named BEFORE the assertions rather than by `_save`: every `PASS` / `FAIL` line carries the state
	# it is about, and a name set on the way out would label each line with the PREVIOUS frame.
	_current_state = String(shape["frame"])
	_assert_shape(id, zone)
	await _save(_current_state)

	_open_picker(zone)
	await _settle()
	for _i in range(FLOAT_SETTLE_FRAMES):
		await get_tree().process_frame
	await _settle()
	_current_state = String(shape["picker_frame"])
	_assert_shape(id, zone)
	_assert_picker()
	await _save(_current_state)
	_dismiss_picker()


# =====================================================================================
# Zone contents — the mock, and the only thing that differs between the three shapes
# =====================================================================================

## Build and hand over all three zones for `shape`. Ownership passes to the panel, which frees the
## previous shape's zones; `_zone_contents` keeps a reference for the copy sweep, which has to see the
## detached tabs too.
func _build_zones(shape: String) -> void:
	var band := _band_zone(shape)
	var work := _work_zone(shape)
	var parties := _parties_zone(shape)
	_zone_contents = {
		BandCityPanel.ZONE_BAND: band,
		BandCityPanel.ZONE_WORK: work,
		BandCityPanel.ZONE_PARTIES: parties,
	}
	_panel.set_zones({
		BandCityPanel.ZONE_BAND: HudWidgets.wrap_zone(band),
		BandCityPanel.ZONE_WORK: HudWidgets.wrap_zone(work),
		BandCityPanel.ZONE_PARTIES: HudWidgets.wrap_zone(parties),
	})


## The band zone: vitals, PEOPLE, WORKFORCE — and, in shape C only, the AWAY block directly under the
## workforce bar.
func _band_zone(shape: String) -> VBoxContainer:
	var col := HudWidgets.make_zone_column()

	var vitals := HudWidgets.make_zone_block()
	for row_variant in VITALS_ROWS:
		var row: Array = row_variant
		vitals.add_child(_key_value_row(String(row[0]), String(row[1])))
	col.add_child(vitals)

	var people := HudWidgets.make_zone_block()
	people.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_PEOPLE, BAND_HEAD_PEOPLE_READOUT))
	# **KEYED BY THE GLYPH, not by the label** — `build_composition_key` looks the key up in
	# `HudWorkVocab.PEOPLE_MARKS` to decide whether the bracket has bundled ART, so a segment keyed by
	# the word renders neither the mark nor the emoji the real band zone draws.
	var people_segments := [
		{"key": HudWorkVocab.PEOPLE_GLYPH_CHILDREN, "count": PEOPLE_CHILDREN, "color": HudStyle.INK_DIM},
		{"key": HudWorkVocab.PEOPLE_GLYPH_WORKING, "count": PEOPLE_WORKING, "color": HudStyle.HEALTHY},
		{"key": HudWorkVocab.PEOPLE_GLYPH_ELDERS, "count": PEOPLE_ELDERS, "color": HudStyle.INK_FAINT},
	]
	people.add_child(HudWidgets.build_composition_bar(people_segments))
	people.add_child(HudWidgets.build_composition_key(people_segments))
	col.add_child(people)

	var workforce := HudWidgets.make_zone_block()
	workforce.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_WORKFORCE, WORKFORCE_HEAD_READOUT))
	var work_segments := [
		{"key": HudWorkVocab.WORKFORCE_KEY_FORAGE, "count": WORKFORCE_FORAGE, "color": HudStyle.HEALTHY},
		{"key": HudWorkVocab.WORKFORCE_KEY_HUNT, "count": WORKFORCE_HUNT, "color": HudStyle.SIGNAL},
		{"key": HudWorkVocab.WORKFORCE_KEY_IDLE, "count": WORKFORCE_IDLE, "color": HudStyle.INK_FAINT},
	]
	workforce.add_child(HudWidgets.build_composition_bar(work_segments))
	workforce.add_child(HudWidgets.build_composition_key(work_segments))
	# **SHAPE C'S WHOLE CLAIM**: the parties are two-line entries in the band's own people readout,
	# directly under the workforce bar, so "a party is part of the band" is literal.
	if shape == SHAPE_C:
		workforce.add_child(_away_block())
	col.add_child(workforce)
	return col


## Shape C's AWAY block — both parties as two-line entries, the far one carrying the haul verb.
func _away_block() -> VBoxContainer:
	var block := HudWidgets.make_zone_block()
	block.add_theme_constant_override("separation", AWAY_ENTRY_SEPARATION)
	block.add_child(HudWidgets.zone_head(AWAY_HEAD, AWAY_HEAD_READOUT))
	var first := true
	for party_variant in PARTIES:
		var party: Dictionary = party_variant
		# **A HAIRLINE BETWEEN ENTRIES, and it is not a difference between the shapes.** Shapes A and B
		# get their separation for free — each party hangs off its own work row / list row — so two
		# blocks stacked flush is a legibility cost unique to the ONE shape that lists them adjacently,
		# and the client's own dashed rule is what answers it.
		if not first:
			block.add_child(HudWidgets.build_dashed_rule())
		block.add_child(_party_block(party, bool(party["haul"])))
		first = false
	return block


## The work zone: a three-row board. In shape A each row that HAS a party carries that party's block
## under its own head, and the party appears nowhere else in the panel.
func _work_zone(shape: String) -> VBoxContainer:
	var col := HudWidgets.make_zone_column()
	col.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_WORK, WORK_HEAD_READOUT))
	var board := HudWidgets.make_zone_block()
	for row_variant in WORK_ROWS:
		var row: Dictionary = row_variant
		board.add_child(_work_row(row, shape == SHAPE_A))
	col.add_child(board)
	return col


## One work board row: the shipped two-line stepper (title + `−`/`+`, then the indented accounts
## line), optionally followed by this row's party block.
func _work_row(row: Dictionary, with_party: bool) -> PanelContainer:
	var host := PanelContainer.new()
	host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	host.add_theme_stylebox_override("panel", HudStyle.work_row_stylebox(false))
	var col := VBoxContainer.new()
	col.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	col.add_theme_constant_override("separation", HudWorkVocab.TWO_LINE_STEPPER_SEPARATION)
	col.add_child(HudWidgets.build_worker_stepper(String(row["name"]), int(row["crew"]), true,
		func(_delta: int) -> void: pass, false, false, "", "", Callable(), "", "",
		String(row["status"])))
	var party_index := int(row["party"])
	if with_party and party_index >= 0:
		var party: Dictionary = PARTIES[party_index]
		col.add_child(_party_block(party, bool(party["haul"])))
	host.add_child(col)
	return host


## The parties zone: the band's one real expedition, plus — in shape B only — the two work parties as
## rows beside it, the far one selected so its inspector strip is up, and the haul verb on the footer
## beside the existing `⚑ Scout`.
func _parties_zone(shape: String) -> VBoxContainer:
	var col := HudWidgets.make_zone_column()
	col.add_theme_constant_override("separation", HudWorkVocab.ZONE_BLOCK_SEPARATION)
	col.add_child(HudWidgets.zone_head(HudWorkVocab.ZONE_HEADER_PARTIES, PARTIES_HEAD_READOUT))

	var rows := HudWidgets.make_zone_block()
	rows.add_child(_expedition_row())
	if shape == SHAPE_B:
		for party_variant in PARTIES:
			var party: Dictionary = party_variant
			rows.add_child(_party_row(party))
			# The FAR party is the selected one, so the strip sits under the row it belongs to — the
			# parties zone's own row → detail disclosure.
			if bool(party["haul"]):
				rows.add_child(_party_inspector(party))
	col.add_child(rows)

	col.add_child(_party_footer(shape == SHAPE_B))
	return col


## The band's one scouting expedition — a compact two-line row, so the work parties in shape B have
## something to sit beside.
func _expedition_row() -> PanelContainer:
	var host := PanelContainer.new()
	host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	host.add_theme_stylebox_override("panel", HudStyle.work_row_stylebox(false))
	var col := VBoxContainer.new()
	col.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	col.add_theme_constant_override("separation", PARTY_LINE_SEPARATION)
	var face := Label.new()
	face.text = EXPEDITION_ROW_FACE
	face.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
	face.add_theme_color_override("font_color", HudStyle.INK)
	col.add_child(face)
	col.add_child(HudWidgets.build_status_part(EXPEDITION_ROW_STATUS, HudStyle.INK_DIM))
	host.add_child(col)
	return host


## Shape B's party ROW — the party's IDENTITY only (crew · source line). Its numbers live in the
## inspector strip, which is what `SHAPE_B_HIDDEN_LINES` records the cost of.
func _party_row(party: Dictionary) -> PanelContainer:
	var selected := bool(party["haul"])
	var host := PanelContainer.new()
	host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	host.add_theme_stylebox_override("panel", HudStyle.work_row_stylebox(selected))
	host.add_child(_party_line([
		{"text": String(party["crew"]), "ink": HudStyle.INK},
		{"text": String(party["source"]), "ink": HudStyle.INK_DIM},
	]))
	return host


## Shape B's inspector strip for the selected party: the same pack / ate / delivery lines the other
## two shapes put on the row itself, in the parties zone's own strip chrome.
func _party_inspector(party: Dictionary) -> PanelContainer:
	var strip := PanelContainer.new()
	strip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	strip.add_theme_stylebox_override("panel", HudStyle.work_inspector_stylebox())
	var col := VBoxContainer.new()
	col.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	col.add_theme_constant_override("separation", PARTY_LINE_SEPARATION)
	col.add_child(_party_line([
		{"text": String(party["pack"]), "ink": HudStyle.INK},
		{"text": String(party["ate"]), "ink": HudStyle.INK_DIM},
	]))
	col.add_child(_party_line([
		{"text": String(party["delivery"]), "ink": _delivery_ink(party)},
	]))
	strip.add_child(col)
	return strip


## The party block shapes A and C both mount — the whole party in four lines, indented onto the status
## gutter so it reads as detail hanging off the row above. `with_haul` puts the far party's verb on
## the block; shape B takes it off and puts it on the zone footer instead, which is the only thing
## that differs between the three.
func _party_block(party: Dictionary, with_haul: bool) -> MarginContainer:
	var margin := MarginContainer.new()
	margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	margin.add_theme_constant_override("margin_left", PARTY_BLOCK_INDENT)
	var col := VBoxContainer.new()
	col.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	col.add_theme_constant_override("separation", PARTY_LINE_SEPARATION)
	col.add_child(_party_line([
		{"text": String(party["crew"]), "ink": HudStyle.INK},
		{"text": String(party["source"]), "ink": HudStyle.INK_DIM},
	]))
	col.add_child(_party_line([
		{"text": String(party["pack"]), "ink": HudStyle.INK},
		{"text": String(party["ate"]), "ink": HudStyle.INK_DIM},
	]))
	var delivery := _party_line([
		{"text": String(party["delivery"]), "ink": _delivery_ink(party)},
	])
	if with_haul:
		var spacer := Control.new()
		spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
		delivery.add_child(spacer)
		delivery.add_child(_haul_button())
	col.add_child(delivery)
	margin.add_child(col)
	return margin


## One line of a party block: parts separated by the client's inline `·`, each in its own ink.
func _party_line(parts: Array) -> HBoxContainer:
	var line := HBoxContainer.new()
	line.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.add_theme_constant_override("separation", PARTY_PART_SEPARATION)
	var first := true
	for part_variant in parts:
		var part: Dictionary = part_variant
		if not first:
			line.add_child(HudWidgets.build_status_part(PARTY_PART_SEPARATOR, HudStyle.INK_FAINT))
		line.add_child(HudWidgets.build_status_part(String(part["text"]), part["ink"]))
		first = false
	return line


## A delivery line's ink: income reads HEALTHY, a haul in transit reads WARN (the ETA register).
func _delivery_ink(party: Dictionary) -> Color:
	return HudStyle.HEALTHY if String(party["delivery_ink"]) == DELIVERY_INK_INCOME else HudStyle.WARN


## The `↥ Haul to …` verb. Carries `HAUL_BUTTON_META` so an assertion can reach it by identity rather
## than by its face.
func _haul_button() -> Button:
	var btn := Button.new()
	btn.text = HAUL_VERB
	btn.focus_mode = Control.FOCUS_NONE
	btn.set_meta(HAUL_BUTTON_META, true)
	HudStyle.apply_button(btn, "ghost")
	HudWidgets.compact(btn, HAUL_BUTTON_FONT_SIZE, HAUL_BUTTON_PADDING_V)
	return btn


## The parties zone footer: the shipped 3 + 2 mission grid, plus — in shape B only — the haul verb
## beside them.
func _party_footer(with_haul: bool) -> VBoxContainer:
	var foot := HudWidgets.make_zone_block()
	var grid := GridContainer.new()
	grid.columns = HudComposeVocab.PARTY_FOOTER_COLUMNS
	grid.add_theme_constant_override("h_separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
	grid.add_theme_constant_override("v_separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
	for face_variant in FOOTER_VERBS:
		var btn := Button.new()
		btn.text = String(face_variant)
		btn.focus_mode = Control.FOCUS_NONE
		btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		HudStyle.apply_button(btn, "primary")
		HudWidgets.compact(btn, HudWorkVocab.WORK_CHIP_FONT_SIZE, HudWorkVocab.WORK_CHIP_PADDING_V)
		grid.add_child(btn)
	if with_haul:
		var haul := _haul_button()
		haul.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		grid.add_child(haul)
	foot.add_child(grid)
	return foot


## A band-zone vitals row: a fixed-width key and its value, so the three rows line up.
func _key_value_row(key: String, value: String) -> HBoxContainer:
	var row := HBoxContainer.new()
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_theme_constant_override("separation", HudWorkVocab.WORKER_STEPPER_SEPARATION)
	var key_label := Label.new()
	key_label.text = key
	key_label.custom_minimum_size = Vector2(VITALS_KEY_WIDTH, 0.0)
	key_label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
	key_label.add_theme_color_override("font_color", HudStyle.INK_FAINT)
	row.add_child(key_label)
	var value_label := Label.new()
	value_label.text = value
	value_label.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
	value_label.add_theme_color_override("font_color", HudStyle.INK)
	row.add_child(value_label)
	return row


# =====================================================================================
# The destination picker
# =====================================================================================

## Open the picker beside the panel card, in the SHIPPED free-floating host (`BandComposeFloat`) —
## the same surface the parties compose sheet floats in, so it reads as the panel's own card and its
## placement (flush to the card's map-facing seam, never overlapping it) is the real one rather than
## arithmetic this file invented.
##
## **IT IS THE SAME CARD IN ALL THREE SHAPES, deliberately.** Where the haul VERB sits is what the
## shapes disagree about; what it opens is not, and a picker drawn three ways would make the frames
## incomparable on the one axis they exist to compare.
func _open_picker(zone: StringName) -> void:
	if _picker_float == null or not is_instance_valid(_picker_float):
		_picker_float = BandComposeFloat.new()
		_hud.compose_host().add_child(_picker_float)
	_picker_float.mount(_picker_sheet(), _panel.card_rect(),
		BandComposeFloat.map_facing_side(_panel.get_dock()), _panel.zone_size(zone).x)


func _dismiss_picker() -> void:
	if _picker_float != null and is_instance_valid(_picker_float):
		_picker_float.dismiss()


## `Send 10.0 food to` over three destination rows over a `Send` / `Cancel` pair.
func _picker_sheet() -> VBoxContainer:
	var sheet := VBoxContainer.new()
	sheet.set_meta(PICKER_META, true)
	sheet.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	sheet.add_theme_constant_override("separation", PICKER_LINE_SEPARATION)

	var head := Label.new()
	head.text = PICKER_HEAD
	head.add_theme_font_size_override("font_size", HudWorkVocab.WORK_ROW_FONT_SIZE)
	head.add_theme_color_override("font_color", HudStyle.INK)
	sheet.add_child(head)

	var index := 0
	for face_variant in PICKER_ROWS:
		var row := Button.new()
		row.text = String(face_variant)
		row.focus_mode = Control.FOCUS_NONE
		row.alignment = HORIZONTAL_ALIGNMENT_LEFT
		row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		row.custom_minimum_size = Vector2(0.0, PICKER_ROW_HEIGHT)
		HudStyle.apply_pill_toggle(row, index == PICKER_SELECTED_ROW)
		sheet.add_child(row)
		index += 1

	sheet.add_child(HudWidgets.build_dashed_rule())

	var actions := HBoxContainer.new()
	actions.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	actions.add_theme_constant_override("separation", PICKER_BUTTON_SEPARATION)
	for spec in [[PICKER_SEND, "primary"], [PICKER_CANCEL, "ghost"]]:
		var btn := Button.new()
		btn.text = String(spec[0])
		btn.focus_mode = Control.FOCUS_NONE
		btn.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		HudStyle.apply_button(btn, String(spec[1]))
		actions.add_child(btn)
	sheet.add_child(actions)
	return sheet


# =====================================================================================
# Assertions
# =====================================================================================

## Everything claimed of one rendered shape: its zones fit their boxes, its copy is the shared copy,
## and the party block is in the zone this shape says it lives in.
func _assert_shape(shape: String, zone: StringName) -> void:
	_assert_zone_content_fits()
	_assert_zone_boxes_hold_every_tab()
	_assert_copy(shape)
	_assert_party_lives_in(shape, zone)


## GUARD: nothing the ACTIVE tab renders may run past its zone box, on either axis —
## `band_panel_preview._assert_zone_content_fits`'s rule, restated here because a shape that overflows
## is a finding rather than a frame to ship.
func _assert_zone_content_fits() -> void:
	var failures: Array[String] = []
	for host_variant in _find_zone_hosts(_panel):
		var host: Control = host_variant
		_collect_zone_content_shortfall(host, host, failures)
		_collect_zone_content_overrun(host, host, failures)
	if failures.is_empty():
		_pass("the rendered zone's content fits its box on both axes")
		return
	for failure in failures:
		_fail("%s — %s" % [_current_state, failure])


## …and the SAME question for the two tabs that are NOT on screen. **The narrow shell keeps them
## detached**, so they have no laid-out rect and the host walk above cannot see them at all — but a
## shape that made one of them overflow would ship exactly as silently. A detached control still
## reports an honest combined minimum, and `zone_size` is a pure function of the dock, the collapse
## state and the window, so the two can be compared without laying anything out.
func _assert_zone_boxes_hold_every_tab() -> void:
	for key_variant in _zone_contents:
		var key: StringName = key_variant
		var content: Control = _zone_contents[key_variant]
		var box := _panel.zone_size(key)
		var needed := content.get_combined_minimum_size()
		if needed.y > box.y + ZONE_BOUNDS_TOLERANCE:
			_fail("%s — zone %s needs %.0fpx of height but its box is only %.0fpx (short by %.0f)" % [
				_current_state, key, needed.y, box.y, needed.y - box.y])
		elif needed.x > box.x + ZONE_BOUNDS_TOLERANCE:
			_fail("%s — zone %s needs %.0fpx of width but its box is only %.0fpx (over by %.0f)" % [
				_current_state, key, needed.x, box.x, needed.x - box.x])
		else:
			_pass("zone %s fits its %.0fx%.0f box (asks %.0fx%.0f)" % [
				key, box.x, box.y, needed.x, needed.y])


## **THE COPY IS SHARED, AND ITS PLACEMENT IS THE WHOLE EXPERIMENT.** Every shared string must appear
## EXACTLY ONCE across all three zones — which is simultaneously "this shape rendered its party" and
## "the party appears nowhere else in the panel". The strings a shape legitimately does not show
## (`SHAPE_B_HIDDEN_LINES`) must appear ZERO times, so a shape cannot quietly satisfy the sweep by
## drawing the party twice.
##
## It sweeps `_zone_contents`, not the panel's own tree: the narrow shell keeps the inactive tabs
## detached, so a walk from the panel would only ever see the zone that is up and the "nowhere else"
## half of the claim would pass vacuously.
func _assert_copy(shape: String) -> void:
	var texts: Array[String] = []
	for key_variant in _zone_contents:
		_collect_texts(_zone_contents[key_variant], texts)
	var hidden: Array = SHAPE_B_HIDDEN_LINES if shape == SHAPE_B else []
	for line_variant in _shared_copy():
		var line := String(line_variant)
		var want := 0 if hidden.has(line) else 1
		var found := _count_texts(texts, line)
		if found == want:
			continue
		_fail("%s — shape %s renders '%s' %d time(s), expected %d" % [
			_current_state, shape, line, found, want])
	_pass("shape %s states every shared line exactly once (%d hidden by selection)" % [
		shape, hidden.size()])


## The party block is in the zone this shape says it lives in — asked of that zone's content root
## alone, so a shape that drew the block in the right zone AND somewhere else fails on `_assert_copy`
## while a shape that drew it in the wrong zone fails here.
func _assert_party_lives_in(shape: String, zone: StringName) -> void:
	var texts: Array[String] = []
	_collect_texts(_zone_contents[zone], texts)
	for line in [NEAR_SOURCE_LINE, FAR_SOURCE_LINE, NEAR_CREW, FAR_CREW]:
		if _count_texts(texts, line) == 1:
			continue
		_fail("%s — shape %s should carry '%s' in its %s zone, and does not" % [
			_current_state, shape, line, zone])
		return
	if _find_meta_control(_zone_contents[zone], HAUL_BUTTON_META) == null:
		_fail("%s — shape %s should carry the haul verb in its %s zone, and does not" % [
			_current_state, shape, zone])
		return
	_pass("shape %s keeps both parties and the haul verb in the %s zone" % [shape, zone])


## The picker is up, states its head, its three destinations and both buttons — and fits the card it
## floats in, which is the claim that matters for a form taken off the panel.
func _assert_picker() -> void:
	if _picker_float == null or not _picker_float.is_floating():
		_fail("%s — the destination picker was mounted but is not floating" % _current_state)
		return
	var texts: Array[String] = []
	_collect_texts(_picker_float, texts)
	var wanted: Array[String] = [PICKER_HEAD, PICKER_SEND, PICKER_CANCEL]
	wanted.append_array(_picker_row_faces())
	for line in wanted:
		if _count_texts(texts, line) == 1:
			continue
		_fail("%s — the picker states '%s' %d time(s), expected 1" % [
			_current_state, line, _count_texts(texts, line)])
		return
	var card := _picker_float.card()
	var needed := card.get_combined_minimum_size()
	var have := _picker_float.size
	if needed.y > have.y + ZONE_BOUNDS_TOLERANCE or needed.x > have.x + ZONE_BOUNDS_TOLERANCE:
		_fail("%s — the picker card needs %.0fx%.0f but the float is %.0fx%.0f" % [
			_current_state, needed.x, needed.y, have.x, have.y])
		return
	_pass("the picker states its head, three destinations and both buttons, and fits its %.0fx%.0f card" % [
		have.x, have.y])


func _picker_row_faces() -> Array[String]:
	var faces: Array[String] = []
	for face in PICKER_ROWS:
		faces.append(String(face))
	return faces


## Every shared line, in one list — the copy contract this prototype is holding all three shapes to.
func _shared_copy() -> Array[String]:
	return [NEAR_SOURCE_LINE, FAR_SOURCE_LINE, NEAR_CREW, FAR_CREW,
		NEAR_PACK, FAR_PACK, NEAR_ATE, FAR_ATE, NEAR_DELIVERY, FAR_DELIVERY, HAUL_VERB]


# ---- tree walks ----------------------------------------------------------------------------------

## The panel's zone HOSTS — the fixed-size boxes a zone's content is anchored into. Named by the panel
## itself (`Zone_*` in the wide shell, `NarrowZoneHost` in the narrow one).
func _find_zone_hosts(node: Node) -> Array:
	var hosts: Array = []
	if String(node.name).begins_with("Zone_") or node.name == "NarrowZoneHost":
		hosts.append(node)
	for child in node.get_children():
		hosts.append_array(_find_zone_hosts(child))
	return hosts


## Walk a zone host for content the box cannot hold VERTICALLY. Zone content roots are plain `Control`
## wrappers reporting no minimum, so the recursion continues past every zero-minimum node; a control
## that DOES report one is measured from where it sits and not descended into, its own minimum having
## already accounted for its children.
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
		if top + needed > host.size.y + ZONE_BOUNDS_TOLERANCE:
			failures.append("zone %s: %s (%s) needs %.0fpx from y=%.0f but the box is only %.0fpx" % [
				host.name, content.name, content.get_class(), needed, top, host.size.y])


## …and the WIDTH twin. A vertical dock RESERVES its width, so content wider than the box has nowhere
## to go and the clip lands on the right end of every row at once — which is why this is asked of the
## whole zone rather than of the row that happens to be widest.
func _collect_zone_content_overrun(node: Node, host: Control, failures: Array[String]) -> bool:
	var overran := false
	for child in node.get_children():
		if not (child is Control):
			continue
		var content: Control = child
		if not content.visible:
			continue
		if _collect_zone_content_overrun(content, host, failures):
			overran = true
			continue
		var needed := content.get_combined_minimum_size().x
		if needed <= 0.0:
			continue
		var left := content.global_position.x - host.global_position.x
		if left + needed > host.size.x + ZONE_BOUNDS_TOLERANCE:
			failures.append("zone %s: %s (%s) needs %.0fpx from x=%.0f but the box is only %.0fpx wide" % [
				host.name, content.name, content.get_class(), needed, left, host.size.x])
			overran = true
	return overran


## Every rendered face under `node` — `Label` text and `Button` text, which between them carry all of
## this prototype's copy.
func _collect_texts(node: Node, out: Array[String]) -> void:
	if node is Label:
		out.append((node as Label).text)
	elif node is Button:
		out.append((node as Button).text)
	for child in node.get_children():
		_collect_texts(child, out)


## How many collected faces are EXACTLY `needle`. Exact rather than `contains`, so a line cannot be
## counted by a neighbour that happens to embed it — each party line is its own `Label`, which is what
## makes the exact compare available here.
func _count_texts(texts: Array[String], needle: String) -> int:
	var found := 0
	for text in texts:
		if text == needle:
			found += 1
	return found


## The first descendant carrying `meta`, or null — reaching a control by IDENTITY rather than by its
## face, which is what `test-harnesses.md` asks of every finder.
func _find_meta_control(node: Node, meta: StringName) -> Control:
	if node is Control and (node as Control).has_meta(meta):
		return node as Control
	for child in node.get_children():
		var found := _find_meta_control(child, meta)
		if found != null:
			return found
	return null


# =====================================================================================
# Window, capture, and the ONE failure sink
# =====================================================================================

## Hold the window at `size` and wait for the LOGICAL viewport to be the projection of it — every
## width asserted above is measured against that canvas, not against `window.size`.
func _pin_window(size: Vector2i, strict: bool = true) -> void:
	_pinned_size = size
	var window := get_window()
	window.mode = Window.MODE_WINDOWED
	window.size = size
	if _pinned_canvas != Vector2i.ZERO:
		window.content_scale_size = _pinned_canvas
	for _i in range(WINDOW_PIN_MAX_FRAMES):
		if window.size == size and window.mode == Window.MODE_WINDOWED and _canvas_is_projected():
			return
		window.mode = Window.MODE_WINDOWED
		window.size = size
		await get_tree().process_frame
	if not strict:
		return
	if window.size != size:
		_report_canvas_drift("window pinned to %s but reports %s" % [size, window.size])
	elif not _canvas_is_projected():
		_report_canvas_drift("window is %s but the logical viewport is %s, not the %s canvas" % [
			size, get_viewport().get_visible_rect().size, _expected_canvas()])


func _canvas_is_projected() -> bool:
	if _pinned_canvas == Vector2i.ZERO:
		return true
	return get_viewport().get_visible_rect().size.distance_to(_expected_canvas()) <= CANVAS_PROJECTION_TOLERANCE


func _expected_canvas() -> Vector2:
	return Vector2(_pinned_canvas) / maxf(get_window().content_scale_factor, CONTENT_SCALE_MIN)


## Settle the window ONCE, taking the maximize DELIBERATELY on the way — whether a run passes through
## a monitor-sized window is otherwise a coin flip the WM applies asynchronously, and it is a coin
## flip the pixels remember. Ask for it, then undo it, so every run takes the same path.
func _stabilize_canvas() -> void:
	get_window().mode = Window.MODE_MAXIMIZED
	for _i in range(CANVAS_STABLE_MAX_FRAMES):
		if get_window().size != PREVIEW_SIZE:
			break
		await get_tree().process_frame
	var stable := 0
	for _i in range(CANVAS_STABLE_MAX_FRAMES):
		if get_window().size == PREVIEW_SIZE and get_window().mode == Window.MODE_WINDOWED:
			stable += 1
			if stable >= CANVAS_STABLE_FRAMES:
				return
		else:
			stable = 0
			# NOT strict: this loop is deliberately driving the window through a maximize and reports
			# its own failure below if it never settles.
			await _pin_window(PREVIEW_SIZE, false)
		await get_tree().process_frame
	_report_canvas_drift("the window never held the pinned %s canvas — frames will drift" % PREVIEW_SIZE)


## The viewport image at the pinned size (or an integer HiDPI multiple of it), or null when there is
## no renderer to read back from.
func _capture(name: String) -> Image:
	for _i in range(WINDOW_PIN_MAX_FRAMES):
		var image := get_viewport().get_texture().get_image()
		if image == null:
			# **A CONDITION THAT FAILS ONLY BECAUSE THERE IS NO RENDERER IS NOT A FAILURE.** Under
			# `--headless` Godot selects the dummy driver and there is no viewport texture at all;
			# every assertion above still ran and still counted. Run WITHOUT `--headless` for PNGs.
			push_warning("%s: null image (dummy renderer?) — skipping %s.png; run without --headless" % [
				HARNESS_NAME, name])
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
	_fail("viewport never came back to the pinned %s canvas for %s" % [_pinned_size, name])
	return null


## The hang guard from the scene, checked for its method rather than assumed: calling a missing method
## on an untyped `Node` is a runtime error, and one raised here would abort `_ready` exactly the way
## the guard exists to survive.
func _resolve_watchdog() -> Node:
	var node := get_node_or_null(WATCHDOG_NODE)
	if node != null and node.has_method(WATCHDOG_PROGRESS_METHOD):
		return node
	push_warning(("%s: no %s node in the scene — the run has NO hang guard. Restore it from "
		+ "tools/work_party_proto.tscn (see preview_watchdog.gd).") % [HARNESS_NAME, WATCHDOG_NODE])
	return null


func _note_progress() -> void:
	if _watchdog != null:
		_watchdog.note_progress()


## The ONE failure sink, holding this file's only `push_error`, so `_failures` cannot drift from what
## was printed. Every caller passes the text AFTER the `FAIL — ` token.
func _fail(message: String) -> void:
	_failures += 1
	push_error("%s: FAIL — %s" % [HARNESS_NAME, message])


## The passing twin, so a lost assertion shows up as a missing `PASS` line rather than as silence.
func _pass(message: String) -> void:
	_asserts += 1
	print("%s: PASS — %s (%s)" % [HARNESS_NAME, message, _current_state])


## Is this run using the headless display driver, i.e. is there no window behind `_pin_window`?
func _is_headless() -> bool:
	return DisplayServer.get_name() == HEADLESS_DISPLAY_DRIVER


## A window/canvas the pin would not hold: a real failure in a window, and a skip under `--headless`,
## where the stub window can never hold it and reporting one would fail every clean compile pass.
func _report_canvas_drift(message: String) -> void:
	if _is_headless():
		push_warning("%s: %s (no window under the %s display driver — skipped; run windowed to capture)"
			% [HARNESS_NAME, message, HEADLESS_DISPLAY_DRIVER])
		return
	_fail(message)


## **THE ONLY WAY OUT.** The status is derived from the run's own tally in exactly one place, and the
## hang guard is stood down here so a slow shutdown is never reported as a stall.
func _finish() -> void:
	if _watchdog != null:
		_watchdog.disarm()
	if _failures > 0:
		print("%s: RUN FAILED — %d failure(s), %d assert(s) OK; see the FAIL lines above" % [
			HARNESS_NAME, _failures, _asserts])
	else:
		print("%s: run complete — %d assert(s) OK, no failures" % [HARNESS_NAME, _asserts])
	get_tree().quit(EXIT_FAILED if _failures > 0 else EXIT_OK)


func _settle() -> void:
	_note_progress()
	# Re-assert the window EVERY state: the WM's maximize lands asynchronously and can arrive between
	# two states, rendering them at different resolutions.
	await _pin_window(_pinned_size)
	await get_tree().process_frame
	RenderingServer.force_draw()
	await get_tree().process_frame


func _save(name: String) -> void:
	_current_state = name
	var image: Image = await _capture(name)
	if image == null:
		return
	var err := image.save_png("%s/%s.png" % [OUT_DIR, name])
	if err != OK:
		_fail("failed to save %s (err %d)" % [name, err])
	else:
		print("%s: saved %s.png" % [HARNESS_NAME, name])
